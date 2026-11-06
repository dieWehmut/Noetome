# 单个扫描页公式的独立作者样例

本目录是可公开的自造工程材料，正文和图像按 CC0-1.0 使用。不包含私人资料、生产 Vault、真实教材知识或科学正确率基准。独立作者先于任何识别结果制作原件、符号事实和 expected；识别器只读取明确选定的 `region.png`，不能读取作者源码、语义声明或 expected 来生成答案。

单一数学样例是 `x_i = -\frac{a_i}{b_i}`。符号定义、单位、两个条件、两个范围与一个例外全部真实出现在扫描页的像素中。定义与单位仅用于工程保留检查，不表达经科学审阅的领域结论。

## 原件与三种扫描状态

`originals/clear`、`originals/unreadable_sign` 和 `originals/unreadable_subscript` 各保存一份纯图像 PDF、800×1000 的整页 PNG 及实际裁剪的 560×200 公式 PNG。公式区域位于一基物理页 1 的像素矩形 `[120,175,560,200]`；页面左上为原点、Y 向下。PDF MediaBox 为 `[0,0,800,1000]`，只含一张无损 RGB 页图，没有字体或隐藏文本层。像素关系和 PDF 页数/渲染单独核验。

这是一页自造的扫描式栅格工程样例，不是物理扫描仪采集的教材，也不支持据此声称通用扫描 PDF、多栏或任意页公式识别已经通过。两个损伤状态由同一作者清晰页派生，不另计两个独立公式作者样例。

- `clear`：分式前负号、x/a/b 的三个下标都清楚可辨，允许核对重建表达；目标数学能力不足时保留原区域。
- `unreadable_sign`：灰色交叉缺损区域彻底遮住分式前符号，没有可以恢复的原 glyph。作者声明和独立 expected 的 expression/tokens 均为 null；必须保存问题与原区域，并阻断成功。
- `unreadable_subscript`：灰色交叉缺损区域彻底遮住分母下标。对应声明与 expected 保持表达未知；不能从清晰对照或另一处下标猜出答案。

`author-layout.json` 固定页/区域几何、缺损位置、原页上下文字面行及其像素框；`author-source-facts.json` 是手工作者事实，不是 OCR 响应或独立验收结果。`expected-*.json` 另外保存完整原件指纹、数学结构、上下文和可/不可成功预期。运行 CLI 不读取 expected。

## CLI cases 与目标能力

`cases/<state>` 严格包含 `case.json`、`semantics.json`、`page.pdf`、`page.png` 和 `region.png`。case 使用明确的 scan_formula Schema1；语义声明 producer 为 `author_scan_declaration`，未知表达必须为 null 并列出 uncertainty。这些手工声明用于核对实际识别，不能标成模型输出。

`profiles/native.json` 复用实际已测的 Obsidian 1.6.7 / MathJax 3.2.2 组合。`unverified.json` 和 `no-images.json` 分别验证未经实测目标及附件能力不足。profile 自报不是实机证据；干净 Obsidian 验收单独记录。

`frozen-sha256.json` 固定所有原件、case、作者事实、独立 expected 与 profile 的 SHA256 和字节数。冻结在识别前及普通 CLI RED 前发生；不能在看到识别输出后将当前结果接纳为新预期。

## 作者生成与字体

`generate_author.py` 用 Pillow 12.3.0 和明确字体路径手工排版，直接绘制字母、下标、等号、负号和分式横线。它只接受尚不存在的新输出根，不覆盖冻结原件，也不导入 OCR、模型库或生产渲染器。选定字体是 DejaVu Serif / DejaVu Sans，授权原文保留在 `author-font-license.txt`，精确字体字节 SHA 记录于布局和生成脚本。字体不进入运行输入，已生成 PNG 才是被识别的扫描原件。

示例重建到新的测试目录，路径由调用者提供：

```text
python fixtures/scan-formula/generate_author.py --output-root <尚不存在的作者生成目录> --serif-font <选定DejaVuSerif.ttf> --sans-font <选定DejaVuSans.ttf> --font-license <选定字体授权原文>
```

生成脚本输出的是作者原件包。CLI case 包装、目标 profile 与冻结清单由本次独立作者在识别前另外固定。既有原件不能由重建或新增包装覆盖。
