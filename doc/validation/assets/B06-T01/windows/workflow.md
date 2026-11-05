# B06-T01 原生验收准备

本根 `D:\Noetome\target\compiler-temp\native-b06-t01-8ce6623db0ad` 为全新自有测试目录。准备阶段尚无 `repo`，尚未运行 B06 原生回归、NTFS 路径或真实产品渲染。旧准备样例只证明已安装 Obsidian 1.6.7 的连接路径可用。

## 稳定源码之后

1. Root 明确给出稳定已提交源码与 Linux 完整/14 case GREEN；在 WSL 仓库运行 `python3 <本根WSL路径>/prepare_snapshot.py <stable-commit> <本根WSL路径>`。导入器要求 HEAD 相同、产品文件与 Git blob 相同，并核对新 snapshot 所有字节。不复制旧 target 或不稳定 checkout。
2. 用 native PowerShell 执行 `launch-validation.ps1 -Root <本根Windows路径>`。五阶段依次为 fmt、workspace check/test、完整 Python、14 case 独立审计；Cargo 离线、锁定，fake 与 PDF 使用各自选定解释器。原流与每次 receipt 保留，不覆盖失败历史。Rust nocapture 记录实际 v1/v2/v3 held-input/results-saved 操作原件。
3. 五阶段实际通过后，用选定 native Python 运行 `probe_ntfs.py <本根Windows路径>`；检查 junction 的五个核对角色、普通 CLI 输入/输出角色、lexical parent 与 hardlinked gold。仅在本根创建并移除自有 alias，原输入和作者字节保持。

## 真实 Obsidian

1. 对通过的作者场景运行 `launch-actual-obsidian.ps1 -Root <本根Windows路径> -CaseName <name>`。每次创建新的 profile/Vault，将原 CLI plan.notes 逐份按字节复制；不将 Obsidian 配置写进原审计输出。
2. 对生成的新 work 运行 WSL `build_render_callback.py <work-WSL-path>`；执行 exact `actual-render.js` 的 Playwright unsafe callback。通过 CDP 连接 native Obsidian，核对 own profile/Vault、无社区插件、实际版本1.6.7。逐笔记读取真实 DOM，对照已独立核对的计划逐格/逐段匹配，实际点击概览链接并确认目标路径和知识明细标题，保存截图。
3. Playwright runner 内不能使用 Node `require`/dynamic import 保存文件，两次准备探测失败原件保留。实际 callback 返回 DOM；应将完整 MCP tool 输出与解析出的 Result 原件保存到新 work，通过 tools.apply_patch 精确写入自有文件，不插值进 shell。截图由 actual native page.screenshot 写到共享新 work。
4. 核对 own profile 后只调用该页面的 `require('@electron/remote').app.quit()`；再 native 执行 `finalize_actual_obsidian.py <root-Windows-path> <work-Windows-path>`，保存具体 PID、特定端口 listener、原证据和复制笔记 before/after。其他实例不操作。

表格、正文、中文路径、特殊名称/单位、缺失、多 context、history、v1/v2 和 cross-role 应从实际新审计 case 中选取。阻断场景没有成功 plan/笔记，不交给启动器冒充成功。仅报告实际1.6.7，完整版本兼容矩阵未执行。源码、账本、提交与最终任务验收仍由 Root 管理。
