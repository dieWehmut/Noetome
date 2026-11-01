# B03-T03 PDF 来源登记实施计划

> 按subagent-driven-development在当前任务内分工；Root负责共同接口、输入/解析编排、CLI、账本、最终审查与推送，各分工先实际红再实现及定向验证。

**目标**：单份PDF能在项目中安全import/inspect、复用已确认raw与成功parse、记录真实失败，并经Schema3升级保留旧文本历史。

**架构**：共用原始Source/Version，独立PdfStore与解析快照；复用Writer candidate/配对发布，扩展typed PDF暂存树及单链接复制。Core在每次inspect复验持久原始、正文和定位。

**技术**：Rust/Cargo1.98.1 Windows x64、SQLite、CPython3.12.14及唯一锁定pdfplumber adapter；不新增依赖或作者PDF。

**规范**：[PDF来源Schema3](../technical/PDF_SOURCE_SCHEMA.md)、[PDF协议](../technical/PDF_PARSE_PROTOCOL.md)、[近期任务卡](NEXT_STEPS.md)、已采纳D02/D12/D13/D14与当前测试/工作流程。

## 固定接口与职责

| 分工 | 文件 | 消费/交付 |
| --- | --- | --- |
| Schema/Model | pdf_source_schema.rs、pdf_source_model.rs、source_schema.rs、source_model.rs、project.rs | Schema3 upgrade/validate；Store统一raw校验；PdfStore.load(connection,&Store,project_id)、validate_snapshots(root,&Store,project_id,staged_parse,staged_raw)→BTreeMap<parse_id,PdfEvidence>；insert_parse/insert_import写精确关系；project readonly1/2/3 |
| Writer | source_write.rs | 保留旧API；新增run_pdf_parser(parser,raw,limits,cancel)→Result<PdfParseOutput,ProjectError>、pdf_artifact_bytes(report)，stage_pdf_raw(version_id,raw)，stage_pdf_parse(parse_id,&BTreeMap<String,Vec<u8>>)；同时一个raw+parse，typed目录、单链接复制、六阶段真实故障/kill回归 |
| 验收 | tests/pdf_source.rs、tests/support/pdf_source_checks.rs及既有tests/source.rs PDF否定例 | 12组独立作者预期+实际CLI/SQLite/持久快照矩阵；只变造新测试项目；命令从env选两个实际Python，不写开发机路径 |
| Root | pdf_source.rs、pdf_parse.rs、lib.rs、main.rs、Cargo target、标准/证据 | 输入守卫与版本选择、真实parser或ready复用、Evidence/关系写入/候选及发布重读、报告Schema2、CLI max-pages/timeout；T02 Host可严格Deserialize owned字符串 |

新类型统一位于pdf_source_model.rs，Root与Writer不用另造同名类型：`PdfCacheConfig`、`PdfEvidence`、`PdfStoredParse`、`PdfImportRecord`、`PdfStore`。各字段按规范命名；方法若需调整先通知Root并同步此接口。

T02内部`parse_pdf_retained`交付output及不序列化的receipt；receipt保留原始Workspace/Input/资源原生守卫。Writer在receipt仍存活时捕获全部身份及字节，前后复验，随后释放receipt并用普通读取守卫复验；公开`parse_pdf`仍返回原输出并释放能力。原项目config/DB守卫从state读、选取文件读取直到Writer取得原件守卫连续持有。

## 步骤一：冻结契约和可定位真红

- [ ] 三方审查Schema2旧语义、Writer清单/硬链接和独立验收；冻结Schema3及报告/路径/缓存身份。
- [ ] 先运行真实旧CLI的PDF import预期成功负例（旧命令拒绝），保存退出及原项目/哨兵字节；Model/Schema/Writer新接口行为先在测试stub定位红。
- [ ] Schema3在candidate迁移旧ID/行顺序/指纹及记录严格负例，故障只影响新测试项目。

## 步骤二：原始版本、解析尝试及安全发布

- [ ] Schema3精确DDL、迁移、统一Store与只读project路由；既有Schema1/2规则保持。
- [ ] PdfStore严格证据与关系重建、资源/原始/正文/坐标及状态复算；没有完整raw不造Version或parse。
- [ ] Writer固定selected.pdf及受控T02树handoff，typed raw/parse暂存与单链接发布、完整回退和unknown现场保留。
- [ ] Root以保持守卫的raw选择来源/版本；ready身份复用，否则实际parser。写candidate import/parse/index，暂存字节后重读，再配对发布和最终重读。
- [ ] CLI与报告分别表达acquired、parse ready/review/failed/not_run、knowledge pending；配置错误2、真实失败3，inspect核对失败事实仍0。

## 步骤三：独立验收与收束

- [ ] 12组真实CLI/SQLite/快照/Windows矩阵，实际不同runtime失败后恢复与max-pages选项变化，原作者expected不改。
- [ ] 同步篡改report/DB/SHA负例仍由Core拒绝；旧文本/fixture/T02回归、真实三阶段kill和备份指纹证明中断保护。
- [ ] fresh fmt/check/test workspace、Python12项/devlog/diff；记录实际数量、runtime/源码/资源指纹、未测范围，独立review无必要缺口。
- [ ] complete B03-T03、分步commit/push并核对remote；B03三项完成后仅滚动规划下一批，不自动启动API或OCR。

复现：`cargo_local.ps1 test -p noetome-cli --test pdf_source --locked --offline`，真实Python由`NOETOME_TEST_PDF_PYTHON`及可选different-runtime env指定。规划不代替实际执行证据；任务状态仅账本维护。
