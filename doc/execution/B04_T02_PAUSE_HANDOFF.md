# B04-T02 暂停交接

用户要求把手头阶段收束、保存后暂停。本轮已完成失败诊断补修与首次真实调用的事实归档；**T02 真实候选验收仍未完成，T03 未启动**。任务状态以 `dev-log/tasks.json` 为准。

- 首次实测：1 次 POST，HTTP 200，`MODEL_API_RESPONSE_INVALID`，无 raw/canonical/candidate；实际返回模型和 Token/费用未知。具体字段不兼容原因尚不能从首次记录确认。来源五文件、原件和哨兵保持原字节。
- 本轮补修：安全的严格 JSON 即使 envelope 不合格也保留为失败诊断；不放宽候选接受规则。原生 reader 只认可当前及已审阅旧 adapter 的精确指纹，并按其原版本语义核对。
- 检查：Rust 全 workspace 516/0，Python 60/0；fmt/check/build 与独立审阅通过。新 CLI 对实际根和归档根分别在移除密钥的环境中重读，退出 3、报告与首次保存事实相同、库存及来源不变，额外 API 调用为 0。
- 保存：[首次真实失败及补修记录](../validation/B04-T02-api-attempt-1.md)、[机器记录](../validation/B04-T02-api-attempt-1.json)、[证据目录](../validation/B04-T02-api-attempt-1-evidence)。旧离线检查点与失败历史保留。

恢复时先读状态、TODO、tasks 与 NEXT_STEPS，再只读核对首次失败记录。现场由 `.artifacts/B04-T02-api-live/active-root.txt` 定位，首次调用参数为该根的 `launch-3.json`，结果为 `dialog-result-3/api-result`；补修后的独立 CLI 副本与 SHA 记录在 `reviewed-cli-diagnostics/noetome.exe` 和 `diagnostic-cli.json`。真实密钥没有保存，不能从报告恢复。

原先最多一次生成授权已使用。本轮没有授权或执行额外生成；不能复用首次已消耗的授权、换输出根自动重试，或把 HTTP 200 当 T02 成功。若恢复后要再实测，先明确确认额外一次请求及预算，仍限定同一 R7 样本、目标/模型、1024 输出 Token/30 秒、无重试；随后才建立新授权、新独占输出和本机输入入口。获得合法且可重读的真实候选前，不 complete T02 或启动 T03。
