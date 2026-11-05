# B06-T01 原始证据

本目录保存有界主动表达的原始执行材料；任务是否完成以账本和完整验收记录为准。

- `rust-red/`：在实现前执行普通 CLI 来源导入、`extract verify` 和成功协调。`knowledge present` 尚未接线，返回 2；Cargo 返回 101。原始来源与协调材料保持不变，未创建预览根。ZIP 保存 118 项原件以及当时的测试和基础实现。
- `rust-matrix-red/`：11 个先冻结的无图作者样例逐项准备。9 个协调通过，2 个合法未决；未接线的呈现命令全部返回 2，未满足独立期待的 0 或 3。ZIP 保存 1,404 项原件，包含逐条参数、stdout、stderr、退出码及保护清单。
- `renderer-compile-red/`、`renderer-green/`：新渲染模块首次测试误用私有错误字段，编译失败；改用公开方法后定向 5 项单测通过。这是当时组件结果，不替代最终 CLI、完整回归或实际 Obsidian 验收。
- `capture.json`：复制后逐字核对原流、全部 ZIP CRC 和成员 SHA/长度。初次归档校验错误及修正保留在记录中。

作者样例为公开自造的工程输入。来源核对、协调和表达预览不等于真实模型抽取、科学正确率或生产 Vault 发布。后续实现失败、修复与完整验收证据继续追加，原 RED 不替换。

后续完整验收见[任务记录](../../B06-T01.md)：

- `development-runs/`：清理遗漏、跨角色身份、小节定位的失败/修复和 Linux 回归原流。
- `multiline-label-red/`、`multiline-label-green/`：合法换行/Tab 名称的普通 CLI 失败与修复；`review.json` 保留最初独立发现。
- `independent/`：初轮及扩展语义篡改矩阵，最终复审发现的 Python 显示名误拒，以及三版本修复的 RED/GREEN 原件。`display-checker-final-review.json` 和 `display-checker-review/` 是另一位审查者的最终复查与实际调用。
- `windows/`：固定源码的完整原生回归、初轮 TEMP 错误、最终 623 项 Rust 与 177 项 Python结果、独立审计、持有期实际写/删拒绝和 NTFS 路径原件。`closeout-readback.json` 保存归档成员和当前源码的实际读回结果。
- `obsidian/`：实际 1.6.7 的 13 场景、38 笔记、26 次点击原件，以及最终 Windows CLI 生成笔记与实机渲染原件的字节对应关系。

所有失败与修复按实际发生记录；二进制构建产物留在自有测试根，以指纹与固定源码关联，不把大量可重建程序入库。中间重复累计审计快照可在归档中省略，但相应清单保留其指纹和省略理由。
