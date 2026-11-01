# 单请求候选知识 v1

本契约落实 B04-T01 的有限离线入口，不是完整 Knowledge IR、模型质量或收费 API 验收。旧 fixture v1、Compiler Schema1、项目来源 Schema3 保持原义。

## 边界与选择

Core 只读取用户明确选择的已登记版本及完整文本 Span。模型接口只提供可信的 ProviderIdentity 和进程 Invocation，Core 统一调用现有有界 Windows 进程宿主。这样复用真实超时、并行管道和整树退出验证；仅在任意同步函数前后测 elapsed 无法约束阻塞。无类型 JSON 不能承担候选验证，因为重复字段、错身份和外来引用需要在原始字节上严格拒绝。

本项的 private fake 只在测试可执行文件中存在，经同一宿主及严格解码器返回事先审阅的响应。生产 CLI 只提供 `knowledge inspect <项目> <候选根>` 只读入口；库 API 接受明确 Provider。真实提供商适配器由 T02 单独验收，普通回归没有网络、密钥或真实费用。PDF 引用暂不接受；T03 另建立 ready parse/page/region/artifact 桥接，不能伪装成 text_span。

## 请求

协议为 `noetome-model-single-v1`，schema_version 为 1。所有对象只接受命名字段，拒绝未知、重复、缺失字段及位置数组；允许 null 的字段也必须声明。

- ExtractionRequest：schema_version、protocol、request_id、provider、source、policy。
- ProviderIdentity：producer（fake/model）、provider_id、adapter_version、requested_model。
- SourceSlice：project_id、source_id、version_id、revision、raw_sha256、normalized_sha256、normalization_version、parser_version、units。
- SourceUnit：kind=text_span、span_id、ordinal、start_byte、end_byte、text_sha256、text。start/end 为规范 UTF8 的全局半开区间，text 恰为该完整 Span；不得截断、拼接伪造或补零。
- Policy：max_input_bytes、max_response_bytes、max_output_tokens、timeout_ms、max_calls、retries、search_enabled、tools_enabled。

初始上限分别为来源文本 UTF8 总字节 8192、完整请求 wire 65536、响应 524288、输出 1024 Token、执行 30000ms；可收紧正值，不能放宽。max_calls=1、retries=0、search/tools=false。清理另受现有 2 秒边界约束；来源预检、同步 OS 调用不冒称全部函数的严格墙钟期限。输出 Token 是发送政策；返回未知时不能伪造实际计量。

Source/Version/Span 均须当前项目真实小写 UUID v4。Core 从持续守卫的配置/数据库及固定快照重建来源，重读登记 raw、规范文本和 Span，不读取已移走的原选取文件。Provider 不收到本地路径，响应不能扩大 sources/units。

## 响应与候选

Outcome：schema_version、protocol、request_id、request_sha256、provider、actual_model、status、candidates、usage、failure。request_sha256 绑定实际完整请求字节，不嵌入请求自身。身份和协议须精确匹配；actual_model 非空，记录真实返回值。complete 必须有 1..256 个候选且 failure=null；failed 必须候选为空并有分类失败。

Candidate：ordinal、statement、qualifiers、evidence。ordinal 从零连续，statement 为 1..4096 UTF8 字节；qualifiers 最多 32 个，每个非空且最多 256 字节。它们保存待审的完整表达与条件、单位、例外，不进行自动实体合并或结构化科学解释。

TextEvidence：span_id、excerpt_start_byte、excerpt_end_byte、excerpt_sha256。每个候选有 1..8 个证据，必须属于实际发送 Span；非空局部半开区间、UTF8 字符边界和真实片段 SHA 全部核对。raw offset、bbox、任意路径和 PDF 引用拒绝。证据字节匹配不等于 statement 科学正确，条件与遗漏由独立样本和后续审阅分别判断。

Core CandidateIr：schema_version=1、producer、candidate_id、revision=1、verification=unverified、request_id、request_sha256、response_sha256、source、atoms。每个 CandidateAtom 有 Core UUID atom_id、verification=unverified 和 proposal。Provider 不指定 Core UUID、revision、required、审阅状态或输出路径。保存重读核对其与实际响应的逐项一致，不能改成 fixture/fixture_reviewed 绕过旧 Compiler。

失败分类为 authentication、rate_limit、timeout、capability_insufficient、remote_unknown。宿主启动、超限、取消、协议或证据失败分别记录固定分类；失败不生成成功 CandidateIr、NotePlan 或 Release，无自动重试。

## 用量、预算和结算

Usage：provenance（simulated/provider_reported）、input_tokens、output_tokens、cost；两个 Token 字段显式 null 表示未知。fake 只能 simulated，model 只能 provider_reported。Cost：status（known/unknown）、amount_microunits、currency、basis（provider_reported/price_estimate/not_available）。known 需完整金额和币种，unknown 的金额/币种为 null 且 basis=not_available；未知费用不能显示为零。返回已知输出超限拒绝候选，已发请求不能改成未调用。

本项 Budget 仅有 available_calls 和 available_output_tokens；发送前至少预留一次调用和本次输出上限，否则 Invocation/进程零调用、输出根不创建。价格未知仍可用次数和 Token 控制；此接口不提供货币硬上限承诺。fake 的真实 API 调用和实际费用不累计，模拟数值单独呈现。

SettlementLedger 以 request_id 和完整 response SHA 确认一次。同一确认重放返回 AlreadyConfirmed，同请求不同响应 SHA 拒绝；保存的报告只容纳这一确认。只读重载不调用 Provider、不重新累计。响应未知/超时保留 unknown，不能以本地失败证明服务商未计费。费用与未返回 Token 不从字符数补造。

## 保存与重读

CandidateStore 只在父目录已存在、与项目不重叠的全新普通目录创建。固定文件为 owner.json、request.json、report.json，以及成功完整响应的 response.json 和 candidate.json。使用 create_new、持续目录/文件身份守卫、nlink1、sync_all，最后不覆盖改名发布 report.json；未知文件、已有/替换根和中断现场拒绝并保留，不自动接管或删除。

report.json 保存版本、UTC 时间、请求/响应指纹、实际宿主状态/PID/耗时、调用次数、预算预留、用量与唯一结算、失败或候选指纹。stderr 只保留字节数和 SHA，错误不带远端原文。响应只有完整、严格合法且无凭据回显才落盘；失败分类响应可保存供重读，但没有 candidate.json。非法或泄露响应只留可获得的完整指纹，不留原文。

凭据只作为调用端临时哨兵引用，不能进入请求、项目、候选、报告或错误。请求与合法解码响应检查原始及 JSON 解码后的回显；发生回显拒绝保存响应/候选。根内哈希只证明自洽；重读还须再次核对明确项目的真实登记版本和发送 Span。原选取文件缺失不影响已存快照核对。未完成报告、未知额外文件、硬链接、junction、共享写锁及 Windows 非法名字均拒绝。

## 独立样本和验证

先冻结 `tests/fixtures/model-candidate-v1` 的虚构纸灯箱 R7 原文及 12 个必要原子，来源 SHA `5aa51f5816ec39ac4eefd7a8299def081d33a2277a8fdddb66243b4a741c7591`，expected SHA `f86f6948a69e15dc8afdacbab0fc72435754067ec88c61b7a2e1b248be14cb04`。该清单由独立 AI 作者先于待测输出编写，不宣称人工科学金标准。

实际测试包括未实现命令行为红；strict codec 的字段/身份/证据负例；真实 private fake 子进程的合法/失败/空/截断/超限/超时；预算零调用；未知费用及幂等；保存重读；凭据和 Windows 保护；旧 fixture/NotePlan/Compiler/来源及 workspace 回归。只有实际运行的检查才可记验收；有限样本不能代替 PRD 98%、正式 200 原子、1000 页或 24 小时基准。
