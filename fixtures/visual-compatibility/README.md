# B06V-T03 独立视觉兼容预期与明确原件

本包在本项首次应用捕获前冻结 **12 个产品输出场景和 1 个独立手写格式探针**。它选择既有公开自造工程作者案例，只检验明确原件、必要语义、阅读格式与本地导航；科学质量为 `not_run`，知识为 `pending`，发布为 `false`。没有扫描私人资料、复制生产 Vault、保存 API key 或运行识别请求。

应用版本、安装指纹、实际 MathJax/Mermaid 运行时及兼容通过状态由本项实际应用验收另行绑定。作者预期不预先批准任何版本；旧原件中的 profile 原字符串仍保持，不能当成此次实际运行版本。

## 原件与独立答案

- `cases.json`：13 个明确场景、来源、原件归档位置、校验器输入和捕获前的 `not_run` 状态。
- `expected-*.json`：每例的完整作者语义与机器可读 collector 预期。符号、单位、定义、条件、范围、例外、未知、节点边、图例和空间关系从原先调用前冻结的作者原文/expected 保留；数学树按作者 TeX 手写，没有读取生成 DOM 或从报告反推答案。
- `source-originals/` 与 `source-pins.json`：选定作者原始字节和来源 SHA。公式、图像、流程原校验器要求完整作者包的冻结库存，因此完整复制这些小型公开作者包，未复制运行库、模型文件或 PE。
- `selected-output/<case-id>/`：既有普通 CLI 输出与明确附件的原始字节。没有更改原笔记或附件名，没有把原 JSON 中的旧运行身份、路径或 profile 字符串改成新的声明。`source-pins.json` 保留 ZIP、成员、字节长度与 SHA，只有选定成员被复制。
- `verification-inputs/`：表格/流程对应的原实际导入项目、来源、SQL、case、原复核、计划和输出；不是新建的 fake SQL。供原独立校验器重新打开源资料核对，不能只凭旧报告 `passed` 接纳输出。
- `collector-bridges/`：表格/流程原生捕获时保存的 `visual-input.json` 原字节，用于沿用原 collector 输入格式。bridge 中的生成计划只有在完整原件重查以及独立作者语义核对后才可作为显示布局使用，不能替代本包的作者答案。
- `probe/格式兼容探针.md`：单独标记的手写 Markdown 与 YAML；没有 Noetome 生成、来源抽取、发布或科学验收含义。
- `pre-capture-check.json`：本项实际只读重查记录。12 产品例调用原领域独立校验器通过，另一个探针仅进行作者字节/结构检查；应用缓存、Properties 可见性、Callout 和真实点击仍为 `not_run`。前后输入指纹相等，新增模型请求为零。
- `frozen-sha256.json`：本包全部其他文件的字节锚。其 SHA 应在启动本项捕获前独立保存；不得同步替换作者预期与冻结清单后自称原作者未变。

## 十二个产品场景

| 场景 ID | 明确作者与验收对象 |
| --- | --- |
| `table-escaping-names` | `active-presentation-v1/escaping_names`：两对象、速率 10/12 m/s、2020 年、已列明适用条件；方括号、希腊字、竖线、星号保留，中文对象文件/知识明细链接真实点击 |
| `flow-branch` | `process-flow-v1/branch`：四节点、判定菱形、先后与真假分支、完整签名判据、向下端箭头及明细锚点 |
| `flow-multiline-labels` | `process-flow-v1/multiline_labels`：有据文字回退；真实 `ol > li`、LF 换行与 Tab 可见宽度，全部过程/步骤/关系限定保留 |
| `formula-native` | `formula-fidelity/native`：`C_i(t) = -frac(a,b) t²` 的原 TeX、负号、分子分母、脚标与平方、原区域附件和全部符号定义/单位/前提/范围/例外 |
| `formula-native-relational` | `formula-fidelity/native_relational`：`x_i² ≤ frac(a,b)`；组合上下标及小于等于关系方向。原件来自实际旧 CLI 独立审计，这一场景没有借用不存在的旧原生显示记录 |
| `formula-image-fallback` | `formula-fidelity/image_fallback`：已核对公式的 800×240 原区域图片回退、本地原件点击；没有成功 MathJax 表示 |
| `image-signed-arrows` | `image-fidelity/signed_arrows`：960×520 原 PNG、有限 SVG 的 Feed/Response/Outlet、箭头方向、正/负作用、单位、图例归属与左右空间 |
| `image-curve-no-data` | `image-fidelity/curve_no_data`：960×520 原 PNG；原数据缺失，不生成虚构曲线重绘，原附件真实点击 |
| `scan-clear` | 先前冻结的一个扫描作者公式：清晰等价重建、`x_i = -frac(a_i,b_i)`、三个下标、完整来源上下文及原区域/整页/PDF/识别原响应追溯 |
| `scan-fallback` | 同一个清晰扫描作者的真实原区域回退结果；560×200 原区域、800×1000 原页及本地附件 |
| `scan-sign` | 同一个作者的关键负号缺损状态；expression/tokens 为 null，未知不能从清晰对应物填入，阻断且没有成功 plan/compiled TeX/预览 |
| `scan-subscript` | 同一个作者的分母下标缺损状态；未知保持未知，阻断且没有成功 plan/compiled TeX/预览 |

四份扫描显示输入复用 B06V-T02 已捕获的真实 live 结果原字节，新增请求为零。原活动接口/响应/用量/预算链保持历史；本包的扫描只读领域重查不带累计预算参数，不能把这次兼容输入检查重复计成新的真实识别或独立余额核实。原 live receipts、独立预算锚与恢复假设仍由 B06V-T02 原记录说明。本轮当前 profile 的 CLI 零预算阻断验收须单独执行，不能改写这些旧 live 产物。

表格的原 native bridge 与这里完整复核的 `extended-originals.zip` SQL 包来自同一冻结作者资料的不同真实导入，因此 project/request/occurrence 身份不同。它们的三份完整可见 Markdown 字节已实际核对相同。不能重写两组身份伪装同次运行；复核时验证真实 SQL/来源/协调/预览及作者语义，再要求完整笔记逐字相等。bridge 的显示布局不改变原出处，也不能作为新增来源科学真实性证明。

## 独立格式探针

`hand-authored-format-probe` 在实际应用捕获前独立编写，按 CC0 使用。它明确包含 YAML `title` 字符串、`reviewed: false` 布尔、两项 `aliases` 列表，`note` Callout 原标题/正文、中文 H2 标题、同文件标题 Wikilink、合法 `^compat-block` 与块 Wikilink。

只有实际读取应用 frontmatter 缓存与可见 Properties 控件、Callout 类型和文字，并真实点击两个链接后核对中文目标标题/原块，才能通过格式兼容项。源码中出现字符串或链接不能代替此验收。探针必须在矩阵中单列，不能宣称 Core 已实现这些输出生成能力。

## 首轮失败保留

`pre-capture-check-attempt-1.json` 保留真实首轮预检：只复制部分作者包导致原独立校验器的完整冻结库存门槛拒绝；table bridge 与另次真实导入的完整计划身份不相同；选定 Python 没有 PyYAML。后续补全原作者包，保留两次真实身份而以完整 Markdown 字节联结，并用固定作者原文结构检查探针，未安装新运行依赖、未伪造 SQL、未修改独立 expected 接纳错误产品语义。第二轮 13 项只读检查通过；手写 YAML 的实际应用解析和显示仍须运行。

## Collector 接口

每个 `expected-*.json` 的 `collector` 提供领域所需字段：数学 `notes[].math_trees` 使用 `[tag, children-or-text]`；graph 保存作者 ID/显式 `nN` 映射、节点形状和显示标签、各边端点/真假判据/端箭头及 TD 几何；SVG 保留完整作者结构；`attachment_images` 固定 PNG 自然尺寸；links 保留中文目标和标题/块定位；`blocked_absent` 固定没有成功表达的产物。原作者 `semantic_source` 保留完整必要上下文，即使 collector 为了适配原有接口另读原 plan，也不得删除这些前提或放宽未知门槛。

各个版本、主题和场景使用新的隔离 profile/Vault。不能将探针的格式支持、一个场景的回退或某次显示通过扩成所有版本、任意公式、OCR、一般科学质量或生产 Vault 的支持声明。
