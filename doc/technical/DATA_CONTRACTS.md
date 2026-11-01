# Noetome 最小数据契约

本文件定义空项目、桌面窗口偏好、单份文本来源登记及后续 fixture 闭环所需的逻辑字段、引用和不变量。它不是完整数据库 DDL，也不冻结未来所有学科对象。产品语义见 [PRD](../requirements/PRD.md)，处理顺序见 [工程架构](ARCHITECTURE.md)。

实现状态以[任务账本](../../dev-log/tasks.json)和对应验证记录为准；本文件的字段清单本身不是能力验收记录。后续真实解析、模型与研究能力必须通过带版本的扩展进入，不能把缺失值编造成已有结果。

## 1 公共约定

- JSON 使用 UTF-8。JSON schema 及 Rust 类型定义由实现提交维护，并用同一组契约样例核对。
- 各文件、报告及数据库的 `schema_version` 独立演进，从整数 `1` 起；不支持的版本拒绝读取和写入。项目存储当前支持 Schema 1、2，导入报告与缓存所有权文件仍使用版本 1。
- `id` 是不透明字符串，不能由名称或输出路径推导。fixture 使用固定 ID，生产实现使用持久化生成的 ID。
- B02-T01 的项目、来源、来源版本、Span 与导入 ID 必须为标准小写 UUID v4；显式 `source_id` 只允许引用当前项目中的既有来源。后续 fixture 身份由输入包声明，不能冒充生产导入生成的身份。
- `revision` 是同一对象内递增的正整数；实体身份与内容修订分开。引用需要固定内容时同时保存 ID 和 revision。
- 指纹统一采用 SHA-256 小写十六进制字符串，并明确指纹作用于原始字节、规范文本或输出字节。
- 时间为 RFC 3339 UTC 字符串，仅用于审计；不以修改时间代替内容指纹。
- 缺失、未知、失败与空集合不同。必填字段缺失必须失败，不用空字符串或零代替未知信息。
- 外部契约 v1 默认拒绝未知字段和未知枚举。扩展使用显式版本或已声明扩展字段，避免拼写错误被忽略。
- 列表有序性按字段声明；步骤、章节和表格行顺序有意义，引用集合不能依赖数据库返回顺序。

## 2 输入包

### 项目初始化与存储版本

Project 配置位置固定为项目根下 `project.json`，状态库为 `project.sqlite`。B01-T02 的 Schema 1 是空项目基线，`project init` 继续生成该版本；B02-T01 由来源导入触发受保护的 Schema 2 升级。配置只接受以下字段，缺失、未知字段或不支持的值必须拒绝。

| 字段 | 当前约束 |
| --- | --- |
| project_id | 初始化时生成并持久化的标准小写UUID v4，不从名称或目录推导 |
| schema_version | init 为整数 `1`；inspect 支持 `1`、`2`，其他版本明确拒绝 |
| created_at | RFC 3339 UTC字符串，配置与数据库使用同一值 |
| state | `initialized`，只表示项目建立完成；Schema 2 可已有来源记录，此值不表示资料或知识处理成功 |
| database | 固定项目相对路径`project.sqlite`，不接受绝对路径、URI或路径穿越 |

配置上限为64 KiB，用于这五个元数据字段，不承载原始资料或知识内容。Schema 1 只建立 `project_metadata` 基础表，包含 `singleton`、`project_id`、`schema_version`、`created_at` 和 `state`；`singleton=1` 限定单条项目记录，项目 ID 唯一，版本和状态受约束。`PRAGMA user_version=1`，`PRAGMA application_id=0x4e4f4554` 标识 Noetome 项目库。inspect 核对真实表及列结构、配置、数据库记录与这两个标记；Schema 1 只允许该表和 SQLite 自身维护的对象，额外表、索引、视图或触发器必须拒绝。

init要求用户选定的目标目录尚不存在，父目录已存在；已有空目录、项目目录或未知内容均拒绝接管。初始化先独占创建目录和`.noetome-initializing`标记，在同目录的`.project.sqlite.initializing`、`.project.json.initializing`准备数据库与配置，通过不覆盖已有目标的文件硬链接发布两份正式文件，二者完成后才去除中断标记。完成项目只包含正式配置与数据库；链接发布发生在同一项目目录内，不把这种文件发布方式宣称为生产Vault多文件原子更新。

普通失败只清理本次独占创建、仍与创建记录相符的内容；出现未知文件、文件变更或目录替换时保留目录。崩溃留下标记、暂存文件或缺失配对文件时，init和inspect报告`PROJECT_INCOMPLETE`，不补造成功状态、不覆盖残留。当前恢复方式是保留残留供检查，使用新的目标目录重新初始化空项目；未实现原目录自动续写或生产任务恢复。普通失败已经安全回滚时可在原路径重试。

inspect 以只读、immutable 的 SQLite URI 打开数据库，文件路径先编码，拒绝 `-journal`、`-wal`、`-shm` 以及 super-journal 这类待恢复的侧文件；项目库使用 DELETE 日志模式，检查时临时存储限定为内存。检查不得执行迁移、修复或创建 SQLite 侧文件。immutable 会跳过 SQLite 锁和变更检测，见 [SQLite URI规则](https://sqlite.org/uri.html)，因此只用于当前没有写任务的 Schema 1 或 2 项目；Windows 检查期间保持拒绝写入和删除的只读文件句柄，不承诺任意平台的并发外部写入一致性。项目路径及配置、数据库、来源缓存中的符号链接或 Windows reparse/junction 路径必须拒绝，不能借配置或快照定位打开项目外文件。

### B02-T01 Schema 2 与写入保护

Schema 2 保持 `project_metadata` 的五列，元数据及 `PRAGMA user_version` 均为 `2`，应用标识不变；配置、数据库标记和元数据版本必须一致。配置与数据库失配返回 `PROJECT_SCHEMA_MISMATCH`，未来版本返回 `PROJECT_UNSUPPORTED_SCHEMA`，不自动升级或降级。

在基础表之外，Schema 2 只增加以下四张来源表。实际表、列、约束及显式 SQLite 对象必须匹配当前 Schema 定义；未知表、索引、视图或触发器拒绝，外键必须有效。

| 表与实体 | 引用及关键不变量 |
| --- | --- |
| `sources` / Source | `source_id`、`created_at`；每个来源至少有一个真实版本 |
| `source_versions` / SourceVersion | `source_id` 引用来源；同来源的原始 SHA 及 revision 各自唯一，revision 从 1 连续递增 |
| `source_spans` / SourceSpan | `source_version_id` 引用版本；Span ID 全局唯一，同版本 ordinal 从 0 连续排列 |
| `source_imports` / 最小导入记录 | 可空的来源及版本引用；版本与来源必须配对，每个版本有首次导入，后续复用才允许 `reused=true` |

来源导入先只读校验原配置、数据库、全部来源记录和已有缓存，再独占创建项目内普通目录 `.noetome-writing`。该目录保存 `operation.json`、`old-project.json`、`old-project.sqlite` 及候选配置、数据库；新版本快照只在其 `version` 子目录准备。Schema 1 的升级在旧数据库副本中事务重建基础表和建立来源表，保持 `project_id`、`created_at`、`state` 与固定数据库路径；来源记录插入开启外键，不直接修改正式旧库。

发布前重读候选库，核对 Schema、引用、记录、原始与规范快照及完整 Span。替换正式数据库和配置前复核原字节与文件身份；Windows 准备阶段的读守卫在替换前释放，仍保留允许替换的身份句柄作复核。新版本目录及快照文件独占发布，不覆盖已有目标。两份正式文件、来源记录和快照全部重读通过后才清除备份和写目录。

普通写入失败只恢复可确认的原配置、数据库字节，并清理仍属本次操作的内容。发现未知内容、身份或字节变化、进程中断以及无法完整清理时，保留标记、候选或备份，报告 `SOURCE_WRITE_INCOMPLETE`；不伪称成功，也不实现原目录自动续写。这个协议约束合作 CLI 和可观察的文件变化，尚未证明任意外部同时编辑或任意断电时的多文件原子性。

公开 `project inspect` 遇 `.noetome-writing` 的任何残留、旧初始化标记或缺失配对文件均返回 `PROJECT_INCOMPLETE` 并保留现场；它只检查项目元数据与库结构。writer 内部验收仅可忽略自己持有同一身份句柄的普通写目录，旧初始化残留仍拒绝。`source inspect` 在此基础上只读重载全部来源记录和缓存，再返回指定导入报告；它不读取原选取文件、不检测原来源当前可用性、不迁移或修复快照。

### 桌面窗口偏好契约

主窗口×处理前同样重读此受限文件；读失败保留此前已生效值，尚无有效值时明确报告并安全回退到托盘。此查询不写文件，不以另一驻留实例的旧缓存作为当前已保存值。

`desktop-preferences.json` 位于应用本地数据目录，与`project.json`及SQLite项目Schema独立。JSON只接受`schema_version: 1`及`close_behavior: "hide_to_tray" | "exit"`，上限4096字节。缺失文件采用默认`hide_to_tray`且只读查询不创建文件；未知字段、损坏、不支持版本或不安全路径必须报告并保留现场。成功保存后才更新运行时偏好，同目录暂存及替换不得先删除已有设置。

`get_close_behavior`每次实际重读固定设置文件并校验，不只返回启动时的缓存；成功后返回并采用该次读取的值，缺失文件仍采用`hide_to_tray`且不创建文件。设置目录已确定时，即使首次读取失败也保留重试所需路径和错误状态；用户修正文件或访问条件后，“重新读取”必须能恢复，而不是永久重复首次错误。重读失败返回稳定错误，保留文件现场与此前已生效的关闭行为，不把损坏、未知内容或失败读取当作默认设置成功。

`set_close_behavior`只接受上述两个枚举值，成功返回持久保存的值，失败保留原已生效行为。启动时配置不可读取且尚无有效值时，明确报告后关闭安全采用`hide_to_tray`；界面仍须显示读取错误，不能伪称默认值已保存。常规最小化不隐藏任务栏窗口，托盘菜单退出不受关闭偏好影响。

### B02 fixture输入包

本输入包及第 5 至 8 节的 IR、NotePlan、验证、Run、Release 契约供 B02-T02/T03 后续实现；B02-T01 尚未实现这些对象或完整 Vault 编译。

`case.json` 声明 `schema_version`、`case_id`、`sources`、`ir_file`、`note_plan_file` 和 `expected_file`。所有文件路径相对于 fixture 根目录，必须通过目录边界检查。

来源项至少声明固定的 `source_id`、`version_id`、`relative_path`、`media_type` 和预期 `content_sha256`。期望文件由人工审阅，包含必要 Span、原子、排除项及预期输出；不能由待测程序运行后自动覆盖。

IR 包和计划包分别带 `schema_version`、`producer`、`revision`。IR 包用 `source_version_ids` 和 `source_hashes` 固定输入；计划包用 `knowledge_revision` 和 `knowledge_sha256` 固定完整 IR。首闭环 `producer` 必须为 `fixture`；模型适配器引入后才允许另一个明确枚举值。

## 3 来源与定位

### Source 与身份复用

Source 保存 `source_id` 和 UTC `created_at`，身份与路径、显示名称及内容去重分开。未指定 ID 时先保留此前同一明确选取路径的来源关联；没有该关联时按项目内既有原始 SHA 复用来源，否则生成新来源。重命名后同字节可复用原来源和版本，名称不参与身份生成。

指定既有 `source_id` 时先固定该来源，再仅在其版本内按原始 SHA 复用或新增 revision；另一来源具有相同 SHA 也不得导致身份合并。同字节复用不新增版本、Span 或快照，每次导入尝试仍生成新的 `import_id`。内容改变建立新的 `version_id`，旧版本和位置保持不可变；回到曾登记的原始字节复用该旧版本。

### SourceVersion

| 字段 | 类型与含义 |
| --- | --- |
| source_id | 来源身份，重命名文件不改变该身份 |
| version_id | 此份不可变来源内容的版本身份 |
| revision | 同一来源内从 `1` 连续递增的正整数 |
| created_at | 此版本登记的 RFC 3339 UTC 时间 |
| media_type | v1 为 `text/markdown` 或 `text/plain` |
| display_name | 非空报告名称，不参与身份判断；复用版本保留其原登记值 |
| content_sha256 | 原始完整字节的指纹 |
| byte_length | 原始文件字节数 |
| normalized_sha256 | 规范文本 UTF-8 字节的指纹 |
| normalization_version | 规范化规则版本 |
| ingest_status | `ready`、`failed` |
| raw_snapshot | 固定项目相对定位 `.noetome-sources/{version_id}/raw.bin` |
| normalized_snapshot | ready 时为 `.noetome-sources/{version_id}/normalized.txt`，failed 时为 null |
| error | ready 时为 null；failed 时为非空 `code`、`message` 对象，数据库分别保存错误两列 |

以上规范化字段在 `ready` 时必填，错误为空。完整读取但无法解码时登记 `failed` 版本，保存真实原始指纹、长度和 `raw.bin`；`normalized_sha256`、`normalization_version`、`normalized_snapshot` 均为 null，规范文件和 Span 不存在。错误固定为 `SOURCE_INVALID_UTF8` 及非空原因；缓存重读须确认这些原始字节确实不能按 UTF-8 解码，不伪造指纹、替换字符或空文本。

只读取用户明确选取的普通 `.md`、`.txt` 单文件，当前原始字节上限为 64 MiB，读取前后核对文件身份、长度和修改时间。读取失败、超限或读取期间变化分别形成 `SOURCE_READ_ERROR`、`SOURCE_TOO_LARGE`、`SOURCE_CHANGED` 失败尝试；未确认完整字节时不建立 SourceVersion、快照或内容指纹。失败尝试可引用已知来源，尚无来源时不造新来源。

有效 UTF-8 文本仅移除首个 BOM，将 CRLF/CR 统一为 LF；其他字符不自动改写。原始字节 SHA 在规范化前计算，规范 SHA 针对得到的 UTF-8 字节，两者独立保存；不同原始换行即使规范结果相同，也不按规范 SHA 合并版本。当前 `normalization_version` 固定为 `utf8-newlines-v1`。

### 快照目录与重读

快照根固定为项目内 `.noetome-sources`，只允许 `.owner.json` 和数据库中全部版本的 UUID 子目录。所有权文件上限 4096 字节，只接受 `schema_version: 1`、当前 `project_id` 两字段。未知所有权或缓存内容必须拒绝并保留，不接管人工目录；缺失缓存只在没有持久版本时合法。writer 候选验收可从自己固定的 `.noetome-writing/version` 读取唯一新版本，其余版本仍从正式缓存读取，不接受外部暂存路径。

每个版本目录仅有 `raw.bin`，ready 版本另有 `normalized.txt`；目录与数据库版本完全对账。定位字符串必须逐字符合上述固定相对形式，不以数据库任意路径打开文件。文件大小和声明长度在读取前检查，读守卫保护读取，读取前后核对身份、长度和修改时间；原始长度与 SHA 必须真实一致。ready 版本重新执行规范化，规范文件字节及 SHA 必须一致；随后重新清点所有 Span，不用数据库自身的计数代替原文核对。

无效记录报告 `SOURCE_INVALID_RECORD`，已知快照内容损坏报告 `SOURCE_INVALID_SNAPSHOT`，缓存所有权或未知内容冲突报告 `SOURCE_SNAPSHOT_CONFLICT`；链接及 reparse/junction 路径沿用 `PROJECT_UNSAFE_PATH`。检查只涉及明确缓存范围，不扫描其他资料。原选取路径仅供本地审计，原文件删除或搬移不阻断有效快照检查；来源可用性检测与导出包留待后续，导出仍不得泄露无关绝对路径。

### SourceSpan

| 字段 | 类型与含义 |
| --- | --- |
| span_id | 此来源版本中的稳定单元 ID |
| source_version_id | 固定来源版本引用 |
| ordinal | 同版本内从 `0` 连续递增的清点顺序 |
| kind | v1 为 `heading`、`text`、`code`、`formatting` |
| start_byte end_byte | 规范文本 UTF-8 的半开区间 `[start,end)` |
| start_line end_line | 规范文本中一基、包含两端的行号 |
| text_sha256 | 区间原文 UTF-8 字节指纹 |
| parser_version | 此次分段器规则版本 |
| parse_status | `parsed`、`failed`、`needs_review` |

不得将这些定位字段伪装成 PDF 页码。字节边界必须落在 UTF-8 字符边界上，且提取出的区间与 `text_sha256` 一致。行号用于人阅读，字节区间用于精确核对。

当前 `parser_version` 固定为 `blocks-v1`：识别零至三个 ASCII 空格缩进下的标题、反引号或波浪号围栏代码，以及普通文本、空白块；暂不解释复杂 Markdown 语义。全部规范文本字节按顺序无缝、无重叠覆盖，空规范文本为零段。未闭合围栏保留完整 `code` 区间并标为 `needs_review`；其他当前识别单元为 `parsed`，`failed` 枚举为显式契约值，不借失败丢掉区间。重读同时核对 ordinal、类型、字符边界、行号、原文 SHA、parser_version 和 parse_status。

`formatting` 也属于来源完整清点，在 B02-T01 保持待抽取；是否合法排除由后续 CoverageRecord 决定。存在 `needs_review` 的版本仍可表示字节已读取的 ready 来源，不能据此声称知识处理完成。

## 4 处理账本

**B02-T01 最小导入记录与报告**：来源模块已实现 `source import <项目> <文件> [--source-id <既有ID>]` 及只读 `source inspect <项目> <import-id>`；整体验收以任务账本和对应验证记录为准。记录保存于 Schema 2 的 `source_imports`，JSON 报告使用独立的 `schema_version: 1`，不能将报告版本当作数据库版本。

| 报告字段 | 当前约束 |
| --- | --- |
| schema_version、project_id | 报告版本固定 `1`，项目身份与配置及数据库一致 |
| import_id、imported_at | 每次尝试的新 UUID v4 及 RFC 3339 UTC 时间 |
| selected_path | 仅保存此次明确选取的非空绝对路径；拒绝路径穿越和不安全链接，不读取无关目录 |
| source_id、version_id | 可空引用；有版本时必须对应真实来源，无完整读取时 version_id 为 null |
| status | `ready` 或 `failed`；有版本时与版本 ingest_status 相同 |
| reused | 只有该版本已有先前导入时为 true，无版本的失败尝试必须为 false |
| span_count | 等于该版本真实持久化的 Span 数量，不由报告清单的自报数量证明完整性 |
| pending_extraction_count | 本阶段等于同一真实 Span 数量，包括 formatting 和 needs_review；未读取完整、失败 UTF-8 或空文本均为 `0` |
| knowledge_status | 固定 `pending`，不表示知识覆盖或笔记编译成功 |
| version、spans | 重载的 SourceVersion 或 null，以及按 ordinal 排列的完整 Span 清单 |
| error | ready 为 null，failed 为稳定错误码及非空原因；须与失败版本或未完整读取失败尝试一致 |

数据库两项计数受约束保持相等，重载时还必须与真实 Span 行数一致；独立快照重读再核对原文分段结果。每个来源有版本，每个版本有首次导入，报告顺序按持久导入顺序重载。成功保存失败尝试只表示失败事实已经登记：导入返回非零并输出真实失败报告，之后只读查看该有效失败报告可以成功，不把 failed 改成 ready。

此阶段的“来源清单”只表示字节和位置已清点，知识状态仍为 pending；不创建 IR、NotePlan、Release、完整 Run 或 Vault 输出，也不提前建立它们的空表。

**以下完整CoverageRecord用于后续fixture编译阶段**：该阶段创建Run后，再将导入得到的Span映射为该Run的处理账本。单份文本导入不为了run_id提前实现整个编译流水线；B01-T03已按用户要求调整为桌面交付，与来源登记无关。

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
