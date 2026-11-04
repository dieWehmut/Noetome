# B05-T03 独立整分支审查

审查范围：`d935270` 至冻结提交 `dcb6b19` 加当前工作区的 Schema3 Rust 实现、v2 背景构造抽取、独立 Python 核对/审计、作者样例、普通 CLI 测试及冲突契约。审查者未参与本项实现或样例编写。未重复完整测试矩阵；Linux 平台不具备 Windows sidecar/桌面完整回归的限制由控制流程单独记录。

## 结论

没有发现 Critical。发现两项 Important 验收门槛问题；其中祖先链接问题由另一审查者先复现并已交给 Python 实现者处理。本文件记录修复前审查结论，不证明这些问题已解除或整个任务已经完成。

### Important R1：反证样例只证明结构保留，不能证明有效反证与被反驳原子的对应

`fixtures/knowledge-coordination-v3/counter_support-a.txt` 声称速度为 10，附加句为“反证记录：同一范围的速度 12 m/s 不获本来源支持。”，但 `authors.json` 及候选把该句作为 `a-speed`（值 10）的 `refutes` 链接。这句反对的是 12，不能作为对 10 的正确反证。

只读核对真实 CLI 保存根实际得到 independently_verified=true；报告完整保留值 10 原子及 `a-refutes` 的值 12 引文。证据见 `independent-review-counter-receipt.json`。这不是 Rust 证据保留算法错误：现有 source_review 只评估 supports，额外关系是声明且 science=not_run。它是当前“反证”验收样例的语义对应缺口，不能用计数和字段存在升级成有效反证验收。

建议保留已冻结 19 案例和失败/通过历史，明确将旧案例称为结构保留；另追加调用前冻结的公开作者案例，例如原子值 10、实际引用明确反驳相同范围的 10，并保留另一个值 12 的来源。实际普通 CLI、独立核对及同步删除该反证的伪造拒绝应同时有证据。不要求在本项实现任意自然语言反证理解或科学裁决。

### Important R2：独立 v3 核对准入遗漏祖先目录链接

修复前 `_verify` 使用 `inventory(root)`，它检查根和下级对象的 lstat，但不检查传入路径的每个祖先组件。另一审查者已使用 `/tmp/noetome-b05-t03-fresh-review/valid-aliased-root` 取得真实 mismatch：普通 Rust CLI 对带链接祖先的输入返回 2，而独立 v3 checker 对同一保存结果接受。它违反沿用原输入路径保护的准入语义；Python 实现者正在增加仅 v3 的祖先组件检查。

建议保留该 RED，并验证直接链接根、祖先链接根、普通绝对/相对根的拒绝与接受，以及已有有效 v3 输出与全部 v1/v2 行为。此审查尚未检查后续补丁，不能视为复审通过。

## 已核对的实现性质

- v3 input、identity、report、IR 显式版本分派，旧 v1/v2 分支与分类未扩展新关系；共享 v2 抽取只移动原背景构造逻辑。
- 对象/属性先隔离；unknown 原出现自身待复核，无争议互补对象不会因另一对象问题自动变为争议。完整条件/例外、单位及已知时间约束冲突；不同时间按有据先后保留，未知/重叠时间保留不足证据。
- 精确 ASCII 十进制比较不使用浮点，原词法事实不合并，signed zero 与末尾零比较一致；超长/非十进制值不被推测成可比较数。
- 原 atom 保留所有链接，original_evidence 按首次链接保留唯一证据 ID；evidence_links 数实际链接，counter_evidence_links 数 refutes，避免同 ID 不同关系误计为两个资源。
- 未决报告 canonical_facts=null、无成功 IR；无自动赢家或发布成功。每份处理记录关联完整原出现，JSON/Markdown 的状态和计数由同事实构造。
- Python 核对重读原 source_review 九件、真实来源与背景，以作者 expected 重建关系和全部分母，再完整比较 report、IR、可见 Markdown 及保存原件；不根据 Rust passed 标记放行。

## 审查限制

本审查是有界工程实现审查。它不验证科学准确率、任意自然语言反证、完整 PDF 协调、GUI 对本功能的接入、生产 Vault 写入或所有平台兼容性。最终完整回归、门禁及修复后限定复审由控制流程补齐。
