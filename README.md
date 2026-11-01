# Noetome

面向个人学科学习的知识编译器，将多份电子资料整合为可追溯、可维护的 Obsidian 知识库。

当前已实现 Rust CLI 最小骨架，可显示中文帮助和版本；项目初始化与资料导入尚未实现。

## 开始开发

- [开发指引](AGENTS.md)：每轮工作规则及标准路径。
- [标准文件入口](doc/README.md)：需求、技术、设计、测试和执行规范。
- [近期开发步骤](doc/execution/NEXT_STEPS.md)：仅展开下一批3项，每轮推进一项。
- [当前状态](dev-log/STATUS.md)、[待办事项](dev-log/TODO.md)、[已完成事项](dev-log/DONE.md)：由任务账本自动生成。
- [详细需求](doc/requirements/PRD.md)及[HTML阅读版](doc/requirements/PRD.html)。

原待决策事项已按建议采纳，详见[决策基线](doc/requirements/DECISIONS.md)。目前先做CLI与本地小闭环，后续逐步接入PDF、模型和桌面界面。

## 构建和运行 CLI

在仓库根目录运行。已验证 Windows x64，`rust-toolchain.toml` 锁定 Rust 1.98.1，并安装 rustfmt。需要 Rustup 和可用的 MSVC C++ 链接环境；首次获取工具链需要网络。当前 CLI 仅使用 Rust 标准库，工具链安装后可离线构建。

```powershell
cargo build --workspace --locked --offline
cargo run -p noetome-cli --locked --offline -- --help
cargo run -p noetome-cli --locked --offline -- --version
```

构建后的 Windows 程序位于 `target/debug/noetome.exe`，当前版本为 `0.1.0`。无参数、`--help`（`-h`）显示帮助；`--version`（`-V`）显示版本。每次仅接受一个选项，未知或多余参数返回退出码 `2`、在标准错误输出 `CLI_INVALID_ARGUMENT` 和用法提示。输出失败返回 `1` 和 `CLI_OUTPUT_ERROR`。这些命令不创建项目、数据库或用户配置。

```powershell
cargo fmt --all --check
cargo check --workspace --locked --offline
cargo test --workspace --locked --offline
```

进程测试使用 `target/cli-tests/` 内独占的临时目录，比对运行前后的目录和文件字节，测试结束自动清理。环境、实际结果及本轮遇到的问题见 [B01-T01 验证记录](doc/validation/B01-T01.md)。

## 更新阅读版

修改 Markdown 后运行：

```powershell
python scripts/build_prd.py --format html
```

构建和日志脚本只使用Python标准库。使用可用Python解释器，改PRD后检查生成内容，再将源文档与HTML一同提交。

## 日志和检查

```text
python scripts/devlog.py render
python scripts/devlog.py check
python -m unittest discover -s tests -p "test_*.py"
```

开发时通过start、complete、block、reopen维护状态和证据，工具自动刷新三份清单。具体命令见[日志规则](dev-log/README.md)。不要手动把计划写成已完成。
