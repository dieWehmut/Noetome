# B07-T01 独立审查与失败原件归档

供根代理在 Windows/Linux source checks 完成后复制到指定验收目录。本代理只写 `.artifacts`，没有写公共标准、账本、生产源码或 docassets。

最终文件为 `readbackmanifest.zip`（SHA `0669298a74736c97c4235b027ec02efcb6b6a36e13b6640f778037195a621727`，6675473B）及 `readbackmanifest.json`（SHA `47c9d4825013d5b19cc10f2a919b37693895b46b89aec9954a63e5b0bdad3066`，829014B）。1359个成员全部实际解压回读 CRC、SHA、长度通过，对应原文件字节读取后仍相等。第一次1285成员包及首次 observer 错误保留，最终包没有改写它们。

包含三轮 RootReview 的全文、来源/结果/独立 raw SQL 观测，四次早期代码快照，冻结作者17成员及九份旧来源核对，官方 SQLite 原字节和已锁定 SQLite C 全文，foreign journal 的真实失败 stdout/stderr 与七组数据库/journal/owner 快照，publication 控制与 ACK 的实际 RED，真实两线程竞态 RED/GREEN，早期 public/active/refusal、临时 I/O、真实 hot journal 的源记录与原件，以及三轮 clean Linux build 的完整125项源 ZIP、构建实际 stdout/stderr/launch/receipt。大 ELF 本体没有重复装包；实际重新读取的长度/SHA、原保存路径和明确 omitted 原因在 manifest 中，不能把 ZIP 说成含有其本体。

最终 build04 的125个源成员 CRC、长度、SHA、完整前后 source snapshot 和当前源字节独立核对全部一致；实际选定 ELF `b511949627c2e327278bdbb6edf92caa0eb51a425c179d8fb7de35be2bb342f1`/159246752B，driver `f62c5172016145dea9c9a0b5685df7f7b5126af3daf1501b7570440fce67c51c`/188080656B，已重新读取本体。build01/02 原125项 ZIP 全部成员回读成功，旧源与当前源的实际差异独立列出，不把历史构建算为最终源码通过。

对 PythonAgent 的最终矩阵包 `originals.zip`（SHA `7ec65b8112004530e85b98e06fc0c9566081e2dbad3e5397c4b9df8b7767a37d`，4997310B）独立回读3315个成员的 CRC/SHA/长度与原文件 bytes/ID/mtime，全部通过；嵌套 source125 ZIP 精确等于最终 build04。原矩阵 actual SHA `739310445b28b2b421e41f503e25c69e659a58a80c4e2917668014ae1c8b03f4`，实际320 CLI、65 driver、16 auxiliary，十二个 case group 合计95。该审查未重新运行产品。独立结果在 ZIP 的 `observer/peer-final-matrix-archive-audit.json` 与 `observer/final-build04-source-audit.json`。

两处早期捕获限制没有隐藏：helper 的4项编译错误（工具22923）及第一次竞态 harness 缺父目录 NotFound（工具56789）只有会话中的原 merged tool 输出，没有 standalone raw stdout/stderr；`failure-history-index.json` 只标注事实，不伪造历史原件。具体 foreign、late control、实际竞态 RED 有物理 raw 文件证明。初次 observer 把 `cases` 单数组3项误当总95项，修正为十二组真实合计后归档通过；失败脚本与诚实标注的工具输出一并保存。

竞态 RED 的 run_store 全文当时没有另存独立源文件。最终代码中唯一605B写事务内终态门禁删去后，派生全文精确匹配原 RED source_before/after 的 SHA `01c374f2ecb6528fe666f624dbc9b91e90c1ebefe4c4ecd904d4eb3b1c57aec5`/45342B。ZIP 保存全文、差分和原 SHA 绑定，并明确标注 hash-identical derived source，区别于当时的磁盘捕获。

Windows 通过没有从这些 Linux 原件推定；完整 Linux Rust也没有因 clean CLI/test-driver 构建或 Python 测试而改为通过。科学质量仍 not_run，模型/付费调用0。本归档不推进 B07-T02/T03。
