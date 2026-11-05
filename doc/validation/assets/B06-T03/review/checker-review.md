# B06-T03 独立公式校验器审查

审查者负责 Rust 公式实现，未编写本次独立 Python 校验器；本轮只读审查 `scripts/verify_formula.py`、相关 tests 及冻结作者预期，不修改产品。作者样例是公开自造工程材料，本记录只评价来源保真与验收门禁。

## 实际发现及修复复验

1. **Important：符号字段与值可见关联未验证。** 原检查分别对比标签、值和小节的扁平列表。将第一组改为符号标签、定义标签、C 值、单位标签、定义值、单位值，并重算实际字节映射及全部报告/指纹，错误关联仍被接纳。修复者增加逐组相邻七节点、H3 层级和三组标签/字面值对照。同一份原攻击字节复验已拒绝，正常原件仍通过。
2. **Important：Markdown 报告正文可伪造成功。** 保留原唯一 JSON 与固定状态摘要，将正文改为科学质量 passed、扫描公式准确恢复、published=true 和两次网络调用，原门禁仍接纳。修复者在独立核对 JSON 后验证整个可见报告外壳。同一份原攻击字节已拒绝。
3. **资源界限：PNG 解压分配发生在长度拒绝之前。** 1×1、33,710 字节、CRC 完整的 PNG 曾实际先解压 34,603,008 字节，再因应有 4 字节扫描行拒绝。修复者使用有界 decompressobj，按实际扫描行容量加一设最大解压长度，并校验 EOF、unconsumed_tail 和 unused_data。同一 PNG 现只实际分配 5 字节输出后拒绝。

原审查 checker SHA `e77134babec27fde9caa8401ac0f08a3d1db166a68d3b6091f850b06a340ff65`；三项修复后的复验 SHA 及真实结果见 [GREEN receipt](checker-review-green.json)。原 CLI 正常出口 0，原伪造被接纳的实际返回字典、原件、源码和解压观测见 [RED 原件](checker-label-red-originals/review-result.json)、[完整 ZIP](checker-review-red-originals.zip)及[清单](checker-review-red-manifest.json)。ZIP 共 75 件已实际读回，逐字节长度和 SHA 与清单一致。

## 复核内容与限制

独立 expected 的 manifest SHA 固定；真实 source、metadata 和全部附件先比对该冻结预期。case/plan/report/数学表达/原图和源映射不能靠同步改写自身指纹取得通过；知识 pending、科学 not_run、未发布和零网络/付费字段均使用严格类型相等。条件、范围、例外分别由实际 Markdown 节点读取；固定 H2 小节清单和节点类型拒绝隐藏插图或额外表达。完整原始附件字节、相对本地目标、精确文件清单、输出 mapping 分母与双报告均有独立约束。

一个未外推的 grammar 差异已实际记录：Rust 当前允许 `\exp{...}` 和明确乘除后的有符号操作数，独立 grammar 仅接受 `\exp(...)` 且要求此类有符号操作数括号化。这是有限样例之外的保守拒绝差异，不是伪成功绕过。实际两个普通 CLI 出口 0、独立 grammar 拒绝的来源/命令和输出在 [差异原件](checker-label-red-originals/grammar-boundaries-result.json)。本审查不以这 15 个作者场景证明任意 TeX 两套 grammar 等价，或证明数学、科学、OCR 和一般图像能力。

当前三项已发现门禁问题均已在原攻击字节上解除；最终完整回归与独立 audit 结果仍由任务的总体验证记录核对。

## 合法边界收敛后的限定复查

修复者随后按父代理授权，使独立 grammar 接受 `\exp{...}` 与明确乘除后的单个符号，并按括号组计数允许恰 64 层、拒绝 65 层；仍拒绝相邻加减号及双符号操作数。本审查实际再次核对上述合法/非法边界，重读原正常输出、原字段关联攻击与原假成功报告，正常仍通过，两项原攻击仍拒绝。实际最终 checker/test 指纹与结果见 [最终限定 GREEN](checker-review-final-green.json)。这些边界差异已解除，没有据未运行的一般数学样本扩大结论。

最终只读命令实际执行 `python scripts/verify_formula.py check fixtures/formula-fidelity/native fixtures/formula-fidelity/profile-native.json <原件中的baseline或攻击Vault> fixtures/formula-fidelity/expected-native.json`；正常出口 0、两份原攻击各出口 1。完整 argv、退出码和 stdout/stderr 原件指纹见 [命令 receipt](checker-review-final-commands.json)，输出分别保存为本目录 `final-*.stdout.bin / stderr.bin`。
