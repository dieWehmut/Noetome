# 保留与清理计划合同（retention-policy-v1）

对应需求 FR-40（可审计日志与问题定位）中"存储保留和清理策略可配置，清理前说明是否影响恢复及追溯"与 NFR-08（可恢复）。验收场景是"清理缓存不破坏已发布版本的必需索引"。

本文件描述的是**结构与判据**：给定一份保留声明，能推出哪一种清理计划、哪些条目受保护、删除会丧失什么。它不判断内容质量，也不删除任何文件——`retention` 的全部动词都是只读或只写派生计划。

## 1 边界

- 只生成计划，**不删除、不移动、不压缩任何文件**；输出根里只有派生报告。
- 已发布版本必需的索引与追溯条目一律受保护；只要保留策略会让它们过期，**整条拒绝**，而不是跳过它们继续清理。
- 清理前必须说明影响：哪些条目会删、哪些恢复能力随之丧失、哪些追溯链会变短。
- 同一冻结输入的两次构建必须字节相同（输出清单不含时间戳）。
- 不联网、不调用模型；模型调用与网络请求计数恒为 0。

## 2 保留声明（9 字段）

| 字段 | 含义 |
| --- | --- |
| `policy` | 固定 `retention-policy-v1` |
| `schema_version` | 固定 `1` |
| `producer` | 固定 `fixture_author`（当前只接受自造包） |
| `case_id` | 本次保留声明的编号，仅 ASCII 标识符 |
| `description` | 一句人类可读的说明 |
| `categories` | 可清理类别，见下表 |
| `entries` | 存储条目，见下表 |
| `published` | 已发布版本及其必需条目 |
| `limits` | 四项上限：`max_categories` 1..=8、`max_entries` 1..=64、`max_published` 1..=8、`max_required` 1..=32 |

类别（`CategorySpec`）：`category_id`、`label`、`keep_days`（1..=3650）。

条目（`EntrySpec`）：`entry_id`、`category_id`、`bytes`、`age_days`（0..=3650）、可选 `recovery`（这条记录还能恢复什么）、可选 `trace`（它属于哪条追溯链）。

已发布版本（`PublishedSpec`）：`release_id`、`required_index`（必需索引条目）、`required_traces`（必需追溯条目）。

## 3 冻结根布局

```
<根>/control/project.json     保留声明原文（逐字节，不重新编码）
<根>/control/inputs.json      冻结输入（规范编码，无尾换行）
<根>/control/manifest.json    所有权清单（规范编码，无尾换行，最后写）
```

- 身份只含 `platform` + `device` + `inode`（Windows 上是 volume + index），**不含文件长度**。早期把长度编进身份字符串，导致写清单会改掉它刚捕获的值，每次 `open` 都报身份漂移；教训冻结在这里。
- `manifest` 必须最后写：半成品根上不能先出现清单。
- `adopt` 失败时若根身份未变则整根删除，不留半个根。

## 4 adopt 的顺序

1. 解析路径，父目录必须已存在且可访问。
2. 读声明，`Declaration::parse` 会跑完整 `validate()`。
3. **先跑一遍 `retention_model::plan` 预演**：不能让"产不出计划的声明"建出根。
4. 目标根必须尚不存在。
5. 建 `control/`，写声明原文、规范输入、最后写清单并读回逐字节比对。

## 5 校验顺序与错误码

`validate()` 顺序固定：`policy` → `schema_version` → `producer` → `case_id` → `description` → `limits` → `categories` → `entries` → `published`。

| 错误码 | 退出码 | 触发条件 |
| --- | --- | --- |
| `RETENTION_ARGUMENT` | 2 | 声明结构或引用不合法：类别未声明、条目编号重复、必需条目不存在、超上限、文本超长 |
| `RETENTION_INTEGRITY` | 3 | 冻结根被改动：清单字段不符、身份漂移、摘要不符、输入不能复现当时结论 |
| `RETENTION_UNKNOWN_ROOT` | 3 | 目录不是已建立的保留根（缺 `control/`）：陌生目录保持只读 |
| `RETENTION_PROTECTED` | 3 | 保留策略会让已发布版本必需的条目过期 |

`entries[].category_id` 必须命中已声明类别；`published[].required_*` 的每一项都必须存在于 `entries` 里——"保护一个不存在的条目"等于没有保护。

## 6 受保护即整条拒绝

`protected_map()` 先把每个已发布版本的 `required_index` 与 `required_traces` 映射成 `RequirementRecord{entry_id, release_id, reason}`，`reason` 取 `已发布版本必需的索引条目` 或 `已发布版本必需的追溯条目`。

逐条目判 `expired = entry.age_days > category.keep_days`。**受保护且已过期**的条目全部收集起来；只要集合非空，`plan` 就在产出任何动作前整体报 `RETENTION_PROTECTED`，文案列出全部冲突条目及其所属版本，并说明"整条拒绝，而不是跳过它们继续清理"。

这条取舍的理由：跳过受保护条目继续删别的，会让用户以为"清理成功、只剩保护项"，而实际上他想要的保留期已经和自己的发布策略打架了。冲突必须在动手前暴露。

## 7 逐条动作与理由

`actions` 覆盖**每一个**条目（不只是被删的），按 `entry_id` 排序：

- 过期且未受保护 → `action = delete`，`reason = 超过保留期`
- 未过期 → `action = keep`，`reason = 未超过保留期`
- 受保护（未过期，否则已被第 6 节拒绝）→ `action = keep`，`reason = 已发布版本仍需引用`，并带 `release_id`

`release_id` 只在有值时出现（`skip_serializing_if`），先验与审计器都按"没有这个键"比对。

## 8 影响说明

- `recovery_loss`：每一个被删且声明了 `recovery` 的条目，逐条说明"恢复能力「…」随之丧失"。
- `trace_changes`：按 `trace` 分组计算 `before`（原有条目数）、`after`（保留条目数）、`removed`（被删的条目编号，排序）。只列出真正变短的链。
- `counts` 12 项：`categories`、`entries`、`published`、`deleted`、`kept`、`protected`、`deleted_bytes`、`kept_bytes`、`lost_recovery`、`shortened_traces`、`model_calls`、`network_requests`。

影响说明必须与实际动作逐条一致：审计器用同一条声明独立重算这三组，任何一条对不上就报失败。

## 9 输出根与字节确定性

```
<输出>/plan.json        清理计划（规范编码 + 一个尾换行）
<输出>/report.json      计数与说明（规范编码 + 一个尾换行）
<输出>/report.md        人类可读报告
<输出>/manifest.json    输出清单（规范编码，无尾换行，最后写）
```

输出清单 10 字段：`policy`、`producer`（`retention_plan_output`）、`schema_version`、`case_id`、`source_root`、`source_inputs_sha256`、`plan_sha256`、`report_sha256`、`markdown_sha256`、`counts`。**不含 `created_at`**——同一冻结输入的两次构建必须逐字节相同。

`report.md` 六节：`# 保留与清理计划（case）`、`## 清理规模`、`## 逐条动作`、`## 影响说明`、`## 受保护条目`、`## 边界`。

## 10 动词与只读保证

| 动词 | 行为 |
| --- | --- |
| `retention adopt <声明> <根>` | 冻结声明并预演一次计划，只写 `control/` |
| `retention build <根> <输出>` | 由冻结输入重算并写出计划、报告与输出清单 |
| `retention list <根>` | 只读重算，输出机器可读 JSON |
| `retention status <根>` | 只读重算，输出中文摘要（含逐类别删除/保留计数） |

`list` 与 `status` 都不写任何字节；审计器的"读取前后快照相等"检查就是在证明这一点。

## 11 本轮明确不做

- 不真的删文件、不做"清理后校验"——本阶段只产出可审查的计划。
- 不读取真实运行库或缓存目录，条目全部来自声明。
- 不做后台定时清理，也没有"到期自动执行"。
- 不判断哪条日志/索引更值得留：保留期由声明给出，不由模型推断。
- 不处理已发布版本的退役流程（只是提示"先把已发布版本退役"）。
- 不覆盖真实数据规模下的清理耗时，也不做并发清理。
- 不把审计器算作第二实现：它只复算本文件的判据，任何与产品不符的结论都应当先怀疑声明本身。
