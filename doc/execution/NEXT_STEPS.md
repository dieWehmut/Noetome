# 近期开发步骤

真实状态以[任务账本](../../dev-log/tasks.json)和[当前状态](../../dev-log/STATUS.md)为准。B05U-T01 已验收并合入远端默认 main。持续开发已从 main 启动 B05-T03，当前实现、真实检查及剩余门槛见[B05-T03 验收记录](../validation/B05-T03.md)，未完成前不切 B06。

## B05-T03 真实冲突与证据不足的可追踪报告

**状态与依赖**：已阻塞（支持环境的完整必要回归未通过），依赖已完成的 B05-T02；原 ID、验收与历史保留。原三张详细执行卡归档于[B05步骤](B05_STEPS.md)，本项沿用其冲突/反证/证据不足范围。

**单一结果**：同对象、属性、单位和完整条件下真正不相容的有据结论，与证据不足分别定位；保留原值、适用范围、来源版本/位置、历史与反证，默认需复核，未解决必要冲突不生成成功协调 IR。

1. 取得可用 Windows/支持执行环境，先核对当前分支提交、20 个冻结作者、最终程序及证据指纹。通过日志工具 reopen 后 start 原 B05-T03，不新建替代任务；既有 RED、GREEN、修复和基线失败原件保留。
2. 在匹配的 Rust 1.98.1、Python 3.12.14 与现有锁定依赖中运行完整 `cargo check --workspace`、`cargo test --workspace`，补齐 Windows-only sidecar/来源写入及必要保护回归。已有 v1/v2/v3 文本行为、独立 59 场景及限定复审可核对源码/输入后复用；有新改动或失败才重跑对应范围。
3. 全部必要门槛实际通过后维护 complete 日志、小步提交并普通推送，核对远端。B05 三项全部验收后才细化下一批最多三项；不新增联网研究、科学权威评分、真实模型调用或生产 Vault 发布。

本项业务依据见[协调契约](../technical/KNOWLEDGE_COORDINATION_CONTRACT.md)、[条件/时间契约](../technical/KNOWLEDGE_VARIANTS_CONTRACT.md)及[冲突/证据不足 v3](../technical/KNOWLEDGE_CONFLICTS_CONTRACT.md)。Schema3 文本协调的普通 CLI 与独立核对已取得真实 RED/GREEN，审查问题需实际闭合。当前 Linux 无 Windows 进程树宿主，workspace 桌面依赖也未齐；完整回归失败或跳过不能记录为通过。先完成本项修复和可运行门槛，取得支持环境的完整必要回归后再 complete 与推送。任务状态只由账本事件和实际产物决定。
