# 近期开发步骤

真实状态以[任务账本](../../dev-log/tasks.json)和[当前状态](../../dev-log/STATUS.md)为准。B06 的有据比较表、过程流程与明确选定公式保真分别按[B06-T01](../validation/B06-T01.md)、[B06-T02](../validation/B06-T02.md)、[B06-T03](../validation/B06-T03.md)实际验收；设置侧栏和本地重装见[B06U-T02](../validation/B06U-T02.md)。归档执行范围见[B06 步骤](B06_STEPS.md)。公式回退不代表整个视觉阶段完成，Linux完整运行兼容性和未运行平台检查也不从Windows结果推定。

剩余视觉目标只细化[B06V 三项](B06V_STEPS.md)。图像实现和实际验收依据见[B06V-T01](../validation/B06V-T01.md)；后两项依次推进，真实状态以账本为准，不并行启动后续产品阶段。

## 1 B06V-T01 一般来源图像有据重现与重绘回退

依赖B06-T03。实际交付 `knowledge image`、严格[图像契约](../technical/IMAGE_FIDELITY_CONTRACT.md)、12冻结作者与独立校验器、原PNG留存/有限SVG/未知阻断。已运行55命令/91场景独立审计、Windows688项Rust与完整Python、17次干净Obsidian浅色/深色/原图附件验收，失败及修复历史见[记录](../validation/B06V-T01.md)。手工声明不是自动图像识别，剩余扫描与兼容矩阵不从本项推定。

## 2 B06V-T02 单个明确扫描页公式重建与语义复核

依赖B06V-T01，实施范围见[扫描方案](B06V-T02_SCAN_PLAN.md)与[契约](../technical/SCAN_FORMULA_CONTRACT.md)。识别前冻结一个自造作者公式的三扫描状态及33文件，初始和最终四路径RED均保存；本地失败六轮完整封存，不改expected或删除额外符号。最终唯一活动方案为同一已授权视觉Chat，实际四次单请求通过清晰重建、负号/下标未知阻断及原区回退，新增3,088 Token、累计33,185/pending0；恢复期间未知量按用户指示零假设，见[服务记录](../requirements/MODEL_SERVICE.md)。[最终证据](../validation/B06V-T02.md)包含当时源码绑定Windows Rust720、Windows Python394/6skip、Linux Python333/19skip、五视觉308CLI，干净Obsidian1.6.7浅深四窗口八附件点击及三项独立审查。内部权重、作者原始TeX、通用OCR和科学质量保持未核实；后续跨版本另按下一项实际验收。

## 3 B06V-T03 视觉输出最低与当前稳定版兼容矩阵

依赖B06V-T01和B06V-T02。实际[兼容验收](../validation/B06V-T03.md)覆盖最低1.6.7与执行时官方稳定1.15.0、15产品案和1手写探针、明暗64唯一组合；MathJax分别3.2.2/4.1.3，当前流程实际Pele1.8.0/严格Schema2。完整Windows Rust726、Windows Python441(434通过/7skip)、Linux最终380(361通过/19skip)、五视觉317CLI和有编译源锚Windows30目标+15来源setup均通过，原件/源码/7实质篡改与独立终审完整保存；本项无新增付费模型或生产Vault写入。精确允许表及未知回退见[视觉契约](../technical/VISUAL_COMPATIBILITY.md)。日志完成、提交和远端核对后按[进入评估](../validation/assets/B06V-T03/review/b07-entry-assessment.md)滚动细化B07，当前执行入口仍以账本为准。
