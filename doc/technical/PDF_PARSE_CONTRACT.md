# PDF 单文件解析沙盒 v1

本契约是 B03-T02 的实现约束；完成状态与实测以任务账本和验收记录为准。来源 project.sqlite 仍为 Schema2，compile.sqlite 仍为独立 Schema1。这里的 parse 根不是来源项目或 Vault。

## 入口与布局

`source parse-pdf <明确PDF> <尚不存在的测试根>` 优先使用调用者明确提供的 `NOETOME_PDF_PYTHON` 绝对路径；该变量未设置时，Windows 只选择当前可执行文件相邻的 `pdf-python/python.exe`。不搜索 PATH、工作目录或 PDF 目录。明确提供但为空、相对、不存在或含链接的路径拒绝，不静默回退；非 Windows 仍须明确提供可信解释器。桌面与 CLI 共用 `pdf_parse::runtime_path` 的选择及普通文件检查。安装包是否实际包含完整锁定环境由 B15-T03 的原生安装验收证明。

Python adapter 随 CLI 嵌入，依赖锁定，isolated/no-bytecode 原生 argv，不使用 shell，不下载依赖。Windows 原始代码 `-c` 实际触发过 CreateProcess 长度上限，现以标准库 zlib 解压可信嵌入 payload 再 compile/exec；生成工具核对 payload 解压结果、源码与 SHA，不改变文档数据边界，不写临时 driver。`source inspect-pdf <测试根>` 只读重查已保存字节，不依赖原文件仍存在。

```text
new-test-root/
  parser/
    raw.pdf
    pdf-manifest.json
    pages/000000.txt             只存在于实际提取完的页
  parse-result.json              Core 已复核事实和原始 sidecar 响应
  parse-result.md                同一事实的阅读版
```

宿主失败时可留下 `failure.json` 和 parser 未确认现场；它们不构成成功解析，inspect 返回非零。不递归清理未知内容。已存在根、路径穿越、链接/junction、身份替换、资源碰撞、未知清单和占用均拒绝。

## Wire 与 PDF manifest

复用 [sidecar protocol 1](SIDECAR_PROTOCOL.md)，不新增 permissive wire 字段。资源 `raw-pdf` 为完整原始二进制；`page-text-000000` 等为逐页 UTF8；`pdf-manifest` 为 UTF8 JSON。本 manifest 只列原始与逐页资源，自己的 SHA 由响应资源清单绑定，避免自引用。

PDF manifest 必填：`schema_version=1`、`format=pdf-page-regions-v1`、`input_fingerprint`、`parser`、`raw_resource_id=raw-pdf`、可空 `page_count`、`status`、`pages`、`resources`、`issues`。所有对象拒绝未知字段；nullable 字段也必须存在。

parser 精确身份：engine pdfplumber / engine_version 0.11.9，engine_dependency pdfminer.six / engine_dependency_version 20251230，adapter_version noetome-pdf-text-v1，options_version crop-pt-lines-v1，python_version 3.12.14。协议响应 name/version 必须与该身份一致。影响输出的版本不可静默变化。

每页必填 `physical_page_index`、`display_page_number`、可空 `printed_page_label`、`status`、可空 `geometry`、可空 `text`、`glyphs`、`regions`、`visuals`。页索引是 0 基物理页；显示物理页为索引加一；本 adapter 不验证印刷页码，label 始终 null。

完整枚举后的 page_count 为真实页对象总数，pages 恰为 `[0,page_count)`。页级失败仍有原来的物理页条目。枚举未完成时 page_count=null，只保存已确认前缀，整体 failed；未知总数或 0 页另有 `pdf-document` failed 单元。正常页对应 `page-000000`，与 sidecar ordinal、状态、全文引用和资源逐页双向对账。

空文本页保留已确认的 `page.text`、SHA 与实际0字节 UTF8资源，但 protocol v1 的单元为 `kind=unknown`、`text=null`，资源引用仍包含该逐页artifact。v1要求每个非空引用 `start_byte < end_byte`，不得用 `[0,0)` 伪造正文。该页必须 needs_review；零字符不证明合法空白或完成覆盖。

## 坐标

geometry 的 `unit=pt`、`origin=crop_top_left`、`y_axis=down`。`media_box`、`crop_box` 保存 PDF 原始 user space 的四个角坐标 `[x0,y0,x1,y1]`，先规范化角序；CropBox 必须位于 MediaBox 内。`user_unit` 指每 PDF user unit 的 pt 数；`rotation_degrees` 为顺时针 0/90/180/270。`width_pt`/`height_pt` 是旋转后的显示 CropBox 尺寸。

`pdf_to_display=[a,b,c,d,e,f]` 表示 `u=a*x+c*y+e`、`v=b*x+d*y+f`。设 CropBox 为 `[L,B,R,T]`、UserUnit 为 s：

| Rotate | 矩阵 | 显示宽高 |
| --- | --- | --- |
| 0 | `[s,0,0,-s,-s*L,s*T]` | `(s*(R-L),s*(T-B))` |
| 90 | `[0,s,s,0,-s*B,-s*L]` | `(s*(T-B),s*(R-L))` |
| 180 | `[-s,0,0,s,s*R,-s*B]` | `(s*(R-L),s*(T-B))` |
| 270 | `[0,-s,-s,0,s*T,s*R]` | `(s*(T-B),s*(R-L))` |

pdfplumber 的字符坐标先去除其 MediaBox 显示偏移，再去除经旋转的 CropBox 相对 MediaBox 的左上偏移，乘以 s。旋转 CropBox 偏移不能仅使用库的未分方向 box 排序。Core 独立核对上述矩阵和宽高。异常 geometry、非有限/越界 bbox 不报告 parsed；不能通过静默裁切掩盖字符丢失。

Glyph 必填 `ordinal`、`text`、`bbox`、`font_name`、`font_size`、`upright`；ordinal 对应本页实际字符清单，bbox 为显示 CropBox `[left,top,right,bottom]`。区域必填 `region_id`、`locator_kind=pdf_page_region`、`physical_page_index`、`bbox`、`text` 和 `glyph_indices`。区域 bbox 必须与所分配字符的真实包络一致；全部字符恰分配一次，不能以页内任意框替代真实位置。

text artifact 必填 `resource_id`、`sha256`、`byte_length`；区域 text 必填 `resource_id`、`resource_sha256`、`start_byte`、`end_byte`。UTF8 范围是相对指定本页 artifact 的半开字节区间，须在字符边界，实际片段须等于字符清单重建的行。它不是 PDF 二进制 offset，也不是旧 blocks-v1 全局 byte/line locator。

行策略固定为按 top 排序、距行首 top 不超过 3pt 分组、行内按 left/原字符序号排序；实际相邻字形空隙超过 3pt且两边不是空白时插入一个空格。每个声明行的正文半开范围不含 LF；artifact在正文后追加 LF，下一区域从上一end_byte+1开始。Core 重建全文并检查顺序、行范围及 bbox。该工程一致性审计不能独立证明待测解析器没有漏读原 PDF；固定 authoring 预期与独立页面回看另外核对有限真实样例。

## 状态与边界

存在 failed 页/枚举失败时整体 failed；否则任一 needs_review 页或 blocking issue 令整体 needs_review；全部页 parsed 且无 blocking issue 才 parsed，CLI 退出 0。其他情况退出非零并保存已确认页和问题。

无文本层扫描与未确认空白均 needs_review；只有空白字符的非空artifact亦需复核，保留实际正文和非空wire范围。图形/图片/注释、表格候选、公式字体或上下标、重叠/列间大间隔、编码不可靠、非水平文字、非零旋转或非默认 UserUnit 按保守检测需复核。多码点glyph decode保留原字符及原始证据，但字体宽度/对应关系未经独立验证，必须needs_review；空格推断只看前glyph末字符和后glyph首字符，不在已有边界空白外多插空格。几何大间隔/重叠检查独立于空白字符。

相邻规范行的水平包络不相交且间隔大于36pt、top差介于0和max(72pt,6×两行最大高度)时，视为可能错开栏/侧注并需复核。adapter与Core都做这项线性候选检查；它阻止已复现的交错栏误parsed，不声称完整分栏算法或全部版面检测正确。

视觉条目包含 `visual_id`、`kind=image|vector|annotation`、`physical_page_index`、`bbox` 和 `evidence_resource_id=raw-pdf`；仅证明原始页面保留，不声称图片、公式结构或 OCR 已提取。

政策上限：最多 128 个已确认页、每页 50,000 字符、总 200,000 字符；sidecar 字节/资源/时限进一步约束，默认完整 raw 资源不得超过 16MiB。资源用独占 create 写入并核对实际字节。限额、超时、取消和未确认读取均失败，完整原始 SHA 或页总数未知时保持未知。

Core 报告包含 schema_version=1、report_kind=pdf_parse_sandbox、input_fingerprint、parser、status、可空 page_count、parsed/needs_review/failed 页数、manifest_path=parser/pdf-manifest.json、manifest_sha256、实际 elapsed_ms/resource_bytes 和 sidecar。inspect 重读报告、manifest、原始及全部资源并重新计算摘要，Markdown 与 JSON 同一事实。elapsed 测量原始准备、sidecar 和初次 Core 验收完成前的实际时长，不含最后报告写入/重读与 stdout；完整 CLI 时长由独立验证脚本另测，不是未来性能预测。没有知识 IR、Run/Release、知识覆盖或科学指标。
