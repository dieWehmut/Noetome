# Noetome 设计标准

本文件约束 CLI、报告和后续桌面界面的表达。界面服务于真实状态、可核对产物及人工控制；功能范围以 [PRD](../requirements/PRD.md) 为准。

## 1 当前交付形式

首阶段以CLI和本地确定性闭环为基础。当前CLI支持项目初始化、只读项目检查、B02-T01单份UTF8 MD/TXT来源登记与只读查看、B02-T02明确标为fixture的IR/NotePlan/独立expected契约校验，以及B02-T03将通过校验的fixture编译到全新测试根。compile建立本轮Run、CoverageRecord和Release，生成有限Markdown笔记、JSON/Markdown验证报告，并以最后SQLite事务登记发布依据；来源登记的知识状态仍为pending，verify仍保持只读。fixture的知识和表达计划是作者输入，必须在结果中保留`producer=fixture`。

根据2026年11月1日的用户要求，已提前提供可本地安装的Windows桌面骨架；来源和fixture业务尚未接入GUI，完整桌面功能继续逐项建设。当前实现及原生应用的实际验收状态以[任务账本](../../dev-log/tasks.json)为准，不能把CLI工程检查描述为Obsidian应用或完整桌面业务已验收。

报告写“fixture 编译验证通过”，不能写“AI 已完成教材学习”“知识准确率100%”或“产品已支持 PDF”。没有发生的网络请求、模型推理和费用一律不编造。

B03-T02另提供项目外PDF解析沙盒，只表达有限文本页、原始证据、真实位置和需复核/失败状态。单栏文本样例通过不扩大为全部PDF、结构或知识处理已支持。

B03-T03将明确选定的PDF登记到来源项目，显示独立原始版本、解析尝试与获取/解析状态；重复导入、重解析及只读重查沿用CLI，知识仍pending。验收范围和原始证据见[B03-T03](../validation/B03-T03.md)。

## 2 信息优先级

用户首先应看见结果和下一步动作，其次是范围、问题和细节。

1. 本轮是否成功，是否生成可打开的产物。
2. 实际读取的资料范围与生产方式。
3. 哪些检查通过、失败或未运行。
4. 受阻对象、原因及可执行的处理方式。
5. 产物和报告路径、版本及重现命令。

不能只显示一个综合百分比。运行、编译、质量和发布是不同状态，任一维度的成功不能掩盖另一维度的失败。

## 3 CLI 行为

- 参数名称使用稳定、可解释的英文；帮助文本、摘要和诊断优先使用简体中文。
- 路径作为独立参数处理，支持中文、空格和非 ASCII 字符，不拼成可执行命令。
- 正常执行无需用户在每个阶段敲确认；遇到超出安全边界的写入时明确拒绝并返回问题。
- 非交互调用不得等待隐藏输入。后续增加交互模式时，必须提供显式开关与可重复的非交互替代。
- 标准输出用于结果，标准错误用于诊断。输出 JSON 时不混入进度条、颜色或提示文字。
- 错误必须有稳定 `rule_id` 或 `error_code`，不能要求调用方匹配中文句子。
- ANSI 颜色可用于终端，但关闭颜色后信息完全保留，管道和文件输出默认无控制字符。
- 不把密钥、认证头、整个原文或未经筛选的堆栈作为默认诊断输出。

首闭环退出码遵循 [工程架构](../technical/ARCHITECTURE.md)：0 成功、2 输入错误、3 受阻或未完成、1 内部错误。错误输出不因“已经写了部分文件”返回成功。

B01项目命令为`noetome project init <path>`与`noetome project inspect <path>`。成功摘要显示实际项目ID、Schema版本和initialized基础状态。init仍建立Schema1空项目；Schema2/3检查不能固定宣称“尚未登记来源”，initialized只表示项目建立完成，不能写成“知识已生成”。已有目标或初始化中断返回受阻；无效配置、Schema及数据库内容返回输入错误；每项错误带稳定`PROJECT_*`代码及可执行建议。中断目录保持原样，提示用户保留残留并使用新的目标目录重试，不把自动覆盖伪装成恢复。inspect严格只读支持Schema1/2/3，不在前端或CLI根据文件存在推断项目完整。

B02-T01来源命令为：

```text
noetome source import <project-path> <MD/TXT/PDF-file> [--source-id <existing-id>] [--reparse]
noetome source inspect <project-path> <import-id>
```

`source --help`、`source import --help`和`source inspect --help`只显示帮助，不访问项目或写文件。中文、空格路径作为单独参数；以-开头的路径可在动词后加`--`。非法参数、未知选项、不支持类型、不安全路径及非法或未登记身份在写入前拒绝，不提供force接管入口。

import只读取明确选取的单份文件，当前上限64 MiB，不扫描目录。首次登记在候选库把Schema1升级到2并保持项目身份/创建时间；配置与数据库版本失配、旧初始化残留或来源写目录必须明确拒绝。新来源生成不透明UUID；同字节复用版本和Span，每次选择仍有新的import-id。显式source-id固定既有来源后在其内部复用或换版，不能因另一来源SHA相同而合并。显示名称不决定身份，旧版本快照和原位置保留。

来源import和inspect的stdout固定为报告Schema1 JSON，与项目Schema版本分开。报告包含项目/导入/来源/版本身份、时间与明确选取路径、ready或failed、reused、knowledge_status=pending、span_count、pending_extraction_count、版本字段、完整Span及错误。版本提供原始/规范SHA、长度、规则版本和真实相对快照位置；Span为规范UTF8字节半开区间及一基包含端点的行号。empty或仅首BOM文本为0段，未闭合围栏保留code区间并标needs_review。来源清点与ready仅说明字节、规范化和位置登记状态，不宣称知识覆盖100%或Markdown语义完全解析。

来源错误和退出码按实际命令区分：

| 情形 | stdout与诊断 | 退出码 |
| --- | --- | --- |
| ready来源登记，候选与正式记录/快照均已重读通过 | 导入JSON，无成功提示混入JSON | 0 |
| 完整读取但UTF8无效 | failed JSON及SOURCE_INVALID_UTF8；仅有真实raw版本/指纹/长度，无规范快照或Span | 3 |
| 缺失、未读全、超过上限或读取期间变化 | failed JSON及SOURCE_READ_ERROR、SOURCE_TOO_LARGE或SOURCE_CHANGED；无假版本、假SHA或截断内容 | 3 |
| inspect核对已保存ready或failed记录通过 | 返回持久导入JSON；failed状态保留 | 0 |
| 参数、类型、身份、路径或记录/快照契约无效 | stderr稳定CLI_INVALID_ARGUMENT、SOURCE_*或PROJECT_*代码，不输出成功JSON | 2 |
| 未完成写入、未知缓存、项目占用或I/O阻断 | stderr稳定SOURCE_WRITE_INCOMPLETE、SOURCE_SNAPSHOT_CONFLICT、PROJECT_INCOMPLETE、PROJECT_DATABASE_BUSY或PROJECT_IO_ERROR，不输出成功JSON | 3 |
| 内部操作、报告序列化或stdout输出失败 | stderr稳定错误，例如SOURCE_REPORT_ERROR或CLI_OUTPUT_ERROR | 1 |

source inspect返回0只表示只读核对完成，不能把报告内failed改成成功。import保存来源读取失败记录与普通写入失败不同：前者保留可查看的真实失败对象，后者按可确认的自身文件回滚；未知内容、替换或清理失败保留标记和可用备份，提示检查现场，不自动接管。Windows准备阶段的句柄保护与发布复核不构成任意外部并发或任意断电下多文件原子恢复承诺。

### B03-T02 PDF解析和重查

```text
noetome source parse-pdf <selected-PDF> <new-test-root>
noetome source inspect-pdf <test-root>
```

`NOETOME_PDF_PYTHON`须指向调用者配置的绝对锁定解释器；失配/缺失明确PDF_RUNTIME或依赖错误，不触发安装或交互。parse只用尚不存在的新根，已有目录、路径跳转、链接、占用和变化都拒绝。输出原始、逐页文本与manifest，再由Core写相同事实的parse-result.json/.md；stdout纯JSON，诊断stderr。

JSON明确report_kind=pdf_parse_sandbox、原始SHA/字节、完整或未知页总数、物理页的parsed/review/failed数量、精确parser身份、manifest位置/SHA、实际elapsed/resource字节和sidecar事实。Markdown列相同事实、物理页状态及逐条问题；印刷页码未知，不以文件名或章节号冒充。只有整份parsed退出0；needs_review/failed及读取、路径、限额、超时/取消、子进程或Core失败退出3。语法无效退出2，stdout写失败退出1。

inspect保持只读、重查原始/文本/manifest真实字节和全部位置、摘要及Markdown；成功读取needs_review/failed沙盒仍返回3并保留原状态。宿主失败可有failure.json及未确认现场，整体不算有效完整沙盒，inspect拒绝接管或修复。未确认原始或完整页清单保留null/已确认前缀，不补造成功对象。该报告不显示IR、知识覆盖、OCR、科学质量或发布成功。

### B03-T03 PDF来源登记与重解析

```text
noetome source import <project-path> <PDF-file> [--source-id <existing-id>] [--reparse]
noetome source inspect <project-path> <import-id>
```

PDF导入使用受保护的Schema3，原始版本与解析尝试分别显示ID。重复同字节与重命名可复用原始版本及默认的已验证缓存；每次导入仍有新import_id。显式source_id限定该来源内的版本复用；--reparse只用于PDF，每次创建新解析尝试并保留旧证据。MD/TXT配合--reparse在写入前拒绝。

stdout为schema_version=2、report_kind=source_pdf_import的纯JSON，知识状态pending；原始获取状态、解析状态、实际页/区域数量、parser身份、选项、缓存键与问题各自明确。完整原始字节后的解析失败仍有原始版本；未读全时version/parse为空，页总数未知、区域数0。报告不得把原始获取成功或区域数解释为知识抽取完成。

PDF只有parsed返回0，needs_review、failed及not_attempted返回3；只读inspect保留同一状态和退出码，从已保存原始与完整解析资源核对结果，原文件移动/删除后仍可重查。旧文本inspect仍按原规则返回0表示核对完成。未知Schema、额外缓存、owner变化、文件篡改、占用或未完成写入按已有稳定错误码拒绝；现场标记和备份用于后续检查，重查不自动修复。

### B02-T02 fixture 检查


`noetome case verify <case-root>`及其帮助入口使用与来源命令一致的路径参数规则，动词后`--`可分隔以`-`开头的路径。验证只读，不登记Run、不写报告文件或Vault。有效输入stdout为Schema1 JSON，清楚标`producer=fixture`，显示实际来源/Span/原子/必要原子/计划笔记/节点数量与四包指纹。

五个已运行检查为`sources`、`coverage`、`knowledge`、`note_plan`、`expected`，各有status和issue_count；来源失配时后续项为not_run。`rendering`与`scientific_quality`始终not_run，不能显示科学正确率或发布成功。每个问题含稳定rule_id、object_refs与原因；无效规范位置不作为可用链接目标返回。

契约通过退出0；语义问题stdout保留`passed=false`报告，stderr为`CASE_VERIFICATION_FAILED`，退出3；不合法JSON、版本、字段或路径在报告前拒绝，stdout为空、退出2；读取占用、变化或I/O阻断退出3。输出错误退出1。数量与问题来自本次输入，不能用固定成功摘要代替实际校验。

计划明文字段与结构化链接分开，原始wiki/Markdown链接、HTML和隐藏注释不得绕开映射与链接检查。scope必须完整显示原子内容、条件和单位，规范定义不能被另一个相关数值替代。verify只验证输入契约；实际Markdown转义、文件内容和链接重读由独立的compile命令完成。

### B02-T03 fixture 编译

```text
noetome case compile <case-root> <new-output-root>
noetome case compile -- <case-root> <new-output-root>
```

两个路径分别是明确选择的只读fixture根与尚不存在的测试输出根；动词后的`--`用于以`-`开头的路径。输出根父目录必须存在。既有目录或文件、输入输出互为祖先、junction/reparse和不安全Windows组件在创建根前拒绝；重叠按组件判断，合法相邻目录可用。没有force、接管已有Vault或自动修复输入的入口。

新根中的`state/compile.sqlite`是独立编译工作区Schema1，保存本轮运行、覆盖、Release及文件与报告指纹，不能用`project inspect`把它当作项目Schema2。`staging/vault/`保留生成候选；暂存实际重读通过后才写`vault/`并再次独立验证。所有本地wiki目标必须定位到实际笔记中的唯一canonical标题，正文与别名各有独立字节范围；源、计划、独立expected、目录清单及文件身份在发布前复核。

stdout外层固定提供`schema_version=1`、`producer=fixture`、`operation=case_compile`、case/run/release身份、`release_status`、`output_created`、`paths`、`report`及问题。`paths.root`为绝对根路径；`vault`、`database`、`json_report`、`markdown_report`为根内相对路径或null，可用产物分别为`vault`、`state/compile.sqlite`与已保存报告。不得从空目录存在推断笔记可用，也不得把预检中的计划路径作为已生成产物。

`report.result=validated`仅表示验证事实快照通过。报告的`publication`只引用数据库、`publication_commits`表和本轮release_id；最后数据库事务同时登记publication commit、Run.succeeded、Release.published和Coverage.validated后，stdout才允许`release_status=published`。已生成笔记、运行结束或两份报告存在都不能单独证明发布。

| 情形 | stdout与诊断 | 退出码 |
| --- | --- | --- |
| 完整校验、暂存/Vault重读、双报告重读及最后事务提交通过 | CompileOutcome JSON，release_status=published，report与磁盘JSON相等 | 0 |
| 参数、JSON/UTF8、字段/版本或不安全路径无效 | stdout为空，stderr稳定CLI_INVALID_ARGUMENT、CASE_*或PROJECT_UNSAFE_PATH | 2 |
| 既有目标、输入输出祖先重叠、缺失父目录或真实读取占用/I/O | stdout为空，stderr为COMPILE_OUTPUT_EXISTS、COMPILE_INPUT_OUTPUT_OVERLAP、PROJECT_IO_ERROR或PROJECT_DATABASE_BUSY | 3 |
| fixture语义或有限渲染预检失败 | JSON保留顶层checks/issues，output_created=false、release_status=not_started；run_id/release_id/report及产物路径为空，stderr为CASE_COMPILATION_FAILED | 3 |
| 拥有根后渲染、实际输出验证或登记失败 | 可确认的结果JSON和问题，release_status=staged或recovery_required；stderr为CASE_COMPILATION_FAILED，保留现场 | 3 |
| 内部编码或stdout输出失败 | stderr稳定COMPILE_REPORT_ERROR或CLI_OUTPUT_ERROR；无伪造成功输出 | 1 |

渲染或初次暂存尚未完成时可保留staged；暂存/Vault实际验证、报告或发布登记失败为recovery_required。初期失败若能保存报告，使用`reports/validation.json`与`.md`记录`result=failed`；已保存validated快照后再失败则另存`reports/failure.json`与`.md`，保留原快照。报告对无法完整保存时不提供虚构的双报告路径；报告或失败状态登记无法完成时，仅在仍能确认本轮目录身份的情况下独占写`state/recovery.json`保存诊断。未知内容和替换路径保持原样，建议用户检查现场并选择新的测试根重试。

最后提交之后若调用方关闭stdout，CLI仍可能退出1并报告CLI_OUTPUT_ERROR，已经提交的数据库发布依据必须保留；不能把传输失败反写为Run.failed或取消Publication。中断现场不自动接管或清理，受控故障检查不扩大为任意断电下多文件原子恢复保证。

### 摘要示例

```text
结果：fixture 编译验证通过
资料：2 份，处理范围与 case 清单一致
来源覆盖：完成
知识编译：完成
质量验证：完成，仅含本轮确定性检查
可选研究：不适用
发布：本轮 SQLite 发布事务已提交
报告：<报告路径>
产物：<临时 Vault 路径>
```

此处为成功compile结果的阅读示例，CLI实际输出为JSON。数量、状态和路径必须来自本次运行及已提交依据；当前来源命令没有这些完成状态或Vault产物。科学质量仍为not_run，不能把确定性质量验证完成描述为事实核验通过。

## 4 四类完成状态

| 维度 | 应表达的信息 | 禁止做法 |
| --- | --- | --- |
| 资料覆盖 | 总单元、处理、排除、失败和待复核 | 将排除、失败合并成已处理成功 |
| 知识编译 | 必要原子、实际输出映射、缺失原子 | 计划中有 ID 就算已输出 |
| 质量验证 | 具体检查及其版本，未运行项 | 将语法可渲染说成事实正确 |
| 可选研究 | 批准范围、完成与剩余目标 | 用未来候选掩盖本轮必需缺口 |

分母为零显示“不适用”，不显示100%。如果来源数量在后续研究中增加，显示新增原因和范围版本；不要制造连续单调的假进度。

首阶段可以只显示当前阶段和计数，不提供预计剩余时间。后续 ETA 没有可靠依据时显示“尚无法估算”。长时间无进展要说明等待对象，不能一直显示模糊的“处理中”。

当前来源清单只展示实际Span数量、待抽取数量、解析问题及pending知识状态，不提前套用完整CoverageRecord或四维成功报告。无效UTF8和未读全文件没有规范Span；0段不能解释为知识工作已完成。compile四维分别使用source_coverage、knowledge_compilation、quality_validation、optional_research，每项有state、reason和counts；前三维只有各自所需检查实际完成才为complete，未完成为incomplete。本轮未启用研究，optional_research固定not_applicable；scientific_quality检查固定not_run，不列出虚构研究成果或模型价格。

## 5 报告结构

当前来源报告为stdout JSON，事实来自持久SQLite记录及快照重读，使用import-id再次查看，不生成Markdown双报告。调用方可将stdout重定向为文件；诊断走stderr，不要求从彩色摘要解析机器状态。报告保留失败字段，不能把inspect操作成功改写为来源内容成功。

B02-T03的JSON和Markdown报告由同一个typed [CompileReport](../../crates/noetome-cli/src/compile_model.rs)快照生成，[compile_report.rs](../../crates/noetome-cli/src/compile_report.rs)提供Markdown视图。Schema1包含schema_version、producer、case_id、run_id、release_id、started_at、finished_at、result、dimensions、checks、counts、issues、files、input_hashes、versions、publication。stdout内report逐字段等于磁盘JSON，不能分别拼装两套数量、状态或文件清单；不向来源登记添加虚构run_id或Vault产物。

Markdown固定标题为“Noetome fixture 验证报告”，先列Schema、生产者、case/run/release身份、时间和result，再依次列“四维状态”“计数”“检查”“文件”“内容映射”“语义引用”“问题”“输入与规则”“发布依据”。表格保留稳定英文键；scope计数区分overall与各维度，问题逐条保留rule_id、object_refs及message。文件表列relative_path、note_id、plan_revision、byte_length和content_sha256；正文映射列出实际UTF8半开区间、scope与atom_ids，语义引用单独列出完整scope、目标概念、路径、heading、别名及区间。scope包括kind、item_id、row_id、column_id，写为`kind:item_id:row_id:column_id`，空字段保留占位；不能借用别名范围证明正文已经显示必要内容。

input_hashes固定case_sha256、ir_sha256、plan_sha256、expected_sha256，versions保留实际规范化/解析/覆盖政策、IR/计划Schema、validator/report及renderer版本；当前renderer为fixture-markdown-v1。必需检查为sources、coverage、knowledge、note_plan、expected、rendering、staging_output、vault_output，发布前须全部passed且问题为0。输出清单、映射、内容与引用还分别列出output_manifest、output_mapping、output_content、output_link，不以总计掩盖失败。

报告的“发布依据”只写`database=state/compile.sqlite`、`commit_table=publication_commits`和release_id，并明确验证通过与提交发布的关系。实际数据库提交记录还绑定两份报告SHA及文件manifest SHA；validated报告没有已提交记录时仍不得称published。

失败项至少包含对象、失败原因、影响和建议动作。例如“第2份来源不是有效 UTF-8；该资料未进入知识编译；请转换编码或移除此份测试输入”。不得写“发生未知错误，请重试”作为唯一说明。

输出路径应可复制或点击。报告之外的任何“可打开产物”链接必须来自已存在且已核对的文件，不链接尚未创建的计划路径。

## 6 中文与术语

正文使用简体中文、完整句子和清晰主语，功能名保持统一。代码类型、文件名和稳定枚举保留英文，不把不同状态翻译成同一个词。

| 术语 | 界面推荐表达 |
| --- | --- |
| Knowledge IR | 详细信息中为“知识模型”；技术视图可显示 IR |
| NotePlan | 笔记计划 |
| SourceSpan | 来源单元或原文片段 |
| CoverageRecord | 来源处理记录 |
| staged candidate | 待验证候选 |
| published | 已发布 |
| needs_review | 待复核 |
| blocked | 受阻；必须同时说明原因 |
| not_applicable | 不适用 |

区分“验证通过”和“事实核验通过”；区分“排除”和“删除”；区分“执行结束”和“任务完成”。面向用户的“来源”说明原始资料，不能误写成代码仓库。

不要使用“绝对无遗漏”“完全无损”“已掌握全部知识”一类没有证据的结论。具体通过哪些样例就写哪些样例，不把 fixture 数量夸大为产品能力。

## 7 产物视觉规则

首闭环 Markdown 用清晰标题、短段落、必要列表和适量表格。视觉形式必须服务于比较、顺序或关系，不为增加图表数量重复全文。

- 同类型笔记使用一致的标题习惯；不规定每篇字数或知识量硬上限。
- 比较表保留单位、条件和例外；列过多时拆分，不靠删内容压缩。
- 正文中的语义链接由结构化SemanticLink解析到canonical文件和标题，不让用户手动猜测目标；fixture提供链接意图，当前不自动选择表达形式。
- 必需信息不能只靠颜色、位置或折叠状态传达。
- YAML 只放简短属性，不把整个证据数据库塞入笔记头部。
- 来源不默认占据正文，但争议和适用范围不能因为“干净风格”而被隐藏。
- 当前公式、Mermaid、嵌入、代码块和自定义HTML没有结构化渲染支持，不宣称已完成其视觉兼容性测试。

fixture-markdown-v1只生成`#`笔记标题、`##`小节、段落、从`1.`起始的有序列表、完整表格和本地wiki标题链接。结构标记不转义；明文中的ASCII标点逐个加反斜线，中文标点及`5 cm`等内容保留。正文或单元格换行使用固定`<br>`、Tab使用`&#9;`，来源本有这些字面字符时先转义，避免二次解释。每块后留空行，笔记以两个LF结尾；不把run_id、绝对输出根或时间写入正文，保证同fixture两次运行的笔记字节稳定。

SemanticLink按声明顺序追加在所属段落、列表项或单元格末尾，正文与链接之间为一个空格。目标使用完整Vault相对路径去掉`.md`及精确标题，例如`[[笔记/乙容器#定义与条件|乙容器]]`；表格中的别名分隔符为`\|`，例如`[[笔记/乙容器#定义与条件\|乙容器入口]]`。标题与别名拒绝换行/Tab，别名拒绝`[]|`分隔字符；本版canonical标题不支持反斜线，预检返回RENDER_TEXT_SYNTAX，不静默改写。普通`x < 2`和`[1,2]`仅作为明文呈现，不承诺MathJax功能。

[真实CLI测试](../../crates/noetome-cli/tests/compile.rs)当前覆盖12组、35次调用，包括固定两笔记/表格、表内别名、字面转义、两目录独立run/release、JSON/Markdown/SQLite对账及Windows锁/junction/外部哨兵保护。这些工程检查不替代原生Obsidian显示与交互验收；原生验收的当前结果由实际验证记录维护。

后续主动图表应能说明选择理由，并映射到知识依据。生成图的箭头、表格空值、图例和单位须可核对；“看起来更好”不能覆盖语义错误。

## 8 后续桌面交互

当前桌面骨架包含工作台、运行检查和关于页面，使用用户选定图标派生的本地资源。工作台保持真实空状态，尚未接入的项目初始化、CLI来源登记/查看以及未实现的知识生成不提供假成功入口；CLI能力不能被描述为桌面已经能创建或打开项目、选择文件或登记来源。运行检查在实际Rust调用后显示版本、能力和命令结果；受限cli_preview只接受help/version，帮助中出现source命令不代表GUI可以执行。未运行、运行中、成功与失败分别表达。

B01A-T01的窗口行为遵循用户确认：最小化按钮保持普通Windows最小化，任务栏仍保留窗口；右上角 × 在未配置时默认隐藏到托盘，保留进程、导航和已有运行输出。左键单击既有应用图标的托盘，或右键菜单选择“显示窗口”，取消最小化、显示并聚焦原窗口。托盘菜单“退出”始终结束程序并移除图标，不受关闭窗口设置影响。托盘创建失败必须显示稳定错误并停止启动；隐藏或恢复失败须报告原因与可执行建议，不能把用户留在无恢复入口的隐藏窗口。

关于页提供一个“窗口行为”设置区，“关闭窗口时”可选“隐藏到托盘”或“直接退出”。打开设置和失败后的“重新读取”都从实际文件重读；成功后显示该次读取的已生效值，保存成功后再显示已保存，重启后保留选择。首次读取失败时显示错误与“重新读取”入口，用户修正设置或访问条件后可以重试恢复。加载和保存过程有明确状态、可见标签及键盘焦点；失败提示具体原因，保留原有已保存值与当前行为，不能用浏览器默认值或未完成的读取、保存制造成功状态。启动读取失败且尚无有效值时须明确报告，并安全采用隐藏到托盘，设置区仍显示读取失败。此设置只控制 ×，不改变最小化按钮和托盘“退出”的含义。

安装后须分别验证默认 × 隐藏及进程存活、普通最小化的任务栏窗口、左键与菜单恢复、导航/输出保留、选择直接退出后的 × 行为及重启持久化、改回隐藏后的行为、托盘明确退出与快捷方式重启。设置的加载/保存失败、修复后重新读取及键盘操作另有检查。实际状态由任务账本及[当前关闭设置验证记录](../validation/B01A-T01-settings-refresh.md)维护，不以构建、静态检查或旧版安装验收替代。

界面使用本地字体和静态资源，导航和操作按钮有可见键盘焦点。桌面安装验收必须在安装后的应用内执行，检查窗口、导航、Rust通信、帮助与版本、焦点、关闭和重启；静态前端预览不能替代安装后测试。

桌面前端复用 core 的状态和错误，不在前端根据计时器推测任务成功。创建任务的主要字段是资料、目标、风格、模式、输出位置和预算；IR、哈希和缓存键只在详情中出现。

任务中心分别呈现运行状态、可用结果、待审批变更和研究候选。候选预览应有明确标签，与已经发布的笔记区分。局部结果成功不改写全库总状态。

来源侧栏从当前版本内容映射进入证据。映射因外部编辑过期时显示“需要重新匹配”，不高亮到近似位置并假称精确定位。

审批按具体差异、理由和影响组织。接受操作绑定当前指纹与候选版本；外部文件变化后，界面必须显示审批已失效。关闭普通审批不提供覆盖人工内容的后门。

所有主要操作可使用键盘，焦点可见。状态同时用文字表达，不能只用颜色。长公式、图片和表格可放大、滚动或查看原图，不通过裁切隐藏重要内容。

后续接入持久任务时，“后台继续”和“安全暂停后退出”要有不同、可理解的文案，并明确关闭窗口的选择。B01A的托盘与关闭方式设置只管理当前窗口和进程状态，尚未引入驻留任务。对用户未启用的自动运行、网络或电源策略，不默默代替其选择。

## 9 设计检查

每次影响 CLI 或报告的变更都检查：状态是否来自真实数据，失败是否可定位，摘要能否说明产物，是否出现假百分比、假价格、未实现能力或泄露内容。

桌面端加入后再增加对应页面的截图和键盘检查。界面原型、静态截图、浏览器预览不替代实际运行验证；测试要求见 [测试标准](TESTING.md)。
