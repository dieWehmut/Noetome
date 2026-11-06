# 固定有限 Run 检查点作者包 v1

本包只服务 B07-T01 的一个确定性 fixture 流程。真实有效原包是 `fixtures/text-compare-v1/`，不是尚不存在的 `knowledge-compile-valid`。两份来源、知识 IR、NotePlan 和独立语义 expected 保持旧原字节，位于 [fixture/](fixture/README.md)。原包 README/audit 文件为历史原件，其当时执行声明不被改写为本任务验收。

[author.json](author.json)声明作者身份、稳定 ID 和分母；[work-graph.json](work-graph.json)固定六单元顺序：`source_contract -> render -> stage_verification -> vault_readback -> reports -> publication`。分别对应原来源/契约核对、两篇 Markdown 渲染、暂存语义与字节核对、新自有 Vault 写入后重读、双报告核对、确认发布。它是有限作者工作图，不代表通用任务调度框架或尚未实现的命令已经可用。

[expected.json](expected.json)是独立预期：规范 Markdown 从先前的 `crates/noetome-cli/tests/support/compile_expectations.rs` 中 ALPHA/BETA 字面值展开，原字面文件保存于 [originals/compile_expectations.rs.txt](originals/compile_expectations.rs.txt)。没有执行新 Run 实现或渲染器生成 expected。既有语义作者 expected 原字节继续独立保存于 `fixture/expected.json`，两篇规范 Markdown 位于 `expected-markdown/笔记/`。

固定两来源、12 Span/Coverage（4 included、7 excluded、1 duplicate）、2 概念、4 必要原子、5 证据、2 笔记、2 章节、4 显示节点；比较表 3 列/2 行/6 单元格，2 个段落、1 个有序列表项，9 处有 scope 的内容映射和2条双向章节链接。两个数值原子分别保留5 cm/8 cm及全部条件、单位和例外；缺失单价仍为“未给出”。稳定知识、NotePlan、节点、规范位置和链接 ID 不因恢复重新生成。

运行 UUID、Release UUID、真实时间、attempt、中断/暂停/取消及复用记录是审计原件，与规范内容文件分别核对。同一运行恢复必须保持其持久身份，不重复确认知识或发布；独立从头基线允许不同运行 UUID 与时间，不能为字节比较删掉审计记录。报告与数据库的原字节指纹仍须保存并绑定实际运行，不凭摘要 passed 推定完整。

暂停按 NFR-13 在2秒内停止调度新单元；已经在途单元及安全落盘可以继续到安全边界并显示实际状态。输入/策略/版本变化需使受影响单元及依赖结果失效或明确拒绝不兼容恢复，不接管未知/外部替换或无可信 owner 的旧现场。中断、版本变化、输入和输出篡改测试只能在独立新测试副本进行，不能修改本冻结包。

[source-pins.json](source-pins.json)记录9个旧作者原件的准确出处、SHA256与长度；[frozen-sha256.json](frozen-sha256.json)列出本包完整成员及指纹（清单本身除外），不将文件存在、输入复制或冻结成功视为恢复验收。当前仅冻结作者/expected和做只读字节/结构审阅；实际 RED、普通 CLI 运行、中断恢复、暂停时序、完整回归及独立审计由主执行者在本任务后续保存。

全部资料为自造虚构工程样例；科学正确率、正式200原子金标准、24小时/多教材规模、生产人工保护仍未验收。本包不访问模型、HTTP、真实凭据或生产 Vault，也不推进 B07-T02/T03。
