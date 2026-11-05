# B06-T01 独立 Python 核对审查与实跑

本记录由 `/root/independent_audit` 子代理在本轮实际检查、运行和归档后形成。对象为有界主动表达核对器及其作者/矩阵测试；不是整个产品、最终 Rust 修订、Windows 或 Obsidian 的验收结论。

## 审查范围与依据

已读当前 AGENTS、任务账本/待办、B06 执行步骤及 `ACTIVE_PRESENTATION_CONTRACT.md`。逐段检查 `scripts/verify_knowledge_presentation.py` 的作者冻结、独立对象/属性/单元格预期、路径准入、来源与既有协调重验、计划重建、实际 Markdown 和 UTF-8 范围核对、双报告精确对账，以及 audit 中普通 CLI 调用和同步篡改过程；检查 `tests/test_verify_knowledge_presentation.py` 的 14 个测试。相关上游检查包括 `verify_knowledge_coordination.py` 的严格 JSON 比较、来源包与库存函数，以及 `verify_knowledge_conflicts.py` 的祖先/链接准入。

核对器从原 SQL/资源、九件来源核对材料及独立作者 gold 重建协调和表示，不将保存的 `plan.json` 当作期待值。14 个作者场景覆盖比较表、稀疏属性、单对象/无共享属性正文、条件及历史并列、原单位、复杂名称、合法未决、旧 v1/v2 和跨事实概念角色复用。作者包是公开自造工程样例。

## 发现与修复

初次实际 audit 已通过，但矩阵只有错对象行，没有单独执行错属性列，未完全提供任务中“错行列”的实际篡改证据。只修改核对脚本矩阵，增加：

- `wrong-property-column`：在同一对象行中交换两个属性的完整事实文本和引用，保留列位置，然后同步重建 Markdown、映射和指纹。
- `wrong-property-header-provenance`：交换两个属性表头的名称、概念引用和原出现引用，并同步重建输出。

修复后的完整 audit 中，前者在独立计划的 `notes/0/nodes/0/rows/0/cells/1/text` 比较失败，后者在 `notes/0/nodes/0/columns/1/label` 比较失败。两者保持可读 JSON 和同步指纹，拒绝依据是对象/属性语义与原已核对知识不一致。没有为该覆盖补充修改 Rust 产品代码、作者预期或冻结来源，也没有发现需要修改核对器主逻辑的实质正确性问题。此结论只适用于上述审查范围。

## 实际运行

选定解释器为 `/tmp/noetome-b05-t03-python/bin/python`。两轮均先执行 `-m unittest discover -s tests -p test_verify_knowledge_presentation.py -v`，再执行 `scripts/verify_knowledge_presentation.py audit <独立复制的绝对 CLI> <不存在的 matrix 根>`。原始参数、耗时、退出码、标准输出和标准错误均保存为同名 receipt/stdout/stderr 原件。

| 运行 | Python 定向测试 | CLI 调用 | 来源到预览场景 | 同步篡改拒绝 | CLI 路径拒绝 | 总场景 | Audit 结果 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| initial | 14 通过，0 跳过 | 125 | 14 | 20 | 3 | 37 | exit 0，40.29 秒 |
| extended | 14 通过，0 跳过 | 125 | 14 | 22 | 3 | 39 | exit 0，38.84 秒 |

两轮最终 audit 的 `error` 均为 null；输入、输出、CLI 和外部哨兵在检查前后保持原有清单/字节。同步篡改还覆盖删条件、单位、例外和观点时间，丢来源、版本和条件组，缺失填零，错链接与 UTF-8 范围，假科学/发布通过，旧版限定证据删除、未决假成功、未知文件/字段及错误 JSON 类型。`py_compile` 和当时的 `git diff --check` 亦实际返回 0。

两轮使用同一实际 Linux ELF CLI，SHA-256 为 `e56b54395c0527f4bcb8b7361fd0564f62b636874ac76c49784d5a422471f3cb`，127,504,360 字节。Cargo 构建产物有硬链接，故先复制为独立普通文件并记录字节指纹，再通过 audit 严格准入；没有放宽来源或输出链接保护。该二进制留在原运行目录，不入 Git。

## 原件与归档

- 初轮原根：`/tmp/noetome-b06-independent-audit-a80148d2844f`，归档 `initial-originals.zip`，最终记录 `originals/matrix/audit-160.json`。
- 补充后原根：`/tmp/noetome-b06-independent-audit-6be5912af70c`，归档 `extended-originals.zip`，最终记录 `originals/matrix/audit-162.json`。
- 各 ZIP 保留全部命令原件、作者包、所选来源、SQL/资源、来源核对、协调、正常预览及同步篡改的完整输出，含实际祖先链接的链接目标和外部哨兵。链接作为链接条目保存，未跟随读取。
- 仅省略构建二进制和最终快照以前的重复累计 `audit-N.json`；各省略项的 SHA-256、长度及理由在对应 `*-archive.json` 中保留。最终快照保有全部调用与场景。可执行文件保留本地原件及完整指纹。
- 两个核对器版本和测试源码在 `source-pins/` 中按运行前 `inputs.json` 的 SHA-256 校验保存。初版源码通过移除本轮新增的两项矩阵恢复，恢复后与初轮运行前已记录的 SHA-256 完全相同；没有把现在的源码冒充初轮原版。
- 每个成员均实际读取归档后的 bytes，检查 SHA-256、长度及 CRC32；同时通过 ZIP 全量 CRC 检查，并再读原根确认保存的原件未变化。`capture.json` 和两个 `*-archive.json` 保留完整结果。

## 尚未由本记录证明

本记录不证明后续产品修复后的重新验收、完整 Rust/Python workspace 回归、Windows NTFS/持有期操作、真实 Obsidian 渲染及兼容矩阵。相关门槛由主代理及其他独立分工记录。新预览仍为 `knowledge=pending`、`scientific_quality=not_run`、`published=false`，没有执行网络/付费模型调用，也不证明真实文献科学正确率或生产 Vault 发布。
