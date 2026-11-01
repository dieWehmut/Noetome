# Noetome

面向个人学科学习的知识编译器，将多份电子资料整合为可追溯、可维护的 Obsidian 知识库。

当前的 Rust CLI 和桌面应用骨架提供帮助、版本与本地连接检查；项目初始化、资料导入和知识生成尚未实现。桌面构建及安装验收以[任务日志](dev-log/STATUS.md)和实际验证记录为准。

## 开始开发

- [开发指引](AGENTS.md)：每轮工作规则及标准路径。
- [标准文件入口](doc/README.md)：需求、技术、设计、测试和执行规范。
- [近期开发步骤](doc/execution/NEXT_STEPS.md)：仅展开下一批3项，每轮推进一项。
- [当前状态](dev-log/STATUS.md)、[待办事项](dev-log/TODO.md)、[已完成事项](dev-log/DONE.md)：由任务账本自动生成。
- [详细需求](doc/requirements/PRD.md)及[HTML阅读版](doc/requirements/PRD.html)。

原待决策事项已按建议采纳，详见[决策基线](doc/requirements/DECISIONS.md)。根据2026年11月1日的用户要求，提前建设可本地安装的桌面骨架；后续继续完成空项目、文本来源和确定性输出闭环，再逐步接入PDF、模型及完整桌面功能。

## 构建和运行 CLI

在仓库根目录运行。已验证 Windows x64，`rust-toolchain.toml` 锁定 Rust 1.98.1，并安装 rustfmt。需要 Rustup 和可用的 MSVC C++ 链接环境；首次获取工具链需要网络。CLI 仅使用 Rust 标准库，单独构建 CLI 时不编译桌面依赖。

```powershell
cargo build -p noetome-cli --locked --offline
cargo run -p noetome-cli --locked --offline -- --help
cargo run -p noetome-cli --locked --offline -- --version
```

构建后的 Windows 程序位于 `target/debug/noetome.exe`，当前版本为 `0.1.0`。无参数、`--help`（`-h`）显示帮助；`--version`（`-V`）显示版本。每次仅接受一个选项，未知或多余参数返回退出码 `2`、在标准错误输出 `CLI_INVALID_ARGUMENT` 和用法提示。输出失败返回 `1` 和 `CLI_OUTPUT_ERROR`。这些命令不创建项目、数据库或用户配置。

```powershell
cargo fmt --all --check
cargo check -p noetome-cli --locked --offline
cargo test -p noetome-cli --locked --offline
```

进程测试使用 `target/cli-tests/` 内独占的临时目录，比对运行前后的目录和文件字节，测试结束自动清理。环境、实际结果及本轮遇到的问题见 [B01-T01 验证记录](doc/validation/B01-T01.md)。

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

本地可手动运行安装包，或在 PowerShell 中执行 `./scripts/install_desktop.ps1` 安装到当前用户的 `Programs/Noetome`。当前构建、安装尝试及剩余问题见 [B01-T03 验证记录](doc/validation/B01-T03.md)。

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
