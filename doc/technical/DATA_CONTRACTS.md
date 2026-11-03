# Noetome 最小数据契约

本文件定义空项目、桌面窗口偏好、文本与PDF来源登记、独立PDF解析沙盒、离线fake与文本API候选及fixture闭环所需的逻辑字段、引用和不变量。它不是完整数据库 DDL，也不冻结未来所有学科对象。产品语义见 [PRD](../requirements/PRD.md)，处理顺序见 [工程架构](ARCHITECTURE.md)。

实现状态以[任务账本](../../dev-log/tasks.json)和对应验证记录为准；本文件的字段清单本身不是能力验收记录。后续真实解析、模型与研究能力必须通过带版本的扩展进入，不能把缺失值编造成已有结果。

## 1 公共约定

- JSON 使用 UTF-8。JSON schema 及 Rust 类型定义由实现提交维护，并用同一组契约样例核对。
- 各文件、报告及数据库的 `schema_version` 独立演进，从整数 `1` 起；不支持的版本拒绝读取和写入。来源项目存储支持Schema1/2/3，旧文本导入报告与缓存owner仍使用版本1，PDF导入报告使用版本2；独立compile.sqlite保持Schema1。
- `id` 是不透明字符串，不能由名称或输出路径推导。fixture 使用固定 ID，生产实现使用持久化生成的 ID。
- B02-T01 的项目、来源、来源版本、Span 与导入 ID 必须为标准小写 UUID v4；显式 `source_id` 只允许引用当前项目中的既有来源。后续 fixture 身份由输入包声明，不能冒充生产导入生成的身份。
- `revision` 是同一对象内递增的正整数；实体身份与内容修订分开。引用需要固定内容时同时保存 ID 和 revision。
- 指纹统一采用 SHA-256 小写十六进制字符串，并明确指纹作用于原始字节、规范文本或输出字节。
- 时间为 RFC 3339 UTC 字符串，仅用于审计；不以修改时间代替内容指纹。
- 缺失、未知、失败与空集合不同。必填字段缺失必须失败，不用空字符串或零代替未知信息。
- 外部契约 v1 默认拒绝未知字段和未知枚举。扩展使用显式版本或已声明扩展字段，避免拼写错误被忽略。
- 列表有序性按字段声明；步骤、章节和表格行顺序有意义，引用集合不能依赖数据库返回顺序。

## 2 输入包

### B04-T01 离线知识抽取网关

交换消息独立使用`schema_version=1`，完整字段和限额见[有界抽取契约](EXTRACTION_CONTRACT.md)。`producer=fake`只用于显式`extract fake`入口，与parser测试fake及作者fixture的审阅标记分开。来源项目继续支持Schema1/2/3；抽取不迁移、不改SQL或pending、不建立KnowledgeIR/Run/Release。

每次请求绑定实际project/import/source/version及原始SHA。文本单位固定实际SourceSpan及规范全文SHA；PDF固定实际parse_id、manifest SHA、物理页/区域、逐页资源SHA与bbox，规范文本字段保持null。证据使用资源内UTF8非空字节半开区间、真实切片quote及其SHA。来源的`needs_review`保留在请求和报告，不能借严格候选通过升级为知识已核验。

候选复用Concept/KnowledgeAtom的通用ID、修订、明文及唯一引用规则；fake原子固定required=false、verification=unverified且有有效supports。measurement必填可空，unknown数值为null并保留已知单位，条件和单位保留在qualifiers。所有DTO拒绝未知版本/枚举、多余或重复字段、缺少必填nullable项和尾随JSON。

fake按完整UTF8字节计量并在一次回放前预留预算；真实模型tokenizer、模型用量和价格为null，网络及付费调用实测为0。来源和fake输入守卫贯穿验证及保存，缓存硬链接在语义读取前拒绝；最终核对身份、字节、mtime及清单。新输出根仅保存请求、原响应、成功候选和JSON/Markdown验证快照；失败不保存candidates.json，不发布知识。

### B04-T02 单请求文本API

独立契约[EXTRACTION_API_CONTRACT.md](EXTRACTION_API_CONTRACT.md)固定配置与API候选Schema1。API输出只有schema_version、request_id、binding、concepts、evidence、atoms；Core保存的候选包装及报告明确producer=api、transport_mode与provider，不让模型自封来源类型，也不转换成fake DTO。领域候选与fake共用同一纯校验，来源绑定及资源内UTF8证据语义沿用T01，非零Span或PDF区域的anchor保留真实绝对起点。

配置只读且严格，凭据为配置指名的外部环境变量引用。live限定官方HTTPS Responses及用户明确指定的糖猫云deepseek-flash精确端点；local_http_test限定数字环回、明确端口与人工noetome-test-模型/凭据。外部脚本只在内存将CurrentUser DPAPI凭据注入选定子CLI，Core不读取默认认证。请求正文保存实际字节与SHA；input_reserved_tokens为正文UTF8字节数加1024的工程预留，model_tokenizer=null。一次调用前预留输入加输出，返回用量必须为有效非负整数且总量、输入和输出分别满足限额；未知价格为null，金额预算但价格未知时不请求。已授权10M为累计额度，按[保存的模型服务与使用记录](../requirements/MODEL_SERVICE.md)先查剩余再预留，不在每次调用重置。

text_api_extraction报告Schema1区分endpoint、request_strategy、配置模型和返回模型、实际网络尝试、HTTP状态、完整/partial、原响应收据及已验证用量。旧官方/本地Responses维持strict JSON Schema；所选v2输入自足task/完整response_schema/原request，仅显式completed回包可省略两个空错误字段。所选Chat通过system/user messages携带同一内容、thinking disabled及JSON Object；v4只在task中明确atoms每项为{atom,measurement}，不变更Schema或枚举，不修补v3原响应。单choice/finish_reason/普通JSON仍由Core严格核对。provider/path与主机/模型匹配，原策略和历史数据不改。Chat的prompt/completion/total规范为统一input/output/total，usage.raw_model_usage保留原对象；完整200元数据核对独立于候选、refusal与incomplete，无效/缺失用量为null，失败不抹去已核对计量。安全原响应保持原字节；凭据回显含嵌套/转义JSON时只保存收取长度/SHA和省略原因，不持久化泄密原文或候选。JSON与Markdown携带同一完整验证快照，产物指纹来自实际文件。

候选成功仍unverified、knowledge_status=pending、published=false、scientific_quality=not_run；本地假HTTP服务的真实模型/付费调用为0。API接入不改来源Schema/SQL/pending，不登记KnowledgeIR/Run/Release。离线通过与真实服务验收分别记录；用户服务/预算已保存，取得本机凭据并实际完成有界调用后仍按真实任务卡验收，不以配置文件存在替代模型成功。

### B03-T01 parser sidecar 协议

单请求 parser 协议独立采用 `protocol_version = 1`，完整字段、限额和稳定错误见[专门契约](SIDECAR_PROTOCOL.md)。它与来源项目 Schema2、编译数据库 Schema1、fixture JSON Schema1 分开演进，不将 sidecar 响应直接当作数据库记录。

Core 固定 request_id、单份原始输入指纹及新工作区能力；响应中的 producer、解析器/能力版本、有序完整单元、资源和问题全部复核。提取文本范围相对于指定 UTF8 resource，而非原始文件二进制或旧 normalized.txt；资源的相对路径、真实字节、身份和精确清单共同约束读取范围。普通入口只接受 parser producer，fake 许可仅编译进测试。

本项的 fake 单元为合成数据。完整序号与引用校验不能证明真实 PDF 页数或 bbox；PDF 原生定位在 B03-T02 采用显式契约扩展，不放宽既有文本来源 Schema2。

### B03-T02 PDF解析沙盒

[pdf-page-regions-v1](PDF_PARSE_CONTRACT.md) 是protocol v1中由资源SHA绑定的严格manifest，所有字段与nullable项均必填、未知字段拒绝；不是新增项目Schema。保存完整raw-pdf、逐页UTF8、manifest及Core JSON/Markdown。每个物理页有零基索引、一基显示物理页、未知printed_page_label=null、真实几何、glyph/region/visual清单和parsed/needs_review/failed；未枚举完总数保持null并保留已确认前缀。完整总数与有序页面和wire单元双向一致，失败/需复核页不算整份parsed。

区域定位独立声明pdf_page_region、物理页及显示CropBox左上原点/Y向下的pt bbox，保存raw MediaBox/CropBox、Rotate、UserUnit与pdf_to_display矩阵。字节范围只绑定指定逐页UTF8的resource_id和SHA，正文区间不含artifact追加的LF；不是PDF二进制偏移或旧blocks-v1 SourceSpan。空artifact可有已确认0字节及空SHA，但wire单元为unknown/text=null，不制造[0,0)正文。字符/区域一致性不能独立证明任意PDF内容无遗漏；固定作者预期及原始页回看另验证有限样例。

解析器身份绑定精确engine/依赖、adapter/options及Python版本。只读inspect从已保存的原始和全部资源核对实际字节与报告；原文件移动不影响这次重查。来源项目的PDF持久对象使用B03-T03的显式Schema3，Schema2继续限定原有text/plain/text/markdown、utf8-newlines-v1、blocks-v1、raw.bin/normalized.txt语义。

### B03-T03 Schema3 PDF来源登记

精确DDL、必填nullable字段、状态和快照布局见[PDF来源契约](PDF_SOURCE_CONTRACT.md)。Schema3保留四张旧来源表，另加pdf_parses、pdf_pages、pdf_regions、pdf_resources、pdf_imports；旧Source/Version/Span/Import身份、字段和rowid原样迁移。两类import共享实际正rowid顺序，新记录取两表最大值加一，拒绝跨表重复ID或顺序；时间戳不替代导入顺序。

PDF SourceVersion的原始文件为raw.pdf，ingest_status=acquired，normalized字段及error为空，旧Span清单为空。版本身份与parse身份分离，页JSON保存原始数值词法，解析结果引用完整原始SHA、精确parser身份、协议、adapter源码SHA和导入选项版本。PDF报告同时表达acquisition_status与pdf.status；未完整获取时version和parse为null、状态not_attempted；完整字节后的解析失败保留版本与真实失败parse。

PDF导入报告schema_version=2、report_kind=source_pdf_import，knowledge_status=pending，只统计实际区域和对应待抽取数。已验证parsed或needs_review可按默认键复用，failed不复用，显式重新解析始终创建新parse。重查依据项目内完整快照，原文件删除不使已确认证据失效；未知文件、owner变化、链接、篡改及候选不一致必须拒绝。

### 项目初始化与存储版本

Project 配置位置固定为项目根下 `project.json`，状态库为 `project.sqlite`。B01-T02的Schema1是空项目基线，init继续生成该版本；文本导入将Schema1受保护地升级到2，PDF导入将Schema1/2受保护地升级到3，Schema3中的文本导入保持3。配置只接受以下字段，缺失、未知字段或不支持的值必须拒绝。

| 字段 | 当前约束 |
| --- | --- |
| project_id | 初始化时生成并持久化的标准小写UUID v4，不从名称或目录推导 |
| schema_version | init为整数1；inspect支持1、2、3，其他版本明确拒绝 |
| created_at | RFC 3339 UTC字符串，配置与数据库使用同一值 |
| state | initialized只表示项目建立完成；Schema2/3可已有来源记录，此值不表示资料或知识处理成功 |
| database | 固定项目相对路径`project.sqlite`，不接受绝对路径、URI或路径穿越 |

配置上限为64 KiB，用于这五个元数据字段，不承载原始资料或知识内容。Schema 1 只建立 `project_metadata` 基础表，包含 `singleton`、`project_id`、`schema_version`、`created_at` 和 `state`；`singleton=1` 限定单条项目记录，项目 ID 唯一，版本和状态受约束。`PRAGMA user_version=1`，`PRAGMA application_id=0x4e4f4554` 标识 Noetome 项目库。inspect 核对真实表及列结构、配置、数据库记录与这两个标记；Schema 1 只允许该表和 SQLite 自身维护的对象，额外表、索引、视图或触发器必须拒绝。

init要求用户选定的目标目录尚不存在，父目录已存在；已有空目录、项目目录或未知内容均拒绝接管。初始化先独占创建目录和`.noetome-initializing`标记，在同目录的`.project.sqlite.initializing`、`.project.json.initializing`准备数据库与配置，通过不覆盖已有目标的文件硬链接发布两份正式文件，二者完成后才去除中断标记。完成项目只包含正式配置与数据库；链接发布发生在同一项目目录内，不把这种文件发布方式宣称为生产Vault多文件原子更新。

普通失败只清理本次独占创建、仍与创建记录相符的内容；出现未知文件、文件变更或目录替换时保留目录。崩溃留下标记、暂存文件或缺失配对文件时，init和inspect报告`PROJECT_INCOMPLETE`，不补造成功状态、不覆盖残留。当前恢复方式是保留残留供检查，使用新的目标目录重新初始化空项目；未实现原目录自动续写或生产任务恢复。普通失败已经安全回滚时可在原路径重试。

inspect以只读、immutable的SQLite URI打开数据库，文件路径先编码，拒绝-journal、-wal、-shm以及super-journal待恢复侧文件；项目库使用DELETE日志模式，检查时临时存储限定为内存。检查不执行迁移、修复或创建SQLite侧文件。immutable跳过SQLite锁和变更检测，见[SQLite URI规则](https://sqlite.org/uri.html)，因此只用于当前没有写任务的Schema1/2/3项目；Windows检查期间保持拒绝写入和删除的只读句柄，来源缓存owner守卫与精确清单贯穿全部读取。其他平台的并发外部写入一致性需分别实测。配置、数据库及来源缓存中的链接或Windows reparse/junction路径必须拒绝，快照定位只在选定项目内。

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

本输入包及第5至8节由B02-T02的只读fixture检查和B02-T03的临时Vault编译逐步落实；B02-T01来源导入不创建IR、计划或完整Run/Release。以下较完整的通用对象仍以各任务明确实现的字段为准，不提前保存空候选或问题身份。

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

### B02-T02 已实现的只读 fixture v1

本节固定当前已实现的输入契约，后续RenderedFile/Run/Release仍按下节规划。结构定义为[`schemas/fixture-v1.json`](../../schemas/fixture-v1.json)，字段和跨包语义实施于`fixture_model`、`fixture_input`、`fixture_knowledge`及`fixture_plan`。四份JSON均`schema_version=1`、`producer=fixture`，对象拒绝未知字段；所有声明字段必填，包括允许null的覆盖处置字段。ID为1至128字节ASCII，首字符字母或数字，余字符可含`_`、`-`；SHA为64位小写十六进制。fixture作者提供固定身份，不使用生产导入生成的UUID来假称自动抽取。

| 包 | 当前作用 |
| --- | --- |
| `case.json` | case_id、1至32个来源及明确相对文件定位、raw/normalized SHA、规范化/分段规则版本、完整SourceSpan声明，以及ir_path/plan_path/expected_path |
| `ir.json` | revision、coverage_run_id审阅作用域、来源版本绑定、Concept、Evidence、Atom及逐Span CoverageRecord |
| `note-plan.json` | revision、knowledge_revision、完整原始IR字节的knowledge_sha256、Note/Section/paragraph/ordered_list/table与placements |
| `expected.json` | 独立required_atoms、完整evidence集合、coverage、canonical_placements、required_links及tables |

每份JSON最多4 MiB，四包及所有来源读取总量最多64 MiB。来源只接受UTF8 MD/TXT，复用既有规范化与分段实现，再逐字段对账独立Span声明。证据的`excerpt_start_byte`/`excerpt_end_byte`是所属Span内的UTF8半开局部区间，不能为空、越界或切断字符；excerpt_sha256必须与实际片段字节一致。expected的Evidence同时锁定evidence_id、source_span_id、局部区间与SHA，合法短片段须本身已经独立审阅，不能缩成一个仍正确哈希的字符后继续通过。

覆盖声明必须与实际Span集合双向一一对应，run_id/policy_version/stage有效。此任务只接受`mapped`或`planned`，included/duplicate需要有效映射与自身来源位置的支持证据；required_atom_ids恰为映射内必要原子的子集。排除保持空映射和非空审阅理由，确定性格式排除只能针对formatting。duplicate要求原文字节指纹及映射一致，链最终到included，不能自引用或成环。没有创建持久Run，coverage_run_id只标识fixture作者声明的审阅作用域。

每个必要原子必须`required=true`、`verification=fixture_reviewed`、`value_status=known`，有有效supports证据并保留原子content与qualifiers中的条件、单位和例外。独立expected逐字段锁住这些内容及引用，不能从待测计划反向生成expected。每个显示scope（段落、单独列表项或单独内容单元格）完整承载其映射原子，别处同ID不能补足当前缺失表达。

规范位置仍由Concept与Placement固定；expected的每个`canonical_placements`还必填非空、无重复、相关且有效的`atom_ids`。这些解释原子须在声明的note/section完整显示，仅留下比较值不能代替定义。SemanticLink必须解析到唯一有效规范位置。输出`.md`路径额外拒绝`#[]^`，并拒绝大小写或文件/目录祖先碰撞。明文字段拒绝原始wiki/Markdown链接、HTML、隐藏注释及非法控制符，引用通过结构化SemanticLink声明；普通比较符和数学单括号保留。

`case verify`报告Schema1包含case_id、validator_version、实际counts、issues和resolved_targets及四包原始SHA。`checks.sources/coverage/knowledge/note_plan/expected`分别为passed或failed；来源不可信时依赖检查为not_run。`rendering/scientific_quality`始终not_run，passed仅代表本次契约检查通过。本报告没有伪造run_id/candidate_id/issue_id，也不代替后续发布报告。实际证据见[B02-T02记录](../validation/B02-T02.md)。

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

### B02-T03 当前编译契约

`case compile <case-root> <new-output-root>`复用同一受保护LoadedCase，先运行来源、覆盖、知识、计划与独立expected检查。输入语义失败不创建输出根；不支持的字段/版本或不安全路径返回2，来源读取/路径碰撞/未完成阻断返回3，输出传输错误返回1。普通帮助不写入。

`RenderedFile`在上述最小字段外保存`links`；每个`content_mappings.scope`含`section_id/node_id/kind/item_id/row_id/column_id`及atom_ids和UTF8字节半开区间，表格标签/未给出单元格也有空原子映射。`LinkMapping`包含同样完整scope、target_concept_id/target_path/target_heading/display_text及实际token区间。别名区间与正文区间分开，不能用链接别名补足原子条件。校验独立解析实际有限Markdown，并以计划和expected核对，不只信任指纹或映射。

编译报告Schema1使用[CompileReport](../../crates/noetome-cli/src/compile_model.rs)：schema_version、producer、case_id、run_id、release_id、started_at、finished_at、result、dimensions、checks、counts、issues、files、input_hashes、versions、publication。当前issues沿用fixture的rule_id/object_refs/message，不生成尚未实现的issue_id、candidate_id或severity/status记录。input_hashes固定四包SHA；versions保存渲染/校验/报告/契约及来源解析、规范化和覆盖政策版本。Run与Release为真实小写UUID v4，时间为UTC RFC3339；fixture原coverage_run_id只作为审阅作用域保存在metadata。

三个核心维度根据实际检查为complete或incomplete；研究为not_applicable，scientific_quality检查not_run。必需检查为sources/coverage/knowledge/note_plan/expected/rendering/staging_output/vault_output；任意缺失、failed、not_run或非零issue_count均禁止发布。额外output_manifest/output_mapping/output_content/output_link记录实际输出问题。成功报告result为validated，失败为failed；这两个值都不自行证明发布已经提交。

`publication`只引用state/compile.sqlite、publication_commits及release_id。Markdown显示同一份状态、理由、计数、问题、指纹、文件修订号以及完整内容/链接scope。stdout的外层CompileOutcome给出真实release_status、output_created、路径、原样report及issues；预检失败时run/release/report与不存在的路径为null。保存后的失败报告是当时事实快照，后续数据库错误另在外层issues及可用recovery.json中记录，不改写已有验证报告。

独立编译数据库为Schema1（application_id=0x4e434d50），不升级来源project.sqlite Schema2。固定六表为compile_metadata/runs/releases/coverage_records/rendered_files/publication_commits，列与约束见[执行方案](../execution/B02-T03_PLAN.md)。初始化Run.running与Release.staged；完整暂存后Coverage.rendered；暂存和Vault重读通过后Release.verified。最后事务同时绑定文件清单SHA和双报告SHA、插入publication_commits、Run.succeeded/stage=finished、Release.published及全部Coverage.validated。最终COMMIT之前准备返回数据，之后没有必需文件写入或清理。

数据库使用DELETE journal/FULL sync/外键检查。COMMIT失败显式回滚活动事务，不能把未提交的发布记录带入失败登记；只读核对前拒绝journal/wal/shm，不绕过热日志推断成功。渲染未完成留staged；实际输出验证或后续登记失败留recovery_required，无法更新状态时保留最后已提交阶段和诊断。报告或失败状态登记无法完成时，仅在整个目录及数据库身份仍可确认的场景独占保存recovery.json；未知内容或替换目录继续保留，不能删除、覆盖或接管。

本轮输出根默认保留，重新编译必须另选新目录。暂存后真实进程终止、真实SQLite COMMIT Busy与热日志测试证明受控现场可识别；不表示任意断电自动恢复或生产Vault增量合并已经实现。证据见[B02-T03](../validation/B02-T03.md)。

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
