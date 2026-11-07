# 有界检索契约（B08-T02）

本契约定义 Noetome 第一版「批准范围内的有界检索与终止理由」：检索计划长什么样、
预算怎么预留、七类终止理由靠什么证据成立、中断后怎么恢复，以及哪些边界本轮明确不做。

对应需求：FR-11（资料驱动的覆盖边界）、FR-12（三类内容与授权）、FR-13（终止理由）、
FR-36（重试与故障隔离）、FR-38（预算与在途请求）、NFR-15（外部材料按不可信输入处理）、
AC-31（禁止外部检索）、AC-32（越界研究只进队列）。

## 1. 边界

- 本层**不联网**。`external_retrieval: false` 时对所有标了 `requires_external` 的查询
  一律不发出任何网络请求，判为 `requires_external` 并把对应单元标成 `gapped`（待核验缺口）。
  输出里固定报告 `network_requests: 0`。
- 本层**不调用任何模型**。扫描是确定性大小写无关子串匹配，输出里固定报告 `model_calls: 0`。
- 本层**不写知识库、不写托管 Vault**。命中只是证据线索；把线索变成知识属于 B08-T03 及之后的编译流程。
- 本层**不扩大授权范围**。计划里出现 `scope_class` 不是 `authorized_target` 或
  `required_prerequisite` 的查询时，`adopt` 直接拒收（exit 2），不创建任何根目录。
- 检索只操作**新的测试根**；不接管生产项目目录。

## 2. 检索计划（作者侧输入）

`plan.json` 的字段：

| 字段 | 说明 |
| --- | --- |
| `policy` | 固定 `retrieval-plan-v1` |
| `schema_version` | 固定 `1` |
| `producer` | 作者包固定 `fixture_author` |
| `case_id` | 标识符（ASCII 字母数字加 `-_.`），必须与语料清单一致 |
| `corpus.root` | 相对目录，指向冻结语料 |
| `corpus.manifest` | 相对路径，指向语料清单 |
| `external_retrieval` | 本轮是否允许外部检索；`false` 即资料驱动模式 |
| `limits` | 见下节 |
| `goal_subjects` | 本轮要覆盖的主题；每个都必须出现在某条已授权查询里 |
| `queries[]` | `query_id`、`subject`、`terms`、`scope_class`、可选 `requires_external`、可选 `conflict_group` |

`terms` 每项 1 到 6 个、不含空白、长度不超过 32；`query_id` 在计划内唯一。

## 3. 预算：预留即计入，用完不回收

预算三维：**查询次数**、**累计字节**、**累计耗时（毫秒）**。每次调度前必须先落账：

1. 读账本算出已用额度（等于历次预留之和）；
2. 按最坏情况估算下一次预留：`1` 次查询 + `max_query_bytes` 字节 + `max_query_millis` 毫秒；
3. 三维都还需有剩余，才接受这次预留；
4. 预留**先写盘再扫描**，中途崩溃只会留下一条 `reserved` 记录，`step`/`run` 下次会先补结算它；
5. 结算时**不改动已用额度**：实际命中多少字节都不退还。

这条口径的直接后果是：接近上限时并发调度不能各取全部剩余额度——先落账的一方拿走预留，
后来者看到的剩余额度已经减少，必然被拒。账本里任何一条预留被改小，都等于偷偷回收额度，
独立审计会判失败。

## 4. 查询结果分类

| `outcome` | 含义 |
| --- | --- |
| `evidence` | 有命中的证据线索，且至少一条此前未出现过 |
| `duplicates_only` | 有命中，但全部是此前已出现过的线索 |
| `no_hits` | 在冻结语料内零命中 |
| `conflict` | 与该 `conflict_group` 内已结算的线索集合不一致 |
| `requires_external` | 该查询需要外部来源，而本轮关闭了外部检索 |

命中的键是 `"{文档}#{词项}@{字节偏移}"`，偏移按 UTF-8 字节累加，因此同一份语料在任何机器上
都产生同样的键。`duplicates_only` 与 `conflict` 都基于这些键判定，不看自然语言措辞。

## 5. 单元状态

| `state` | 含义 |
| --- | --- |
| `pending` | 还没调度 |
| `settled` | 已结算，结果落在对应尝试里 |
| `gapped` | 本轮无法补齐，保留为待核验缺口 |

`gapped` 不是失败，也不是完成；它必须原样保留到报告里，不得被"补齐"成证据。

## 6. 七类终止理由与它们的证据判据

终止理由只写一次，写完再写即拒。每类都必须能从账本里找到证据：

| 理由 | 判据 |
| --- | --- |
| `goal_met`（目标满足） | 所有 `goal_subjects` 都被「已结算且结果为 `evidence`」的尝试覆盖，且没有待执行查询 |
| `no_reliable_new_evidence`（无可靠新证据） | 至少有一次 `no_hits` |
| `marginal_gain_insufficient`（边际收益不足） | 至少有一次 `duplicates_only` |
| `budget_exhausted`（预算不足） | 仍有待执行查询，且剩余额度不足以再完成一次预留 |
| `user_paused`（用户暂停） | 账本里存在生效中的暂停控制 |
| `network_restricted`（网络受限） | 本轮关闭外部检索，且账本里已有一次结算结果为 `requires_external` 的尝试（光在计划里标记 `requires_external` 不够：必须真的调度过、并确认它缺外部来源） |
| `manual_judgment_required`（需要人工判断） | 至少有一次 `conflict` |

判据不成立时返回 `RETRIEVAL_REASON_UNSUPPORTED`（exit 3），并把缺什么写进消息里。
**预算不足是本轮停止，不是研究完整**：待执行查询与未完成目标必须保留，`status` 里照实写出来。

## 7. 磁盘布局与 SQLite 身份

```text
<检索根>/
  control/
    owner.json        所有权记录（含根、控制目录、账本、锁六处身份）
    retrieval.sqlite  账本
    run.lock          互斥锁（必须为空文件）
```

SQLite：`application_id = 0x4E525452`（ASCII `NRTR`），`user_version = 1`，
`journal_mode = delete`，`synchronous = full`。五张表：`metadata`、`units`、`attempts`、
`controls`、`termination`，定义逐字冻结：任何列、约束或表名改动都判为损坏。

根目录下只允许 `control/`；`control/` 下只允许上面三个文件。多一个文件即拒收（exit 3），
且不改动现场。

## 8. 所有权与身份校验

`owner.json` 记录根、父目录、`control/`、`owner.json`、`retrieval.sqlite`、`run.lock`
六处文件身份（Unix 用 device+inode，Windows 用 volume+index），并在每次打开时复核。
另外记录计划清单与语料清单的 SHA256，以及 `created_at`；这些字段必须与账本 `metadata` 一致，
不一致判为损坏。

`owner.json` 与 `metadata_json` 都必须是**规范 JSON**：重新序列化后必须与原文字节完全一致，
尾随空白、字段乱序、未知字段、重复键都判为损坏。

## 9. CLI 动词与错误码

```text
noetome retrieve adopt <计划文件> <尚不存在的检索根>
noetome retrieve step <检索根>
noetome retrieve run <检索根>
noetome retrieve pause <检索根>
noetome retrieve resume <检索根>
noetome retrieve finish <检索根> <终止理由文件>
noetome retrieve list <检索根>
noetome retrieve status <检索根>
```

| 错误码 | 退出码 | 场景 |
| --- | --- | --- |
| `RETRIEVAL_ARGUMENT` | 2 | 参数、计划或输入文件不合法；没有待执行查询；重复记录终止理由 |
| `RETRIEVAL_INTEGRITY` | 3 | 账本或所有权记录被篡改、非规范编码或字段不符 |
| `RETRIEVAL_UNKNOWN_ROOT` | 3 | 给定路径不是本产品建立的检索根 |
| `RETRIEVAL_BUSY` | 3 | 另一个进程持有锁 |
| `RETRIEVAL_PAUSED` | 3 | 已暂停，不再调度新查询 |
| `RETRIEVAL_REASON_UNSUPPORTED` | 3 | 终止理由缺少账本证据 |
| `RETRIEVAL_CORPUS_MISMATCH` | 3 | 冻结语料的字节或长度与清单不符 |

## 10. 报告与四维状态

- `run` 输出 JSON：`steps`、`stopped`、`records`（每条查询的 `query_id`/`outcome`/`hits`/`new_hits`）、
  `notes`（人类可读说明）、`budget`、`network_requests: 0`、`model_calls: 0`。
- `list` 输出完整账本视图；`status` 输出多行中文摘要。
- 研究检索在整体报告里单列，**不计入主任务的完成或失败**；未批准候选不改变本轮必做集合。
- 未完成的查询与未补齐的缺口照实计数，不因"模型调用结束"或"本轮停止"而算作完成。

## 11. 本轮明确不做

- 不做真实外部检索、不做网页抓取、不接任何检索服务。
- 不调用模型做摘录、总结或语义匹配；扫描只是确定性字符串匹配。
- 不把命中写进知识库、不生成笔记、不触碰托管 Vault。
- 不做并发多进程调度（只做单进程互斥锁），不做分布式预算。
- 不实现"自动提升越界候选为授权目标"的路径：那必须重新 `adopt` 新作者包。
