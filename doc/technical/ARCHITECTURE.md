# Noetome 工程架构

本文件规定当前采用的工程方向和首个可运行闭环。产品目标以 [PRD](../requirements/PRD.md) 为准；这里的分步实现不删减双模式、真实资料解析、主动图表表达和长期维护目标。

状态：工程基线。B01/B01A提供项目与桌面骨架，B02-T01实现CLI单份文本来源登记，B02-T02提供只读fixture知识/覆盖/计划及独立预期校验；B02-T03将有效fixture编译到全新测试输出根，重读有限Markdown、保存双报告并以独立SQLite事务登记发布。B03-T01提供内部有界parser sidecar宿主；B03-T02接入单一锁定文本PDF adapter，在全新项目外测试根保存原始、物理页、真实区域和逐页文本，Core重新核对并保存JSON/Markdown，独立inspect保持只读。B03-T03以显式Schema3将PDF原始版本、独立解析尝试和页/区域/资源索引纳入受保护的来源项目；验收见[B03-T03](../validation/B03-T03.md)。任务完成状态与完整验证证据以[任务账本](../../dev-log/tasks.json)为准，不能把本文件中的后续设计或工程样例当作已验收能力。第三方库及版本在引入时验证并锁定。

## 1 当前决定

| 领域 | 采用方向 | 当前边界 |
| --- | --- | --- |
| 核心与命令行 | Rust core 与 CLI | 先建立可单独执行和测试的核心，不依赖 GUI |
| 资料解析 | Rust文本入口及单一pdfplumber adapter的有界sidecar | 来源项目登记明确选定的MD/TXT/PDF；PDF保存原始版本和独立解析尝试，扫描和复杂版面保守需复核 |
| 持久化 | 本地 SQLite | 来源项目支持Schema1/2/3，PDF在Schema3保存页、区域、资源和真实导入顺序；fixture编译工作区另以Schema1保存Run、Coverage、Release和发布依据，数据库不放进阅读Vault |
| 交换契约 | 带版本的 JSON | 显式校验输入、引用与结果；详见 [数据契约](DATA_CONTRACTS.md) |
| 输出 | 有限Markdown、标题语义链接与比较表 | 当前只写尚不存在的新测试根中的Vault；完整Obsidian兼容及生产库维护需分别验收 |
| 桌面端 | Tauri 与 React TypeScript | 按用户要求提前桌面骨架及安装实测；调用同一核心接口，完整用例逐项接入 |
| 模型接入 | API 优先，再 Agent，再本地模型 | 首闭环不使用网络或模型；后续逐个能力接入 |

Markdown 是可阅读产物，Knowledge IR 是可验证的知识表达。文件路径、概念身份和来源身份分别管理。简单原型也不得让 Markdown 成为唯一的状态存储。

B06-T03 的[公式保真契约](FORMULA_FIDELITY_CONTRACT.md)采用独立选定附件命名空间。Rust 从明确 TeX 与作者语义转写分别解析有界表达，核对全部数学 token 和符号定义后选择 MathJax、等价兼容表达或 PNG 原区域；语义未知保持阻断。原始文件及编译输入分别保存，PNG 使用锁定解码器完整验证。沿用持有文件/目录能力及新根输出保护，不向旧 fixture 或过程 IR 混入公式字段，也不从手工转写生成已登记项目来源或科学正确率结论。

B03-T02的 [PDF 沙盒](PDF_PARSE_CONTRACT.md) 复用protocol v1：可信嵌入Python adapter从完整原始字节的BytesIO取得字符和页面，Rust独立核对资源SHA、页序、CropBox/Rotate/UserUnit矩阵、字形分配、正文范围和整体状态。`pdf_model`/`pdf_validate`负责严格类型及语义，`pdf_workspace`保护独占外层与最终报告，`pdf_parse`协调进程、限时/取消及只读重查。嵌入payload解压必须逐字节等于可读源码；实际Windows原始-c长度失败后采用短压缩wrapper，无临时driver或资料决定的命令。该一致性审计加固定作者预期只证明有限样例，扫描、公式、图形、多栏及旋转阅读顺序仍需复核；它不创建来源版本、IR或Run/Release。

桌面窗口偏好与项目状态分开：`desktop-preferences.json` 固定在应用本地数据目录，由Rust读取和持久保存；前端仅通过受限设置命令选择关闭行为。×默认隐藏到托盘，可保存为直接退出；常规最小化保留任务栏窗口，托盘菜单退出始终终止应用。缺失偏好使用默认值且查询不创建文件；损坏或不支持版本保留文件并报告。设置查询实际重读文件，只有成功读取或保存才更新运行时偏好；重读或保存失败保留此前已生效的行为。启动读取失败且尚无有效偏好时，明确报告后安全采用隐藏到托盘，不伪称默认值已保存。此偏好不引入后台任务或项目数据库迁移。

## 2 首个闭环

B03-T03的跨模块边界见[PDF来源契约](PDF_SOURCE_CONTRACT.md)：`pdf_source`只登记明确选定的资料，`pdf_source_model`核对严格持久图、原始页JSON及已保存报告，`source_schema3.sql`单独定义新Schema，旧Schema1/2与文本报告保持其原有含义。`RunSession`与`JobSession`保留原始目录能力到Core校验和捕获结束；`CapturedTree`在能力仍存活时取得守卫并移交`source_write`，Writer独占复制PDF文件、发布配置/数据库并完整重读后才清理。

默认复用按原始版本和完整解析身份/协议/选项键选择已验证的parsed或needs_review尝试；失败重试与`--reparse`产生新parse身份，旧定位保持独立。来源缓存的owner守卫贯穿全部快照读取，返回前复查目录清单和初始缺失状态。完整原始字节已取得但解析失败仍保存原始版本；获取未完成只保存获取失败导入，知识状态保持pending。

目标是证明输入、账本、计划、输出和报告之间能够对账，而不是证明模型能够理解教材。B02-T01完成来源登记、不可变快照和完整Span清单；B02-T02核对自造fixture的IR、覆盖与NotePlan契约及独立expected。B02-T03复用同一只读加载结果，增加确定性渲染、暂存与Vault实际重读、Run/Coverage/Release登记及JSON/Markdown验证快照；实现边界见[本任务方案](../execution/B02-T03_PLAN.md)。

输入包括少量可审阅的 UTF-8 MD/TXT 文件，以及同一 fixture 包中的预先编写的 IR、NotePlan 和期望结果。IR 明确记录 `producer = fixture`；不得把它展示为自动抽取或已核验学科知识。

```text
只读 fixture 四包与完整来源
  -> 真实指纹、确定性分段与声明 SourceSpan 对账
  -> Coverage、Knowledge、NotePlan 与独立 expected 预检
  -> 独占新根并登记本轮 Run、Coverage 与 staged Release
  -> 确定性 Markdown 渲染到 staging/vault
  -> 暂存实际清单、SHA、内容映射和全部语义链接验证
  -> 独占写入 vault 并再次完整重读，Release = verified
  -> 写入并重读同一 typed 事实的 JSON/Markdown 验证快照
  -> 最后 SQLite 事务登记 publication_commits 与 published 状态
```

当前[自造fixture](../../fixtures/text-compare-v1/README.md)包含两份关联笔记、两处规范解释标题、一个三列比较表和两个跨笔记标题链接；必要原子保留定义、虚构数值、单位、条件及例外。表格数据与语义链接来自已声明的计划，不能宣称已实现AI表达规划。正文、列表项与单元格均有实际UTF8字节范围映射，语义别名范围单独记录，不能借别名掩盖正文知识遗漏。

渲染器`fixture-markdown-v1`当前支持H1笔记标题、H2小节、普通明文段落、有序列表和结构化比较表，以及追加到所属正文/列表项/单元格末尾的canonical标题链接。明文ASCII标点转义，换行与Tab使用固定编码后单次解码核对；表格内wiki别名分隔符写为`\|`。不把明文数学、HTML、公式、代码、Mermaid或嵌入解释为额外已支持的展示语义；原生应用视觉检查与工程解析校验分开记录。

报告分别表达`source_coverage`、`knowledge_compilation`、`quality_validation`和`optional_research`的状态、理由与计数。前三维只描述本次工程校验；`optional_research = not_applicable`，`scientific_quality = not_run`。自造fixture通过不证明真实PDF/API抽取、科学正确率或原生Obsidian兼容。

本阶段不实现 OCR、联网研究、事实裁决、自动笔记拆分、复杂图像重绘、生产 Vault 接管、自动三方合并或完整审批 UI。未知能力必须显示为未实现，不得以成功空输出替代。

## 3 模块与依赖

以下是逻辑边界，不要求立即拆成多个 crate 或进程。只有复用、编译边界或测试隔离需要时才拆包。

| 模块 | 输入与输出 | 不应承担的职责 |
| --- | --- | --- |
| CLI | 参数、退出码、运行摘要 | 自行决定知识正确性 |
| Application | 用例编排、事务边界、阶段状态 | 拼接 Markdown 细节 |
| Ingestion | 来源字节到规范文本、Span 清单 | 推测事实或静默筛除知识 |
| Knowledge | IR 引用、证据关系与不变量 | 直接写输出文件 |
| Planning | NotePlan、Placement、表达节点 | 以文件名代替概念身份 |
| Rendering | 已校验计划到 Markdown 与映射 | 猜测不存在的链接目标 |
| Validation | 独立问题清单和结果 | 自动降低范围以让校验通过 |
| Storage | SQLite 持久化与版本读取 | 隐式修改来源内容 |
| Publication | 路径防护、暂存、发布清单 | 绕过验证或人工保护 |

依赖方向为 CLI 到 Application，再到领域能力与适配器。核心规则不依赖终端输出、Tauri、浏览器或特定模型 SDK。外部副作用通过文件、数据库、解析器和模型等窄接口进入。

B01-T02的项目初始化与检查逻辑先放在`noetome-cli`库的`project`模块，独立于终端输出；B02-T01在同一库中增加来源模块，不提前拆全套core crate。当前文件职责为：

| 文件 | 当前责任 |
| --- | --- |
| [lib.rs](../../crates/noetome-cli/src/lib.rs)、[main.rs](../../crates/noetome-cli/src/main.rs) | 纯参数解析、帮助与版本、受限命令分发、结果stdout及诊断stderr、退出码 |
| [project.rs](../../crates/noetome-cli/src/project.rs) | Schema1空项目初始化、Schema1/2严格只读检查、路径及待恢复侧文件保护；writer内部验收仅可忽略自身写目录 |
| [text.rs](../../crates/noetome-cli/src/text.rs) | 纯UTF8规范化、原始/规范SHA与确定性完整分段，不生成身份或写文件 |
| [source_schema.rs](../../crates/noetome-cli/src/source_schema.rs) | Schema2精确表结构与约束、候选SQLite中Schema1到2的事务升级 |
| [source_model.rs](../../crates/noetome-cli/src/source_model.rs) | 持久来源与导入记录加载、身份/引用/状态/计数校验、真实快照与完整Span重读验证 |
| [source_write.rs](../../crates/noetome-cli/src/source_write.rs) | 独占写目录、原文件备份、候选库与新快照发布、受保护的失败回滚与清理 |
| [source.rs](../../crates/noetome-cli/src/source.rs) | 明确单文件读取、来源身份和版本复用、最小导入登记、候选与正式文件验收、只读导入查看 |
| [fixture_model.rs](../../crates/noetome-cli/src/fixture_model.rs) | 四份fixture严格输入类型、枚举、ID与问题/规范目标结构；不定义生产生成器 |
| [fixture_input.rs](../../crates/noetome-cli/src/fixture_input.rs) | 有界只读输入、普通路径/句柄保护、版本与SHA、复用实际分段并对账声明Span |
| [fixture_knowledge.rs](../../crates/noetome-cli/src/fixture_knowledge.rs) | 纯IR证据/引用/必要原子校验、逐Span覆盖及重复链对账 |
| [fixture_plan.rs](../../crates/noetome-cli/src/fixture_plan.rs) | 纯计划/显示scope/规范位置/结构化链接校验与独立expected逐字段对账 |
| [fixture.rs](../../crates/noetome-cli/src/fixture.rs) | 只读case verify编排与Schema1 JSON事实结果，来源失败时依赖项not_run；不发布文件 |
| [fixture_render.rs](../../crates/noetome-cli/src/fixture_render.rs) | 纯有限Markdown渲染；独立解析重读文件，对账实际scope、全文语义引用、规范解释与独立expected，不调用render生成验证期望 |
| [compile_model.rs](../../crates/noetome-cli/src/compile_model.rs) | 编译上下文、检查、四维状态、文件映射和验证报告的严格typed事实；发布引用不等于已提交发布 |
| [compile_report.rs](../../crates/noetome-cli/src/compile_report.rs) | 从同一typed验证快照生成Markdown事实表，与磁盘JSON保持同一键、理由、计数和问题 |
| [compile_store.rs](../../crates/noetome-cli/src/compile_store.rs) | 独占新根、目录/文件身份与精确清单保护、独立编译SQLite Schema1、阶段登记及最后发布事务 |
| [fixture_compile.rs](../../crates/noetome-cli/src/fixture_compile.rs) | case compile预检、输入守卫、暂存/Vault双重验证、报告与存储协调；失败保留可确认现场，不接管已有目录 |
| [sidecar_model.rs](../../crates/noetome-cli/src/sidecar_model.rs) | 严格protocol_version=1 DTO、解析器能力、来源单元/资源/问题和限额；独立于项目与编译Schema |
| [sidecar_process.rs](../../crates/noetome-cli/src/sidecar_process.rs) | 可信原生argv、并发有界管道、单请求身份/引用校验及Windows受控进程树停止与退出确认 |
| [sidecar_workspace.rs](../../crates/noetome-cli/src/sidecar_workspace.rs) | 明确输入只读守卫、独占新目录能力、资源身份/SHA/UTF8/路径与精确清单复核 |

来源登记不创建完整Run、CoverageRecord、IR、NotePlan或Release。case verify只读取作者声明的IR/覆盖/计划并验证，coverage_run_id是声明的审阅作用域，不登记持久Run。case compile通过预检后才在独立新根保存真实Run与Release UUID及逐Span Coverage；原fixture审阅作用域另存于compile_metadata.fixture_scope_id，不把它冒充真实运行身份。桌面的帮助、版本、运行信息和B01A窗口偏好接口与项目业务分开，尚未接入项目、来源或fixture业务入口。

## 4 工作区与文件职责

每个测试用例使用一个独立临时工作区，并在验收记录中写明其位置。当前来源项目的真实布局为：

```text
source-project/
  project.json                  项目配置，init为Schema1，来源登记后为Schema2
  project.sqlite                项目与来源持久记录
  .noetome-sources/              存在来源版本时使用的固定快照根
    .owner.json                 绑定项目身份
    <version-id>/
      raw.bin                   完整原始字节
      normalized.txt            仅ready版本保存
  .noetome-writing/              写入期间的操作说明、旧文件备份、候选与暂存快照
```

缓存和写目录由程序管理，不接受外部指定的内部输出路径。无效UTF8版本只保留raw.bin；未读全文件只登记失败尝试，不建立版本快照。fixture编译使用以下独立布局，不初始化来源项目或迁移project.sqlite：

```text
fixture-case/                  只读输入与审阅过的预期
  sources/
  case.json
  ir.json
  note-plan.json
  expected.json
new-output-root/               当前编译独占，运行前必须不存在
  state/compile.sqlite         编译工作区Schema1，与来源项目Schema2分开
  state/recovery.json          失败登记或报告无法完成且目录仍属本轮时保存诊断
  staging/vault/<note-paths>    暂存候选，成功或失败均保留
  vault/<note-paths>            通过暂存验证后写入并再次重读
  reports/validation.json      本轮验证事实快照，result为validated或failed
  reports/validation.md        同一事实的Markdown报告
  reports/failure.json         已有验证快照后的失败另存，不覆盖原快照
  reports/failure.md
```

fixture目录不能作为工作区或输出目录。项目根和编译输出根均由用户明确选定，可以是绝对路径或当前目录的相对路径；init与case compile都只创建尚不存在的目标，不自动补缺失父目录。编译输入与输出不得互为祖先；按路径组件判断，合法相邻目录仍可使用。工作区内部的数据库、缓存及产物路径必须相对于实际工作区，拒绝绝对路径、`..`、Windows保留名、不安全组件以及符号链接或junction。

空项目初始化以`project.json`和`project.sqlite`保存同一Schema1项目身份与版本，`.noetome-initializing`及两个临时文件限定未完成状态。SQLite事务和同目录的非覆盖文件发布各自保存证据，正式文件配对且标记移除后才显示初始化成功。普通失败仅清理本轮仍原样的独占文件；中断或未知内容保留供检查，当前可通过新的目标目录重新建立空项目，不声称支持原目录自动恢复。project inspect支持严格只读Schema1/2检查，使用immutable数据库访问，拒绝待恢复侧文件及写目录，不进行修复或迁移；具体字段和约束见[项目数据契约](DATA_CONTRACTS.md)。

首次来源登记在候选SQLite中事务升级为Schema2，保留project_id、created_at及initialized基础状态，并将配置与数据库配对发布；initialized在Schema2只表示项目建立完成。Schema1只允许基础表，Schema2只允许项目、来源、版本、Span和导入五张契约表及其约束。配置、数据库版本或元数据失配必须拒绝，不能让旧inspect忽略新表。

输出目标必须是本轮尚不存在的目录。若遇到已存在的目标、文件或未知内容，返回阻断结果，不自动清空、覆盖、移动或接管。这个保守策略是首闭环的人工内容保护，不等同于已经实现增量合并。

独立编译SQLite的`user_version = 1`、`application_id = 0x4e434d50`，固定使用`compile_metadata`、`runs`、`releases`、`coverage_records`、`rendered_files`、`publication_commits`六张表；来源项目Schema2仍只保留原有五张契约表。编译数据库采用`journal_mode = DELETE`、`synchronous = FULL`及外键约束，状态检查拒绝待恢复的`-journal`、`-wal`、`-shm`侧文件，不用immutable读取绕过未完成事务。它没有项目迁移或已有编译根接管入口。

输出 Vault 只包含阅读所需文件。SQLite、日志、凭据、原始资料缓存和未发布候选不混入 Vault。追溯摘要与发布清单保存在相邻报告区，后续再决定可移植索引打包形式。

## 5 处理与事务边界

当前单份文本登记按以下顺序执行：

1. 校验项目、选取路径、MD/TXT类型和可选既有source-id，完整只读读取最多64 MiB原始字节；规范化仅移除首BOM并统一CRLF/CR为LF。
2. 独占创建`.noetome-writing`，保存旧配置与数据库备份；在候选库完成必要升级、来源版本、Span和最小导入记录事务。
3. 新版本在写目录暂存raw及合法规范文本；重读候选数据库、旧快照与暂存快照，核对完整位置和真实指纹。
4. 独占创建新版本快照目录，不覆盖既有内容；检查原配置/数据库身份与字节后配对发布，重读正式数据库、配置和快照。
5. 验证通过后清理仍属本轮的候选、备份和写目录，再返回导入JSON。成功持久保存的来源读取失败记录保留为failed并返回非零；普通写入失败回滚可确认的自身改动，未知内容或清理失败保留标记及可用备份并报告未完成。

Windows准备阶段持有拒绝写入/删除的读句柄，替换前释放并复核身份与字节。该协议约束合作CLI并识别可观察的变更，不承诺任意外部并发编辑或任意断电下多文件原子提交。公开inspect不自动恢复中断现场。

当前fixture编译按以下顺序执行，来源登记仍不提前建立Run：

1. 完整只读加载四包与来源，复核真实SHA、Span边界及全部契约；保持LoadedCase文件句柄、身份和字节守卫至最终提交。纯预检拒绝既有目标、输入输出祖先重叠和不安全路径；语义失败不创建输出根。
2. 独占创建新根、state与编译Schema1，保存本轮Run/Release UUID、IR/计划修订号、四包SHA及规范化/分段/覆盖政策/渲染规则版本；复制全部Span覆盖处置到本轮CoverageRecord，不复制来源项目数据库。
3. 按已校验NotePlan确定性渲染，独占写入staging/vault，保存完整文件清单、SHA、字节数、内容与链接映射，再登记staged阶段。每次写入与阶段结束核对根、父目录、子目录及文件身份和精确清单。
4. 重读暂存文件，独立解析实际标题、列表、比较表、正文scope和全文件wiki链接；对账计划文字、独立expected原子及qualifiers、规范解释位置和实际目标文件/唯一标题。不仅检查声明ID、mapping或SHA。
5. 暂存验证通过后独占写入vault，再次重读与完整验证；每份输出与已验证暂存一致且输入守卫仍有效时登记Release=verified。vault中文件存在不表示已发布。
6. 以同一CompileReport生成并重读JSON/Markdown验证快照，核对四维状态、必需检查、真实计数、问题、清单、映射、链接及输入/规则版本；报告publication仅引用数据库、publication_commits和release_id。
7. 最后重新检查输入、全树身份、文件SHA、精确清单及数据库契约，在同一个SQLite事务中插入publication_commits并同步Run=succeeded/stage=finished、Release=published、全部Coverage=validated。该事务提交是唯一发布依据，随后没有必须成功的文件写入或清理；stdout输出失败也不反写已提交状态。

报告的`result = validated`是验证事实快照，不能直接推导`published`。publication_commits绑定实际JSON、Markdown与compact文件清单的SHA；stdout外层release_status来自最终提交或失败结果。必需的sources、coverage、knowledge、note_plan、expected、rendering、staging_output、vault_output任一failed或not_run，以及前三维incomplete，均阻断发布；scientific_quality固定not_run，不作为已经执行的科学质量结论。

SQLite事务不等于文件系统事务。渲染或早期暂存未完成时保存failed Run与staged Release；实际输出验证、报告或最后登记失败进入recovery_required，保留候选和问题。已写validation快照后发生失败，另写failure快照，不把验证通过改称发布成功。失败状态登记或报告无法完成时，仅在目录及文件身份仍可确认时独占写state/recovery.json；未知内容、目录换号或链接替换不被覆盖、删除或接管。COMMIT失败后检查事务状态并显式rollback未结束事务，禁止将待提交发布记录带入失败路径。

本阶段固定“每轮新目录”，创建后保留现场，没有Drop递归清理、公开自动恢复或强制接管入口。受控中断与占用测试证明已覆盖的失败边界，不保证任意断电或所有外部并发时序下多文件原子发布。后续生产Vault更新仍需独立设计发布日志、补偿、审批绑定和回滚。

## 6 可重复执行与版本

领域 ID 从 fixture 或持久化状态读取，不按笔记标题重新生成。相同输入、契约版本、计划和渲染器版本，应产生相同内容文件；时间、运行 ID 等审计字段在报告中单独比较。

case compile的两次新根运行保留相同Concept/Atom/Note/Section身份和笔记字节，另生成独立Run/Release UUID及UTC审计时间；运行身份、时间和输出根路径不写进正文。报告input_hashes固定包含case_sha256、ir_sha256、plan_sha256、expected_sha256，versions记录渲染及来源/覆盖规则。Windows上的进程验证以[固定作者字面期望](../../crates/noetome-cli/tests/support/compile_expectations.rs)核对两份完整Markdown，并在[12组真实CLI测试](../../crates/noetome-cli/tests/compile.rs)中执行35次调用，覆盖双目录、完整内容/引用/SQLite对账、Windows锁与junction和外部哨兵保护；不从待测输出回填expected。

当前来源版本保存原始/规范指纹、规范化和分段规则版本；同字节复用版本及Span，每次选取仍生成新的import-id。显式source-id先固定来源身份，再在其版本内复用或递增revision，不因另一来源SHA相同而合并。未指定ID时先保留既有选取路径关联，否则按同项目内容复用或新建来源；名称不参与身份生成，内容变化保留旧版本快照。

后续缓存复用还须区分IR、计划和渲染器版本，不得把“文件名相同”当作内容未变。无变化运行不得制造重复知识身份。

数据库变化通过顺序迁移管理，项目记录所用schema版本。当前仅来源import提供已实现的项目Schema1到2候选升级；inspect没有迁移入口。编译工作区Schema1拥有独立application_id和精确六表契约，不迁移或复用来源项目Schema2。不支持的版本必须拒绝并说明原因，未完成现场要求保留检查，不能提供假迁移或自动接管能力。

## 7 命令行边界

当前CLI入口为：

```text
noetome --help
noetome --version
noetome project init <project-path>
noetome project inspect <project-path>
noetome source import <project-path> <MD/TXT-file> [--source-id <existing-id>]
noetome source inspect <project-path> <import-id>
noetome extract fake <project-path> <import-id> <fake-root> <new-output-root> [--cancel-file <signal-path>]
noetome case verify <case-root>
noetome case compile <case-root> <new-output-root>
```

help/version及命令帮助只解析和输出，不访问项目或生成来源。source import只把明确选取的本地单文件作为输入，不扫描资料目录；source inspect不再读取原选取文件，只读核对持久记录与项目内快照。两者不调用网络/模型、不输出Vault。case verify只读核对明确标为fixture的四包，输出分项JSON与稳定问题，不登记Run/Release或生成笔记。case compile接收明确的输入根和新输出根，动词后可用`--`分隔以`-`开头的路径；没有跳过验证、防护、覆盖已有内容或自动接管的快捷开关。

帮助、版本和项目摘要为文字stdout；来源import/inspect与case verify固定输出各自报告Schema1 JSON，JSON版本与项目Schema2分开。诊断写stderr，不混入JSON；来源结果持久保存在SQLite中，可用import-id再次只读查看。fixture验证不持久登记，stdout可由调用方重定向为文件。

case compile成功stdout为Schema1 CompileOutcome，包含producer=fixture、operation=case_compile、case_id、run_id、release_id、release_status、output_created、paths及report。paths.root为实际绝对根，Vault、数据库和已写报告路径相对于根；不存在的产物为null。report逐字段等同磁盘JSON验证快照，外层release_status=published必须有数据库最终提交依据。语义预检失败时output_created=false、release_status=not_started，运行ID、report及产物路径为null，checks/issues只描述实际执行的预检，不伪装双报告已经生成。

退出码约定为：`0`表示命令声明的操作已完成；项目初始化只表示空项目契约已建立，来源import只表示真实快照与来源记录验证完成。source inspect成功查看已保存failed导入记录仍返回0，因为只读核对已完成，不表示该来源登记成功。case compile返回0还要求必需验证全部通过并完成最后发布事务。

`2`表示参数或输入契约不合法，compile的非法JSON/UTF8及PROJECT_UNSAFE_PATH拒绝在分配前输出空stdout；`3`表示来源读取失败、可解释的阻断或未完成，包括COMPILE_OUTPUT_EXISTS、COMPILE_INPUT_OUTPUT_OVERLAP及实际I/O/文件占用。compile语义预检失败输出output_created=false的分项JSON并返回3；拥有新根后的失败返回3，外层为staged或recovery_required并保留可确认现场，诊断CASE_COMPILATION_FAILED。`1`表示内部错误或stdout输出失败；提交后的CLI_OUTPUT_ERROR只说明输出传输失败，不能反写已提交Run/Release/Coverage或否认publication_commits。已保存的失败导入同时输出JSON和稳定诊断；写入失败不输出成功JSON，中断不得转换为0。详见[设计标准](../standards/DESIGN.md)。

## 8 后续适配器

### Python parser sidecar

B03-T01实现[单请求协议](SIDECAR_PROTOCOL.md)：Core验证明确选取的单文件，保持拒绝写入/删除的读守卫，将绝对路径、完整原始SHA/长度、访问约束、全新工作区能力及限额放入请求。响应必须回传protocol_version、request_id、原始指纹及workspace能力，并提供producer、parser/能力版本、完整来源单元、相对资源清单和结构化问题。资源文本的UTF8半开范围与未来PDF物理页/bbox、旧规范文本Span分别定义，不能混用。

宿主用可信可执行文件和原生argv启动，资料及响应不决定命令。stdin写完后关闭，stdout/stderr并发有界读取，stdout只接受一个JSON及末尾空白；合法JSON配非零退出仍失败。请求、响应、诊断、输入、资源、数量和时间有明确政策，超过上限不得截断后报成功。Windows Job Object管理本轮进程树，子/孙进程退出以实际活动PID和原生退出状态确认。仅测试构建允许fake；普通库、CLI和项目登记拒绝fake。

Core复核资源的Windows安全相对路径、文件/目录身份、链接数、真实SHA/长度、UTF8范围及精确清单，保留输入与成功结果的读守卫；拒绝既有工作区、未知内容和路径越界。错误现场保留，不自动接管或递归删除。Job管理和路径复核不构成操作系统文件/网络沙盒；具体守卫、错误及实际边界见[B03-T01验证](../validation/B03-T01.md)。

真实PDF解析在B03-T02另行选择并锁定唯一Python文本解析器，固定完整物理页、坐标变换和独立页面审阅预期，再记录扫描、表格、公式及图片的支持与失败边界。当前协议fake不证明解析器真实性、科学质量、跨平台或长任务指标；不提前接OCR或通用远程任务总线。

### 模型 API

先验证一次低风险结构化响应、错误处理和用量记录，再引入真实抽取。模型返回候选领域数据，不能自行写库或发布文件。共用知识结构规则与来源证据核对，producer各自的审阅和发布政策不能混用。

B04-T01提供显式离线`extract fake`：只读会话从真实导入选择文本Span或已登记PDF页/区域，网关处理严格候选、单请求预留、超时/取消和全新测试根。fake只重放字节，不运行模型；输出`candidates_validated`仍保持知识pending和科学质量not_run。来源、fake和输出能力分别持有并在保存后重查，不把响应中的身份、quote或已发布声明当作可信事实。具体交换与边界见[抽取契约](EXTRACTION_CONTRACT.md)。

B04-T02沿用只读来源与领域校验，分开实现文本API native HTTP/wire/Schema和Core配置、预算、候选与审计保存。HTTP禁用代理继承、重定向和重试，一次POST有时间及实际字节边界；live限定官方Responses及用户明确指定的糖猫云deepseek-flash精确端点，本地HTTP假服务与真实服务明确区分。封闭策略保留旧Responses，所选策略使用自足task/Schema/request输入；两次Responses真实失败后，仅为同一服务增加官方Chat关闭思考/JSON Object入口，body与报告保存版本。第三次Chat结构失败后，v4只明确已有{atom,measurement}包装的提示，严格DTO及原响应不变。reasoning/thinking是客户端请求意图，返回效果另实测。Chat独立wire子模块严格校验单choice和普通JSON，原usage与统一prompt/completion映射独立于候选成功；Responses稀疏completed和原strict规则保持。Core在请求前预留、返回后核对实际用量，凭据回显不持久化。配置、来源和全新输出能力贯穿保存；具体接口见[文本API契约](EXTRACTION_API_CONTRACT.md)。用户已保存服务/模型/10M累计额度及本机凭据，外部PowerShell脚本以CurrentUser DPAPI和子进程环境复用；实际服务能力与抽取按原响应验收。

模型、网络检索和本地解析是不同能力。联网许可、请求预算、未知费用和证据状态分别记录。重试、并发预留和持久化恢复在长任务启用前补齐，不以单次成功调用代表全部能力已支持。

### 桌面前端

2026年11月1日用户要求提前搭建、安装并测试Windows应用。B01的桌面骨架由 `apps/desktop` 中的React TypeScript前端和 `crates/noetome-desktop` 中的Tauri入口组成。CLI和桌面入口共用 `crates/noetome-cli` 的纯Rust帮助、版本接口；B01最初提供 `runtime_info` 和受限的 `cli_preview`。B01A为窗口偏好增加 `get_close_behavior`、`set_close_behavior`，当前仅提供这四个受限命令，不接受任意文件路径、CLI参数或shell命令。

当前 `runtime_info` 返回桌面与CLI版本、编译目标、已有能力和帮助文本；`cli_preview` 仅接受 `help` 或 `version`，不能执行项目或来源命令。前端在调用前显示未运行，收到实际结果后显示连接或执行状态。CLI来源登记能力尚未接入桌面选择、导入或查看，不能因帮助中出现source命令就宣称GUI已支持；模型和知识产物也不由桌面空状态推定为成功。

`get_close_behavior`通过Rust的 `PreferencesState.reload` 实际重读固定设置文件；关于页打开或“重新读取”时不会只返回启动缓存。成功重读更新已生效偏好并返回其值，失败返回稳定错误并保留原有已生效行为。`set_close_behavior`仅接受两个关闭行为枚举，持久保存成功后才更新运行时值，失败保留原值与未知文件内容。设置版本、文件边界与恢复语义见[桌面窗口偏好契约](DATA_CONTRACTS.md#桌面窗口偏好契约)。

Windows构建使用锁定的Cargo和npm依赖，静态界面与指定图标派生资源随包提供。NSIS采用当前用户安装模式；不在安装包中混入原始 `sample/`、用户资料或仓库开发文件。安装包构建、本地安装和安装后运行分别记录证据。

Windows安装脚本在正常用户输出目录准备字节一致的交付副本，并只读校验文件完整性标签，避免开发目录的Low标签传入安装进程；不修改ACL或进程令牌。安装后文件校验允许Tauri对唯一NSIS包类型标记的精确三字节变体，其他变化必须失败。桌面入口在WebView2启动前准备并验证自身数据目录，窗口或目录失败通过原生错误弹窗报告，日志可写时追加保存；此机制不开放额外前端IPC权限。

B01A按用户确认管理窗口关闭事件：×默认隐藏原窗口到托盘，保存为直接退出时结束应用；最小化按钮保持Windows常规最小化和任务栏窗口。托盘左键及“显示窗口”菜单取消最小化、显示并聚焦原窗口，不重建页面；托盘“退出”始终结束程序。托盘创建失败须明确报告并停止启动，隐藏或恢复失败须报告，不能留下没有恢复入口的隐藏窗口。当前原生验收以[关闭设置验证记录](../validation/B01A-T01-settings-refresh.md)为准；[首次托盘记录](../validation/B01A-T01.md)中的最小化隐藏和×退出属于已修订的历史行为。

完整桌面用例后续由React TypeScript承担资料选择、状态展示、预览和审批；文件写入和保护规则仍由core执行。当前托盘与关闭偏好只保留窗口、进程和页面状态，没有后台任务。后续持久任务仍须分别设计隐藏窗口、取消任务与安全退出的语义，不沿用当前关闭设置而隐式丢弃工作。

## 9 继续开发前的证据

首闭环完成必须提供：可复现命令、临时 Vault、来源与原子对账、链接及路径验证、失败样例、SQLite 状态检查、保护测试结果和未实现能力列表。

下一步只引入一个新的不确定因素，例如真实 PDF 解析或单一 API 抽取。对每项能力先用小样本建立支持和失败边界，再扩大资料规模；不以提前搭完整 GUI 取代核心验证。

测试门槛与 fixture 管理见 [测试标准](../standards/TESTING.md)。任何改变 PRD 硬约束的工程简化都必须明确记录，不能只改实现并让文档自动失效。
## 有界离线来源核对

B04-T03在现有只读SourceSession和严格ApiCandidates之上增加普通 `extract verify`。review/decision/gold各自严格解析，受控中文单测量判据分开核对来源、关联概念、正文事实、measurement、必要条件/单位/例外、覆盖和排除。仅全部通过保存source_review IR；unknown、来源needs_review和pending仍无成功IR。报告包含实际输入与产物SHA、逐对象四状态和真实分母，不把API传输成功提升为科学正确。

[独立审计脚本](../../scripts/verify_extraction_knowledge.py)不导入Rust验证器：在调用前冻结作者gold，直接从只读SQLite重建来源身份，读取规范文本或实际登记PDF的资源、范围和bbox，再独立计算知识判据/数量及校验IR和双报告。工程矩阵包含一个先作者化中文句子、后经现有解析器登记的真实文本PDF；这仅证明受控样本闭环，不证明通用PDF语义、OCR或正式科学质量。实际验证范围见[B04-T03](../validation/B04-T03.md)。

## 有界多来源知识协调

B05-T01增加普通 `knowledge coordinate`，消费同项目2至3份已保存的单原子source_review依据。[严格模型](../../crates/noetome-cli/src/knowledge_coordination_model.rs)声明版本、显式身份、原出现、事实、关系和分母；[输入核对](../../crates/noetome-cli/src/knowledge_coordinate_input.rs)逐份重开SourceSession并重建原来源核对，不能以保存报告的passed标志替代实际SQL、资源、原文和九件依据。[事实协调](../../crates/noetome-cli/src/knowledge_coordinate_facts.rs)按明确对象/属性UUID及完整值、单位、条件、例外分组，保留所有原概念、原子、支持证据和实际来源位置。

[入口与保存](../../crates/noetome-cli/src/knowledge_coordinate.rs)持有输入守卫至三个结果保存后，仅撤去本轮可确认的结果文件；未知内容、原输入及来源项目不被接管。完整同键才去重，不按相同内容SHA合并另一实际来源身份，不按显示名称推断身份。无法处理的同对象/属性差异及同来源版本差异保留待复核，无成功IR。完整交换见[协调契约](KNOWLEDGE_COORDINATION_CONTRACT.md)，实际证明与仍未完成范围见[B05-T01记录](../validation/B05-T01.md)。

[独立协调审计](../../scripts/verify_knowledge_coordination.py)从外部作者gold、明确映射、只读实际来源及原source_review重新计算事实键、出处ID、关系和全部分母，然后核对JSON、Markdown、IR及原件副本；不共享Rust协调算法。当前是公开自造材料的离线明确身份原型，科学not_run、研究not_requested、知识pending、published=false。条件/历史变体和正式冲突分类由后续任务分别验收。

B05-T02按[严格v2契约](KNOWLEDGE_VARIANTS_CONTRACT.md)扩展同一入口：版本DTO、实际背景重建和并列事实关系分别承担独立职责，共用原身份/出处构造和完整来源守卫。条件或时间字段不写入原B04 IR；事实仍引用原对象和实际source/version/unit/range。时间区间只用于有来源依据的先后，条件完整性及无法比较的单位独立限制归并；未决不会提升为科学通过。Python v2独立重建和普通audit与旧v1验收分别保留，实测交付按[B05-T02记录](../validation/B05-T02.md)确认。

B05-T03新增[严格v3契约](KNOWLEDGE_CONFLICTS_CONTRACT.md)，复用完整来源核对及仅重建背景的内部函数。冲突分类在对象、属性、单位、完整适用范围与有据时间检查后进行；精确十进制比较不改写原表述。v3保留原支持、反证和限定链接及唯一证据切片，每处处理记录关联完整原出现。未决关系不选择胜者，无争议对象仍保留通过状态。独立Python从真实来源和调用前作者expected重建，不共享Rust分类器；实际验证及平台局限见[B05-T03记录](../validation/B05-T03.md)。

## 有据主动表达

B06-T01增加普通 `knowledge present`，从原协调case和已保存输出重新持有全部来源核对材料及真实SourceSession。`knowledge_presentation_input`复用三个旧协调算法重建完整结果，再严格核对旧版本IR、双报告和原件；`knowledge_presentation_plan`从明确对象/属性及原始出现记录选择比较表或正文。它恢复原词面并保留完整事实组，未决输入不进入成功规划。

`knowledge_presentation_render`编译结构化节点，独立解析实际Markdown、正文和链接字节范围；入口将结果写到全新测试Vault并复验来源和输出身份。新的[表达契约](ACTIVE_PRESENTATION_CONTRACT.md)使用独立schema/scope，与B02 fixture发布数据库分开。独立[Python校核](../../scripts/verify_knowledge_presentation.py)从调用前作者和真实来源重建计划，不借Rust计划生成expected。知识pending、science未运行和未发布状态保持；完整任务验收以账本为准。

## 有界过程与流程表达

B06-T02 的过程来源核对采用独立 `process_source_review / bounded_process_text_v1` 命名空间；不扩展旧测量原子或协调版本来容纳图。受控文字解析先从实际 SourceSession 得到完整过程/步骤/关系及逐行证据，再与独立 gold 比较。`knowledge flow` 重建并核对整个保存的 review 后才提交流程计划，语义支持与渲染能力分别判定。明确有向无环过程进入有限 Mermaid 编译，能力不兼容时使用完整有据文字；相关性、歧义或缺关键关系阻断。详细字段、阶段和验收边界见[过程流程契约](PROCESS_FLOW_CONTRACT.md)。

## 明确选定图像与有限重绘

B06V-T01 使用独立 image Schema1，`image_input` 持有选定整图 PNG、作者手工语义与 profile，完整解码而非只信任 PNG 首部。`image_fidelity` 分开核对关键语义、边端点与空间几何、目标能力和文字布局，再决定有限 SVG、原图或阻断。`image_render` 只生成固定本地安全图元和字面 Markdown，并保存原字段到实际图元/正文的 UTF-8 映射；不引入 OCR、图像模型或完整知识图谱。

输出沿用新根和输入/祖先/库存持有守卫，独立 Python 从冻结作者原图、预期和实际输出重验含义，原件保存不等于语义已经通过。曲线/观测保留原图，未知标签/关系与矛盾几何不能靠原图回退冒充成功。接口与限额见[图像契约](IMAGE_FIDELITY_CONTRACT.md)，扫描重建与跨版本矩阵分别验收。
