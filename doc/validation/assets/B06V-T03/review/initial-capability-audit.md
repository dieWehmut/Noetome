# B06V-T03 初始能力与原生采集器审查

本项为当前兼容矩阵任务内的只读初查，基于任务已 start 后的工作树及已有采集源码。没有修改产品、账本或旧原件，没有运行模型、安装应用或启动后续阶段。以下说明源码中的现有行为及需要本项实际验证的边界，不能作为新版本兼容通过证据。

## 编译器与独立核对位置

| 已有能力 | 当前产品判据位置 | 现有明确支持组合 / 保守行为 | 独立 Python 判据 |
| --- | --- | --- | --- |
| 比较表 / 中文内部链接 | `knowledge_presentation_render.rs` 的有限 Markdown writer / reader | 无版本 profile；内容按实际 plan、来源及协调核对。跨版本声明应放在实测矩阵，不能从没有版本限制推定任意版本通过 | `verify_knowledge_presentation.py` |
| Mermaid 流程 | `process_flow_plan.rs:195` | 精确匹配 profile ID `obsidian_1_6_7_flowchart`、Obsidian `1.6.7`、Mermaid `10.6.1`、supported、`decimal_ascii_punctuation_v1`、html labels；任一不符为 `FLOW_TARGET_UNVERIFIED`，保留有据 ordered list | `verify_process_flow.py:323` 精确整个 supported 字典 |
| 来源 TeX / MathJax | `formula.rs:132` | `verified && native_math` 加 Obsidian `1.6.7` / MathJax `3.2.2`；未知组合为 `FORMULA_NATIVE_UNAVAILABLE`，原区域可用才回退图片；dfrac 控制、源码等价与未知符号另外核对 | `verify_formula.py:324` |
| 图像有限 SVG 重绘 | `image_fidelity.rs:506` | verified 加 Obsidian `1.6.7`；未知组合为 `IMAGE_TARGET_UNVERIFIED`，SVG 不支持为 `IMAGE_SVG_UNAVAILABLE`，可辨原图回退。无数据曲线 / 观察原图另有强制政策 | `verify_image.py:180` |
| 扫描重建 MathJax | `scan_formula.rs:202` | 与来源公式相同的 Obsidian / MathJax 精确组合，未知目标为 `SCAN_NATIVE_UNAVAILABLE` 原区域回退。来源 / 模型关键未知、表达不等价、预算失败先阻断，目标能力不解除阻断 | `verify_scan_formula.py:546` |

相应 profile DTO 为 `process_flow_model.rs:57`、`formula_model.rs:83`、`image_model.rs:133`；扫描复用公式 profile。当前 profile 的 `verified=true` 不能启用任意陌生版本；这四处已有额外精确版本判据。与此同时，profile 本身只是调用者选择的目标，不能证明当前打开的 Obsidian 实际就是所选版本，显示阶段必须另验证安装、运行与输出绑定。

建议产品侧使用一个小型的**按能力区分的已实测精确组合表**，让公式 / 扫描共享 MathJax 判据，让流程与 SVG 查询各自能力。只录入完成本项原生验收的组合；不要改成 `>= 1.6.7`、任意同 major / minor、或仅 `verified=true`。已有 `native_math`、SVG、dfrac、html labels、codec 等 profile 开关应继续作为必要条件，能力表只补充已有实测组合。

独立 Python 语义核对应保持独立实现 / 作者预期，不能导入产品侧判断函数来制造同错通过。可以各自读取严格且有固定作者锚的矩阵预期，另核对产品能力记录与实际 passed 原件、运行版本及安装 / 资源指纹一致；同步改普通 SHA 不应替代外部先冻结的锚。旧冻结作者 pack 和旧 profile 不需要改写；本项新增明确目标配置及矩阵输入应有新的独立冻结。

## 原生采集器哪些可用，哪些需要补齐

| 原采集器 | 已实际实现的观察 | 跨版本验收需要调整 / 补充 |
| --- | --- | --- |
| `B06-T01/obsidian/capture_obsidian.mjs` | 实际 remote app version、独立 profile / Vault 路径、空社区插件；原生表格逐格内容、中文链接目标及真实点击后标题 | 不硬编码 app 版本，但也没有断言与选定目标版本相等；较旧单次 DOM 读取需要防虚拟滚动遗漏，并绑定源文件实际字节到冻结作者预期。table 期望来自 plan 的部分须先有独立来源核对 |
| `B06-T02/obsidian/capture_obsidian.mjs` | 双轮真实滚动；来源过程 / 节点 / 关系独立 expected；SVG 节点形状、中文和正负 / 判据标签、LS / LE 端点、marker-end、TD 实际方向；有据文字 / LF / Tab 回退、中文链接点击及目标标题 | 第31行硬断言实际 app `1.6.7`。未读取实际加载 Mermaid 的版本或资源 SHA；不能仅改这行版本后继续沿用旧归档外挂的 `10.6.1` |
| `B06-T03/native/capture_formula.mjs` | 实际 app version 与源文件字节；双轮滚动；MathML 或可见 CHTML 数学树、字形面积、脚标上下方向、分子分母布局、错误节点；本地图片加载 / 自然尺寸，附件链接解析 / 实际点击；明确 blocked_absent | 第28行硬断言 app `1.6.7`。`window.MathJax.version` 是实际观察，第230行可按 spec 核对版本；需以本项独立安装 / 运行预期替代旧 app 常量。MathJax CHTML 的 combined-script 顺序和特定标签是已测解析策略，不能假定新运行时结构相同；如实际变化应保留失败，补有据解析后重验 |
| `B06V-T01/native/capture_image.mjs` | 精确源字节；原生 SVG 外部 img 的实际 RGBA 解码；同一已验证 Obsidian 实例的辅助 Electron SVG geometry 与原生 RGBA SHA 相等；独立标签 / 正负箭头 / 单位 / 图例 / 空间 / 白色不透明画布与对比；实际 PNG / 附件 | 第113行硬断言 app `1.6.7`；保持原生 img 与辅助同实例联结，不能只拿独立浏览器 SVG 截图代替。PNG 附件点击原通用版只确认 active path；可复用 B06V-T02 生成的强化 image-view / 自然尺寸 / 本地 URL 校验 |
| `B06V-T02/native-live-final/*/capture-script.mjs` | 由上面的公式采集器派生，增加实际主题确认和点击后 image view / 本地 URL / 完整加载 / 固定尺寸检查；保存真实原生与原区域回退 | 仍继承 app `1.6.7` 常量。此前 `prepare_scan_obsidian.py` 还固定 B06V-T02 PE、安装 EXE / ASAR 与 MathJax `3.2.2`；保留其历史窄入口，新矩阵另有严格输入准备，不能直接放宽既有预算原件 / 运行时绑定 |

MathJax 与 Mermaid 要分别检测。当前 MathJax 采集器读取 `window.MathJax.version`，若只有图像 / 阻断报告且模块没有加载，可以如实记录未加载；不要把 null 改成 profile 值。原 Mermaid bundler 检查见 `B06-T02/capability-probe/inspect_bundle.py`：从明确 ASAR 读取 `lib/mermaid.min.js`，核对 member integrity、源码头 `mermaid@10.6.1` 与内部 version literal；原记录明确 global 不导出 `.version`。这是安装资源证据，仍应与本项**同一运行实例实际加载的脚本 / 资源 URL 和 SHA**绑定，例如采集实际 local resource bytes 或 CDP executed script source。保存安装 ASAR / 内部 package version / Mermaid member SHA 与实际加载证据，不从网上另下载一个同名 Mermaid 声称运行时版本。

实际 Obsidian app 版本、底层 Electron / Chromium 与安装 EXE / ASAR 应各自记录，避免只记录一个 launcher 版本而忽略资源包或自更新后的应用差异。独立新 profile、禁自动更新、确切 Vault 路径、源 byte pins、无社区插件和拥有的进程清理仍须继续检查。

## 建议实际反例：同步伪造不能解除语义门槛

1. 在合法来源上将未知目标版本和全部 capability flags 改成 supported / verified，并同步报告、计划和普通指纹：流程仍有据文字，公式 / 扫描原图，图像原图；未知资源版本与已测 app 混配也不能原生成功。现有 `process_flow_plan.rs:632`、`test_verify_formula.py:74`、`test_verify_image.py:45` 已分别覆盖部分未知目标，矩阵需覆盖统一新入口与新组合，防止加入宽版本判断。
2. 对可辨公式改负号或脚标 / 删除前提，对图像改关系正负 / 方向，对流程改端点 / 判据，同时把目标变成合法的新已测版本并重算原响应、报告、plan 和文件 SHA：独立作者语义依然必须拒绝。能力表只能改变表达选择，不能提供来源等价证据。
3. 对两个扫描缺损状态，伪造合法新目标、`verified=true`、无 uncertainty 和猜出的清晰表达，甚至复制清晰 plan / 预览并更新普通指纹：仍按冻结作者的 sign / subscript unknown 阻断，成功文件不得出现。普通 CLI 的 `crates/noetome-cli/tests/scan_formula.rs:188` 已实测 clear guess 不解除来源 unknown；本项版本矩阵应再检查各目标对应产物。
4. 对真实显示记录伪造 `identity.appVersion`、MathJax / Mermaid 字段和矩阵 passed，并同步清单 SHA：独立捕获前的安装 / 原输出 / 作者锚、实际 DOM、实际资源 SHA 与被修改记录不相等时必须拒绝。单纯把旧1.6.7截图或旧固定资源标签重新命名为新版本，不构成新版本验收。
5. 对未知 target 的文字 / 原图回退伪造 representation 为 native / SVG，同时沿用语义正确的正文：独立分类仍应拒绝不被实测能力表允许的表达，避免因为正文正确而接受假兼容声明。

本项可以使用已保存并核对的 B06V-T02 真实识别 / 阻断原件作为跨版本显示输入；若要证明新 profile 经普通扫描 CLI 选择原生表达，可另做明确数字环回人工响应的工程行为验证，须与此前真实识别 / 实际计量分开记录，不能重写旧真实请求、profile 或预算链来冒充新调用。无需新增收费模型请求。

当前实测新增版的能力仍待本项原生运行验收，不能由源码审查判定完成。Linux 完整 Rust test 不在本次跨版本 Windows Obsidian 显示任务的推定范围；既有 Windows 完整回归和当前平台跳过应按最终源绑定保留 / 重跑，也不能以此宣称 Linux 完整运行兼容。
