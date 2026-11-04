**截至修复提交 ae285803 的限定复查结论：原五项均 ADDRESSED；新增一个 Important 尚未解除，当前不能记录 B05-T01 complete 或推送收束。** 未发现新的 Critical。控制器已确认在同一修复波内修复该项，再交付新的不可变提交与实际门禁，供一次只针对该项的限定核对。

审查范围为 `f76496ad87b163603471dfc98b362bd2bd2895a9..ae2858031f947ef22e28328af2b094b498106f32`，共八个文件。已读取 AGENTS、B05 状态/账本/任务卡、NEXT_STEPS、文档入口、相关需求和标准，以及协调与来源核对契约。本次没有重做整分支审查，没有开子代理，没有推进 B05-T02/T03，没有修改 tracked 文件、index、HEAD、分支状态或旧证据；只在 UUID 新建的忽略目录保存探测。控制器同时追加收尾材料，原两处用户文件删除仍保留。

原五项的逐项结果如下：

| 原问题 | 结果 | 实际证据 |
| --- | --- | --- |
| 1. 独立校验 any-match 误接受角色歧义 | **ADDRESSED** | 同步歧义伪造的普通协调返回 `COORD_IDENTITY`/2，独立 `check` 返回 1，并明确拒绝 `original subject/property role`。Python 使用匹配组件映射后的 canonical ID 集合恰等于 `{selected role ID}`。Rust 同样向 BTreeSet 插入映射 ID；两者均按映射后的集合判定，多个原组件映射到同一规范 ID 可接受。该多组件正例以实际源码证明，未另造行为样本。 |
| 2. 合法 supports 链接顺序被误拒绝 | **ADDRESSED** | 原 payload evidence 顺序 `[a-support, a-support-second]`，supports 顺序 `[a-support-second, a-support]`；当前普通协调和独立 `check` 均返回 0，保存顺序与 supports 相同，实际为两事实、三出处、三 source ID、四支持链接。原 `source_review` checks 完整顺序保持。后述重复支持回归是该修复引入的独立 Important。 |
| 3. Schema3 Text 误拒绝与 SQL 版本错配误接受 | **ADDRESSED** | 已有合法 Schema3 项目的三个选择均为 Text，普通协调和独立 `check` 均返回 0。配置 Schema、SQL metadata Schema、`PRAGMA user_version` 和 SQL 项目 ID 的四种单独错配分别被 Rust/独立 Python 以 3/1 拒绝；连接显式关闭，没有把 DB_BUSY 当作正确错配行为。配置版本要求 typed int 且为 2 或 3，metadata 行数、项目 ID、Schema 和 user_version 均核对。此验证没有新增 PDF 协调。 |
| 4. 保存 owner 不支持版本的诊断分类 | **ADDRESSED** | `.owner.json` 版本 2 的普通协调返回 `COORD_SCHEMA`/2，没有成功协调 IR。新早期版本检查位于 [knowledge_coordinate_input.rs:348](D:/Noetome/crates/noetome-cli/src/knowledge_coordinate_input.rs:348)。 |
| 5. 原 gold author 转摘要前漏掉文本边界 | **ADDRESSED** | 空白与 2,048 UTF-8 字节 author 均在新的四输入中预先写定，再通过普通 `extract verify` 实际生成真实九件包，均返回 0。协调分别返回 `COORD_SCHEMA`/2、`COORD_INPUT_LIMIT`/2，独立 `check` 均返回 1，没有成功协调 IR。代码在 GoldInput V1/V2 两分支中取 author，并在 `.into()` 前共用同一有界明文检查；原 B04 DTO 未改。动态探测保留公开 Gold2 的原事实和非空例外，Gold1 分支由源码核对，未单独实跑。 |

以上原定限定行为共 **11 个场景、24 条实际命令**，全部观察退出与相应预期一致，原件、IR、gold、来源项目、外部哨兵，以及每条命令的 154 份源码和 CLI 前后保持。普通合法原件仍实际通过。命令原始 argv、stdout/stderr、exit、指纹、保护前后和输出不存在检查保存在 [probe-summary.json](D:/Noetome/target/compiler-temp/b05-t01-final-rereview-a21963d8eaf94c648cac483ea62e1cf3/probe-summary.json) 及其引用的逐命令 receipt。

**新增 Important — supports 顺序修复允许独立校验误接受重复同一证据关系。**

位置：[verify_knowledge_coordination.py:540](D:/Noetome/scripts/verify_knowledge_coordination.py:540)，尤其第 541–544 行的按 ID 查找。该修复删除了旧 `len(evidence) == len(support)` 检查，而上游独立 Python 未补齐 Rust [fixture_knowledge.rs:169](D:/Noetome/crates/noetome-cli/src/fixture_knowledge.rs:169) 的每原子 `(evidence_id, relation)` 唯一性规则。

真实触发使用公开 `support-order` 的新副本，将 a 的两个支持链接从 `[a-support-second, a-support]` 改为 `[a-support-second, a-support-second]`。payload 保留原两证据，调用前作者 gold 保留原四链接分母；实际来源 SQL/字节、身份映射、原 atom ID、两事实 ID、三出处 ID、事实/单位/条件/例外均保持。伪造同步了四输入指纹、原 IR 和完整 source_review checks/双报告、九件 pin、协调副本、JSON/Markdown/IR 与 artifact 指纹；它明确是同步伪造，不能称普通 CLI 成功输出。

实际结果为：

| 操作 | 实际结果 |
| --- | --- |
| 普通 `extract verify` 重查重复 supports 输入 | Exit **3**，`EXTRACT_KNOWLEDGE`，没有成功知识 IR |
| 普通 `knowledge coordinate` 重查同步伪造 case | Exit **3**，`EXTRACT_KNOWLEDGE`，没有成功协调 IR |
| 独立 `check` 同步伪造结果和原作者 gold | Exit **0**，`independently_verified=true`，仍显示两事实、三出处、四链接 |

第 27 条 receipt 明确保存 **required exit 1 / actual exit 0**，不是通过。该结果破坏独立准入门禁，并将一条唯一支持关系重复计成两条；严重性为 Important。建议在原来源候选准入逐原子核对完整 `(evidence_id, relation)` 集合唯一性，保留同一 evidence ID 的不同 relation 及合法反序 supports，再用这个同步伪造先 RED 后 GREEN。

完整证明见 [duplicate-support-result.json](D:/Noetome/target/compiler-temp/b05-t01-final-rereview-a21963d8eaf94c648cac483ea62e1cf3/duplicate-support-result.json)、[forgery-construction.json](D:/Noetome/target/compiler-temp/b05-t01-final-rereview-a21963d8eaf94c648cac483ea62e1cf3/probes/duplicate-support-fix-regression/forgery-construction.json) 和 [第 27 条真实 receipt](D:/Noetome/target/compiler-temp/b05-t01-final-rereview-a21963d8eaf94c648cac483ea62e1cf3/commands/027-duplicate-support-coherent-forgery-check.receipt.json)。该附加探测三条命令均保存完整流和前后保护；原 gold、项目、输入、伪造副本、哨兵、当时 154 份源码和 CLI 均保持。

**已有回归的实际核对。** 没有重复宽套件。我逐一核对最终六个 gate 的退出码、原始流 SHA/长度、前后源码清单和当前实际 154 份字节，确认 Rust **16 suites / 561 passed / 0 failed / 0 ignored**、独立模块 **34 / OK**、完整 Python **142 / OK**。普通 audit 的 **33 场景、56 命令、112 stdout/stderr 指纹、29 原件保护组** 与实际保留文件一致。已核对 corrected Rust RED 三真实失败（owner 3 对 2、两 author 0 对 2）、Python 三问题 RED 及 Schema 两误接受 RED；较早的 author 重序 Markdown 准备失败保持，未当作 author 准入 RED。

固定修复 patch SHA256 为 `21b8a14011cb1178b194cda9e9260b1c29ae3826b456d76fd960a29f615f5882`。当前此次 27 命令所用 CLI 为 17,975,296 字节，SHA256 `2016a13f7bd23e86df5f4b2500b2eb45059fa7dd35157f9b4f1041c8a42b4efd`。Python full 与 Cargo 并行，CLI 在该历史 full 执行前后发生正常重建；源码保持，不宣称当时 CLI 未变。

公开原 probe ZIP 的 817,152 字节、598 成员、完整 member SHA/长度及 ZIP CRC 均核对，SHA256 `9163f306bb515f5fa1e98c9e3e463e48e8655424a47d45dc2067b8054da0dd20`。原件只复制到本忽略根，未改旧探测或旧报告。核对数据见 [evidence-inspection.json](D:/Noetome/target/compiler-temp/b05-t01-final-rereview-a21963d8eaf94c648cac483ea62e1cf3/evidence-inspection.json)。控制器后来新增的最终修复归档不在本次核对声明中，不因其存在而宣称已验证。

**探测局限与保留失败。** 自制检查器初次错误地要求中间 scoped green 的源码与所有后续修复后的最终源码一致；修正核对范围后，最终六 gate 的 154 份字节实际匹配。owner 的辅助 Python 检查因其 source_report 未同步 owner artifact 指纹，在更早的指纹 guard 已拒绝；初次 harness 误期待后续 owner 版本标签而停止，此失败保留，不能用它证明独立 Python 的版本诊断。普通 Rust owner 的 `COORD_SCHEMA`/2 有独立实跑。初次 Gold1 准备发现公开 Gold2 有非空例外，在任何 author CLI 前停止，保留该准备根；随后不删事实，直接以原 Gold2 跑空白/2048 边界。上述失败 receipt 原样保留，未重跑先前 11 场景或改旧输入。

尚未确立通用科学准确率、真实三教材融合、PDF 协调质量、跨平台性能、生产 Vault 人工保护或 B05-T02/T03 能力。本轮只使用明确选定的公开自造材料，没有读取凭据/模型配置、联网研究、模型或付费调用，也没有写生产 Vault。

完整中间报告：[interim-rereview-report.md](D:/Noetome/target/compiler-temp/b05-t01-final-rereview-a21963d8eaf94c648cac483ea62e1cf3/interim-rereview-report.md)。报告 SHA256 由同目录 `interim-rereview-report.sha256` 独立保存，避免文档自引用。
