# 本机构建与重装

当前桌面安装目标是 Windows x64 NSIS。Linux/WSL 可用于开发检查；本机缺失依赖与本次 Windows 重装的真实结果见 [B06U-T02 验证](../validation/B06U-T02.md)。

## Ubuntu 22.04 / WSL：补齐构建依赖

2026-11-05 本机 apt 查询确认以下四包均未安装且有可用候选：pkg-config、libssl-dev、libwebkit2gtk-4.1-dev、libayatana-appindicator3-dev。CLI 的 reqwest 使用 native TLS，在 Linux 上需要 OpenSSL；完整 workspace 还会编译 Tauri 桌面模块，需要 WebKitGTK 和托盘开发库。

在 Ubuntu / WSL 终端执行（sudo 需要本机用户权限）：

```bash
sudo apt update
sudo apt install -y build-essential pkg-config libssl-dev \
  libwebkit2gtk-4.1-dev libayatana-appindicator3-dev \
  librsvg2-dev libxdo-dev curl wget file
```

只构建 CLI 时，最小新增依赖为 build-essential、pkg-config、libssl-dev；上面的整组命令也覆盖桌面 workspace 编译需求。依据：[rust-openssl 官方安装说明](https://docs.rs/openssl/latest/openssl/)、[Tauri 2 Linux 前置依赖](https://v2.tauri.app/start/prerequisites/)。

在仓库根目录执行：

```bash
pkg-config --modversion openssl webkit2gtk-4.1
cargo fmt --all --check
cargo build -p noetome-cli --locked
cargo check --workspace --locked
```

仓库 rust-toolchain.toml 选择 Rust 1.98.1。首次下载工具链和 Cargo 依赖需要网络，因此首次命令不加 --offline。已有完整缓存时可以加 --offline。

上述 apt 安装及 Linux 完整 workspace 构建在 B06U-T02 中没有执行：用户要求提供执行方法，本轮实际构建与安装在 Windows 完成。不能把 Windows 通过记录算成 Linux 通过。

## 完整测试所需 Python

实际 PDF 测试要求 CPython 3.12.14 及 runtime-lock.json 中的全部精确依赖。系统 Python 3.14 或仅安装 pdfplumber 均不能满足要求。已有 uv 的 Linux 环境可在新的工具目录准备：

```bash
uv venv --python 3.12.14 .artifacts/python312-linux
uv pip install --python .artifacts/python312-linux/bin/python \
  pdfplumber==0.11.9 pdfminer.six==20251230 pypdfium2==5.14.0 \
  Pillow==12.3.0 charset-normalizer==3.5.2 cryptography==50.0.2 \
  cffi==2.1.1 pycparser==3.11
export NOETOME_PDF_PYTHON="$(pwd)/.artifacts/python312-linux/bin/python"
export NOETOME_TEST_PYTHON="$NOETOME_PDF_PYTHON"
export NOETOME_TEST_BINARY="$(pwd)/target/debug/noetome"
cargo build -p noetome-cli --locked
cargo test --workspace --locked
"$NOETOME_PDF_PYTHON" -m unittest discover -s tests -p 'test_*.py'
"$NOETOME_PDF_PYTHON" scripts/devlog.py check
git diff --check
```

选择尚不存在的 venv 路径。这里为 Linux 精确版本安装；sidecars/pdf_parser/requirements-lock.txt 的 wheel 哈希面向 Windows，不能原样用它验证 Linux wheel。完整 Linux Rust 测试尚未据此验收，平台特有用例需按实际结果记录。

## Windows：常规构建和本地安装

在包含所需修改的 Windows 工作目录根部打开 PowerShell，沿用 README 中要求的 Node、Rust/MSVC 与 WebView2 环境：

```powershell
npm.cmd ci
npm.cmd run desktop:build
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\install_desktop.ps1
```

ExecutionPolicy 参数只作用于这次 PowerShell 进程，用于执行本仓库已审阅的本地安装脚本，不修改持久执行策略。先正常退出旧 Noetome。安装脚本会核对安装包、目标目录、完整程序内容、快捷方式和卸载登记，默认安装到当前用户 Programs/Noetome。

本机 D:\Noetome 具有 Low 完整性标签；若复用该处的构建缓存，应将独立源码副本放在同一构建区域，并使用 scripts/cargo_local.ps1 或为本次构建指定可写的 TEMP/TMP。不要把 Low 标签构建进程和普通完整性源码写入目录混用。最终安装包通过 prepare_desktop_installer.ps1 复制到正常输出位置并核对完整性标签。

本轮安装包保存在 C:\Users\Lyinn\AppData\Local\NoetomeBuilds\windows-b06u-t02\Noetome_0.1.0_x64-setup.exe。旧安装的卸载登记缺失，故核对旧程序与上次验收 SHA 相同且目录仅含程序/卸载器后，将旧目录完整备份到 Programs/Noetome.backup-b06u-t02-20261105，再在原位置重新安装。备份保留，不删除用户配置。
