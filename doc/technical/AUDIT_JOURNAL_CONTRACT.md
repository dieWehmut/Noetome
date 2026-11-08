# 审计事件日志合同（audit-journal-v1）

本文件冻结 `noetome journal adopt|build|list|status` 的结构、判据与产物口径，对应需求
`FR-40 可审计日志与问题定位`、`NFR-15 安全与隐私`。它描述的是**结构**：这条日志记录
谁在什么时候对哪个任务做了什么，以及失败时能定位到哪四层；它不判断内容质量与科学正确性。

## 1 边界

- 只读：`adopt` 冻结输入的副本，`build` 只写一个新的输出根，`status`/`list` 一个字都不写。
- 零模型、零网络：`model_calls` 与 `network_requests` 恒为 0，报告里明写。
- 只追加、不重写：日志条目的先后由 `(task_id, at, event_id)` 决定，重排声明不会悄悄改变日志。
- 不做「过滤后继续」：声明里出现疑似凭据字样时整包拒绝，而不是把它替换掉再往下走。
- 本阶段不提供界面、不做日志轮转、不做加密、不做远程上传。

## 2 声明结构（13 字段）

| 字段 | 说明 |
| --- | --- |
| `policy` | 必须是 `audit-journal-v1` |
| `schema_version` | 必须是 1 |
| `producer` | 必须是 `fixture_author` |
| `case_id` | 标识符，1..=48 |
| `description` | 说明文本，1..=300 |
| `sources` | 来源范围：`source_id`、`locator`（相对路径）、`description` |
| `objects` | 知识对象：`object_id`、`source_id`、`label` |
| `files` | 受影响文件：`file_id`、`object_id`、`path` |
| `tasks` | 任务：`task_id`、`title`、`sources`、`objects`、`files` |
| `events` | 事件：`event_id`、`task_id`、`at`、`kind`、`class`、`action`、`message` |
| `limits` | `max_tasks` ≤16、`max_events` ≤256、`max_sources` ≤16、`max_objects` ≤32、`max_files` ≤64 |
| （以上为 11 项，另含嵌套记录字段） | 全部记录都 `deny_unknown_fields`：多写一个字段就是拒绝 |

事件 `kind` 只有四种：`started`、`failed`、`retried`、`resolved`。严重级由 kind 推导，
不接受声明：`failed` 是 `error`，其余三种是 `info`。

## 3 冻结根布局

`adopt` 建出的根：

```
<root>/control/project.json     声明原文（保留来源缩进，不重新编码）
<root>/control/inputs.json      规范编码、无尾换行
<root>/control/manifest.json    规范编码、无尾换行，13 字段含五处身份与两份摘要
```

顶层只允许一个 `control/`，`control/` 必须恰好三项。`manifest.json` **最后写**：
半成品根上不能先出现清单。任一步失败且根身份未变时删掉整个根。

`manifest.json` 的五个身份字段来自 `crate::compile_identity::Identity`，只有
device+inode（Unix）或 volume+index（Windows），**不含文件长度**。早期版本把长度编进身份，
写清单本身就会改变清单刚捕获的值，于是每次 `open` 都报身份漂移；这条教训冻结在此。

## 4 adopt 的顺序

1. 读声明、解析、`validate()`（下面第 5 节的顺序）。
2. **先跑一次 `journal_model::journal()` 预演**：产不出日志的声明不得建根。
3. 目标必须尚不存在；父目录必须可访问；父目录身份记入清单。
4. 建根、写三个文件、算身份、回读比对。
5. `open()` 复核 `case_id` 一致后才返回摘要。

## 5 validate 的顺序与判据

顺序固定，先命中先报：

1. `policy` → `schema_version` → `producer` → `case_id` → `description`。
2. `limits` 五项范围。
3. `sources` 非空、`source_id` 唯一、`locator` 必须是相对路径。
4. `objects` 数量上限、`source_id` 必须已登记、`object_id` 唯一。
5. `files` 数量上限、`object_id` 必须已登记、`file_id` 唯一。
6. `tasks` 非空且不超上限；**每个任务必须至少声明一个来源与一个知识对象**——
   定位不到来源或对象的失败无法复核，所以这种任务在声明阶段就退回。
   引用的 `source`/`object`/`file` 必须存在且不重复；**声明的文件必须属于该任务列出的对象之一**。
7. `events` 不超上限、`task_id` 必须存在、`at ≥ 1`、kind ∈ 四种、`class`/`action`/`message` 非空、
   `event_id` 唯一、**同一任务内 `at` 必须严格递增**（两个事件同一序号，先后无法判定）。
8. 最后扫全部字符串字段找凭据形状字样（8 个标记：`api_key`、`apikey`、`password`、`secret`、
   `bearer `、`authorization:`、`token=`、`-----begin`，ASCII 小写比较），命中即
   `JOURNAL_SECRET_MARKER`（exit 3）并列出命中的标签与标记。

## 6 日志产物

`journal.json` 三张表，**三张表都按 `task_id` 排序**（同一份日志不许有两种说法）：

- `tasks[]`：`task_id`、`title`、`sources`、`objects`、`files`、`events`、`failures`、`outcome`。
  `outcome` 是该任务最后一条事件的 kind，没有事件时为 `none`。
- `events[]`：`event_id`、`task_id`、`at`、`kind`、`severity`、`class`、`action`、`message`。
- `failures[]`：在 failed 事件基础上补 `task_title`、`sources`、`objects`、`files` 与
  `followed_by_action`。**这个布尔值是必需的**：没有它，读者分不清"我们重试了"和"我们放弃了"。
  判定方式是顺序扫描——`failed` 进待处理集合，后续 `retried`/`resolved` 清空待处理并标记已跟进。

`report.json` 的 `counts` 是 13 项：`tasks`、`events`、`started`、`failed`、`retried`、`resolved`、
`sources`、`objects`、`files`、`located_failures`、`unresolved_failures`、`model_calls`、`network_requests`。
其中 `located_failures` 必须等于 `failed`（每条失败都得能定位到四层），`unresolved_failures` 是
`followed_by_action == false` 的条数。

`report.md` 四节：`## 事件规模`、`## 失败定位`（表：事件|任务|分类|来源范围|知识对象|受影响文件|后续动作）、
`## 任务状态`（表：任务|标题|事件|失败|当前结果）、`## 边界`。

## 7 输出根与字节确定性

```
<output>/journal.json       规范编码 + 一个尾换行
<output>/report.json        规范编码 + 一个尾换行
<output>/report.md          原样
<output>/manifest.json      规范编码 + 一个尾换行，10 字段
```

输出清单**不含 `created_at`**：同一冻结输入的两次构建必须四个文件全字节相同。
输出根必须尚不存在；不得与审计根互相包含；写盘失败时删掉整个输出根。

## 8 status 与 list

`status` 是人类可读的 11 行：已建立、所有权（两摘要与建立时间）、冻结规模、事件构成、
失败定位（含未处理条数）、逐任务一行（标题、事件数、失败数、当前结果）、凭据检查、零调用边界。

`list` 是 pretty JSON，13 键：`policy`、`producer`、`journal_version`、`schema_version`、`case_id`、
`root`、`created_at`、`project_sha256`、`inputs_sha256`、`counts`、`tasks`、`failures`、
`model_calls`、`network_requests`。

两个动词都只读：运行前后审计根的字节树必须相同。

## 9 CLI 与错误码

```
noetome journal adopt <声明> <审计根>
noetome journal build <审计根> <输出根>
noetome journal list  <审计根>
noetome journal status <审计根>
```

| 错误码 | 退出码 | 何时出现 |
| --- | --- | --- |
| `JOURNAL_ARGUMENT` | 2 | 参数不合法、目标已存在、声明结构或引用不成立 |
| `JOURNAL_INTEGRITY` | 3 | 冻结内容被改动、身份漂移、布局不符、写盘校验失败 |
| `JOURNAL_UNKNOWN_ROOT` | 3 | 目标不是已建立的审计根（陌生目录保持只读） |
| `JOURNAL_SECRET_MARKER` | 3 | 声明里出现疑似凭据字样，整包拒绝 |
| `CLI_INVALID_ARGUMENT` | 2 | 动词或参数个数不对 |

## 10 独立审计器

`scripts/verify_audit_journal.py` 不 import Rust 产品、不跑 CLI 判定，只用标准库独立复刻：
`validate()` 的 8 步顺序、`secret_hits`、三张表的排序、`followed_by_action`、13 项 counts、
markdown 四节与边界句。

三种用法：

- `<root>`：只查一个已建立的根（布局、清单、声明、输入、身份、冻结）。
- `<root> --output <输出根>`：再加查输出四文件与清单摘要。
- `--package <夹具包>`：只查夹具包自身（先验、cases、变体、冻结清单），不需要根。
- `--freeze`：重写 `frozen-sha256.json`，打印冻结了多少成员。

任何缺口都是失败（退出码 1），而不是提示。

## 11 本轮明确不做

1. 不做界面上的日志浏览与筛选。
2. 不做日志轮转、压缩、归档或加密。
3. 不记录模型请求与响应的正文（本阶段没有模型调用）。
4. 不做跨机器的日志合并与时钟对齐。
5. 不判断事件是否"应该发生"，只核对结构、顺序与可定位性。
6. 不把 `status`/`list` 变成写操作（不落缓存、不更新访问时间以外的元数据）。
7. 不承诺凭据检测覆盖所有形态：它只认 8 个字面标记，写法足够怪就可能漏过。
