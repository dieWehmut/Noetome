# Noetome

面向个人学科学习的知识编译器，将多份电子资料整合为可追溯、可维护的 Obsidian 知识库。

Rust CLI 提供空项目初始化与只读检查，以及帮助和版本命令。桌面骨架提供帮助、版本与本地连接检查，项目业务尚未接入桌面；资料导入和知识生成仍在开发中。能力验收以[任务日志](dev-log/STATUS.md)和实际验证记录为准。

## 开始开发

- [开发指引](AGENTS.md)：每轮工作规则及标准路径。
- [标准文件入口](doc/README.md)：需求、技术、设计、测试和执行规范。
- [近期开发步骤](doc/execution/NEXT_STEPS.md)：仅展开下一批最多3项，每轮推进一项。
- [当前状态](dev-log/STATUS.md)、[待办事项](dev-log/TODO.md)、[已完成事项](dev-log/DONE.md)：由任务账本自动生成。
- [详细需求](doc/requirements/PRD.md)及[HTML阅读版](doc/requirements/PRD.html)。

原待决策事项已按建议采纳，详见[决策基线](doc/requirements/DECISIONS.md)。根据2026年11月1日的用户要求，提前建设可本地安装的桌面骨架，并插入最小化托盘交付；后续在CLI空项目基础上完成文本来源和确定性输出闭环，再逐步接入PDF、模型及完整桌面功能。

## 构建和运行 CLI

在仓库根目录运行。已验证 Windows x64，`rust-toolchain.toml` 锁定 Rust 1.98.1，并安装 rustfmt。需要 Rustup 和可用的 MSVC C/C++ 编译、链接环境；首次获取工具链和锁定依赖需要网络。CLI 使用随 rusqlite 编译的 SQLite，不要求单独安装 SQLite；单独构建 CLI 时不编译 Tauri。

```powershell
cargo build -p noetome-cli --locked --offline
cargo run -p noetome-cli --locked --offline -- --help
cargo run -p noetome-cli --locked --offline -- --version
```

构建后的 Windows 程序位于 `target/debug/noetome.exe`，当前版本为 `0.1.0`。无参数、`--help`（`-h`）显示帮助；`--version`（`-V`）显示版本。帮助和版本每次仅接受一个选项；项目命令用法见下文。未知或多余参数返回退出码 `2`、在标准错误输出 `CLI_INVALID_ARGUMENT` 和用法提示。输出失败返回 `1` 和 `CLI_OUTPUT_ERROR`。帮助、版本及无效语法不创建项目、数据库或用户配置。

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

inspect 核对实际配置和 SQLite 状态，不执行迁移、修复或事务恢复；重复运行不改写项目文件。无效契约或不支持的 Schema 返回 `2`，已有目标、初始化中断、数据库占用或文件访问失败返回 `3`，并显示稳定错误码。参数语法错误仍返回 `2`。

初始化中断时，标记、暂存文件或缺失的配对文件会被明确报告并保留。已经确认安全回滚的普通失败可在原路径重试；保留的中断目录需先检查，当前恢复方式是另选新目录初始化空项目。当前没有原目录自动续写、生产任务恢复或资料导入。以 `-` 开头的项目路径前可加 `--` 分隔符，使用 `noetome project --help` 查看说明。

本机原 `target/` 可执行文件继承 Low 标签，不能向普通 Medium 用户目录写入；测试示例因此使用构建树内的新目录。向正常用户目录创建项目时，须从正常输出位置的 CLI 副本运行。不得用用户生产 Vault 验证本轮能力。

Windows 路径、文件占用、真实初始化中断与恢复测试及命令输出见 [B01-T02 验证记录](doc/validation/B01-T02.md)。

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

启动时应用检查自身数据目录是否可写，失败会显示原生错误弹窗、稳定错误码和具体路径；可写时追加启动错误日志。桌面接口没有新增文件或对话框 IPC 权限。

本轮B01A-T01的托盘行为约定为：使用既有应用图标，最小化时隐藏主窗口并保留进程；左键单击托盘图标，或右键菜单选择“显示窗口”，恢复并聚焦原窗口，保留导航页面和已有运行输出。右键菜单的“退出”结束应用并移除托盘图标；窗口关闭按钮保持退出行为。托盘创建失败应显示`DESKTOP_TRAY_UNAVAILABLE`并停止启动，隐藏或恢复失败应显示`DESKTOP_TRAY_HIDE_FAILED`或`DESKTOP_WINDOW_RESTORE_FAILED`及处理建议。

托盘版已完成本机安装，原生操作仍待验收，详见 [B01A-T01 验证记录](doc/validation/B01A-T01.md)。构建和代码检查不能代替安装后的最小化、恢复与退出实测。

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
