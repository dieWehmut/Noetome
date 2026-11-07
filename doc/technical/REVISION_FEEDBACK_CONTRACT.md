# 写作反馈修订契约（B08-T03）

本契约定义 Noetome 第一版「写作反馈发现知识缺口 → 修订 → 依赖重编译」：缺口长什么样、
修订怎么核验、新 IR 版本怎么生成、哪些表示必须重编译、反复修订怎么收敛，以及哪些边界本轮明确不做。

对应需求：FR-14（写作反馈与知识修订）、FR-28（结题与阶段报告）、FR-35（持久化任务与断点恢复）、
AC-34（写作反馈触发修订并重编译所有依赖表示）。

## 1. 边界

- 本层**不联网**。输出里固定报告 `network_calls: 0`。
- 本层**不调用任何模型**。渲染是确定性字符串拼接，输出里固定报告 `model_calls: 0`。
- 本层**不写知识库、不写托管 Vault**。渲染结果只落在修订根自己的 `render/` 目录里。
- 本层**不把缺口变成研究候选**。缺口是「已知要写的内容缺参数」，必须回到修订流程；
  研究候选是「越出批准范围的新主题」，只在研究队列里等待授权。两者是不同实体，不能互相顶替。
- 本层**不扩大授权范围**。修订只能引用冻结语料清单里的来源；计划外来源直接拒收。
- 修订只操作**新的测试根**；不接管生产项目目录。

## 2. 作者清单（作者侧输入）

`author.json` 的字段：

| 字段 | 说明 |
| --- | --- |
| `policy` | 固定 `revision-feedback-v1` |
| `schema_version` | 固定 `1` |
| `producer` | 作者包固定 `fixture_author` |
| `case_id` | 标识符（ASCII 字母数字加 `-_.`），必须与语料清单一致 |
| `corpus.root` | 相对目录，指向冻结语料 |
| `corpus.manifest` | 相对路径，指向语料清单 |
| `limits.max_revisions` | 单个缺口允许的修订次数上限，1 到 8 |
| `atoms[]` | 知识原子，见下节 |
| `representations[]` | 表示（表格 / 正文段落 / 追溯关系），见下节 |

校验：原子编号唯一、同一 `subject` + `property` 不得重复、表示引用的原子必须存在、
**每个原子必须被至少一个表示引用**（否则写作反馈无法验证修订效果）、表示数 1–16、原子数 1–128。

## 3. 知识原子与核验状态

| 字段 | 说明 |
| --- | --- |
| `atom_id` | 原子编号 |
| `subject` | 对象（如「糖酵解」） |
| `property` | 属性（如「净生成ATP」） |
| `value` | 值 |
| `unit` | 单位，允许为空字符串 |
| `evidence[]` | `{source, locator}`，`source` 是冻结语料相对路径，`locator` 必须以 `#` 开头 |

核验状态由证据条数决定，不由修订者声明：

| 状态 | 条件 |
| --- | --- |
| `verified` | `evidence` 非空 |
| `pending_verification` | `evidence` 为空 |

**无来源的补充允许写进 IR，但只能是待核验**：渲染里必须带 `【待核验】` 标记，状态列写
`pending_verification`，不得显示成已核验结论，也不得进入发布为结论的路径。

## 4. 表示与依赖

三种表示类型：`table`（表格）、`paragraph`（正文段落）、`traceability`（追溯关系）。

渲染是确定性的：同一份 IR 在任何机器上都产生逐字节相同的文本。

- `table`：`## 标题` + 表头 `| 对象 | 属性 | 值 | 单位 | 证据 | 状态 |`，单位为空显示 `-`。
- `paragraph`：`# 标题` + 每原子一行 `- {对象} 的 {属性} 是 {值}{单位}（证据 N 条；状态 X）`。
- `traceability`：`# 标题` + 每来源一行 `- {atom_id} ← {source}{locator}`；无来源则 `- {atom_id} ← 无来源，待核验`。

**依赖范围逐原子计算**：某个表示是否要重编译，取决于它的 `atom_ids` 里是否含被修订的原子，
不存在"改了这类内容就全部重排"的路径。

## 5. 缺口（Gap）

写作、编表、绘图时发现必要参数缺失，必须登记缺口，字段：

| 字段 | 说明 |
| --- | --- |
| `gap_id` | 缺口编号，全局唯一 |
| `representation_id` | 在哪个表示里发现的，必须在影响范围里 |
| `atom_id` | 涉及哪个原子 |
| `reason` | 缺口原因 |
| `source[]` | 发现时的来源线索，**必填**（`{source, locator}`） |
| `impact[]` | 影响范围，**必须恰好等于引用该原子的表示集合**（多写少写都拒收） |
| `note` | 补充说明 |

缺口状态机：

| 状态 | 含义 |
| --- | --- |
| `open` | 已登记，尚未修订 |
| `resolved` | 修订后该原子已核验 |
| `pending_verification` | 修订后该原子仍是待核验（无来源补充） |
| `escalated` | 修订次数超出上限，升级为人工判断 |

`escalated` 的缺口还会把 `severity` 置为 `manual_judgment_required`，此后**不再接受任何自动修订**。

登记缺口不改 IR 版本、不改任何渲染字节——它只是把问题记下来。

## 6. 修订与它的四道闸门

`revise` 依次检查，任一条不成立即拒收，且**所有拒收都发生在写盘之前**：

1. **升级锁定**：缺口状态为 `escalated` → `REVISION_LIMIT_ESCALATED`（exit 3）。
2. **原子一致**：修订指向的原子必须与缺口声明的原子相同。
3. **编号唯一**：`revision_id` 不得与已有修订重复。
4. **无新证据不循环推翻**：若这次把值改回该原子历史上出现过的旧值，且证据集合相对该缺口历史证据
   没有任何新来源 → `REVISION_NO_NEW_EVIDENCE`（exit 3）。
5. **来源必须在冻结清单内**：证据 `source` 不在语料清单里即拒收。
6. **空修订**：值、单位、证据与当前 IR 完全相同 → `REVISION_ARGUMENT`（exit 2），不生成新版本
   （不允许用空修订刷版本号）。

通过后：生成新的 `IR v{n+1}`，对**每个**表示重新渲染并与旧字节比较——

- 引用该原子的表示 → 记入 `recompiled`，字节变了就更新 `render/`；
- 不引用该原子的表示 → 记入 `unchanged`，**字节必须完全不变**；若变了即判账本损坏（exit 3）。

## 7. 修订循环上限与升级

每条修订记录 `revision_id`、`gap_id`、`atom_id`、新值、新单位、证据、原因、`ir_version`、
`recompiled`、`unchanged`、时间——即"版本、原因、结果"三样都在账本里。

同一缺口的修订计数达到 `limits.max_revisions` 时仍允许最后一次；
**超过上限的那一次修订仍会生效并生成新版本，但缺口随即升级为 `escalated`**，
后续自动修订一律拒绝。这样"最后一次修订"不会丢失，同时把问题交给人。

## 8. 磁盘布局与 SQLite 身份

```text
<修订根>/
  control/
    owner.json        所有权记录（含根、父目录、control、owner、账本、锁六处身份）
    revision.sqlite   账本
    run.lock          互斥锁（必须为空文件）
  render/
    <representation_id>.md   每个表示的当前渲染字节
```

SQLite：`application_id = 0x4E525652`（ASCII `NRVR`），`user_version = 1`，
`journal_mode = delete`，`synchronous = full`。五张表：`metadata`、`gaps`、`revisions`、
`representations`、`ir_versions`，定义逐字冻结：任何列、约束或表名改动都判为损坏。

根目录下只允许 `control/` 与 `render/`；`control/` 下只允许上面三个文件。多一个文件即拒收（exit 3），
且不改动现场。

## 9. 所有权、身份与错误码

`owner.json` 记录根、父目录、`control/`、`owner.json`、`revision.sqlite`、`run.lock`
六处文件身份（Unix 用 device+inode，Windows 用 volume+index），并在每次打开时复核。
另外记录作者清单与语料清单的 SHA256 与 `created_at`，必须与账本 `metadata` 一致。

`owner.json` 与 `metadata_json` 都必须是**规范 JSON**：重新序列化后必须与原文字节完全一致，
尾随空白、字段乱序、未知字段、重复键都判为损坏。

```text
noetome revision adopt <author.json> <尚不存在的修订根>
noetome revision gap <修订根> <缺口文件>
noetome revision revise <修订根> <修订文件>
noetome revision list <修订根>
noetome revision status <修订根>
```

| 错误码 | 退出码 | 场景 |
| --- | --- | --- |
| `REVISION_ARGUMENT` | 2 | 参数、作者清单或输入文件不合法；缺口/修订编号重复；空修订；影响范围不符 |
| `REVISION_INTEGRITY` | 3 | 账本或所有权记录被篡改、非规范编码或字段不符；未受影响表示字节意外改变 |
| `REVISION_UNKNOWN_ROOT` | 3 | 给定路径不是本产品建立的修订根 |
| `REVISION_BUSY` | 3 | 另一个进程持有锁 |
| `REVISION_LIMIT_ESCALATED` | 3 | 缺口已升级为人工判断，不再接受自动修订 |
| `REVISION_NO_NEW_EVIDENCE` | 3 | 改回旧值却没有带来任何新来源 |
| `REVISION_CORPUS_MISMATCH` | 3 | 冻结语料的字节或长度与清单不符 |
| `PROJECT_INVALID_DATABASE` | 2 | 账本文件已损坏（沿用项目级数据库分类，与其他能力一致） |

## 10. 报告与四维状态

- `list` 输出完整账本视图：原子、表示、缺口、修订、IR 版本号与原子摘要、核验计数、`network_calls: 0`、`model_calls: 0`。
- `status` 输出多行中文摘要：范围、所有权、冻结语料、当前 IR 版本、核验计数、缺口四态计数、
  修订次数与上限、重编译范围、检索与模型调用 0 次。
- 缺口、修订与重编译结果在整体报告里单列，**不计入主任务的完成或失败**；
  **重编译成功不等于知识正确**——它只证明依赖的表示都按新 IR 重算过。
- 待核验原子照实计数，不因"修订流程跑完"而算作已核验。

## 11. 本轮明确不做

- 不做真实写作、不做模型生成、不做语义等价判断；渲染只是确定性模板拼接。
- 不把修订结果写进知识库或托管 Vault，也不触发发布审批。
- 不做跨缺口的级联修订，不做"一次修订解决多个缺口"。
- 不做自动提升人工判断结论的路径：那必须由人给出新证据后重新 `revise`（若缺口已升级则需新建作者包）。
- 不校验磁盘 `render/` 与新 IR 是否仍然一致（渲染目录不是账本真相，改它不会让 `status` 失败）；
  回归由独立审计脚本 `scripts/verify_revision_feedback.py` 按契约重新渲染后逐字节比对。
