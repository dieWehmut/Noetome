# B03-T02 真实文本 PDF 与物理页定位实施方案

**目标**：明确选取一份 PDF，在尚不存在的测试根中保留原始证据、完整物理页、逐页文本及可复核区域；任何失败或需复核页均返回非零。

**架构**：复用 protocol_version=1 的进程宿主，不改变 wire 或来源项目 Schema2。唯一文本提取 adapter 为 pdfplumber；其 PDF manifest 是一个受宿主 SHA/路径/UTF8 校验的资源。Rust 再核对页、字符清单、区域、文本范围和整体状态。原始 PDF、提取文本和 PDF 原生区域分别定位。

**技术**：Windows x64、Rust 1.98.1、Python 3.12.14、pdfplumber 0.11.9 / pdfminer.six 20251230；依赖精确版本和许可另存于 `sidecars/pdf_parser/`。本方案实施 [NEXT_STEPS 的 B03-T02](NEXT_STEPS.md) 和 [PDF 契约](../technical/PDF_PARSE_CONTRACT.md)，用户已授权按此任务卡实施、普通提交与推送。

## 全局约束

- 本轮唯一活动任务为 B03-T02；不登记 project.sqlite，不写 Vault，不接 OCR、模型或项目 GUI。
- CLI：`source parse-pdf <PDF> <尚不存在的测试根>`；只读重查：`source inspect-pdf <测试根>`。
- `NOETOME_PDF_PYTHON` 仅由调用者选择绝对解释器路径；adapter 随 Rust 二进制嵌入，使用原生 argv、Python isolated/no-bytecode 模式。文件内容不选择命令或输出路径。
- 页索引从 0 连续；显示物理页为索引+1；本版本 printed_page_label 始终 null，不推测印刷页号。
- 坐标为显示 CropBox 左上原点、Y 向下的 pt；原始 MediaBox/CropBox、UserUnit 和顺时针旋转到显示坐标的矩阵独立保留。
- 未读全不声明完整原始 SHA；未枚举完不声明完整 page_count。失败、未知空白、扫描、视觉关联与不可靠顺序不算整份解析成功。
- 输入、工作区与资源的文件身份、链接、SHA、精确清单及取消/时限继续通过 B03-T01 的守卫检查。
- 数量和时间限制是政策，不是实测性能。小样例不证明 1000 页、24 小时、OCR、知识覆盖或科学准确率。

## 文件职责及接口

| 文件 | 职责 / 接口 |
| --- | --- |
| `fixtures/pdf-text-v1/generate_fixtures.py`、`expected.json`、`README.md`、PDF 文件 | 自造字节与 authoring 坐标；在 adapter 运行前固定预期，不导入待测解析器 |
| `sidecars/pdf_parser/adapter.py`、`requirements-lock.txt`、`README.md`、`licenses/` | 唯一 Python 文本提取与协议响应；固定原始、逐页文本、manifest 资源 |
| `pdf_model.rs` | 严格 PDF manifest、geometry、glyph、region、visual、report 类型 |
| `pdf_validate.rs` | `validate_manifest(manifest, response, resource_data, control)`；核对页、完整字符分配、真实文本范围、bbox 包络、阅读顺序和状态 |
| `pdf_workspace.rs` | 独占外层新根、只读报告守卫、报告 create_new/重读与精确外层清单 |
| `pdf_parse.rs` | `parse(input, output)` / `inspect(output)`；复用 sidecar、全过程限时/取消、JSON 与 Markdown 持久报告 |
| `sidecar_process.rs` / `sidecar_workspace.rs` | 仅添加 crate 内受保护资源读取/重查入口；公开协议和 fake 边界不变 |
| `tests/pdf_parse.rs`、`tests/support/pdf_parse_tests.rs` | 真实 CLI、Core 篡改、路径/输入/进程故障及支持矩阵 |
| `scripts/verify_pdf_parse.py` | 独立 expected、实际资源 SHA、正文/顺序/位置、CLI 非零与哨兵；记录实际时间/资源 |

## 1 固定独立样例与红灯

- [ ] 核对官方锁定版本的 README、许可证、requirements 和已安装元数据；记录来源链接及运行时，不自动升级共享环境。
- [ ] 创建一页/三页单栏、偏移 CropBox、旋转、扫描、空白、损坏、加密、表格、公式、图片、多栏、部分页损坏及绘制顺序样例。PDF authoring 的正文、font、baseline、页数和坐标确定预期；原始 SHA 随固定字节登记。
- [ ] 用 Poppler 渲染全部可渲染页，核对 authoring 预期的正文和区域；review 记录与预期分开，不宣称人工科学金标准。
- [ ] 写真实 CLI 红灯：

```rust
let output = Command::new(env!("CARGO_BIN_EXE_noetome"))
    .args(["source", "parse-pdf"])
    .arg(fixture("single-page.pdf")).arg(new_root).output().unwrap();
assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
```

- [ ] 实跑 `scripts/cargo_local.ps1 test -p noetome-cli --test pdf_parse --locked --offline`，记录因入口缺失而非 typo 失败。Core 定位负例在空验证实现上实跑红灯后再实现。
- [ ] 聚焦提交固定输入、契约与真实红灯记录。

## 2 Adapter 与 Core 定位验证

- [ ] Python 完整读取并对账请求 SHA；只按固定路径以 exclusive create 写资源。枚举原始页对象之后逐页构建；枚举中断保留已确认前缀并令 page_count=null。
- [ ] 输出原始 PDF、逐页 UTF8 和 PDF manifest；manifest 自身 SHA 由外层协议声明，不做自引用。原始页和视觉区域引用原 PDF，不声称图片/OCR/公式结构已抽取。
- [ ] 核对全部物理页、display 页、page_count 与 sidecar units 的双向一一映射；未知页总数附 document 失败单元，不能把前缀当完整清单。
- [ ] Rust 以逐字符清单重建声明的行分组、全文与 bbox 包络；区域完整分配字符且顺序确定。UTF8 半开范围绑定 page resource ID/SHA，必须与实际正文字符边界一致。
- [ ] 红/绿覆盖漏页、错页、越界及页内错误 bbox、缺正文、乱序、Unicode 切断、资源路径/SHA、未知字段和不诚实状态。真实独立 expected 另核对实际位置；字符审计一致性不等于一般 PDF 内容正确性。
- [ ] 扫描/空白、图形/表格/图片、公式与多栏检测按保守策略 needs_review；部分页异常 failed，正文已确认的其他页保留。旋转坐标保留真实变换，旋转阅读顺序仍 needs_review。
- [ ] 定向检查通过后聚焦提交 adapter 与 Core 验证。

## 3 沙盒重查、故障及验收

- [ ] 新根布局仅 `parser/`、Core `parse-result.json` / `parse-result.md`；失败可保留 `failure.json` 和未确认 parser 现场。所有输出 create_new，不接管既有根。
- [ ] inspect-pdf 只读重查原始、manifest、所有文本/资源 SHA 和精确清单，以及 Core 重新计算的摘要/Markdown；原文件移动/删除后仍可重查项目外独立沙盒。
- [ ] 实跑读失败、输入改变、各实际限额、时限/取消、子进程失败、部分页、目标碰撞、路径/junction/硬链接/占用/目录替换；生产 CLI 不暴露 fake 或故障开关。异常保留定位原因，未确认原始/页面不补造。
- [ ] `scripts/verify_pdf_parse.py` 按事先 expected 核对真实 CLI、全部页和区域；保存支持矩阵、原始 SHA、实际时间/输出资源字节以及未运行的性能/科学项。
- [ ] 独立 code-review 子代理只审 B03-T02；解决重要问题后运行相应修复回归。
- [ ] 运行格式、locked 离线 workspace check/test、Python unittest、devlog check、diff check；逐条验收、同步标准/日志、普通提交及推送，核对远端 HEAD。
- [ ] 本项验收后交接 B03-T03，另轮 start，不将持续目标偷换为 PDF/模型全部已完成。
