# Noetome 最小数据契约

本文件只定义首个 fixture 闭环所需的逻辑字段、引用和不变量。它不是完整数据库 DDL，也不冻结未来所有学科对象。产品语义见 [PRD](../requirements/PRD.md)，处理顺序见 [工程架构](ARCHITECTURE.md)。

状态：工程基线，待实现验证。后续真实解析、模型与研究能力必须通过带版本的扩展进入，不能把缺失值编造成已有结果。

## 1 公共约定

- JSON 使用 UTF-8。JSON schema 及 Rust 类型定义由实现提交维护，并用同一组契约样例核对。
- `schema_version` 从整数 `1` 起；不支持的版本拒绝写入。兼容策略改变需要版本说明。
- `id` 是不透明字符串，不能由名称或输出路径推导。fixture 使用固定 ID，生产实现使用持久化生成的 ID。
- `revision` 是同一对象内递增的正整数；实体身份与内容修订分开。引用需要固定内容时同时保存 ID 和 revision。
- 指纹统一采用 SHA-256 小写十六进制字符串，并明确指纹作用于原始字节、规范文本或输出字节。
- 时间为 RFC 3339 UTC 字符串，仅用于审计；不以修改时间代替内容指纹。
- 缺失、未知、失败与空集合不同。必填字段缺失必须失败，不用空字符串或零代替未知信息。
- 外部契约 v1 默认拒绝未知字段和未知枚举。扩展使用显式版本或已声明扩展字段，避免拼写错误被忽略。
- 列表有序性按字段声明；步骤、章节和表格行顺序有意义，引用集合不能依赖数据库返回顺序。

## 2 输入包

### B01项目初始化契约

在完整fixture闭环之前，B01-T02只实现Project最小状态：`project_id`、`schema_version`、`created_at`和`state`。`state`初态为`initialized`，不表示知识已生成。配置位置固定为项目根下`project.json`，状态库为`project.sqlite`，二者引用同一项目ID与Schema版本。

配置中数据库位置使用项目相对路径，不写死创建时的绝对目录。inspect不需要生成Run、Release或知识对象，也不得为只读查看执行迁移和修复。初始化完成标志和失败恢复策略在T02实现时用测试确定；已有内容不得被静默覆盖。

### B02 fixture输入包

`case.json` 声明 `schema_version`、`case_id`、`sources`、`ir_file`、`note_plan_file` 和 `expected_file`。所有文件路径相对于 fixture 根目录，必须通过目录边界检查。

来源项至少声明固定的 `source_id`、`version_id`、`relative_path`、`media_type` 和预期 `content_sha256`。期望文件由人工审阅，包含必要 Span、原子、排除项及预期输出；不能由待测程序运行后自动覆盖。

IR 包和计划包分别带 `schema_version`、`producer`、`revision`。IR 包用 `source_version_ids` 和 `source_hashes` 固定输入；计划包用 `knowledge_revision` 和 `knowledge_sha256` 固定完整 IR。首闭环 `producer` 必须为 `fixture`；模型适配器引入后才允许另一个明确枚举值。

## 3 来源与定位

### SourceVersion

| 字段 | 类型与含义 |
| --- | --- |
| source_id | 来源身份，重命名文件不改变该身份 |
| version_id | 此份不可变来源内容的版本身份 |
| media_type | v1 为 `text/markdown` 或 `text/plain` |
| display_name | 用于报告的名称，不参与身份判断 |
| content_sha256 | 原始完整字节的指纹 |
| byte_length | 原始文件字节数 |
| normalized_sha256 | 规范文本 UTF-8 字节的指纹 |
| normalization_version | 规范化规则版本 |
| ingest_status | `ready`、`failed` |

以上规范化字段在`ready`时必填。已完整读取原始字节但无法解码时，可保留`failed`版本和已知的原始指纹、长度；`normalized_sha256`及规范文本快照明确为null或不存在，不伪造指纹和空文本。失败必须记录稳定错误码和原因。若原文件根本无法完整读取，只记录失败导入尝试及其路径描述，不创建声称具有完整内容指纹的SourceVersion。

首闭环接受有效 UTF-8 文本，可移除开头 BOM，将 CRLF/CR 统一为 LF；其他字符不自动改写。原始字节指纹在规范化前计算。无法解码时记录失败，不能使用替换字符继续宣称完整。

运行数据库中的来源缓存位置不属于可移植契约；导出不能泄露无关绝对路径。内容改变建立新的 `version_id`，旧证据仍指向原版本。

### SourceSpan

| 字段 | 类型与含义 |
| --- | --- |
| span_id | 此来源版本中的稳定单元 ID |
| source_version_id | 固定来源版本引用 |
| kind | v1 为 `heading`、`text`、`code`、`formatting` |
| start_byte end_byte | 规范文本 UTF-8 的半开区间 `[start,end)` |
| start_line end_line | 规范文本中一基、包含两端的行号 |
| text_sha256 | 区间原文 UTF-8 字节指纹 |
| parser_version | 此次分段器规则版本 |
| parse_status | `parsed`、`failed`、`needs_review` |

不得将这些定位字段伪装成 PDF 页码。字节边界必须落在 UTF-8 字符边界上，且提取出的区间与 `text_sha256` 一致。行号用于人阅读，字节区间用于精确核对。

分段规则 v1 应简单、确定且有测试：识别标题、围栏代码和普通文本块；暂不解释复杂 Markdown 语义。全部规范文本区间需被有序、无重叠地清点，空白也可作为 `formatting` 单元合法排除。分段失败仍必须保留对应区间。

## 4 处理账本

**B01-T03的边界**：只保存来源版本、SourceSpan和最小导入登记记录（import_id、来源引用或失败对象、状态、span_count、待抽取数量、错误）。知识处理状态为pending；不创建IR、NotePlan、Release或完整Run。此阶段的“来源清单”表示字节和位置已清点，不表示必要知识覆盖完成。

**以下完整CoverageRecord从B02开始**：fixture编译创建Run后，再将导入得到的Span映射为该Run的处理账本。这样T03不必为了run_id提前实现整个编译流水线。

每个 Span 在本轮有且仅有一个 CoverageRecord，分段清单与账本必须能够双向对账。

| 字段 | 类型与含义 |
| --- | --- |
| run_id span_id | 联合限定当前处理记录 |
| disposition | `pending`、`included`、`excluded`、`duplicate`、`failed`、`needs_review` |
| required_atom_ids | 本单元已确认必须表达的原子引用，可多项 |
| duplicate_of_span_id | 仅重复归并时填写，不得自引用或形成环 |
| exclusion_reason | 排除时必填，如空白、纯格式或已审阅的非知识内容 |
| decision_source | 首闭环为 `fixture_review` 或确定性格式规则 |
| policy_version | 本轮筛选规则版本 |
| stage | `registered`、`mapped`、`planned`、`rendered`、`validated` |

`included` 要求非空知识映射。`excluded` 不能用于代替解析失败或未知；`duplicate` 必须保留来源出现位置，并能解析到有效知识映射。后续阶段只能在前置条件满足后推进。

记录处置结论不代表已编译。来源覆盖通过要求没有 pending、failed、needs_review，合法排除已审阅，所有必要映射的当前阶段已满足本轮要求。不能仅统计 `disposition != pending`。

## 5 最小 Knowledge IR

### Concept

最小字段为 `concept_id`、`revision`、`canonical_name`、`aliases`。名称和别名不具有全局唯一性。首闭环的身份关系由 fixture 声明，不实现自动消歧，也不把同名字符串自动合并。

### Evidence

最小字段为 `evidence_id`、`source_span_id`、`excerpt_start_byte`、`excerpt_end_byte`、`excerpt_sha256`。片段偏移相对于该 Span 的 UTF-8 字节，范围必须在 Span 内；其原文必须可重取。

首闭环只验证来源与片段匹配，不能据此授予“事实正确”状态。后续支持图像区域和网页定位时应增加显式 locator 类型。

### KnowledgeAtom

| 字段 | 类型与含义 |
| --- | --- |
| atom_id revision | 稳定知识身份与修订 |
| kind | v1 为 `definition`、`statement`、`comparison_value` |
| concept_ids | 所涉及概念引用 |
| content | 结构化载荷或原子正文，不接受未经校验的任意 HTML |
| qualifiers | 条件、适用范围和单位等显式字符串列表 |
| evidence_links | evidence_id 与 `supports`、`refutes`、`qualifies` 关系 |
| required | 是否属于本轮必要原子，由审阅过的 fixture 固定 |
| verification | 首闭环为 `fixture_reviewed` 或 `unverified` |

必要原子必须有非空证据关系；仅反证不足以把正文发布成已成立结论。首闭环必要原子仍为 `unverified` 时阻断发布。fixture 的审阅标记仅表示测试数据已审阅，不是通用科学金标准或模型核验结果。

v1 不单独冻结完整 Claim Schema。当前一个 statement 原子承载一条有条件陈述；引入真实多源融合时再将 Claim 与 Atom 的组织职责拆开，迁移必须保留原子及证据身份。

## 6 NotePlan 与表达

| 实体 | 最小字段 |
| --- | --- |
| NotePlan | note_id、revision、title、output_path、sections |
| SectionPlan | section_id、heading、ordered_nodes |
| Placement | concept_id、note_id、section_id、canonical |
| SemanticLink | target_concept_id、display_text |

`output_path` 是 Vault 相对路径，使用 `/`，v1 后缀必须为 `.md`。同一计划不得产生路径碰撞；Windows 大小写差异、尾随点或空格不能用于区分两个目标。

每个需要完整解释的概念在当前计划内恰有一个 `canonical = true` 的位置。v1 规范位置为笔记标题小节，不提前实现自动章节提升和块级迁移。Section 身份不会因展示标题变化而改变。

`ordered_nodes` v1 支持以下判别联合：

- `paragraph`：明确的原子引用与语义引用序列；渲染时保留条件，不由渲染器改写事实。
- `ordered_list`：有序项，每项声明 atom_ids 与需要显示的内容。
- `table`：列定义、行定义和单元格内容；每个实质单元格声明 atom_ids，标签列可明确标为纯标签。

每个节点有当前计划内唯一且稳定的 `node_id`，保存 `representation_origin = fixture` 和覆盖的 `atom_ids`。表格不是从文本自动推断出来，而是由计划选定；后续主动表达规划器使用相同入口提交候选节点。

空单元格不得暗示数值为零；来源未给出时使用明确的缺失状态，不补造值。表格转义、链接目标和正文映射都由确定性编译器处理。

## 7 渲染与验证产物

RenderedFile 最小字段为 `relative_path`、`content_sha256`、`byte_length`、`note_id`、`plan_revision` 和 `content_mappings`。每条映射记录 section_id、node_id、atom_ids 及输出 UTF-8 字节区间。

映射用于验证文件中确有相应内容，而不是只验证计划里出现过 atom_id。所有必要原子都应映射到最终文件，未展示的条件不得通过一个 ID 引用蒙混过关。

ValidationIssue 包含 `issue_id`、`rule_id`、`severity`、`object_refs`、`message`、`status`。severity 为 `warning` 或 `blocking`，status 为 `open` 或 `resolved`；解决必须记录依据，不可通过删除问题让发布通过。

ValidationResult 包含 `run_id`、`candidate_id`、`validator_version`、`checks`、`issues` 和 `passed`。checks 应列明 `passed`、`failed`、`not_run`、`not_applicable`，未运行不能默认为通过。

## 8 Run 与 Release

Run 保存 `run_id`、输入及配置指纹、producer、开始和结束时间、阶段、执行状态、候选 ID 和报告路径。首闭环执行状态为 `running`、`succeeded`、`blocked`、`failed`、`cancelled`；状态迁移由 core 决定。

Release 保存 `release_id`、`run_id`、IR 与计划版本、渲染器版本、验证结果引用、文件清单和 `status`。status 为 `staged`、`verified`、`published`、`recovery_required`，仅重读校验后的文件集合可记为 published。

报告使用四个独立字段：`source_coverage`、`knowledge_compilation`、`quality_validation`、`optional_research`。值为 `complete`、`incomplete`、`not_applicable`；另有理由和计数。首闭环研究字段为 not_applicable，不显示“研究完成100%”。

来源失败、必要原子遗漏、断链、路径风险、外部内容碰撞或未运行必需校验均禁止成功发布。Run 执行结束不等于 Release 已发布。

## 9 扩展顺序

| 后续能力 | 到该阶段再引入的契约 |
| --- | --- |
| PDF 与扫描 | 页或区域 locator、图片资产、解析置信度及 sidecar 能力声明 |
| 真实 API 抽取 | provider 调用、模型版本、输入指纹、用量和结构化失败 |
| 多源融合 | Claim、EvidenceRelation、条件结构、身份合并与撤销历史 |
| 自动规划 | Topic、多重归属、计划理由、锁定约束和候选方案 |
| 图示及公式 | RepresentationPlan、图元素证据、公式源码与语义核对 |
| 增量发布 | ChangeSet、基准与当前指纹、审批绑定、依赖与发布日志 |
| 研究扩展 | ResearchItem、批准范围、预算与停止原因 |

扩展提交必须包含版本处理、旧 fixture 影响、有效及无效样例，并通过 [测试标准](../standards/TESTING.md)。不为尚未实现的所有未来类型预先建立空表或含义不明的万能 JSON 字段。
