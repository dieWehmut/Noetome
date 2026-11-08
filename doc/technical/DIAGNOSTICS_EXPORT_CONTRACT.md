# 诊断导出合同（diagnostics-export-v1）

本文件冻结 `noetome diagnostics adopt|build|list|status` 的结构、判据与产物口径，对应需求
`FR-40 可审计日志与问题定位`、`NFR-08 可恢复`、`NFR-15 安全与隐私`。它描述的是**结构**：
一次导出包含哪些范围、每个范围有多少条目与字节、哪些被明确排除；它不判断内容质量，
也不替代真实采集（`scientific_status` 恒为 `not_run`）。

## 1 边界

- 只读：`adopt` 冻结输入的副本，`build` 只写一个新的输出根，`status`/`list` 一个字都不写。
- 零模型、零网络：`model_calls` 与 `network_requests` 恒为 0，报告里明写。
- 勾选决定内容：没被勾选的类别如实标为未导出，**不静默补全**，也不在预览里掺它的条目。
- 整包拒绝而非「先清洁后继续」：绝对路径与疑似凭据出现即拒绝，且拒绝时不写出任何字节。
- 预览是短摘要，不是原文；本阶段不做加密、不做压缩包、不做远程上传、不做保留清理（属 B14-T03）。

## 2 声明结构（11 字段）

| 字段 | 说明 |
| --- | --- |
| `policy` | 必须是 `diagnostics-export-v1` |
| `schema_version` | 必须是 1 |
| `producer` | 必须是 `fixture_author` |
| `case_id` | 标识符，1..=48 |
| `description` | 说明文本，1..=300 |
| `config` | 复现信息：`key`、`value`，最多 16 项，键唯一 |
| `fingerprints` | 输入指纹：`name`、`digest`（64 位小写十六进制），最多 16 项 |
| `categories` | 范围类别：`category_id`、`label`、`sensitive`、`entries` |
| `selection` | 勾选的类别编号，必须是已声明类别且不重复 |
| `consent` | `sensitive_approved`：是否已记录「同意包含敏感材料」 |
| `limits` | `max_categories` ≤8、`max_entries` ≤64、`max_preview` ≤240 |

嵌套记录同样 `deny_unknown_fields`。`entries` 每项是 `entry_id`、`label`、`bytes`、`preview`；
`entry_id` 在整个声明里全局唯一——预览表按条目点名，重名就无法定位。

## 3 冻结根布局

```
<root>/control/project.json     声明原文（保留来源缩进，不逐字节重新编码）
<root>/control/inputs.json      规范编码、无尾换行
<root>/control/manifest.json    规范编码、无尾换行，13 字段含五处身份与两份摘要
```

顶层只允许一个 `control/`，`control/` 必须恰好三项。`manifest.json` **最后写**：
半成品根上不能先出现清单。任一步失败且根身份未变时删掉整个根。

五个身份字段来自 `crate::compile_identity::Identity`，只有 device+inode（Unix）或
volume+index（Windows），**不含文件长度**。把长度编进身份会让写清单本身改变清单刚捕获的值，
于是每次 `open` 都报身份漂移——这条教训在 B14-T01 已经踩过一次，此处照抄。

## 4 adopt 的顺序

1. 读声明、解析、`validate()`（下面第 5 节的顺序）。
2. `inputs_of()` 内部**先跑一次 `export()` 预演**：产不出导出的声明不得建根。
3. 目标必须尚不存在；父目录必须可访问；父目录身份记入清单。
4. 建根、取根身份、写三个文件、回读比对。
5. `open()` 复核 `case_id` 与规模一致后才返回摘要。

## 5 validate 的顺序与判据

`policy` → `schema_version` → `producer` → `case_id` → `description` → `limits`（三项范围）
→ `config` 上限与键唯一 → `fingerprints` 上限、名称唯一、摘要形态 → `categories` 非空且不超上限、
id 与 label 唯一、**类别不得为空**、条目数不超上限、`entry_id` 全局唯一、预览长度不超上限
→ `selection` 每项必须是已声明类别且不重复 → **绝对路径扫描** → **凭据扫描**。

「类别不得为空」不是洁癖：空类别会让预览看起来比实际更完整，读者以为这一类内容也在包里。

`open()` 会把 `inputs.json` 的规模与自己重新算出的 `inputs_of()` 逐项比对；不相等报
`DIAGNOSTICS_INTEGRITY`，文案是「这份输入已经不能复现当时的结论」。

## 6 同意检查前置

`selection` 命中 `sensitive: true` 的类别且 `consent.sensitive_approved == false` 时，
**在计算任何计数之前**返回 `DIAGNOSTICS_CONSENT`：

```
勾选了敏感范围 source-body，但没有记录同意：导出前必须明确同意包含敏感材料。
```

这是硬停，不是预览注记：导出包会被分享出去，同意必须在导出之前落定，而不是事后提醒。

## 7 勾选、未导出与字节口径

- 勾选的类别：`included = true`，`selected_entries = entries`，`exported_bytes = bytes`。
- 未勾选的类别：`included = false`，`selected_entries = 0`，`exported_bytes = 0`，
  但 `bytes` 保留真实值，并整体计入 `excluded_bytes`。
- `preview` 取该类别**首条目**的预览；空类别在校验阶段已被拒绝，因此不存在「预览为空」。
- 报告里单列 `## 未导出的类别`，逐条写「未勾选，如实标为未导出；这 N 条、M 字节不在包里」。
  全部勾选时保留该节并写「没有未导出的类别」。

`excluded_bytes` 与 `exported_bytes` 相加等于声明里所有类别的字节总和——两个数字都对不上，
就说明有人在某一侧动了手脚。

## 8 绝对路径与凭据

绝对路径不只判「整串以 `/` 开头」：路径常常被粘在句子中间（`见 /home/lyinn/... 下的样例`）。
扫描器逐字符走，遇到 `/` 或 `\` 时看它是否**起一个 token**（index 为 0，或前一个字符是空白、
`(`、`（`、`「`、`[`、`【`、`：`、`:`、`,`、`，`、`;`、`；`、`=`、`"`、`'`），另外单独认
盘符形态 `X:/`、`X:\`。命中即 `DIAGNOSTICS_ABSOLUTE_PATH`。

凭据扫描沿用 8 个标记：`api_key`、`apikey`、`password`、`secret`、`bearer `、`authorization:`、
`token=`、`-----begin`；比对前统一 ASCII 小写。命中即 `DIAGNOSTICS_SECRET_MARKER`。

两者都在 `validate()` 内、`adopt` 建根之前触发，因此**拒绝时不写出任何字节**，
并留下 `left=0` 的证据。

## 9 报告与产物

`report.json` 8 字段：`policy`、`schema_version`、`case_id`、`description`、`categories`、
`counts`、`model_calls`、`network_requests`。`counts` 13 项：

| 名称 | 含义 |
| --- | --- |
| `categories` / `selected_categories` / `excluded_categories` | 类别总数、勾选数、未勾选数 |
| `sensitive_categories` / `sensitive_included` | 敏感类别数、其中被勾选（且已同意）的数 |
| `entries` / `selected_entries` | 条目总数、导出条目数 |
| `exported_bytes` / `excluded_bytes` | 导出字节、未导出字节 |
| `config` / `fingerprints` | 复现信息项数、输入指纹数 |
| `model_calls` / `network_requests` | 恒 0 |

输出根恰好四个文件：

```
<output>/export.json    规范编码 + 一个尾换行
<output>/report.json    规范编码 + 一个尾换行
<output>/report.md      原样
<output>/manifest.json  规范编码 + 一个尾换行，10 字段，无 created_at
```

输出清单不含时间戳，因此同一冻结输入的两次构建必须字节相同。`control/manifest.json`
才有 `created_at`：冻结那一刻值得记录，派生包则不该带上运行时间。

## 10 动词与错误码

| 动词 | 行为 |
| --- | --- |
| `diagnostics adopt <声明> <根>` | 冻结声明，建根，打印摘要 |
| `diagnostics build <根> <输出根>` | 重算导出，写四个文件 |
| `diagnostics list <根>` | 只读，输出机器可读 JSON（15 键） |
| `diagnostics status <根>` | 只读，输出中文摘要（12 行，逐类别一行） |

| 错误码 | 退出码 | 触发 |
| --- | --- | --- |
| `DIAGNOSTICS_ARGUMENT` | 2 | 参数、上限、选择、空类别、输出根已存在或互相包含 |
| `DIAGNOSTICS_INTEGRITY` | 3 | 根布局、清单字段、身份或摘要漂移、输入无法复现 |
| `DIAGNOSTICS_UNKNOWN_ROOT` | 3 | 目标不存在或缺 `control/`：陌生目录保持只读 |
| `DIAGNOSTICS_CONSENT` | 3 | 勾选敏感类别但未记录同意 |
| `DIAGNOSTICS_SECRET_MARKER` | 3 | 声明里出现疑似凭据 |
| `DIAGNOSTICS_ABSOLUTE_PATH` | 3 | 声明里出现绝对路径（含句中路径） |

## 11 本轮明确不做

- 不做真实日志采集：输入是声明，不是从运行中的任务里抓取。
- 不做加密与压缩：输出是四个明文字节文件。
- 不做保留与清理策略：属 B14-T03。
- 不做远程上传或一键反馈。
- 不做预览截断重排：超长预览直接拒绝，不悄悄截到上限。
- 不判断内容质量与科学正确性：`scientific_status` 恒为 `not_run`。
- 不提供界面。
