# Noetome

面向个人学科学习的知识编译器，将多份电子资料整合为可追溯、可维护的 Obsidian 知识库。

默认开发分支为 `main`；已验证开发历史和侧栏/设置改动已合入，GitHub 默认分支及远端 HEAD 已实际核对。[本次验收](doc/validation/B05U-T01.md)保存分组侧栏、分类设置、太阳/月亮主题持久化、浏览器与安装版证据。

Rust CLI提供空项目初始化、单份MD/TXT/PDF来源登记与只读重查、文本PDF解析/重解析、fixture IR与NotePlan校验及新测试Vault编译，以及单请求文本API候选、有界来源知识核对和明确身份的多来源重复/互补协调。来源版本、PDF解析尝试、真实位置与原始资源独立保留，来源知识仍pending；fixture明确标作者输入，API候选工程通过与来源核对、科学质量和发布分别表达。现有pdfplumber sidecar用于有界文本PDF，扫描和复杂版面保守需复核。桌面提供帮助、版本、本地连接检查、单行菜单及托盘/关闭设置；项目、资料和知识业务仍通过CLI。能力验收以[任务日志](dev-log/STATUS.md)和实际验证记录为准。

比较表、流程、公式、选定PNG图像及单个明确扫描公式的有界预览已保留来源和独立语义核对。Windows原生兼容实测涵盖最低Obsidian1.6.7与2026-11-06核验的稳定1.15.0、明暗主题共64个组合；公式分别使用MathJax3.2.2/4.1.3，当前流程使用实际Pele1.8.0。未知组合仍有据回退或阻断，精确策略见[视觉能力契约](doc/technical/VISUAL_COMPATIBILITY.md)及[实际矩阵](doc/validation/assets/B06V-T03/matrix.json)。这些工程预览不代表科学质量、通用OCR或生产Vault维护已验收。

## 开始开发

- [开发指引](AGENTS.md)：每轮工作规则及标准路径。
- [标准文件入口](doc/README.md)：需求、技术、设计、测试和执行规范。
- [近期开发步骤](doc/execution/NEXT_STEPS.md)：仅展开下一批最多3项，每轮推进一项。
- [当前状态](dev-log/STATUS.md)、[待办事项](dev-log/TODO.md)、[已完成事项](dev-log/DONE.md)：由任务账本自动生成。
- [详细需求](doc/requirements/PRD.md)及[HTML阅读版](doc/requirements/PRD.html)。

原待决策事项已按建议采纳，详见[决策基线](doc/requirements/DECISIONS.md)。根据2026年11月1日的用户要求，提前建设可本地安装的桌面骨架，并插入托盘与关闭方式设置交付；后续在CLI空项目基础上完成文本来源和确定性输出闭环，再逐步接入PDF、模型及完整桌面功能。

## 构建和运行 CLI

在仓库根目录运行。已验证 Windows x64，`rust-toolchain.toml` 锁定 Rust 1.98.1，并安装 rustfmt。需要 Rustup 和可用的 MSVC C/C++ 编译、链接环境；首次获取工具链和锁定依赖需要网络。CLI 使用随 rusqlite 编译的 SQLite，不要求单独安装 SQLite；单独构建 CLI 时不编译 Tauri。

```powershell
cargo build -p noetome-cli --locked --offline
cargo run -p noetome-cli --locked --offline -- --help
cargo run -p noetome-cli --locked --offline -- --version
```

构建后的 Windows 程序位于 `target/debug/noetome.exe`，当前版本为 `0.1.0`。无参数、`--help`（`-h`）显示帮助；`--version`（`-V`）显示版本。帮助和版本每次仅接受一个选项；项目、来源和case命令用法见下文。未知或多余参数返回退出码 `2`、在标准错误输出 `CLI_INVALID_ARGUMENT` 和用法提示。输出失败返回 `1` 和 `CLI_OUTPUT_ERROR`。帮助、版本及无效语法不创建项目、数据库或用户配置。

本机 Low 完整性构建树中的原生 C 编译器无法使用默认系统临时目录时，可改用 `./scripts/cargo_local.ps1`，其余参数与 Cargo 相同。脚本为编译选择 `target/compiler-temp`，结束后恢复调用者的临时目录环境，不修改权限。

普通构建/check不启动Python。运行完整Cargo/Python测试前须建立[锁定PDF环境](sidecars/pdf_parser/README.md)，将其绝对解释器路径赋给NOETOME_PDF_PYTHON；fake进程测试另外用NOETOME_TEST_PYTHON选择可信解释器。直接adapter unittest使用运行unittest的Python，亦须匹配锁定版本；没有设置时真实测试明确失败，不自动下载或跳过。

```powershell
./scripts/cargo_local.ps1 check --workspace --locked --offline
./scripts/cargo_local.ps1 test --workspace --locked --offline
```

```powershell
cargo fmt --all --check
cargo check -p noetome-cli --locked --offline
cargo test -p noetome-cli --locked --offline
```

进程测试使用 `target/cli-tests/` 内独占的临时目录，比对运行前后的目录和文件字节，测试结束自动清理。环境、实际结果及本轮遇到的问题见 [B01-T01 验证记录](doc/validation/B01-T01.md)。

## 空项目初始化与检查

```powershell
New-Item -ItemType Directory -Path ./target/manual-projects -Force
./scripts/cargo_local.ps1 run -p noetome-cli --locked --offline '--' project init './target/manual-projects/学习项目'
./scripts/cargo_local.ps1 run -p noetome-cli --locked --offline '--' project inspect './target/manual-projects/学习项目'
```

PowerShell 调用脚本时须把传给 Cargo 的分隔符写为 `'--'`，避免未加引号的分隔符被脚本参数解析消耗。

init 的父目录须已存在，目标目录须尚不存在；已有空目录、已有项目或未知内容不会被接管。配置 `project.json` 和数据库 `project.sqlite` 保存同一个 UUID v4、Schema 1、UTC 创建时间和 `initialized` 状态。数据库位置为项目相对路径，项目搬移后身份不变。

inspect 核对实际配置和 SQLite 状态，支持严格Schema 1/2，不执行迁移、修复或事务恢复；重复运行不改写项目文件。配置、数据库metadata及user_version须一致。无效契约或不支持的 Schema 返回 `2`，已有目标、初始化中断、数据库占用或文件访问失败返回 `3`，并显示稳定错误码。参数语法错误仍返回 `2`。

初始化中断时，标记、暂存文件或缺失的配对文件会被明确报告并保留。已经确认安全回滚的普通失败可在原路径重试；保留的中断目录需先检查，当前恢复方式是另选新目录初始化空项目。当前没有原目录自动续写或生产任务恢复。以 `-` 开头的项目路径前可加 `--` 分隔符，使用 `noetome project --help` 查看说明。

本机原 `target/` 可执行文件继承 Low 标签，不能向普通 Medium 用户目录写入；测试示例因此使用构建树内的新目录。向正常用户目录创建项目时，须从正常输出位置的 CLI 副本运行。不得用用户生产 Vault 验证本轮能力。

Windows 路径、文件占用、真实初始化中断与恢复测试及命令输出见 [B01-T02 验证记录](doc/validation/B01-T02.md)。

## 文本来源登记与检查

只读用户明确选取的一份MD/TXT，不扫描目录。下例中的文件须由执行者准备；使用新的测试项目，避免以生产Vault验证功能。

```powershell
./target/debug/noetome.exe source --help
./target/debug/noetome.exe source import './target/manual-projects/学习项目' './target/manual-projects/自造来源.md'
./target/debug/noetome.exe source inspect './target/manual-projects/学习项目' '<上次输出的import_id>'
./target/debug/noetome.exe source import './target/manual-projects/学习项目' './target/manual-projects/自造来源.md' --source-id '<既有source_id>'
```

成功输出纯JSON，包含`import_id`、`source_id`、`version_id`、`status`、`reused`、版本和完整Span清单。报告`schema_version=1`是JSON接口版本；首次导入将项目配置与数据库从Schema 1升级到2，保留项目身份、创建时间和`initialized`基础状态。该状态只表示项目已经建立，不能代表项目仍为空或知识生成完成。

单文件上限64 MiB。规范化仅移除首个BOM并将CRLF/CR统一为LF；不替换无效UTF8或删除未理解文字。标题、文本、围栏代码和空白Span无缝覆盖全部规范UTF8字节，以半开字节区间和1起始的包含式行号定位；空文件0段，未闭合代码围栏保留内容并标为`needs_review`。`span_count`和`pending_extraction_count`均为实际Span数量，`knowledge_status=pending`，不表示知识覆盖率。

未指定ID时优先保持此前同一选取路径的来源关联，否则复用已有同字节来源；相同字节再次导入或重命名不重建版本/Span，每次仍生成独立导入记录。显式`source_id`固定既有来源身份，再在该来源内部复用或增加修订，不因SHA相同合并另一来源。变化创建新的`version_id`，旧快照和位置保持不变。内部快照固定在项目内`.noetome-sources/<version_id>/`，owner绑定项目ID，路径为项目相对定位。

完整读到但UTF8无效时，保存`failed`版本及真实原始SHA、长度和`raw.bin`，规范字段为null且Span为0；未读全、文件占用、不存在或超过上限时，只登记失败尝试，不制造版本或截断文本。失败记录成功持久化时stdout仍有JSON，stderr显示稳定错误码，导入退出`3`。非法身份、类型、路径或契约在写入前拒绝，stdout为空、退出`2`；文件访问或受保护写入失败退出`3`。

`source inspect`重读持久记录、真实快照和全部区间；记录本身为`failed`但完整有效时查看退出`0`。查看保持只读，不访问原文件来重建缓存，不迁移或修复项目。缓存相对定位支持项目搬移，原文件重新定位及可用性状态仍属后续功能。

升级和导入使用独占`.noetome-writing`目录保存候选、旧配置/数据库备份及操作说明，发布前后重读验证。普通失败只有在身份和字节仍属于本次写入时才恢复与清理；中断、未知内容、替换或无法确认的清理保留现场并报告未完成。遇该目录时公开inspect拒绝，不能把事务残留显示为成功。目前未提供自动恢复入口，也未承诺任意外部并发或任意断电下多文件原子提交。

本机Windows完整来源验证、失败保护、独立SQLite/快照检查和正常完整性CLI副本实测见[B02-T01验证记录](doc/validation/B02-T01.md)。可用`scripts/verify_text_source.py`按记录中的命令在另一个全新测试根复现；输入全部自造，不调用模型，不生成IR或笔记。

## 已保存API候选的来源知识核对

单请求文本API及用户已保存的网关、模型、外部凭据引用和累计额度见[模型服务](doc/requirements/MODEL_SERVICE.md)与[API契约](doc/technical/EXTRACTION_API_CONTRACT.md)。保存配置与累计账本可复用，不把密钥写入仓库，不在每轮重置预算。

```text
noetome extract verify <项目路径> <import-id> <review-case> <全新输出根>
```

review-case严格包含review、原候选、独立作者gold和纳入/排除依据；单个实际来源单位、UTF8引文及一个受控测量事实逐项核对概念、内容、数值、条件、单位、例外和覆盖。只有全部通过才生成未发布的source_review IR，原atom仍unverified/required=false。失败报告保存实际分母和逐对象诊断，JSON/Markdown同事实，已有输出或人工新增文件不覆盖。普通verify不读取模型服务/密钥/预算，也不发API请求。

[B04-T03实际验证](doc/validation/B04-T03.md)包含原第六保存模型候选、调用前gold与源SQL/字节核对，以及负例、独立校验器和完整回归。本能力仅支持契约声明的有界受控语法，scientific_quality=not_run、knowledge_status=pending、published=false；它不代替正式准确率、完整自然语言/PDF或知识发布。输入字段、原子分母及保护规则见[来源核对契约](doc/technical/EXTRACTION_REVIEW_CONTRACT.md)。

## 从已核对知识主动生成比较表

```text
noetome knowledge present <项目路径> <coordination-case> <已保存协调输出> <尚不存在的测试Vault>
python scripts/verify_knowledge_presentation.py check <项目路径> <coordination-case> <已保存协调输出> <测试Vault> <独立expected>
```

入口重新核对实际来源及已保存的 v1/v2/v3 协调结果。多个明确对象共享属性时主动选择比较表，否则使用正文；完整保留原值、单位、条件、例外、有据时间与原出现映射。缺失格显示“选定事实未给出”，未决协调不会生成成功计划或笔记。

全新测试 Vault 保存 `知识比较.md`、对象详情、`plan.json` 和 `presentation.json`/`presentation.md`。表内链接定位固定的知识明细小节，出处保存在内部索引。成功仅表示有界工程预览，知识仍为 pending、科学质量 not_run、发布 false；不写生产 Vault。范围和输出契约见[有据主动表达](doc/technical/ACTIVE_PRESENTATION_CONTRACT.md)。

[B06-T01 实际验收](doc/validation/B06-T01.md)保存独立篡改核对、完整 Windows 回归、原件保护及真实 Obsidian 表格/正文/链接结果，包含多行名称误拒的发现、修复和双实现回归。

## 明确身份的多来源重复与互补协调

```text
noetome knowledge coordinate <项目路径> <coordination-case> <全新输出根>
python scripts/verify_knowledge_coordination.py check <项目路径> <coordination-case> <保存输出根> <独立gold>
python scripts/verify_knowledge_coordination.py audit <实际CLI绝对路径> <全新测试根>
```

case明确选取同项目2至3份已通过的单原子source_review，并给出对象/属性的规范身份映射。协调重新打开实际来源、原gold/候选、IR和双报告；完整值、单位、条件与例外相同才去重，所有原ID和每个出处仍保留。正常作者样例中两个速度事实归并、第三个质量事实独立保留，形成2个事实和3个实际来源出现；这是自造结构样例的预期，实际验证见[B05-T01](doc/validation/B05-T01.md)。

成功保存未发布coordination-ir.json以及coordination.json/coordination.md。未处理的条件、单位或来源版本差异返回待复核、保留原组，canonical_facts为null且没有成功IR。显示名称或排序不改变身份，相同资源SHA也不自动合并另一已登记来源；同一实际来源单位不能重复计数。独立Python从作者预期与真实SQL/资源重建结果，再核对全部保存内容。范围、严格字段、错误分类和保护限额见[协调契约](doc/technical/KNOWLEDGE_COORDINATION_CONTRACT.md)。科学not_run、研究not_requested、知识pending、published=false；正式条件/历史变体及冲突分类有后续独立任务。

## 离线 fake 知识候选验证

用仓库的[受控作者样例](fixtures/extraction-gateway-v1/README.md)导入真实文本，再对该import-id重放fake候选。输出父目录须已存在，目标为新测试根。

```powershell
./target/debug/noetome.exe extract --help
./target/debug/noetome.exe source import '<测试项目路径>' './fixtures/extraction-gateway-v1/source.txt'
./target/debug/noetome.exe extract fake '<测试项目路径>' '<上次返回的import_id>' './fixtures/extraction-gateway-v1' '<尚不存在的测试输出根>'
```

fake.json明确选择Span ordinal或已登记PDF页/区域，并声明一次请求的能力、输入/响应/输出边界、时限和预算。Core只读核对真实来源身份、版本、资源SHA、UTF8范围、quote、严格候选及条件/单位；不更新来源SQL或pending，不写生产Vault。

保存request.json、原响应response.bin、仅成功时的candidates.json与validation.json/validation.md。双报告包含相同的来源、状态、实际fake计量、错误和文件指纹。成功`candidates_validated`仍保持原子unverified、知识pending、published=false、科学质量not_run；fake使用UTF8字节计量，真实模型用量和价格未知，网络及付费调用为0。

旧输出、路径重叠或越界拒绝。预算不足、价格未知而设置金额上限、能力不足、无效候选、超时和取消返回非零，失败不保存已验收候选。`--cancel-file`指定输入/输出根外的普通停止信号，可在运行期间创建。契约与独立普通CLI验证见[抽取契约](doc/technical/EXTRACTION_CONTRACT.md)及`scripts/validate_extraction_gateway.py`。

## 只读 fixture 契约校验

仓库提供[自造案例](fixtures/text-compare-v1/README.md)：两个虚构容器、12个完整来源Span、4个必要原子、两份计划笔记和比较表。来源、IR、计划与独立expected均明确声明`producer=fixture`，不会被当成模型抽取结果。

```powershell
./target/debug/noetome.exe case --help
./target/debug/noetome.exe case verify './fixtures/text-compare-v1'
```

`case verify`保持只读，不创建项目、数据库、Run、Release或Vault。case根内四份JSON和明确选取的1至32个MD/TXT必须是普通文件；每份JSON最多4 MiB，总输入最多64 MiB。相对路径拒绝越界、Windows保留名、大小写复用及reparse/junction；输入版本、必填字段、未知字段、枚举和范围严格校验。

成功stdout为纯JSON、退出0；语义问题输出`passed=false`、问题规则及对象引用，stderr为`CASE_VERIFICATION_FAILED`、退出3。无法解码或输入契约无效时stdout为空、退出2，文件访问或变化阻断退出3。JSON分别列出`sources`、`coverage`、`knowledge`、`note_plan`、`expected`；`rendering`和`scientific_quality`保持`not_run`，不能用契约通过宣称笔记已经生成或科学质量合格。

校验器核对原始/规范SHA与完整分段，Span和覆盖记录双向一一对应，重复记录保留自身证据并拒绝环；必要原子具有有效支持片段和已审阅状态。计划固定完整IR字节指纹，每个显示scope须完整包含原子内容、条件与单位；规范位置须实际显示独立expected要求的定义原子。expected锁住必要原子、完整证据定位/指纹、覆盖、规范位置、必需链接和比较表，不能靠计划中ID齐全蒙混过关。显示文本使用明文与结构化链接，拒绝原始wiki链接、HTML和隐藏注释；正常数学明文`x < 2`和`[1,2]`可用。

结构Schema见[fixture-v1.json](schemas/fixture-v1.json)，跨文件语义由Rust继续校验。开发时可在独立工具目录安装`jsonschema==4.23.0`，运行以下检查；它不是产品或离线Cargo测试的依赖，不修改全局Python环境。

```powershell
python -m pip install --target '<新工具目录>' jsonschema==4.23.0
python scripts/check_fixture_schema.py --module-dir '<工具目录>'
python fixtures/text-compare-v1/audit_fixture.py
```

实际Windows只读CLI、故障保护、审查回归和Schema验收见[B02-T02验证记录](doc/validation/B02-T02.md)。

## Fixture 临时 Vault 编译

```powershell
./target/debug/noetome.exe case compile './fixtures/text-compare-v1' '<父目录已存在的全新测试输出根>'
```

`case compile`只接受尚不存在的新根，拒绝既有内容、输入输出重叠和不安全路径。通过同一fixture的来源、覆盖、知识、计划和独立expected检查后，按计划渲染两份关联笔记、比较表及标题链接；重读暂存与Vault中的实际字节，核对单位、条件、映射、SHA和全部本地引用。它不读取用户生产Vault，不调用模型或PDF解析器。

新根下`vault/`为可单独打开的笔记，`staging/vault/`为候选，`reports/validation.json`与`.md`共享真实验证事实。独立`state/compile.sqlite`为编译Schema1，与来源项目Schema2分开；最后单个事务绑定本轮文件和双报告指纹，更新Run、Release及完整Coverage。报告`validated`表示验证快照，发布依据为已提交的`publication_commits`，成功stdout外层才为`release_status=published`。

实际内容或发布登记失败保留`staged`或`recovery_required`现场并返回非零，不覆盖已有报告和人工内容。若验证报告已写入，后续错误使用`failure.json/.md`；无法保存完整报告或失败状态时只在仍能确认归属的目录保存恢复诊断。没有重试接管或自动恢复原目录的入口。

支持范围是普通标题、有序列表、比较表、明文转义及完整相对路径的wiki标题链接；表格别名使用`\|`。科学质量为`not_run`，可选研究为`not_applicable`。实际CLI、进程终止、SQLite失败和Obsidian 1.6.7有限语法验收见[B02-T03记录](doc/validation/B02-T03.md)。可在另一个新根复现：

```powershell
python scripts/verify_fixture_compile.py --cli '<实际可读CLI路径>' --root '<父目录已存在的新测试根>'
```

## 固定 fixture 流程的持久检查点

B07-T01 的有限 Run 接口复用明确fixture输入与既有编译/语义核对，使用新运行根保存六单元进度。命令与恢复边界见[运行契约](doc/technical/RUN_CHECKPOINT_CONTRACT.md)，实际验收状态仍见[任务账本](dev-log/tasks.json)与[本项执行计划](doc/execution/B07-T01_RUN_PLAN.md)。以下路径占位符须替换，运行根父目录先存在，初次运行根必须不存在。

```text
noetome run start fixtures/run-checkpoint-v1/fixture <new-run-root> --paused
noetome run status <owned-run-root>
noetome run resume fixtures/run-checkpoint-v1/fixture <owned-run-root>
noetome run pause <owned-run-root>
noetome run cancel <owned-run-root>
```

`control/run.sqlite`保存原Run/Release、输入/版本、工作单元、控制请求与检查点；`output/state/compile.sqlite`保留原六表及唯一发布依据，阅读文件位于`output/vault`。恢复重验原身份、完整字节、来源语义与状态迁移，复用确认结果，不接管旧case compile或陌生Vault。输入/版本变化、未知/损坏或外部替换现场保留拒绝；只有明确拥有且通过前置验证的有限journal可在显式resume中交给SQLite正常回滚。

暂停请求与安全确认分开显示，在途工作可继续落盘；取消保留确认结果且不能继续调度或撤销已提交publication。完成前保存的迟到控制显示 `completed before control took effect`；完成后的resume核对结果或补该控制确认，不增加工作尝试或发布。该接口不调用模型/HTTP或使用生产资料，不表示桌面后台任务、跨平台搬移、24小时或生产Vault恢复已经验收。

## Parser sidecar 开发入口

`noetome_cli::sidecar_process::run` 接收可信程序与原生argv、明确单文件、尚不存在的工作区、限额和取消令牌。Core保持输入和目录守卫，并复核唯一JSON响应、身份、来源单元、资源实际字节和精确清单；响应及诊断超限、超时、取消或管道失败返回分类错误并收束受控进程树。wire与默认政策见[协议](doc/technical/SIDECAR_PROTOCOL.md)，验收范围见[B03-T01](doc/validation/B03-T01.md)。

测试helper仅用Python标准库，在测试构建中允许`producer=fake`；普通库入口拒绝fake，CLI没有fake或任意parser命令覆盖参数。运行进程组前选择可用Python解释器；若使用Codex工作区运行时，把工具返回的绝对路径赋给`NOETOME_TEST_PYTHON`，不在仓库写死开发机路径。

```powershell
./scripts/cargo_local.ps1 test -p noetome-cli --lib sidecar_process::tests --locked --offline
./scripts/cargo_local.ps1 test -p noetome-cli --test sidecar --locked --offline
```

进程与资源保护当前只实现Windows。Job Object管理进程树，不提供文件或网络沙盒；协议测试的合成单元不证明真实PDF页、区域、阅读顺序或科学质量。错误工作区保留可定位现场。下述PDF入口另使用真实锁定adapter和独立作者预期验收。

## 单份文本 PDF 解析沙盒

先按[adapter安装和许可说明](sidecars/pdf_parser/README.md)创建新的CPython 3.12.14 Windows x64环境，按requirements-lock.txt的全部精确版本和wheel SHA安装。运行时不自动安装依赖；调用者只设置可信解释器，PDF内容不选择命令、代码或输出路径。

```powershell
$env:NOETOME_PDF_PYTHON = '<锁定测试环境的绝对路径>/Scripts/python.exe'
./target/debug/noetome.exe source parse-pdf './fixtures/pdf-text-v1/single-page.pdf' './target/compiler-temp/pdf-example-one'
./target/debug/noetome.exe source inspect-pdf './target/compiler-temp/pdf-example-one'
```

目标的父目录须存在且根尚不存在。本机target的Low标签限制同样适用，示例使用构建树内的新测试根。parse仅保存parser/raw.pdf、逐页UTF8、PDF manifest及Core核对后的parse-result.json/.md；不写项目SQLite或Vault。原文件移动或删除后inspect仍可根据已保存证据重查，不修复残留或接管既有目录。

每页有零基physical_page_index、一基display_page_number和未知printed_page_label=null；区域为显示CropBox左上原点、Y向下的pt bbox，保留原MediaBox/CropBox、Rotate/UserUnit及变换矩阵。UTF8半开范围只绑定该页文本artifact和SHA，正文范围不含行尾LF；原始PDF二进制偏移和旧blocks-v1全局行号不能替代这些位置。

全部页面parsed且无blocking问题才退出0。扫描无文本层、未知空白、表格/公式/图像、多栏、非水平/旋转文字等needs_review仍保留物理页和raw证据，退出3；损坏、加密、读取/资源/位置/时限/取消失败同样非零。未确认完整原始或页数时保留null/已确认前缀，不造完整SHA或页面清单。inspect同样保留needs_review/failed的非零状态。宿主失败现场可有failure.json，不能作为完整沙盒读取成功。

本阶段政策为最多128页、每页50,000字形、总200,000字形，默认单raw资源16MiB，另受[sidecar字节/进程限额](doc/technical/SIDECAR_PROTOCOL.md)约束；这些是上限而非吞吐实测。适用支持矩阵、实际小样例时间/资源和已执行故障见[B03-T02验收](doc/validation/B03-T02.md)，精确字段见[PDF契约](doc/technical/PDF_PARSE_CONTRACT.md)。[14份自造PDF](fixtures/pdf-text-v1/README.md)的作者预期与独立25页Poppler渲染不是一般PDF或科学质量金标准；错开栏、只有空白字符及多码点decode也保守复核，OCR、复杂结构和模型知识仍未实现。

```powershell
python scripts/build_pdf_sidecar.py --check
python scripts/verify_pdf_parse.py '<父目录存在的全新测试根>' --binary '<编译CLI的绝对路径>'
```

源码经标准库zlib压缩嵌入，字节绑定检查避免可读源码与实际CLI代码不同；原生短argv避免已实测的WindowsCreateProcess长度失败。独立核验工具不导入parser/Core助手，按事先预期和实际保存字节检查正文、页、坐标、状态与只读inspect，保存JSON/Markdown支持矩阵。

## 构建桌面应用

桌面前端位于 `apps/desktop`，Tauri 入口位于 `crates/noetome-desktop`，调用与 CLI 共用的 Rust 帮助和版本接口。应用界面包含工作台、运行检查和独立设置页；设置分为外观、窗口行为及关于Noetome。连接和命令结果在实际调用后显示，未实现的项目与资料功能有明确说明。

Windows 构建需要上述 Rust/MSVC 环境、Node.js 22.12.0 或更新版本，以及 Microsoft Edge WebView2 Runtime。根目录的 `package-lock.json` 和 `Cargo.lock` 锁定依赖；首次获取依赖及 NSIS 打包工具需要网络。

```powershell
npm ci
npm run web:check
npm run web:build
npm run desktop:build
```

`desktop:build` 自动先检查并构建前端，再生成 Windows x64 的 NSIS 安装包，输出目录为 `target/release/bundle/nsis/`。安装模式为当前用户。安装后的窗口、快捷方式、版本、运行检查和重启都须实际验证，再把安装位置、包指纹和结果写入 `doc/validation/B01-T03.md`；生成安装包本身不能代替安装验收。

在 PowerShell 中执行 `./scripts/install_desktop.ps1`，脚本先将安装包复制到当前用户的 `NoetomeBuilds/windows` 目录并校验字节和文件完整性标签，再安装到 `Programs/Noetome`。只准备手动安装包时运行 `./scripts/prepare_desktop_installer.ps1`，使用它返回的路径。

本机仓库带有继承的 Windows Low 完整性标签，直接运行 `target/` 中的程序会因无法写入 AppData 而失败。交付脚本通过普通文件复制生成正常输出位置的副本，不修改仓库权限或进程令牌；如果输出位置仍为 Low，会明确拒绝。已经打开的旧安装器可能占用原安装包，须关闭后再覆盖该包，或通过 `-InstallerPath` 选择独立生成的新包。实际安装位置、指纹和测试见 [B01-T03 验证记录](doc/validation/B01-T03.md)。

启动时应用检查自身数据目录是否可写，失败会显示原生错误弹窗、稳定错误码和具体路径；可写时追加启动错误日志。关闭方式由受限的读取和保存接口管理，不提供任意文件路径或命令执行入口。

本轮B01A-T01的窗口行为按用户确认执行：最小化按钮保持普通Windows最小化，任务栏仍有窗口；右上角 × 默认隐藏窗口到托盘，保留进程、导航页面和已有运行输出。左键单击托盘图标，或右键菜单选择“显示窗口”，恢复并聚焦原窗口。托盘菜单“退出”始终结束程序并移除图标。

左下角设置按钮进入分类设置，正上方的月亮/太阳直接切换暗色/亮色；外观分类也可选择主题，真实保存的偏好下次打开继续使用。侧栏采用窄图标栏和分组列表，项目显示当前空状态，最近访问显示本窗口实际访问的页面；顶部侧栏按钮可收放列表，底部入口始终保留。导航图标使用锁定的Phosphor图标库，许可保留在[本地MIT许可](apps/desktop/public/phosphor-icons-LICENSE.txt)并随安装包提供。

设置页的“窗口行为”分类提供“关闭窗口时：隐藏到托盘 / 直接退出”，未配置时默认隐藏到托盘；成功保存的选择在重启后保留。打开设置及失败后的“重新读取”都实际重读设置文件，读取成功后显示并采用当前值；读取或保存失败明确报告，保留此前已生效的关闭行为，不把未保存的选择显示为已生效。启动时无法读取设置且尚无有效值时，明确报告后安全采用隐藏到托盘；这不表示默认值已保存。托盘创建、隐藏或恢复失败应报告稳定错误码及处理建议。

窗口 × 在决策前也重新读取已保存的设置，避免驻留实例沿用另一个窗口保存前的缓存。B01A最终安装版实际验收任务栏最小化恢复、×隐藏后的两种托盘恢复及输出保留、托盘退出与快捷方式重启，以及保存直接退出后的×终止和重启持久性。历史上下文见[B01A-T01设置重读记录](doc/validation/B01A-T01-settings-refresh.md)。

按用户最新截图，当前顶部移除`Noetome / 工作台`与Windows系统Noetome标题行，仅保留一行File/Edit/View/Help、历史/侧栏和右侧窗口按钮。实际更新安装、完整窗口、菜单IPC、原生最小化/最大化/还原/关闭、直接退出/重启持久化，以及用户确认的托盘恢复/空白拖动见[单行窗口验证](doc/validation/B04-T03-ui-single-row.md)。

开发调试使用 `npm run desktop:dev`。只预览前端使用 `npm run web:dev`；前端预览没有 Tauri Rust 运行时，点击运行检查会显示连接失败。

应用图标来自用户选定的 `sample/icon.png`，通过 `scripts/generate_app_icons.py` 派生 Windows ICO、PNG 和界面资源。整个 `sample/` 被 Git 忽略，应用构建只读取提交的派生资源，不依赖本地样例目录。需要更新图标时运行以下命令；此资源生成步骤需要 Pillow，普通桌面构建不使用 Python。

```powershell
python scripts/generate_app_icons.py sample/icon.png
```

资源来源和生成文件指纹记录在 `crates/noetome-desktop/icons/manifest.json`。不将原始样例目录或安装产物加入版本控制。

## 更新阅读版

修改 Markdown 后运行：

```powershell
python scripts/build_prd.py --format html
```

阅读版构建和开发日志脚本只使用Python标准库。使用可用Python解释器，改PRD后检查生成内容，再将源文档与HTML一同提交。

## 日志和检查

```text
python scripts/devlog.py render
python scripts/devlog.py check
python -m unittest discover -s tests -p "test_*.py"
```

开发时通过start、complete、block、reopen维护状态和证据，工具自动刷新三份清单。具体命令见[日志规则](dev-log/README.md)。不要手动把计划写成已完成。

设置页隐藏主导航侧栏的修正版已实际本地重装，安装包、原生验证及备份说明见[B06U-T02记录](doc/validation/B06U-T02.md)。Ubuntu/WSL缺失构建依赖和Windows重装命令见[本机构建说明](doc/technical/LOCAL_BUILD.md)。

明确选定的公式可通过独立 CLI 入口生成全新测试 Vault：

```text
noetome knowledge formula <formula-case目录> <profile.json> <尚不存在的新测试Vault>
```

[公式契约](doc/technical/FORMULA_FIDELITY_CONTRACT.md)说明原始 TeX/作者语义声明/PNG 区域、逐 token 核对、兼容另存及原图回退。[公开自造样例](fixtures/formula-fidelity/README.md)的 `native` 和 `profile-native.json` 可作为明确输入；[验证记录](doc/validation/B06-T03.md)保留实际 RED、篡改、运行期保护及干净 Obsidian 证据。未知符号、不可辨原区和无数据曲线阻断成功，原件仍保存；工程预览不证明科学正确率或 OCR 能力。

明确选定的整图 PNG 和作者手工结构声明使用图像入口：

```text
noetome knowledge image <image-case目录> <profile.json> <尚不存在的新测试Vault>
```

[图像契约](doc/technical/IMAGE_FIDELITY_CONTRACT.md)限定标签、方向/正负、单位、图例和空间核对；[作者样例](fixtures/image-fidelity/README.md)包含 `signed_arrows`、`spatial`、原图回退与关键未知阻断。原 PNG 完整留存，可验证图示才生成有限 SVG，曲线和观测保持原图；结构声明是手工输入，工程预览不证明自动识别、科学质量或生产发布。

明确扫描页的选定公式区域使用独立入口，识别配置与目标阅读器能力分别提供：

```text
noetome knowledge scan-formula <scan-case目录> <api-config.json> <profile.json> <尚不存在的新测试Vault>
```

[扫描契约](doc/technical/SCAN_FORMULA_CONTRACT.md)区分原扫描、未经修补的识别响应、手工来源核对与等价表达。[公开作者页](fixtures/scan-formula/README.md)在识别前冻结清晰及两个关键缺损状态；已知未知不能靠模型猜测通过。识别请求只包含固定任务与区域 PNG；真实服务每次单请求、累计预算预留结算，离线数字环回测试不冒充真实模型。失败本地模型实验保持记录。实际实施与验收见[当前方案](doc/execution/B06V-T02_SCAN_PLAN.md)及账本，不从入口存在推定一般扫描或科学正确率通过。
