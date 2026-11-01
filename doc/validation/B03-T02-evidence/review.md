# B03-T02 独立边界审查

当前无未关闭阻断。审查发现的两个真实缺口已修复并独立复验：运行时配置退出码，以及取消后已发布 ready 报告与 API 失败不一致。详细实际命令、源码/构建/证据指纹见 `review.json`。

- R1：Python 3.12.7 实际 adapter 返回 native 2。原 CLI 返回 3；新 D:/Noetome/target/debug/noetome.exe 返回 2，并保留 native_exit_code=2、failed 和有界诊断。
- R2：原 Host 在完整 parse.json 后取消实际返回 CANCELLED，却残留 ready/passed/null 报告。新实现观察完整 .parse.pending.json 后取消返回 CANCELLED、保留 pending 和原始范围、没有正式 parse.json；观察正式 parse.json 后的晚取消允许 API 成功。
- 新目标正常 N01 实际 CLI 退出 0，stdout 字节与已提交 parse.json 精确相等，原 PDF SHA 前后相同，没有 pending 残留。
- 另实际检查子进程预造 parse.json 时拒绝接管，以及原生可写映射仍使选定输入读取被拒绝。这两项早于报告交付修复，当前相应源码路径仍检查。

本审查共 4 次 CLI 与 6 次直接 Host 调用，包含一次误选历史 target 的 setup，以及一次没有触发 cancel 的正常对照；它们不计为修复绿。当前所有修复复验显式使用 D:/Noetome/target 的新构建。审查只新增 .artifacts/B03-T02-review 下文件，没有修改生产源码、日志或 Git。

未重跑 full workspace、15 份 PDF 全矩阵、全部 Core/adapter tests、Poppler/作者位置回看或孙进程矩阵；这些不能从本审查推断为通过。未测试 1000 页、24 小时、科学质量、生产 Vault、OS 沙盒及分发安装。不能要求 Core 从同时自洽伪造的全部 parser primitives 中独立证明科学正确性；作者预期未被待测 parser 重新生成。
