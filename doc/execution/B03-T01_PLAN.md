# B03-T01 有界 parser sidecar 实施方案

本方案执行[已规划的 B03-T01](NEXT_STEPS.md)，以[账本](../../dev-log/tasks.json)的验收为准。协议与生命周期是同一交付，不推进 B03-T02、项目 Schema 或 GUI。当前用户已授权实现、修复、小步提交与检查后推送。

**目标**：Rust Core 对明确选定的一份原始文件与实际测试 fake 子进程完成一次严格、有界请求，或返回分类失败并收束受控进程树。

**架构**：Core 固定请求、输入只读守卫和独占新工作区；sidecar 只接一份 UTF8 JSON。stdout/stderr 并发有界读取，stdin 在发送后关闭。进程退出、协议、身份、输入指纹、资源真实字节和精确清单均确认后才交付经过协议校验的结果。

**技术**：现有 Rust 1.98.1 / Windows x64。只添加所需的安全进程管理适配依赖，精确锁定版本并审阅上游源代码；保留工作区 `unsafe_code = forbid`。fake 使用测试目录内的标准库 helper，不引入 PDF、OCR、模型库或网络操作。

## 文件与接口边界

| 文件 | 责任 |
| --- | --- |
| `crates/noetome-cli/src/sidecar_model.rs` | 严格版本化请求、响应、单位、资源、问题与限额 |
| `crates/noetome-cli/src/sidecar_process.rs` | 可信原生 argv、并发有界管道、取消/超时、进程树终止与 wait |
| `crates/noetome-cli/src/sidecar_workspace.rs` | 明确输入守卫、独占新根、资源路径/身份/SHA与精确清单 |
| `crates/noetome-cli/tests/sidecar.rs` 及测试支持文件 | 实际 fake 进程、恶意响应、树握手与普通 CLI 排除 fake |
| `doc/technical/SIDECAR_PROTOCOL.md` | 冻结最终 wire/API、默认政策及稳定错误码 |
| `doc/validation/B03-T01.md` | 真红/绿、Windows 环境、实际数量与命令；明确未运行项 |

普通库/CLI 入口只接受真实 parser producer；允许 `producer=fake` 的入口仅编译到测试。若集成测试不能访问 `cfg(test)` 入口，可将完整宿主进程断言编译在库测试中，另以集成测试证明普通 CLI 不接受 fake；不能为了测试增加普通 CLI 开关。

## 实施与审查步骤

1. 冻结 wire 与 API，再写实际 fake 进程测试。首先运行可编译的负例并保存真实行为红灯；不得用缺失模块、缺依赖或 setup 错误代替红灯。
2. 实现单请求宿主：请求/响应/诊断/输入/资源/时间限额可注入；stdout/stderr 同时读取，超过上限明确失败，不能将截断结果交付为成功。所有停止路径终止进程树、wait，并收回 reader/writer 线程。
3. 在 Windows x64 用 root/child/grandchild PID 握手测试超时、取消、输出超限以及父进程提前退出。先确认后代存活，再确认受控进程退出；库宣传或仅 `Child.kill()` 不替代探针。
4. 验证路径、真实文件身份/链接数、SHA/长度、UTF8资源、引用、完整单位清单和实际目录清单。已有工作区、未知文件、junction/链接、保留名、大小写/前缀冲突及外部哨兵纳入断言。错误现场保留，只清理测试确认归属的目录。
5. 实现代理保存定向检查证据并分步提交。独立审查协议、Windows 生命周期、测试真实性及 fake 隔离；必要修复先取得对应红灯。
6. 运行格式、workspace check/test、Python unittest、devlog check、diff check；同步标准和验证记录，通过日志工具 complete，最终提交并推送分支，核对远端 SHA。

## 验收边界

协议 fake 的单位是合成数据，数量与引用检查只证明协议规则。没有实跑真实 PDF、物理页/bbox 定位、Python PDF 库、OCR、模型、科学质量、跨平台或性能目标。Job Object 管理本轮受控进程树，不能表述为操作系统级文件/网络沙盒。验收后停止，B03-T02 保持 todo。
