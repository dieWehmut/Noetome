# 自造 PDF 文本/边界 fixture v1

这些短 PDF 为本项目自行编写的公开工程样例，不含用户资料，不表达科学正确率，也不是人工知识金标准。`generate_fixtures.py` 不导入 pdfplumber/pdfminer 或 Noetome；用可读的未压缩 PDF 页树和字面 drawing commands 编写原始字节。ReportLab 仅提供 authoring Helvetica 字宽和加密 PDF，Pillow 仅提供无文本层的 raster。

最初14个样例在待测adapter首次运行前固定原始SHA/长度、authoring页数、关键完整正文及顺序、字号/baseline和显示CropBox坐标。后续独立审查按作者规则补齐旋转字形，并在columns/blank各增加1页回归；当前14PDF有25个可渲染页。SHA来自作者原始字节，文本和bbox来自drawing/字体度量，不从parser输出生成。修订须解释并审阅，不能自动接受当前输出为预期；初始19页、扩旋转23页及当前25页的实际证据分别保留。

| 文件 | 独立预期与用途 |
| --- | --- |
| single-page.pdf | 一页单栏，三行完整正文及非 ASCII 字符，物理页 1；label 未知 |
| multiple-pages.pdf | 三个物理页，逐页字面全文/顺序/位置，漏中间页或末页不得通过 |
| crop-offset.pdf | 非零 MediaBox 与 CropBox 偏移，全文位于 CropBox 内；显示原点独立 |
| rotations.pdf | 八页：0/90/180/270 度、三种旋转的偏移 CropBox、UserUnit=2；逐字符框和变换独立固定，后七页需复核 |
| scanned-page.pdf | 仅 raster XObject，无文字操作符；不能把空文本当覆盖完成 |
| blank-page.pdf | 两页：无文字操作符及仅三个空格；都需未知空白复核，后者保留非空文字层 |
| damaged.pdf | 无完整页树/xref，解析失败且不声称完整页数 |
| encrypted.pdf | 自造加密一页，公开测试密码 fixture-only / fixture-owner，仅用于独立 authoring 回看；产品不接密码，失败且页总数未知 |
| table-page.pdf | 两列表格及线条，布局/结构需复核 |
| formula-page.pdf | 上标字号与基线变化，不能把拼接正文当公式已保留 |
| image-page.pdf | 文字与图片关联需复核，原 PDF 证据保留 |
| columns-page.pdf | 两页：对齐基线及错开12pt的双栏；不承诺可靠阅读顺序，不能仅查同一行大间隔 |
| partial-page-failure.pdf | 第一页有效，第二页 MediaBox 非数字；保留已确认第一页并报告失败 |
| layout-order.pdf | 字面 drawing 顺序与页面 top-to-bottom 顺序不同；预期按实际视觉阅读顺序固定 |

单栏 bbox 是 Helvetica 字符布局包络，而非每个可见墨迹的像素框。预期以字宽和 baseline 手工公式确定，允许 0.08pt 的浮点差；页面 PNG 上另画 authoring 预期框回看，页内任意错位框不能仅因未越界就通过。

`glyph_positions` 独立保留每个 authoring 字符的顺序、文字及旋转/缩放后包络；review 页也核对已确认字形位置，不因不支持阅读顺序而跳过位置。`geometry` 独立固定矩阵、显示宽高与 UserUnit。初始四页旋转样例的 oracle 缺口由独立审查发现并按 authoring 补齐，未使用待测 parser 的输出更新预期；初始记录仍保留在 Git。

后续审查发现错开栏和纯空格误parsed；新增页按独立literal/baseline先固定预期，再实跑失败和修复。其他12份PDF字节保持不变。多码点ToUnicode另在测试中动态作者CMap核对空格边界及needs_review，不纳入这组Helvetica bbox支持声明，编码字体宽度仍需复核。

复现时选定已安装的 Python 解释器运行 generator；它会重写这组自造 fixture，因此普通测试只读固定字节，不调用 generator。正常回归不自动更新 expected。实际页面审阅、支持矩阵和测量在 B03-T02 的验证记录中维护。
