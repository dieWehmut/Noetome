# 当前扫描凭据封装的失败进程边界复查

公开 synthetic key、人工账本和无 HTTP 的 Python 子入口；只读/写新测试范围。实际 RED、准备失败、初次 GREEN、语义修复前完整46项与最终6项原件均在 `originals.zip`，逐成员字节/SHA及ZIP SHA见 `originals-manifest.json`，全部归档成员已实际读回。当前源绑定及实测PID/Job收据见 `review.json`。

原封装在父进程实际退出、仍存活后代继承输出管道后超过20秒观察并抛出TimeoutExpired。修复将单调Stopwatch deadline同时覆盖进程与两个读任务，使用窄Windows Job终止已归入Job的子树；超时和超限保留未知预算并保存独立 `failed` 阶段收据。成功四阶段Schema保持。失败收据只把RanToCompletion当完整读取，原IsCompleted误标的原件保留，不能作为最终正确收据。

最终固定源码上Windows PowerShell 5.1和PowerShell 7各3个边界，合计6项、0失败、0跳过，52.188秒。逐次取得实际父/后代存活句柄并验证退出，Job实际ActiveProcesses降至0；每项账本保留65536未知预留，实际超限stdout标记不完整。完整46项165.078秒的原件来自上述收据/Stopwatch修复前，保留为历史；不从它推定最终全workspace通过，最终全量由root另行验收。

Job分配发生在Process.Start之后，仍有加入前创建窗口；本项真实Rust CLI不创建子进程，不将此封装称作通用不可信进程宿主或文件/网络沙盒。真实识别、用户账本恢复、真实凭据及Obsidian未在这项工程检查中运行。
