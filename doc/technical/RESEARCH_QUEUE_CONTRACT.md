# 研究候选队列契约（B08-T01）

本契约定义 Noetome 第一版「研究候选队列」：什么东西可以进入队列、队列如何记住拒绝、
什么操作会改变状态、磁盘上有哪些文件，以及哪些边界本轮明确不做。

对应需求：FR-11（资料驱动的覆盖边界）、FR-12（三类内容与授权）、FR-13（候选字段、
决定动作、终止理由、拒绝记忆）、FR-28（报告单列研究候选）、AC-32（越界研究只进队列）、
AC-33（四维状态与候选单列）。

## 1. 边界

- 队列**不联网、不调用任何模型**。`list`/`status`/`observe`/`decide` 全部是本地确定性操作，
  输出里固定报告 `retrieval_calls: 0` 与 `model_calls: 0`。
- 队列**不是**知识库，也不是托管 Vault。它只记「有哪些候选主题、谁提出、为什么、证据线索在哪」，
  从不写 `vault/`、从不发布笔记。
- 队列**不扩大本轮必做集合**。必做集合只由 `adopt` 时冻结的
  `objective.authorized_targets` + `objective.required_prerequisites` 决定，之后任何动词都不能增删。
- 队列只操作**新的测试根**；不接管生产项目目录。

## 2. 三类内容（对应 FR-12）

| `scope_class` | 含义 | 是否进必做集合 |
| --- | --- | --- |
| `authorized_target` | 本轮已授权的目标主题 | 是（由 adopt 冻结） |
| `required_prerequisite` | 已授权目标的理解必需前提 | 是（由 adopt 冻结） |
| `out_of_scope` | 新发现、但不在本轮批准范围内的主题 | 否，只进候选队列 |

越界主题只能以 `out_of_scope` 记录。产品不提供把它直接提升为必做集合的动词；
要让它变成目标，必须新建作者包并重新 `adopt`。

## 3. 候选的八个字段（对应 FR-13）

每条候选至少携带以下信息，缺一即拒绝该输入文件：

| 字段 | 说明 |
| --- | --- |
| `subject` | 主题（去空白后非空，≤120 字符，队列内唯一） |
| `scope_class` | 上表三类之一 |
| `reason` | 为什么在本次工作中发现它（发现原因） |
| `relation` | 与当前目标的关系 |
| `benefit` | 可能收益 |
| `source_leads` | 来源线索（相对路径或带锚点的同文件定位；不得绝对路径、反斜杠或 `..`） |
| `estimated_cost` | 预估代价 |
| `depends_on` | 依赖的主题（必须在本轮授权集合或同批观测内，且不得指向自身） |

`state`（状态）由队列自己维护，不由输入决定。

## 4. 状态与决定动作

候选状态：`proposed`、`accepted`、`rejected`、`deferred`、`merged`。

决定动作（`decide` 输入里的 `action`）：

| 动作 | 结果 | 限制 |
| --- | --- | --- |
| `accept` | `accepted` | 对 `rejected`/`merged` 报错；必须给 `reason` |
| `reject` | `rejected` 并追加一条拒绝记忆 | 对 `merged` 报错；必须给 `reason` |
| `defer` | `deferred` | 必须给 `reason` |
| `merge` | 源置 `merged`，证据转移到 `merged_into` 目标 | 目标不得是 `merged`/`rejected`，不得是自身，证据数不得超上限 |
| `revise_scope` | 追加一条范围修订记录（`before`/`after`） | 必须给新的 `scope_note`；超过 `limits.max_revisions` 时要求升级为人工判断 |

`decide` **不能创建新候选**：决定文件里出现队列中不存在的主题时，整份文件被拒（exit 2），
一个字节都不写。

## 5. 拒绝记忆与重开（对应 FR-13 验收）

- 被 `reject` 的主题再次被观测、且**没有** `revisit_reason`：记录 `blocked_repeat`，
  `counters.blocked_repeats` 加一，候选状态保持 `rejected`，不追加证据。
- `revisit_reason` 与既有拒绝理由或既有证据说明**完全重复**：同样按 `blocked_repeat` 挡回。
- 带**真正新理由**：状态回到 `proposed`，追加一条证据，`counters.reopenings` 加一，结果记 `reopened`。
- 同一轮内重复观测同一主题（同 `round`、同 `reason`）：记 `duplicate_evidence`，不重复计数。

因此「用户拒绝的主题不会每轮无理由重提」是可审计的：`blocked_repeats` 与 `reopenings`
都是只追加事件流推导出来的数字。

## 6. 归并

- 观测到已存在的 `proposed`/`accepted`/`deferred` 候选：追加证据，结果记 `merged_evidence`。
- `merge` 动作：源候选的全部证据转移到目标（按 `round` + `note` 去重），源置 `merged` 并记 `merged_into`，
  `counters.merges` 加一。已 `merged` 的候选**不能**再被当作观测目标或合并源。
- `queue_only`（越界集合）自动排除已 `merged` 的候选，因此合并后目标集合不会虚增。

## 7. 磁盘布局

```
<队列根>/
└── control/
    ├── owner.json        所有权记录（规范 JSON，无尾随换行）
    ├── research.sqlite   候选与只追加事件
    └── queue.lock        写入锁（空文件）
```

- 根下**只允许** `control/`；`control/` 下**只允许**上述三个文件。多一个条目即拒绝（exit 3），
  保留现场、不修复、不迁移。
- `research.sqlite` 的 `application_id = 0x4E525148`（ASCII `NRQH`），`user_version = 1`，
  `journal_mode = delete`，`synchronous = FULL`。三张表定义与代码常量逐字一致：
  `metadata`（单行）、`candidates`（按 `position` 排序）、`events`（自增序号，只追加）。
- 所有记录文件必须是**规范 JSON**：字段集完全一致、无重复键、无多余空白。加一个空格都算损坏。

## 8. 所有权与身份校验

`owner.json` 记录根、父目录、`control/`、`owner.json` 自身、数据库、锁文件的身份（Unix 为
`device`+`inode`，Windows 为 `volume`+`file index`），以及作者包字节的 SHA256、冻结目标与限额。

`adopt` 之前队列根必须**尚不存在**；`adopt` 失败时只回滚自己刚创建的目录。
打开既有队列时逐项复核：目录名集合、`owner.json` 的 policy/producer/schema_version、
根的 canonical 路径与记录一致、六处身份一致、数据库结构、`metadata` 与 `owner.json` 的五项一致性。
任一项不符即拒绝（exit 3）并保留现场。

## 9. CLI

```
noetome research adopt <author.json> <尚不存在的队列根>
noetome research observe <队列根> <观测文件>
noetome research decide <队列根> <决定文件>
noetome research list <队列根>
noetome research status <队列根>
```

- `adopt` 输出多行中文摘要；`observe`/`decide` 输出本轮 `applied`、当前计数、必做集合与越界集合；
  `list` 输出机器可读 JSON；`status` 输出多行中文摘要（含零检索零模型声明）。
- 退出码：`0` 成功；`2` 参数/输入不合法（未写字节）；`3` 完整性或未知根（保留现场）。

错误码：`RESEARCH_ARGUMENT`（exit 2）、`RESEARCH_INTEGRITY`（exit 3）、
`RESEARCH_UNKNOWN_ROOT`（exit 3）、`RESEARCH_BUSY`（exit 3）。

## 10. 报告与四维状态（对应 FR-28、AC-33）

编译报告的四维状态目前仍固定把 `optional_research` 记为 `not_applicable`（"本轮未启用研究"）。
研究候选走**独立报告**：`research list` 的 `must_do` / `queue_only` / `counters` 与
`research status` 的中文摘要，明确说明候选单列、不计入主任务的完成或失败。

## 11. 本轮明确不做

- 不做任何外部检索、网络抓取或模型调用（那是 B08-T02）。
- 不把候选自动转成知识、原子、笔记或托管 Vault 发布。
- 不做候选的跨项目共享、导出格式或 UI。
- 不实现写作反馈缺口闭环（那是 B08-T03）。
