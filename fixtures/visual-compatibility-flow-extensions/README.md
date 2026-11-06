# B06V-T03 独立流程标签与正负语义补充包

这是同一个 B06V-T03 任务中的三个流程产品输出补充场景，用于在新的实际阅读器流程引擎中核对 `decimal_ascii_punctuation_v1` 标签和正负关系语义。没有修改原 [13 场景包](../visual-compatibility/README.md) 的任何字节，也没有启动后续产品阶段、调用模型或生成新的科学结论。

本包的原 13 场景锚为 `9d65fc4ae8b3d4b70608c0453b894675aff0d61e2d317acf31b442c54c57915c`。原包在创建/只读重查前后的全部实际文件指纹相等。新包单独冻结，应用版本和实际流程引擎身份由运行时记录另行绑定；旧 profile 原字段不作为此次运行版本证据。准备或预检成功不能批准阅读器能力。

| 场景 ID | 独立作者预期 |
| --- | --- |
| `flow-complex-labels` | `complex_labels`：保留 `预检 [α] \| "卡片"`、`复核 #2 / x < 3`、`归档入口 & 保留` 和正文中的反斜线；三个矩形节点、两条明确“随后”边。文字中的 s2 在 s1 前声明，不改变来源明示的 s1→s2→s3 关系 |
| `flow-promotes` | `promotes`：启用优先控制→提示选择倾向，真实正作用标签必须为 `+：促进`；不填入原文没有给出的比例或概率 |
| `flow-inhibits` | `inhibits`：启用静音控制→蜂鸣器响应，真实负作用标签必须为 `−：抑制`，负号为 U+2212；原文没有断言响应实际发生，不能把可能受抑制的响应升级为已发生 |

每例都保留过程目标、入口、完整节点说明/输入/输出、过程/节点/关系条件和例外，以及来源未列出数组的含义。预期节点身份 `nN` 按明确作者 ID 排序，几何核对要求 TD 端点方向和终点箭头；方向、标点、正负与正文限定分别核对。中文过程明细、步骤和关系锚点必须通过真实点击验证，不能以链接字符串存在代替导航成功。

## 文件和原件

- `cases.json` 沿用原包的 Schema1，严格为 `case_count=3`、`product_case_count=3`、`hand_authored_format_probe_count=0`，case ID 仅上述三项。
- `expected-flow-*.json` 仅从原先调用前冻结的 `process-flow-v1` 作者 expected/gold/原文确定语义；collector 提供完整节点边、可见标签、正负、TD 方向和中文锚点，未从报告、DOM 或新引擎结果反推答案。
- `source-originals/fixtures/process-flow-v1/` 保留完整 54 份小型公开作者包原字节，使原校验器可以核对其完整冻结库存。这里没有新增或改变作者案例。
- `verification-inputs/<case-id>/` 从旧实际 Obsidian 验收归档 `doc/validation/assets/B06-T02/obsidian/originals.zip` 的 `inputs/cases/<author-case>/` 明确选取，含原真实导入项目、SQL、源资料、case、复核、profile 和 Vault；没有创建 fake SQL。
- `selected-output/<case-id>/` 是同一次旧 CLI 的原 Vault 文件字节，包括两个产品 Markdown、双报告、来源和限定；没有改写原件或内部原路径/身份。
- `collector-bridges/` 保留同一次原生验收的 `success/<author-case>/visual-input.json` 原字节，用于旧 collector 接口。原 bridge 的完整计划与原 Vault 计划相等，其作者 process/nodes/edges 与独立新 expected 相等；bridge 布局仍不能替代作者语义。
- `source-pins.json` 记录 ZIP SHA、成员、全部复制指纹和原作者来源。原归档 SHA 为 `e6380e0beacbe3d896ec46be43c0b36322ae33a3d706534e3c492b92caa5a394`，长度 9,593,255 bytes。
- `pre-capture-check.json` 保存实际 `/tmp/noetome-b05-t03-python/bin/python`（3.12.14）调用当前 `verify_process_flow.verify_flow` 的三例只读结果，前后完整文件指纹与原包保护结果。
- `frozen-sha256.json` 固定新包所有其他文件；应独立保存其 SHA 后才启动新应用捕获。

实际预检 **3/3 通过**：重新读取原项目/SQL/来源和复核、全输出/双报告/计划/正文，并逐字核对 bridge 作者语义和完整计划、全部 selected-output 文件与原 Vault。原作者与原件前后不变，模型请求为零。`native_captures=not_run`，科学质量 `not_run`、知识 `pending`、发布 `false`；预检没有运行阅读器或替它批准标点和正负显示。

准备工具使用相同显式入口，以新包的 `--fixture-root` 和独立锚替换原包输入；不得将三个补充场景混写进原 13 项锚。实际场景仍需每个主题各自新 profile/Vault，核对实际引擎安装/运行原件、可见节点/边/标签、端箭头和完整正文/链接后才能写入兼容结果。
