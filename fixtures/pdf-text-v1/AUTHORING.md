# 固定 ASCII PDF 作者与审阅说明

本目录的15份PDF为自造、可公开的结构测试材料。正文、页树顺序、Courier12文字矩阵、原始MediaBox/CropBox/Rotate/UserUnit和定位窗口均在实际adapter抽取前固定。它们不含私人资料，也不证明科学结论、OCR、公式语义、模型或知识质量。

`author.py`只调用标准库、Pillow作者位图和ReportLab作者加密，不导入pdfplumber/pdfminer或消费解析结果。14份资料由直接PDF对象作者构造（含刻意损坏PDF）；真实加密资料由ReportLab 4.4.9构造，公开测试user密码为`fixture-open`，owner密码为`fixture-owner`。扫描图像使用Pillow 12.3.0的bundled default font，自造RGB字图；`scan-source.png`为该实际位图源，不包含selectable文字。

作者脚本只独占创建尚不存在的输出，任何一个输出已存在便拒绝覆盖。复现作者行为时应复制脚本到一个全新目录；不能直接运行脚本覆盖当前基准。PDF创建前已成功执行PDF技能author marker，create/pdf/expected-output-count=15，且仅执行一次。

首次生成与冻结时间为 **2026-11-01T11:55:03.952845+00:00**。`freeze.json`记录15份实际原始PDF的字节数/SHA及冻结时间；`expected.json`为作者金标准，SHA256固定为：

```text
b9ee562d3b180754cdde8aca5de76cb79baf877204ed5384bd35652c63caca77
```

作者脚本SHA256为`37bdbddd273169c363850a6f34293f5efa05af5bdec60e4a80cd339baba97e1e`。expected包含从字面正文计算的完整UTF8/LF文本、SHA、半开字节范围、原始Tm以及由作者四角矩阵计算的显示位置窗口。基准更新必须先说明作者事实为何改变并重新独立审阅，不能接受当前parser产物自动更新expected。

Courier Regular为Base14定宽字体：字号12 user units、每ASCII advance7.2 user units，Tc/Tw=0、Tz=100；常规UserUnit1即pt。每行baseline局部glyph包络y偏移[-4,+12]，x端点±1 user unit，并有更窄的y端点窗口。UserUnit2的实际pt字号/advance/位置和窗口均乘2。窗口表达字体度量容差，不把可见墨迹bbox等同字体bbox。

`expected_label`为Host报告状态，页级`expected_status`遵循协议parsed/needs_review/failed。F01整体needs_review、完整页状态为[parsed,failed,parsed]；损坏/默认无密码加密无法形成完整page inventory，预期page_count=null与空派生页清单，作者知道加密资料实际有1页的事实另存于`independent_designed_pages`。所有资料没有印刷页脚或PageLabels，printed_page_label必须null。

| Case | 原始资料 | 作者页数 | 预期 |
| --- | --- | --- | --- |
| N01 | single-column.pdf | 1 | ready，全部正文/范围/位置 |
| N02 | multi-page-single-column.pdf | 3 | ready，完整0/1/2物理页与不同x位置 |
| E01 | scan-only.pdf | 1 | needs_review，真实raster字图且无文本层 |
| E02 | blank.pdf | 1 | needs_review，独立白页但未自动排除 |
| E03 | damaged.pdf | 未知 | failed，截断Catalog/无Pages与xref |
| E04 | encrypted.pdf | 1 | failed，默认未提供公开测试密码 |
| C01 | table.pdf | 1 | needs_review，正文位置保留，行列未验证 |
| C02 | formula.pdf | 1 | needs_review，可见分式，语义未验证 |
| C03 | image.pdf | 1 | needs_review，红蓝图像/caption关联未验证 |
| C04 | two-columns.pdf | 1 | needs_review，stream顺序故意与阅读顺序不一致 |
| P01 | rotation-upright.pdf | 4 | ready，0/90/180/270真实Rotate加反向文字Tm |
| P02 | crop-origin.pdf | 2 | ready，非零Media原点、非对称Crop及0/90Rotate |
| P03 | rotation-vertical.pdf | 1 | needs_review，实际显示竖排方向 |
| P04 | userunit-two.pdf | 1 | ready，UserUnit2、pt尺度乘2 |
| F01 | mixed-page-failure.pdf | 3 | needs_review，页1未知Filter实际解码失败 |

独立页面回看使用Windows x64 **Poppler26.07.0**。首次实际运行49条命令，正常/加密有密码/混合资料共22页PNG，另外7页MediaBox预览；4条非零命令为损坏资料的info/render与加密资料无密码的info/render。正常N01/N02全文、表格grid、视觉分式、红蓝图片、两栏、全部旋转横排、非零Crop位置、扫描字图和真实竖排均已通过AI视觉回看。空白页为白页；F01页1亦渲染为白页，但实际stderr为`Unknown filter 'B03FixtureUnsupported'`，不能因Poppler该命令exit0就当页面解析成功。完整3页由pdfinfo独立确认，页0/2文本可见。

实际命令/原始stdout-stderr字节/逐页PNG SHA和尺寸保存在本轮 `.artifacts/B03-T02-fixture/poppler/poppler-evidence.json`，contact.png及各页PNG供root继续回看。生成PNG的脚本只读取原始PDF与作者expected，不读取adapter定位。此记录为作者与独立渲染器/AI审阅，不称为人工金标准。

**UserUnit渲染限制**：该Poppler26.07.0的pdfinfo及pdftoppm在本样例没有原生体现UserUnit2的实际pt尺度（144dpi仍给1224×1584、info给612×792）。其截图只证明可见文字和raw位置；P04的pt尺度由已冻结原始`/UserUnit 2`及独立作者数学验证，不能宣称Poppler已经验证native物理尺度，也不修改作者pt预期迁就渲染器。

旧CLI真实红灯位于 `.artifacts/B03-T02-fixture/old-cli-red.json`：对已存在N01及新输出目录、显式选择Python，期待exit0，实际旧CLIexit2/CLI_INVALID_ARGUMENT，stdout空且未创建目录。实现验收由`crates/noetome-cli/tests/pdf_parse.rs`通过真实新CLI/子进程核对manifest与逐页实际资源；自洽伪造glyph/region/SHA的位置数据由独立作者expected拒绝，不能把Core内部一致性误说成任意PDF parser语义正确。
