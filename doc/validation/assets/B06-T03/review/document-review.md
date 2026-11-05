# B06-T03 文档、范围与证据独立审阅

本轮为只读 completion audit；没有修改产品、作者样例、预期、账本状态或其他标准文件。审阅范围为公式契约、B06-T03 验证记录、根 README、doc 入口、架构、数据契约、设计、测试、近期步骤，以及本项独立/真实 Obsidian 归档与源码指纹。

## 当前结论

当前产品、公式契约及已完成独立/原生验收的核心事实一致，没有发现将工程作者样例冒称科学正确率、OCR、任意宏支持或整个 B06 视觉阶段完成的结论。完整 Windows Rust 回归在本轮审阅时仍由父代理跟踪已确认的活进程，因此本审阅不是 task complete 证据，也不把运行中的门槛误判为阻断。

## 发现及处理

1. 验证记录原链接 `assets/B06-T03/refreeze-reason.json` 不存在；实际文件是 `assets/B06-T03/red/refreeze-reason.json`。已向父代理报告，父代理修正后重新读回确认存在。初次检查 12 份相关 Markdown 的 261 个本地链接，仅发现这一项；修正后的同范围 261 个链接已独立重新检查，全部存在。父代理另保存了最终链接检查。
2. 契约原“原图不可辨时阻断”措辞缺少依赖关系：实现只在需要原图表达时将不可辨原区阻断；语义已核对的可信原始 TeX 可使用原生表示。父代理已将该限定写入公式契约，重新读回与 `formula.rs` 的决策分支一致。验证记录中的不可辨场景是 `source_kind=formula_region` 的真实阻断作者样例，不外推为所有附带图片的原生 TeX 都失败。
3. 此前我口头将 audit4 的源码指纹数量误报为八份。实际 `python-tests-stable-final.receipt.json` 和 `final-audit-4-originals.zip` 均为 **七份：两份 Python + 五份 Rust formula 模块**。已经明确向父代理报告，父代理将验证记录对应数量修正为七份，重新读回确认。归档本身没有错误或缺失源码。

## 逐项证据核对

- 公式 case/profile/semantics DTO 与文档字段、严格 nullable、明确直接子附件清单一致；CLI 不读取 expected。TeX 与作者语义转写分别解析，未知变量/定义/宏和表达不符保持阻断，原始字节与编译输入分开保存。没有向旧项目 SourceVersion、科学质量或 Release 冒用新身份。
- 原始 PNG 已选区域的 page/canvas/bounds、实际宽高、8 MiB 输入、2048² 像素、32 MiB 解码器及输出预算、完整 CRC/IEND/APNG 限制与实际实现相符；这些是资源限制，不是性能或峰值 RSS 的实测数字。
- `native/originals.zip` 的 1,570 个成员，集合、CRC、SHA-256 与 byte_length 全部重新读回匹配 `native/archive.json`。ZIP 本身为 3,477,775 字节，SHA `0abb8c4da444d7783a56fb150271c3bf3d2b4ab04b2efa0a32c907ef50bdb500`。
- `independent/final-audit-4-originals.zip` 的 686 个成员，集合、CRC、SHA-256 与长度全部重新读回匹配对应 manifest。七份 `source-pins` 从 ZIP 逐字读出后与当前工作树相同。
- 实际十个 Obsidian 场景的逐例计数重算为：四个原生公式、三个 PNG 回退、三个阻断报告；数学树 4、可见 glyph 44、本地附件鼠标点击 7。七个成功场景的符号组分别为 5/4/4/5/5/5/5，共 33，与真实 DOM 字段归组证据一致。
- README 展示的 `native.png`、`relational.png`、`formula-image.png`、`blocked-unreadable.png` 均有 SHA/长度完全相同的实际归档截图；额外视觉查看原生公式截图确认负号、分式、上下标和符号/定义/单位字段可见。截图没有替代数学树、字段归属、附件点击或文件字节核对。
- `native/final-checker/final-checker-linkage.json` 的十次独立核对均退出 0，每例源输出、复制品与原生读取文件指纹一致。所用最终 checker SHA 为 `585b9688fbcaf838743128c4a35f0d7f869f6f8d93fb151b851def1ce726828b`，与当前源码及独立 audit4 相同。
- 初始错误 MathJax 3.2.1 profile、真实运行时 3.2.2 纠正、普通 CLI RED、harness 失败、三处校验器漏洞 RED→GREEN、合法 grammar 边界收敛和此前被取代的通过记录均保留，没有改写原始数学 expected 来接受实现结果。
- Linux cargo fmt/check 实际退出 0；完整 cargo test 实际退出 101 及 304/100 首库结果被如实登记。最终 CPython 3.12.14 全套 stderr 实际是 231 项、17 跳过、零失败，故 214 通过；两个最终公式 Python 源码 SHA 与 receipt 一致，没有把跳过或 Linux 失败算成通过。
- Windows `source-snapshot.json` 当前为 487 个捕获文件；除之后更新的根 README 外，486 项与当前工作树一致，所有产品/测试文件及四个补充 include_bytes 输入一致。根 README 属于后续文档修改，不是 Rust 产品或测试输入的变化。483→487 的首次编译缺输入失败与补充记录均保留。

## 收尾边界

本审阅确认上述已落地事实和归档。父代理仍需把正在运行的 Windows 完整 Rust 结果、平台保护/回归、最终日志与 Git 推送核对登记后，再判断 B06-T03 可否 complete。一般来源图像知识抽取/重绘、扫描 OCR、任意 PDF 作者源码恢复、科学质量基准、生产 Vault 与完整版本兼容矩阵继续属于剩余路线，本项不能替代这些验收。
