# 实际 Windows Tauri UI 验证

`native-windows.launch.ps1` 只启动 manifest 指定的实际 release 程序，再由 Windows Node
运行 `native-windows.cjs`，通过 WebView2 CDP 操作实际界面和 Rust IPC。不使用 Tauri mock，
不安装或卸载本机 Noetome，不点击真实付费模型按钮，不打开文件选择器。

先在独立 Windows TEMP QA 工具目录安装精确 `playwright@1.65.0`，禁止浏览器下载，
将 npm cache、userconfig、globalconfig 都放在该 QA 目录。此开发依赖不进入产品 payload。
程序必须启用 `custom-protocol`，并包含 `NOETOME_DESKTOP_DATA_ROOT` 的独立所有权目录入口；
仅覆盖 `LOCALAPPDATA` 不能隔离 Windows 系统 known folder。

输入 manifest 字段为 `schema_version:1`、`mode:local_staged_payload|ci_clean_installed|local_ci_installed_payload`、
`source_commit`、`payload_root`、`desktop_exe`、`desktop_sha256`、`cli_exe`、`cli_sha256`、
明确选取的作者 `text_pdf`/`scan_pdf`、`windows_node`、`playwright_module`、`harness` 和
新的 `evidence_root`。所有路径必须绝对且没有链接或重解析点，两个 exe 必须属于该 payload。
程序和所需资源由安装任务提供；launcher不生成替代产品二进制。

launcher创建独立TEMP运行根，复制所选作者资料，创建中文空格项目/输出父目录；
只在 child ProcessStartInfo 设置 `LOCALAPPDATA`、`NOETOME_DESKTOP_DATA_ROOT`、
`WEBVIEW2_USER_DATA_FOLDER` 和 loopback remote-debugging port，不改变父进程环境或注册表。
退出只终止本轮亲自启动并仍存活的进程树，不查找或终止旧 Noetome。

验证实际17条能力、项目创建、作者文字PDF/扫描PDF、完整预检原文和物理页定位、切页保留、
missing凭据/账本门禁、默认未勾选恢复、payload离线自检和明暗主题。
唯一模型形状响应来自作者本地HTTP假服务：安装CLI独立调用一次，保存原始HTTP正文/响应，
GUI恢复到新的待核验Vault，核对0额外HTTP、来源不写、原件字节相同、unknown/null和证据定位。
这不证明真实外部模型或科学质量，实际Obsidian阅读视图和原生picker明确记为未运行。

每项检查及失败、原生截图、只读IPC DTO、真实CLI命令/退出码/stdout/stderr、指纹和隔离
收据保存到manifest选定证据目录。任一检查失败、缺少收据、非零exit都使launcher失败。
`local_staged_payload` 与干净CI安装目录的 `ci_clean_installed` 范围分别记录，不能互相替代。
CI真实安装目录快照ZIP在本机解压后，使用 `local_ci_installed_payload` 并记录
`delivery_installer_sha256`/`installed_payload_zip_sha256`；此模式验证安装产物相同字节，
本机仍没有安装或卸载动作。
