# Noetome

面向个人学科学习的知识编译器，将多份电子资料整合为可追溯、可维护的 Obsidian 知识库。

Rust CLI 提供空项目初始化、单份UTF8 MD/TXT来源登记及只读检查、明确标为fixture的IR与NotePlan契约校验，以及帮助和版本命令。来源登记保存不可变版本、原始/规范文本快照和完整Span清单，知识处理仍为pending；fixture的知识与计划由作者声明，程序核对来源、证据、完整覆盖和独立预期。桌面骨架提供帮助、版本、本地连接检查和托盘设置，项目、来源及fixture业务尚未接入桌面；PDF解析、模型抽取和笔记生成仍在开发中。能力验收以[任务日志](dev-log/STATUS.md)和实际验证记录为准。

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

实际Windows CLI、故障保护、审查回归和Schema验收见[B02-T02验证记录](doc/validation/B02-T02.md)。下一任务才将已校验fixture编译到全新的临时Vault。

## 构建桌面应用

桌面前端位于 `apps/desktop`，Tauri 入口位于 `crates/noetome-desktop`，调用与 CLI 共用的 Rust 帮助和版本接口。应用界面包含工作台、运行检查和关于页面；连接和命令结果在实际调用后显示，未实现的项目与资料功能有明确说明。

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

关于页的“窗口行为”设置提供“关闭窗口时：隐藏到托盘 / 直接退出”，未配置时默认隐藏到托盘；成功保存的选择在重启后保留。打开设置及失败后的“重新读取”都实际重读设置文件，读取成功后显示并采用当前值；读取或保存失败明确报告，保留此前已生效的关闭行为，不把未保存的选择显示为已生效。启动时无法读取设置且尚无有效值时，明确报告后安全采用隐藏到托盘；这不表示默认值已保存。托盘创建、隐藏或恢复失败应报告稳定错误码及处理建议。

窗口 × 在决策前也重新读取已保存的设置，避免驻留实例沿用另一个窗口保存前的缓存。最终安装版已通过本机Windows原生验收：任务栏最小化恢复、×隐藏后的两种托盘恢复及输出保留、托盘退出与快捷方式重启，以及保存直接退出后的×终止和正常快捷方式重启持久性。实际安装、用户反馈和测试上下文诊断见 [B01A-T01 设置重读验证记录](doc/validation/B01A-T01-settings-refresh.md)，[第一修正版](doc/validation/B01A-T01-close-settings.md)及[首次托盘记录](doc/validation/B01A-T01.md)保留旧行为与历史证据。

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
