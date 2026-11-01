# 单请求文本 PDF adapter

本目录是 `noetome-pdf-single-v2` 的唯一实际文字解析入口。`adapter.py` 使用 pdfplumber 0.11.9，其 pdfminer.six 20251230 提供实际页树、字形和几何。pypdfium2 是该包的原生依赖；本 adapter 不使用它建立第二套文字结果。协议与确定行算法见 [PDF_PARSE_PROTOCOL.md](../../doc/technical/PDF_PARSE_PROTOCOL.md)。

## 运行时与依赖

实际验收平台为 Windows x64 / CPython 3.12.14。`runtime-lock.json` 保存协议九个版本字段、八个 distribution 的精确版本/Requires-Python/Requires-Dist，以及27份已安装许可证的真实 SHA 和字节长度。`requirements.txt` 锁定完整解析依赖闭包；reportlab 4.4.9只属于作者 fixture 工具，不是 parser 依赖。

此闭包声明的 Python 下限整体为3.10，adapter 当前精确要求3.12.14；包支持范围不能代替本项目兼容性验收。只有当前锁定平台进行了实际测试。版本锁不冒充完整原始 wheel 档案已验证来源。

解析调用不会下载或安装依赖。准备新的受信运行时时，可用包含八包相应 Windows x64 wheel 的离线目录显式安装；解释器由调用者选择，不在仓库写死开发机路径：

```powershell
& $Python -I -m pip install --no-index --no-deps --find-links $OfflineWheels --requirement sidecars/pdf_parser/requirements.txt
```

Host使用选择的绝对Python路径与可信adapter路径，原生argv为 `-I -B <adapter.py>`，cwd为其独占的新测试根，仅传现存 SystemRoot。`-I` 排除用户site、脚本目录/cwd和Python环境变量污染，仍会运行所选运行时的系统site与`.pth`；这不构成OS沙盒。`-B`禁止新写pyc。当前SystemRoot-only实际库导入和真实PDF解析均通过。

## 输入、输出和失败

stdin必须是一个UTF8严格命名JSON对象：schema2、UUIDv4、固定protocol/operation/options、完整原始SHA/长度、固定`input/raw.bin`/`artifacts`及只收紧的ArtifactPolicy/PdfPolicy。重复键、未知/遗漏字段、数组对象、float/bool整数、无效SHA/UUID、未知选项或可逃逸路径直接拒绝。输入完整分块读取并核对打开/读取后的身份、长度和SHA；无法确认完整读取时不输出伪造的验证响应。

子进程只向空的自有`artifacts`目录独占创建 `pdf_inventory.bin` 和各成功解码页 `P<物理索引>_text.txt`。所有政策及资源大小检查在写文件前完成，stdout是一份唯一JSON，诊断走受限stderr。Host再核对实际资源、状态、原始字形、正文与定位后决定CLI退出码；adapter的合法failed/partial数据响应仍退出0，不能把它当作整份解析成功。协议/配置错误退出2，固定输入或资源IO失败退出3，内部错误退出1。

完整Page树枚举成功并核对根Count后才宣告page_count；不读取异常留下的pdfplumber.pages部分缓存。页内容失败保留物理页、已确认raw geometry和可定位诊断，前后正常页继续解析。pdfplumber 0.11.9的PDF.close会清空缓存后再枚举pages，所以adapter显式释放逐页缓存和自有BytesIO，不调用会重试失败枚举的关闭路径。

## 坐标、文字和有限能力

Inventory保存原始MediaBox/CropBox/Rotate/UserUnit，glyph bbox先减pdfplumber MediaBox偏移，成为旋转MediaBox局部左上、向下Y的未缩放PDF单位。按协议对raw CropBox四角变换，再求字形可见交集，平移并乘UserUnit，得到显示CropBox局部pt。非对称旋转不能盲用pdfplumber.cropbox。完全不可见字形逐项保留并列入excluded，局部裁剪需要复核。

原始glyph Unicode保持不做lossy替换、规范化或ligature expansion。行以raw top/x0/ordinal排序，距当前行首top不超过3 PDF单位归组，行内x0/ordinal排序；相邻非空白glyph间距大于3插入一个空格。每行一个Region，文本以LF连接并在非空行清单末尾保留LF，UTF8半开范围不含分隔LF。空白定义与Rust Unicode White_Space一致，特别保留Python额外视为空白的U+001C至U+001F。

当前实际自造样例验证了单页/多页单栏正文、全部物理页、四种Rotate下反向Tm显示横排、非零Media原点/非对称CropBox、UserUnit=2的真实窄位置窗口；混合三页解码失败保留 `[parsed,failed,parsed]`。独立作者预期与原始SHA在 [fixtures/pdf-text-v1/expected.json](../../fixtures/pdf-text-v1/expected.json) 冻结，未从adapter输出生成expected；最终Host/CLI和独立Poppler回看结果由B03-T02验收记录维护。

无文字层扫描、未知空白、图片/线/矩形/曲线/annotation、公式特征、竖排、不可靠大间隙顺序、同一行明显混合字号、局部裁剪或未知CID均需要复核。加密/损坏资料没有假造页数。公式特征检测只是有限阳性信号；零图片/零表格/零字符不证明结构或来源覆盖。OCR、表格/公式/图片语义、通用复杂阅读顺序、知识抽取和科学正确率没有实现，也未宣称1000页、24小时或其他平台通过。

## 许可证

`licenses/`完整保留当前八包27份随包许可，路径/SHA在runtime-lock逐项记录。pdfplumber/pdfminer/charset-normalizer是MIT；cffi为MIT-0，pycparser为BSD-3-Clause；Pillow为MIT-CMU并在完整LICENSE中合并原生库许可；cryptography为Apache-2.0 OR BSD-3-Clause并保留三份文本。pypdfium2是BSD-3-Clause/Apache-2.0及所含PDFium依赖许可，保留全部18文件树，不能将整个栈简称MIT。

Pillow LICENSE保持完整78,016字节，其中有12个原生库许可章节；cryptography包含LICENSE、LICENSE.APACHE、LICENSE.BSD，当前没有另行声称存在NOTICE。后续换wheel或自编译应按实际分发重新核对。当前许可说明描述已读依赖与保留事实，不裁定最终应用打包或发布的全部许可问题。

官方依据：[pdfplumber v0.11.9源码和MIT许可](https://github.com/jsvine/pdfplumber/tree/v0.11.9)、[pdfminer 20251230源码](https://github.com/pdfminer/pdfminer.six/tree/20251230)、[pypdfium2发行metadata](https://pypi.org/pypi/pypdfium2/5.14.0/json)、[Python3.12隔离参数](https://docs.python.org/3.12/using/cmdline.html)。本轮官方与本地源指纹、环境探针、行为红绿及实际进程证据另保存于`.artifacts/B03-T02-library/`。
