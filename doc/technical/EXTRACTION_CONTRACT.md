# 有界知识抽取契约 v1

适用任务：B04-T01。依据 PRD FR02/04/09/37/38/40、NFR07/08、D04/D07/D08/D09/D14。用户已授权按当前任务卡完成实现、检查、普通提交和推送；本契约把已接受的 fake 优先基线具体化。真实 API、科学质量、生产 Vault 和知识发布仍属于后续任务。

## 入口与只读来源

`noetome extract fake <项目路径> <import-id> <fake目录> <尚不存在的输出根> [--cancel-file <取消信号路径>]`

只接收真实项目中已存在的导入记录。文本使用该版本的真实 SourceSpan；PDF 使用该导入的真实 parse_id、已确认页面/区域、逐页 UTF-8 资源及 SHA。失败获取、失败解析、未确认资源和无文本区域不能产生请求。needs_review 文本保留状态，候选仍需复核。不得将 PDF 伪装为旧文本 Span 或补造 normalized_sha256。

从打开项目到最后保存验收报告，保留配置、SQLite、来源缓存所有权、目录和来源文件的读取能力；重新检查身份、SHA、mtime 和目录清单。来源事务全程只读，不迁移 SQL、不写来源、不改 pending。为该新入口单独限制快照树最多 4096 文件、512 MiB；每文件沿用来源边界，超限明确拒绝，不改变既有来源命令的历史读取规则。未知项目根文件仅核对目录元数据，不读取私人正文。输出与项目、fake 输入目录不能相交，取消路径不得位于这两个输入根或输出根内。

## 严格消息

所有 DTO 使用 deny_unknown_fields；拒绝重复字段、尾随 JSON、未知 schema/枚举和缺失字段。可空字段必须出现，未知用 null，不以 0、空串或假 ID 代替。

SourceBinding：project_id、import_id、source_id、version_id、content_sha256，以及必须出现的 normalized_sha256、parse_id、manifest_sha256（Option）。该对象必须与读取会话双向完全一致。真实身份使用现有 UUID4，SHA 为小写 64 位十六进制。

ExtractionRequest：schema_version=1、producer=fake、request_id（每次新 UUID）、binding、units、limits、budget。unit_id 是本请求稳定局部编号 `unit-0` 起；每单位包括 locator、source_status、start_byte、end_byte、text。文本 locator 包含 span_id、ordinal、resource_sha256（规范全文 SHA）；PDF locator 包含 parse_id、page_index、region_id、resource_id、resource_sha256、bbox。范围是其真实资源内 UTF-8 字节半开区间，text 是该范围的原文。单位选择来自 fake.json 的明确 Span ordinal 或 PDF page_index/region_id，不自动扫描全部资料。

ResponseEnvelope：schema_version=1、producer=fake、request_id、binding、payload。fake 适配器仅把原样 response_file 字节包进请求路由信封，保留非法 JSON；不从正文生成知识。fake.json 的必填可空 binding_override/request_id_override 仅供明确的协议负例。payload：schema_version=1、producer=fake、concepts、evidence、atoms。

Concept 复用既有 concept_id/revision/canonical_name/aliases/needs_full_explanation。Evidence：evidence_id、unit_id、excerpt_start_byte/end_byte、excerpt_sha256、quote。范围必须非空、在指定真实单位以内、落在 UTF-8 边界；quote 与实际切片完全相等，SHA 来自切片字节。

CandidateAtom：atom（复用既有 KnowledgeAtom）、measurement（必填可空）。atom 保留 kind、concept_ids、content、qualifiers、evidence_links、required、verification、value_status；所有 fake 原子必须 required=false、verification=unverified，每项至少有一条有效 supports。通用 ID、修订、明文、唯一概念/证据引用规则与严格 fixture IR 共用，fixture 原有 required 审阅政策保持不变。

Measurement：value_status、value/unit（必填可空字符串）、conditions。comparison_value 必须有 measurement；known 要求非空 value/unit，unknown 要求 value=null 并保留已知单位，not_applicable 要求 value/unit=null。measurement 与 atom 的 value_status 一致，已知单位和每个条件必须保留在 atom.qualifiers；条件和单位是否科学正确只由后续独立质量任务判断。文本数值保持原表达，不经浮点转换。

## Fake 配置和单请求边界

fake.json 的必填字段：schema_version=1、producer=fake、case_id、selection、capabilities、limits、budget、delay_ms、response_file、binding_override、request_id_override。selection 为非空列表：`{"kind":"text","ordinal":N}` 或 `{"kind":"pdf","page_index":N,"region_id":"实际区域ID"}`。capabilities 包含 text/structured_output/vision 布尔值；本任务只需要 text 与 structured_output，图像能力不用于补造正文。response_file 是根内普通相对文件，禁止越界/链接/未知配置。fake 根文件、身份、SHA 和清单保持只读。

limits：max_units（1..64）、max_input_bytes（1..1 MiB）、max_response_bytes（1..1 MiB）、max_output_tokens（1..65536）、timeout_ms（1..30000）。budget：max_requests（0..1）、max_total_tokens、max_cost_usd（必填可空）。这里只运行一次 replay。fake 的计量单位明确为 `fake_utf8_bytes_v1`：输入为单位正文 UTF-8 字节，输出为完整信封 UTF-8 字节，不代表任何真实模型 tokenizer 或使用量。调用前预留 input + max_output_tokens；预算不足、max_requests=0 或带金额上限但价格未知均在调用前停止。价格、模型 tokenizer、真实模型使用量为 null；network_calls/paid_calls=0 是此次离线行为事实。不得以未知价格为零。

超时从准备完成的请求开始，覆盖 fake 等待、响应读取和校验。取消信号为用户明确指定的外部普通文件；入口与等待期间检查，验证程序可在调用运行时创建它。取消文件只作为停止信号，不读取正文、不写它。保存失败审计用新的有界本地检查窗口，不能因原请求已超时跳过来源最终检查。预算、超时、取消、空响应、响应过大或 fake token 超限均为失败。

## 结果和保存

成功返回 0，仅表示 `candidates_validated`；所有原子仍 unverified，knowledge_status=pending、published=false、scientific_quality=not_run。语义/预算/超时/取消失败返回 3；路径/配置/找不到来源等输入错误返回 2（已有 ProjectError 读取受阻沿用 3）。失败不保存 candidates.json，不创建 KnowledgeIR、Run、Release 或 Vault。

只有通过来源与配置预检后才建立全新输出根。保存 `.owner.json`、request.json、response.bin（有响应时）、candidates.json（仅验收成功）、validation.json、validation.md。报告记录实际调用数、fake 计量、nullable 价格/模型使用量、错误码、来源状态、绑定和文件 SHA。写入使用 create_new 和能力持有，重读实际文件与双报告；最终复核来源和输入，再允许成功。未知已有输出拒绝；运行期间出现未知文件或身份变化保留现场并报错，不接管、覆盖或递归清理它。

## 验收与限制

作者先写中文条件/单位文本与独立 gold，实际导入后绑定动态身份；真实 PDF 采用仓库明确自造夹具、已登记解析和实际逐页资源。普通 CLI 覆盖正常、非 JSON/尾随/重复/多字段/缺字段、未知版本或解析/SHA、UTF-8 边界与引用文本错误、failed PDF、能力不足、空响应、输入/输出/token 限制、预算/价格未知、超时、运行时取消、旧输出/越界以及来源/输入/输出变化。独立验证对来源 SQL、字节、mtime、目录清单做前后比对，对规范候选内容做重放比较（request_id 可变）。先记录实现前 RED，再实现和 fresh review，最后完整 Rust/Python/devlog/diff 检查。

工程验收不证明真实模型抽取、科学正确率、全来源覆盖、性能或跨平台信号处理。这里只验证明确取消文件机制，不把它宣称为 Windows Ctrl+C 验收。真实 API 的价格/预算及实际调用证据在 B04-T02 补齐。
