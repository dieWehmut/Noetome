# Windows 安装载荷契约

2026-11-10 的 `v0.1.0-test.1` 按用户最新指令交付已验证的 `6fe4983` 安装包，允许当前电脑已有 VC 运行库；跨机器依赖独立部署和静态 VC 改造后续单独验收，不作为本次测试交付前置条件。范围见[已采纳决定](../requirements/DECISIONS.md)和[测试版说明](../windows/TEST_RELEASE.md)。

目标为 Windows 11 x64、当前用户 NSIS 安装。安装构建显式追加 `--config crates/noetome-desktop/tauri.install.conf.json`；该文件不自动覆盖普通 Cargo、开发或 `--no-bundle` 构建。资源映射使用完整目录 `../../target/windows-installer-payload/` 到安装根空字符串，保留所有嵌套目录；禁止改用会按 basename 展平文件的 glob。

`prepare_windows_installer_payload.ps1` 接受 `-CliBinary`、`-RuntimeDir`、`-SourceCommit`，可选 `-OutputDir`、`-Quickstart`、`-BuildProfile release|debug`（默认 release）、`-PassThru`。RuntimeDir 必须是 `prepare_windows_runtime.ps1` 的完整已验证输出。所有输出写入尚不存在的独立目录，既有目录拒绝覆盖，失败现场保留。CLI 使用普通复制形成单链接文件，完整 Python 环境与许可证保留；不安装或修改用户项目、凭据、预算、现有 Noetome。

| 安装相对路径 | 内容 |
| --- | --- |
| `noetome.exe` | 当前源构建的 CLI，完整字节 SHA 与构建模式记录 |
| `vcruntime140.dll` | 来自同一字节锁定 CPython 树的 VC 运行库，置于 CLI/GUI 相邻目录 |
| `pdf-python/` | 完整 CPython、DLL、标准库、锁定 PDF wheel 与所有许可证 |
| `helpers/` | 四个可信文字模型/DPAPI PowerShell helper |
| `fixtures/model-ledger/` | 冻结完整十条历史账本及独立恢复凭据；不会自动恢复或清零 |
| `fixtures/run-checkpoint-v1/` | 完整作者离线运行样例，包括 fixture、期望 Markdown 与来源 |
| `fixtures/text-compare-v1/` | 完整作者比较样例 |
| `fixtures/pdf-text-v1/` | 完整作者 PDF 包，保留扫描负例和原 expected |
| `samples/single-page.pdf`、`samples/scanned-page.pdf` | 作者单页文字 PDF 和扫描负例原字节便利副本 |
| `QUICKSTART.zh-CN.md`、`README.md` | 中文上手说明和载荷入口 |
| `runtime-lock.json`、`requirements-lock.txt`、`runtime-receipt.json` | 原始锁、依赖哈希与运行环境来源凭据 |
| `licenses/winsafe-0.0.29-MIT.md` | 原生进程句柄封装的完整版权与许可文本 |
| `payload-manifest.json`、`payload-manifest.sha256` | 完整载荷字节清单与派生摘要 |

清单 schema_version=1、producer=`noetome-windows-installer-payload`，记录 source_commit（完整40位）、target=`x86_64-pc-windows-msvc`、build_profile、version、source_inputs、totals 和按 POSIX 路径排序的 files。每行为 `{path,bytes,sha256,source}`。`payload-manifest.json` 自身及派生 `payload-manifest.sha256` 不进入 files，避免自引用；其摘要还应保留到构建外部 receipt。安装器和源构建的 SHA 分别记录，清单不等于代码签名。

独立审计器 `python scripts/verify_windows_installer_payload.py <root> --json` 必须核对精确清单、摘要、普通无链接文件、全部冻结资源原字节、Python/CLI/VC 的 x64 PE、运行环境锁与许可证；失败退出非零。`--manifest-sha256` 可提供构建外部可信摘要；原生 Windows 再加 `--probe-runtime` 真实检查版本、包与许可证。样例仅证明作者工程链路，真实模型调用、科学正确率、安装后原生 UI 分别取证。

WebView2 构建选用 `offlineInstaller` 且 silent=true，把固定架构安装器纳入 NSIS；下载发生在构建阶段。只有实际打包和缺 WebView2 安装验证完成后，才能确认该安装路径无需运行时下载。基础配置、用户原有设置和当前用户安装模式保持独立约束。

Windows 进程退出确认使用精确锁定的 `winsafe 0.0.29`，仅启用 kernel、无默认功能或传递依赖。本仓库继续禁止本地 unsafe。解析器与桌面宿主仅为当前 Job 清单内的进程打开查询/同步句柄，并在持有句柄期间复查同一 Job 成员身份；终止后在原有界清理期限内检查原进程的真实退出信号，不把清单为空当作所有内核对象已退出。库的完整许可随安装资源保留，字节进入载荷清单。

目录映射与 `offlineInstaller` 语义以本仓库锁定的 `tauri-utils 2.10.2` 的 `resources.rs` 与 `config.rs` 为工程依据；对应公开源码见 [Tauri resources](https://github.com/tauri-apps/tauri/blob/tauri-utils-v2.10.2/crates/tauri-utils/src/resources.rs) 与 [配置参考](https://v2.tauri.app/reference/config/) 。
