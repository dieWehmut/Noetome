# B06V-T03 最终独立 Python 目标能力与真实 CLI 核对

本分工已固定四个视觉校验器、独立能力 helper 及相关测试。旧作者包、原 13 用例冻结包和流程补充包没有改写；能力仅按根代理完成的正式原生验收逐项登记。初始只允许最低版、Schema 2 尚未批准时的实际结果分别保留在 [基线](python-capability-baseline-review.md)和 [Schema 2 阶段](python-flow-schema2-review.md)，这些历史记录不覆盖成最终状态。

## 独立判据与精确能力

`visual_capability_registry.py` 不导入 Rust 实现，不读取产品报告或输出声明批准目标。MathJax 只接受已测的 `(Obsidian,1.6.7,3.2.2)` 和 `(Obsidian,1.15.0,4.1.3)`，并要求真实布尔 `verified/native_math`。SVG target 分别登记 1.6.7 与 1.15.0；图像校验器继续独立检查 `verified`、`svg_supported`、本地附件和来源语义，并保留 target/flag 两条诊断。

流程保留完整旧 Schema 1 精确 profile；新 Schema 2 必须明确 `mermaid_version:null`、`renderer:{name,version,source_sha256}`，禁止未知/遗漏/多余字段、版本结构混配及非 64 位小写十六进制 SHA。仅登记 `obsidian_1_15_0_pele_flowchart` / Obsidian 1.15.0 / pele 1.8.0 / SHA `9bf34b3a9a19351dc69d0d238e365560331a8ae392c41656df7f5090653253d0` / `html_labels=false` / `decimal_ascii_punctuation_v1` 的精确组合。未知 renderer、SHA、app 或声明均仍有据回退。

实际原生准入由根代理完成；本分工额外只读核对 6 项数学结果的独立冻结数学树、8 项流程结果的冻结节点标签与关系端点身份、实际引擎执行来源 SHA、CLI receipt 和清理原件。来源与结果 SHA 见 [native-authority-readback.json](python-current-capability/native-authority-readback.json)。SVG 另由根代理在 round 05 的浅深两项完整 source/glyph/原 1px bounds/同实例完整 RGBA/附件与清理门槛通过后批准，其 summary SHA 为 `0869101796edb597ae0d69a3768d46e05620a909dfc4b9507cbf2db084633e62`。此只读核对没有新启动原生应用，也不替代根代理的完整正式捕获。

## 显式目标接口及保留的旧默认约束

formula/image 增加 `verify_output(..., target_profile=<明确原件>)`；flow 增加 `verify_flow(..., target_profile=<明确原件>)`，三者 CLI 都支持 `check --target-profile`。默认调用仍使用旧作者目标及 expected 的精确约束。显式目标始终先核对位置参数的原冻结 baseline，再核对指定目标原件与输出副本/报告输入，并从冻结作者语义与独立 registry 重算目标决定；不从待审输出挑选 profile 或生成作者答案。

首次 Schema 2 unknown renderer 普通 CLI 返回正确文字回退，但 flow checker 末尾仍要求它等于旧目标的 `flowchart`，实际失败保留在 `python-current-capability/unknown-flow-v2/flow-branch/`。显式目标分支修正该目标专属断言，同时保留作者 source/gold/review、完整上下文、语义分母、来源阻断及实际 note/plan/report 核对。原默认约束仍会拒绝这个目标切换。不是修改作者 expected 接纳错误内容。

## 当前目标普通 CLI 与字节联结

| 实际审计 | 普通 CLI | 场景 | 同步造假拒绝 | 已捕获原件联结 |
| --- | ---: | ---: | ---: | --- |
| 当前数学/流程及 unknown、mixed、renderer SHA、来源未知 | 19 | 16 | 4 | 两公式与五流程，12 份 Markdown；两公式 compiled/original TeX 和 PNG 全字节相等 |
| 当前扫描四路径，数字环回重放保存的原响应 | 4 | 4 | 原 46 项扫描篡改回归另列 | 两成功 Markdown/recognized/compiled（如有）与四组 crop/page/PDF 全字节相等；两未知无成功表达产物 |
| 当前 SVG/无数据曲线及 unknown/flag/source blocker | 7 | 7 | 3 | 两图像 Markdown、原 PNG 和重绘 SVG 全字节相等 |
| 合计 | 30 | 27 | 7 | 16 份成功表达 Markdown 及相应源表达/图像附件相等 |

分别见 [数学与流程实际记录](python-current-capability/current-target-cli/actual-checks.json)、[扫描重放实际记录](python-current-capability/current-scan-replay/actual-checks.json)、[图像实际记录](python-current-capability/current-image-cli/actual-checks.json)。原 profile 改变仅影响这些目标的报告 metadata，所核对的可见 Markdown、SVG、TeX 与附件相等，因而当前 CLI 可直接联结到已捕获的对应冻结原件。

数学/流程和扫描使用根代理冻结的 ELF SHA `00a0a8942917500270cfa92c0ca54ec4b3724b4b8ac2cf41902e74447704d99b`，其对应实现子集不随后续 SVG 登记改变；图像与最终针对性 Python 使用最终 ELF SHA `038f106a483945b31594009598decec882bc19d53c73ffc1b462c778b793bfe4`。此前另外保存的 5 项 unknown renderer Schema 2 回退采用 math-only/schema2-parser 冻结 ELF，不把该旧构建当成最终全部能力构建，见 [原阶段实际记录](python-current-capability/unknown-flow-v2-final/actual-checks.json)。

扫描只在 `127.0.0.1` 的显式人工 HTTP 服务重放原四份 `response.bin`，原响应 envelope 字节、识别 JSON 语义、全部 uncertainties 与无 confidence 分数状态保持。子进程环境只传必要 OS 项和显式 synthetic 凭据，真实模型调用与收费均为 0，真实外部凭据及 33,185 账本没有访问或修改；本轮累计 budget binding 为 `not_run`。报告正常记录新的人工 transport/usage provenance，不要求它与旧 live metadata 字节相同，也不计成四次新真实识别。

扫描首次联结循环误要求 null recognition 也存在 `recognized.tex`，Core 正确没有创建该文件；这一审计假设错误和修正见 [首轮说明](python-current-capability/current-scan-replay/first-attempt-checker-assumption.json)。修正仅调整 null 状态的文件不存在判据，没有填入识别表达，没有重发前三次人工请求，总共四次环回 POST。负号/下标未知保持 null、blocked，并无 recognized TeX、plan、compiled 或成功 preview。

## 作者诊断文本的独立勘误包

最低版捕获发现原两个 collector `required_texts` 把既有 T02 诊断拼作 `SCAN_SEMANTIC_UNKNOWN`。依据兼容采集前已有的 T02 规范、Core、独立 checker 与两份旧审计，创建完整 [corrected-v2 包](../../../../../fixtures/visual-compatibility-corrected-v2/README.md)，没有修改原包。两份 expected 各仅增加一个 ASCII `S`，其他字段逐值相等、528 个保留原文件逐字节相等，完整 13 项原领域预检实际通过。

原锚 `9d65fc4ae8b3d4b70608c0453b894675aff0d61e2d317acf31b442c54c57915c` 保持；新锚为 `c174f543947b6e72c7ef7e786624c4d2fa870342df79abcd9d0fc7fbb90cc60d`。新包保存原冻结/预检、每份 expected 勘误前后 SHA、旧 authority SHA、实际失败 ZIP/member SHA 与字节保留证明。只有 sign/subscript 的浅深回测使用新包，其他已通过原件不重跑，也不改未知或阻断语义。

## 最终源绑定与检查范围

最终针对性 Python 实际 **151 项、151 通过、零跳过、零失败**：公式 30、图像 30、扫描 46、流程 37、registry 8。新 current、unknown/mixed/forged 声明、来源未知、逐原件读取以及全部既有同步篡改测试均包含。源码与二进制在运行前后字节相等，`git diff --check` 退出 0；完整 stdout/stderr 与 10 份源/测试指纹见 [最终 actual-checks.json](python-current-capability/final-python/actual-checks.json)。最终 [source-pins.json](python-current-capability/final-python/source-pins.json) SHA 为 `4bef516085b9a9a0ddfdfd5eead035f8be4bb9c08273fda1dc308cafe655224f`。

四校验器/helper/测试现已固定，完整 Linux/Windows Python、最终 PE 与完整 Rust 由根代理接续运行并另行记录。本文不宣称这些未由本分工执行的完整平台检查通过，不把人工重放或 bounded engineering fixtures 计成 OCR、模型科学质量或生产 Vault 验收。
