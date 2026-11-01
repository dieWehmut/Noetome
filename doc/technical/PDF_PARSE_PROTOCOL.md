# 单份文本 PDF 解析协议 v2

B03-T02遵循已采纳需求和本批执行卡，扩展真实PDF页/区域与正文，不改变[原始字节v1](SIDECAR_PROTOCOL.md)、来源项目Schema2或编译Schema1。实现前按当前自动推进授权冻结本契约；任务状态只由账本维护。

## 选择与边界

采用一个`pdfplumber` Python adapter。它能提供字形、真实页及图形对象信息，许可证和当前版本由本轮官方源码、包元数据与Windows试验核对。`pypdf`的文本visitor不足以单独保证字形区域，PyMuPDF则另有许可证和分发决策；本项不引入这两个解析入口。Poppler仅用于自造样例的独立页面回看，ReportLab仅用于作者样例生成，不参加生产解析判定。

Host复用已验收的原生argv、Windows Job、三个有界管道、只读输入快照、新工作区和资源复核。可信调用者选择Python及仓库adapter；资料内容、JSON和资源不能选择程序、参数或输出根。Python以`-I -B`启动，只继承现存SystemRoot；依赖和运行时按本任务锁定说明安装，调用不触发下载。生产入口只接真实parser，不接受fake producer。

当前能力是有限文本层与字形定位。OCR、表格/公式/图片语义、复杂阅读顺序及科学正确率没有通过；相关页保留原PDF、实际页号、字形和已提取文字，并进入needs_review或failed。正常单栏文本必须能完成，不以全部needs_review规避正文验收。

## 调用与固定布局

Rust入口为`pdf_parse::parse_pdf(&PdfParserSpec, &Path, &Path, PdfLimits, &AtomicBool)`。`PdfParserSpec`只有可信`python: PathBuf`和`adapter: PathBuf`。CLI为：

```text
noetome source parse-pdf <PDF> <尚不存在的测试根> --python <绝对Python路径> [--timeout-ms <0..30000>]
```

动词后支持`--`分隔以`-`开头的路径；两条数据路径必须独立明确，Python路径是运行时配置。adapter使用构建时已选仓库文件，普通CLI没有fake或任意adapter开关。帮助只读。父目录必须存在，已有根、重叠、链接/junction或不安全Windows路径拒绝。

```text
<new-root>/
  .owner.json       Host request_id和拥有标记
  request.json      Host的完整v2请求
  input/raw.bin     完整原始PDF快照
  artifacts/
    pdf_inventory.bin   严格JSON字形及原始页面证据
    P0_text.txt          页0规范提取文本；其余页按索引编号
  .parse.pending.json  Host待提交报告，仅提交前或失败现场存在
  parse.json        Host最终报告，原子发布；失败不覆盖未知内容
```

资源仍遵循`artifacts/{artifact_id}.bin|.txt`、真实长度/SHA、精确清单、文件身份和单链接规则。Host验证后保留句柄至结果写入及最终复核结束。报告由Host创建；子进程不得写它。未知或未能确认归属的现场保留，不能为了生成报告覆盖或接管。

Host报告Schema1含producer=`parser`、operation=`source_parse_pdf`、ready/needs_review/failed状态、原始input、实际request、已验证response或null、page_display、validation_status、failure、violations、受限diagnostics字节、pid、native_exit_code、processing_elapsed_ms、verified_artifact_count/bytes及report_path=`parse.json`。page_display只从已验证页生成，每项包含physical_page_idx、displayed_physical_page=索引加1、printed_page_label与页状态；未确认页不制造显示条目。

报告最多8MiB。先独占创建`.parse.pending.json`，保持读写/身份句柄，复核报告、全部资源、原始快照及精确目录清单。最后检查取消后，通过仍打开的文件句柄不覆盖地改名为`parse.json`，此原子发布为交付点；发布后返回相同JSON，不把后来取消改成失败。提交前失败或取消保留待提交现场，只能将`parse.json`视为正式报告；待提交内容不能证明成功。同步OS调用不能保证取消在任意指令处抢占。

## 必填严格DTO

全部对象只接受命名对象，拒绝数组、未知字段和重复键；枚举只接受字符串。nullable字段必须显式null，不能遗漏。整数不接受浮点/负数，所有坐标必须有限。SHA为64位小写十六进制；ID遵循v1的ASCII及大小写碰撞规则。

| 对象 | 必填字段与约束 |
| --- | --- |
| PdfRequest | schema_version=2，protocol=`noetome-pdf-single-v2`，request_id=Host UUIDv4，operation=`parse_pdf`，input，workspace，artifact_policy，pdf_policy，options_version=`pdfplumber-glyph-lines-v1` |
| input/workspace/artifact_policy | 与v1共享严格DTO；input仅`input/raw.bin`、实际完整长度/SHA；workspace仅`artifacts` |
| PdfPolicy | max_pages，max_glyphs，max_regions，max_objects |
| PdfResponse | schema_version=2，protocol，request_id，adapter，input，status，page_count，inventory，pages，artifacts，issues |
| PdfAdapter | producer=`parser`，adapter_id=`pdfplumber-text`，adapter_version=`1.0.0`，options_version，runtime |
| PdfRuntime | python，pdfplumber，pdfminer，pypdfium2，pillow，cryptography，cffi，pycparser，charset_normalizer；逐项核对锁定版本 |
| ArtifactRef | artifact_id，sha256；必须指向同一实际资源 |
| PdfPage | physical_page_index，printed_page_label，status，geometry，glyph_count，excluded_glyph_ordinals，text_artifact，regions |
| PdfPageGeometry | media_box，crop_box，rotation，user_unit，display_width_pt，display_height_pt |
| PdfRegion | region_id，ordinal，locator，content，glyph_ordinals |
| PdfLocator | kind=`pdf_page_region`，physical_page_index，bbox_pt=[x0,top,x1,bottom] |
| PdfContent | artifact_id，sha256，start_byte，end_byte；指定页文本的UTF8半开范围 |
| PdfIssue | code，message，page_indices；消息受限，引用只到本轮真实页，未知页库存的问题使用空引用 |

pages必须完整包含`[0,page_count)`，按物理索引排序、无漏页/重复。显示物理页为索引+1；`printed_page_label`当前为null，没有真实印刷页依据。它不等于文件名、章节号、行号或PDF标签。

## 原始字形证据与Core复核

`pdf_inventory.bin`是独立于派生正文/区域的严格资源，由实际解析器读取PDF primitives得到，不从正文或Region倒推。Host复核它的SHA和以下内容，再根据它重算派生输出。它仍是解析器证据，不能证明任意解析器或科学结论正确；自造样例另有作者预期和独立渲染验收。

| Inventory对象 | 必填字段 |
| --- | --- |
| PdfInventory | schema_version=1，input，page_count，pages |
| PdfSourcePage | physical_page_index，geometry，glyphs，visual_objects，decode_error |
| PdfSourceGeometry | media_box，crop_box，rotation，user_unit；均来自原始Page attrs，未被pdfplumber改写 |
| PdfGlyph | ordinal，text，media_bbox，upright，font_size，font_name |
| PdfVisualObjects | images，lines，rectangles，curves，annotations |

字形ordinal按PDF实际字形清单从0连续编号；text保持提取到的原始Unicode，不做lossy替换或把未知CID伪装正确文字。`media_bbox`是旋转后MediaBox局部左上坐标，单位为未乘UserUnit的PDF单位；先减pdfplumber自身mediabox偏移才能得到它。raw MediaBox/CropBox必须从原始PDFPage属性获得，不盲用库已交换轴的cropbox。

完整Page树枚举成功后才保存page_count。页内容解码失败仍保留该物理页，`glyphs/visual_objects`为null、decode_error为明确原因，派生页failed且没有伪造正文。若文档枚举本身失败、加密无法打开，或全局政策越界而不能形成可验证的完整库存，则page_count/inventory均null，pages/artifacts为空、status=failed，问题明确区分原因；null表示没有可供Host验证的完整库存，不宣称库从未读到任何元数据。完整已读原始SHA仍保留。不能用部分缓存重试len(pages)补造完整页数；各项全局政策检查结束前不写资源，避免失败空清单残留未列资源。

## 坐标变换

raw盒为PDF默认用户空间`[x0,y0,x1,y1]`，原点在左下、Y向上。令MediaBox左下/右上为`m0x,m0y,m1x,m1y`，宽W、高H。先令`u=x-m0x, v=y-m0y`；到旋转后MediaBox左上坐标的映射为：

| Rotate | 旋转后点(X,T) | 旋转后Media尺寸 |
| --- | --- | --- |
| 0 | (u, H-v) | (W,H) |
| 90 | (v,u) | (H,W) |
| 180 | (W-u,v) | (W,H) |
| 270 | (H-v,W-u) | (H,W) |

把原始CropBox四角分别映射，取轴对齐最小/最大得到`C=[Cx,Ct,Cr,Cb]`。最终可见页坐标为`((X-Cx)*UserUnit,(T-Ct)*UserUnit)`，显示宽高为`(Cr-Cx,Cb-Ct)*UserUnit`。单位明确是pt，x向右/y向下。Rotate只允许0/90/180/270；UserUnit必须有限、正值且不超过64；盒须有限、非空，CropBox在MediaBox内。异常页进入failed，不夹到合法范围伪造定位。

对字形media_bbox与C求交集后再平移/缩放。完全在CropBox外的字形必须精确列在excluded_glyph_ordinals，原字形仍在inventory；部分裁剪不宣称文字完整，页needs_review。Region bbox是实际参与字形的可见交集矩形并集；Core重算并逐项匹配，不能只检查它在页内。

Python与Rust对派生坐标的比较使用绝对容差1e-6 pt，原始对象身份与资源SHA仍精确匹配。它只容纳浮点运算舍入，不允许以页内任意框代替实际字形区域；作者定位窗口另为独立审阅门槛。

## 文字、顺序和状态

选取可见字形，先按media top、x0、原ordinal排序；距当前行首top不超过3个PDF单位的字形归为同一行，行内按x0及原ordinal排序。相邻非空白字形间距大于3个PDF单位时插入一个空格，其余保留原字形text。每行作为一个Region；页文本为各行以LF连接且非空清单末尾一个LF。Region的UTF8范围仅覆盖该行，不包含分隔LF。Core重算行清单、原ordinal覆盖、文本字节、UTF8边界、范围和bbox；重复/漏字形、少正文、错顺序、错页或错误页内bbox均失败。

字形不能因为大间隙丢失。横向间隙超过72个PDF单位、非upright字形、局部裁剪、同一重算行内最大/最小字号之差超过1个PDF单位、未知CID/替换字符，以及公式特征文字或字体，均产生needs_review；跨行标题字号差异不自动阻断普通单栏文本。公式特征为`=,^,_`及`∑∫√±²³×÷≤≥≠`，或font_name小写含math/symbol；未知CID为`(cid:`、替换字符为U+FFFD。images/lines/rectangles/curves/annotations存在时保留原PDF与页定位并标视觉结构需复核。空白或无文本层均needs_review，不能据零字形/图片数认定已覆盖合法空白。这些是有限支持门槛，不是能识别所有复杂结构的分类器。

Core要求每个failed/needs_review页有可定位问题，已解析页不含阻断问题；全页parsed且全部验证通过才status=complete。存在failed或needs_review页整体partial；文档无完整页库存时整体failed。合法partial/failed不是整份解析成功，CLI返回3并保存已确认范围；非法协议或资源不能附已验证页。

Host聚合complete为ready、partial为needs_review、failed或未验证输出为failed；即使partial仅含parsed与failed页，也保留需复核状态和页级真实失败。

nullable字段全部必填：response.page_count/inventory，page.printed_page_label/geometry/glyph_count/text_artifact，source_page.geometry/glyphs/visual_objects/decode_error。正常空白页有glyph_count=0、空UTF8 text资源及空regions，页needs_review；逐页解码失败glyph_count/text为null、regions和excluded为空。geometry若仍能确认则保留，否则null并failed。所有这些null都不能冒充空成功。

## 限额、错误和验证

沿用v1默认输入64MiB、请求64KiB、stdout4MiB、stderr256KiB、资源16MiB/总64MiB、资源64个、运行30s、清理2s。额外PdfPolicy默认最多63页（为inventory保留一个资源）、32768字形、4096区域、4096视觉对象；公开调用只收紧。PDF枚举/处理超过政策明确失败，不以截断清单报告complete。数字是政策，不是1000页或性能验收。

CLI成功0；输入/配置/协议/路径错误2；真实读取/解析/限额/超时/取消/未完成或写入受阻3；内部/输出错误1。错误具有稳定`PDF_*`或继承的`SIDECAR_*`代码。已确认的raw及本轮现场保留；默认诊断受限，不回显整个原文或无关堆栈。同步OS和CPU校验不能承诺任意时刻硬抢占，分块读取/校验和最终交付继续检查取消。

观察到的子进程非零退出保留`SIDECAR_EXIT_NONZERO`、native_exit_code及受限stderr；native1映射CLI1，native2映射CLI2，其余映射CLI3。分类只依据原生退出码，不把stderr内容当可信协议或已验证PDF响应。

验收需要实际Python子进程、作者冻结的单页/多页正文与区域预期、Poppler页面回看、严格DTO/资源/定位篡改负例，以及扫描/空白/损坏/加密/表格/公式/图片/多栏/旋转/CropBox/UserUnit/部分页失败矩阵。测试不能把parser输出拿来生成expected，现有fake回归也不依赖PDF库。实现与实测裁定在[B03-T02计划](../execution/B03-T02_PLAN.md)及后续实际验证记录维护。
