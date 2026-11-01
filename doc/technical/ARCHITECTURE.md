# Noetome 工程架构

本文件规定当前采用的工程方向和首个可运行闭环。产品目标以 [PRD](../requirements/PRD.md) 为准；这里的分步实现不删减双模式、真实资料解析、主动图表表达和长期维护目标。

状态：工程基线。B01/B01A提供项目与桌面骨架，B02-T01已实现CLI单份文本来源登记，B02-T02提供只读fixture知识/覆盖/计划及独立预期校验；B02-T03当前实现将有效fixture编译到全新测试输出根，重读有限Markdown、保存双报告并以独立SQLite事务登记发布。任务完成状态与完整验证证据以[任务账本](../../dev-log/tasks.json)为准，不能把本文件中的后续设计或工程样例当作已验收能力。第三方库及版本在引入时验证并锁定。

## 1 当前决定

| 领域 | 采用方向 | 当前边界 |
| --- | --- | --- |
| 核心与命令行 | Rust core 与 CLI | 先建立可单独执行和测试的核心，不依赖 GUI |
| 资料解析 | Rust文本入口与有界 sidecar 宿主，随后接 Python parser | 当前CLI仍只登记明确选取的UTF8 MD/TXT；sidecar v1只验证原始字节/fake协议，真实PDF尚未接入 |
| 持久化 | 本地 SQLite | 来源项目保存项目、来源版本、Span与最小导入记录；fixture编译工作区另以Schema1保存Run、Coverage、Release和发布依据，数据库不放进阅读Vault |
| 交换契约 | 带版本的 JSON | 显式校验输入、引用与结果；详见 [数据契约](DATA_CONTRACTS.md) |
| 输出 | 有限Markdown、标题语义链接与比较表 | 当前只写尚不存在的新测试根中的Vault；完整Obsidian兼容及生产库维护需分别验收 |
| 桌面端 | Tauri 与 React TypeScript | 按用户要求提前桌面骨架及安装实测；调用同一核心接口，完整用例逐项接入 |
| 模型接入 | API 优先，再 Agent，再本地模型 | 首闭环不使用网络或模型；后续逐个能力接入 |

Markdown 是可阅读产物，Knowledge IR 是可验证的知识表达。文件路径、概念身份和来源身份分别管理。简单原型也不得让 Markdown 成为唯一的状态存储。

桌面窗口偏好与项目状态分开：`desktop-preferences.json` 固定在应用本地数据目录，由Rust读取和持久保存；前端仅通过受限设置命令选择关闭行为。×默认隐藏到托盘，可保存为直接退出；常规最小化保留任务栏窗口，托盘菜单退出始终终止应用。缺失偏好使用默认值且查询不创建文件；损坏或不支持版本保留文件并报告。设置查询实际重读文件，只有成功读取或保存才更新运行时偏好；重读或保存失败保留此前已生效的行为。启动读取失败且尚无有效偏好时，明确报告后安全采用隐藏到托盘，不伪称默认值已保存。此偏好不引入后台任务或项目数据库迁移。

## 2 首个闭环

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

B03-T01 的已实现边界见[单请求协议v1](SIDECAR_PROTOCOL.md)：Rust Host 仅提供自有输入快照和新根，严格验证完整原始分区及实际资源；三个并发管道有明确限额，Windows JobList 在挂起创建时原子关联进程树，异常路径终止并等待。fake helper 只在测试 target，无生产 CLI 开关。CLI/desktop 的 unsafe forbid 保留；Win32 FFI 仅在小型平台 crate 的私有模块封装。

v1 input_bytes 定位不表达 PDF 页或正文。后续真实 PDF 协议显式升级，不能修改旧 locator 含义；项目 Schema2 与编译 Schema1 仍分开。受控进程树及资源校验不是 OS 文件/网络沙盒，实测和未测范围见验证记录。

真实 PDF 接入前，已用小型协议测试程序验证进程启动、单个请求、结构化输出、超时、退出码和取消。请求包含版本、请求 ID、限定只读快照引用和工作区；响应包含来源指纹、完整原始字节单元、资源清单、问题和能力信息。

协议先采用有边界的 JSON 请求响应。若使用标准输入输出，标准输出只传协议，日志走标准错误；必须限制响应大小。不要为了首个 PDF 提前设计通用远程任务总线。

Core 再次验证 sidecar 输出，不能相信其路径、引用或成功标志。解析器只读取明确提供的输入，不获取整个用户目录的搜索权限。具体 PDF/OCR 库逐项试验后确定，并记录扫描、表格、公式及图片的支持边界。

### 模型 API

先验证一次低风险结构化响应、错误处理和用量记录，再引入真实抽取。模型返回候选领域数据，不能自行写库或发布文件。fixture 与真实 provider 共用同一验证入口。

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
