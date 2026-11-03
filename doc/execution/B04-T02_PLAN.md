# B04-T02 单请求文本 API 实施计划

> 执行者：使用 superpowers:subagent-driven-development。全部分工只属于 B04-T02；独立 reviewer 作 spec 与质量裁定，集成后 fresh broad review。状态仅由 devlog 工具维护。

**Goal**：普通 CLI 对一个配置的 Responses 文本 API 发送一次有界请求，保存原响应/用量和来源候选；离线部分完成后保留真实调用门槛。

**Architecture**：native HTTP/wire/Schema 与 Core 配置/来源/持久化分开。API 与 fake 共用严格领域规则，来源只读能力及全新输出沿用 T01；API DTO 和 provenance 独立。

**Tech Stack**：Rust 1.98.1、既有 serde/SQLite/SHA、reqwest=0.13.5（native-tls-no-alpn，default-features=false）、tokio=1.54.1（rt/net/time）、futures-util=0.3.34（std）。Python 标准库作独立普通 CLI/HTTP 审计。

**Spec**：[EXTRACTION_API_CONTRACT.md](../technical/EXTRACTION_API_CONTRACT.md)，依赖 [T01 验收](../validation/B04-T01.md)和[当前卡](NEXT_STEPS.md)。

## 全局约束与裁定

- 当前 codex/b04-t02-text-api 分支从已推送 b4fae25 继续；用户两个旧 docs/PRD 删除保留，不暂存。普通 commit/push 已获用户授权，不再询问。
- 同时只一个 in_progress；每项内分工分文件，子代理不修改任务状态、不调用真实服务、不推进 T03。controller 负责 Git 提交和账本。
- live 只认官方 HTTPS Responses；本地假服务只数字环回和 noetome-test- 人工凭据/模型。未知价格/null、一次调用、最多 4 选定单位、64 KiB 输入、256 KiB request body、1 MiB response、16..8192 output、1..60000 ms。
- 保留真实配置/预算缺失，先完成可审查的离线结果；按 D08 不搜默认密钥、不实际收费、不改真实验收条件。
- 采用当前独立任务分支，来源及产物位于新的测试根；不迁移 checkout 或接管用户删改。若用户给出另一个提供商，先按其官方接口调整单适配器，而非增加全部 SDK。

## Task 1：HTTP、严格 wire 与结构化 Schema

**文件**：新增 extraction_api_transport.rs、extraction_api_wire.rs、extraction_api_schema.rs；仅为接线修改 lib.rs 顶部 mod、Cargo.toml/Cargo.lock。controller 已写普通 CLI 作者测试和真实 RED，worker 自行补针对性 native socket/wire RED/GREEN，不改 CLI 分发或该测试。

**接口**：

```rust
// transport.rs
pub(crate) struct TransportResult {
    pub(crate) status: Option<u16>,
    pub(crate) request_id: Option<String>,
    pub(crate) body: Vec<u8>,
    pub(crate) complete: bool,
    pub(crate) attempts: u32,
    pub(crate) failure: Option<crate::project::ProjectError>,
}
pub(crate) fn send(
    endpoint: &str, bearer: &str, body: &[u8],
    timeout: std::time::Duration, max_response_bytes: usize,
    check: &dyn Fn() -> Result<(), crate::project::ProjectError>,
) -> Result<TransportResult, crate::project::ProjectError>;
// wire.rs: ApiCandidates 的字段与 spec 完全一致；公共领域类型用现有 crate 类型
pub(crate) struct ApiCandidates {
    pub(crate) schema_version: u32, pub(crate) request_id: String,
    pub(crate) binding: crate::extraction_model::SourceBinding,
    pub(crate) concepts: Vec<crate::fixture_model::Concept>,
    pub(crate) evidence: Vec<crate::extraction_model::CandidateEvidence>,
    pub(crate) atoms: Vec<crate::extraction_model::CandidateAtom>,
}
pub(crate) struct ParsedResponse {
    pub(crate) id: String, pub(crate) model: String,
    pub(crate) usage: serde_json::Value, // 已验证整数 input/output/total；缺失不得填 0
    pub(crate) service_tier: Option<String>,
    pub(crate) candidates: ApiCandidates,
}
pub(crate) fn decode(body: &[u8]) -> Result<ParsedResponse, crate::project::ProjectError>;
// schema.rs
pub(crate) fn request_body(
    model: &str, request_id: &str,
    binding: &crate::extraction_model::SourceBinding,
    units: &[crate::extraction_model::RequestUnit], max_output_tokens: u64,
) -> Result<Vec<u8>, crate::project::ProjectError>;
```

- [ ] 写本地 TCP listener 和 wire 变异测试，实跑 RED：一 POST/HTTP 错误，超时/partial，重复 key、缺用量、refusal/incomplete、非法候选 nullable。先确认失败来自新增行为。
- [ ] 实现默认验证 native TLS、no_proxy/redirect none/retry never、HTTP1、future 与 5 ms 停止检查；完整/partial 实际字节由 TransportResult 保留。静态错误码：EXTRACT_API_TRANSPORT、EXTRACT_API_TRUNCATED、EXTRACT_RESPONSE_LIMIT、EXTRACT_API_HTTP_AUTH、EXTRACT_API_HTTP_RATE_LIMIT、EXTRACT_API_HTTP_SERVER、EXTRACT_API_HTTP_STATUS；control 的原 EXTRACT_TIMEOUT/CANCELLED 保持。
- [ ] Schema 给真实单位提供完整 quote/字节边界/SHA anchor，输出严格 SourceBinding/领域对象，全部对象 required+additionalProperties=false、measurement 可空；不在 output 请求 fake producer。发送 spec 列出的实际参数。
- [ ] 完整 wire 允许未来元数据但拒绝所有重复 key/尾随 JSON。completed/error-null/incomplete-null、一个 assistant output_text；reasoning 元数据可保留，不接受工具。用量基础整数加总正确；详细项有则验证类型，缺则保持缺失。错误码 EXTRACT_API_RESPONSE/STATUS/INCOMPLETE/REFUSAL/OUTPUT/USAGE/CANDIDATES。
- [ ] targeted cargo tests GREEN；worker 自检并写报告到本计划 SDD workspace。controller 派 fresh task reviewer，重要问题复现修复后提交此能力。

## Task 2：配置、Core 校验、报告与普通入口

**文件**：新增 extraction_api_config.rs、extraction_api.rs；修改 extraction.rs 的纯共享候选函数、extraction_output.rs 的 API owner、lib/main CLI。不改 Task1 的 HTTP/wire/Schema，必要接口更正先向 controller 说明。controller 的作者测试位于 tests/extraction_api.rs。

**接口**：公开 `extraction_api::run(project:&Path, import_id:&str, config:&Path, output:&Path, cancel:Option<&Path>) -> Result<source::CommandOutput, ProjectError>`。source::CommandOutput 复用 json 与 failure；fake::fake 的签名和 v1 交换保持原样。共享 `validate_candidates(units, concepts, evidence, atoms, control, cancelled, signal)` 使用领域切片，不造 producer=fake 的 API payload。

- [ ] 阅读作者普通 CLI RED。为 config 严格 DTO/环回与官方 endpoint/明确外部凭据、secret echo/预算停止补 meaningful RED；配置以普通读取守卫持有至最后，不能扫描目录正文。
- [ ] 加载来源并预检路径、取消和凭据；组装 Task1 body，实际 bytes+1024 预留，不足/金额未知 0 次请求。secret 含于 request 时调用前拒绝；local test 的人工 key/model 强制前缀。
- [ ] 使用 send 一次，记录真实 attempts/status/partial；响应含凭据或 JSON 转义回显时省略原文，只保存已收 SHA/长度及错误，绝不保存候选。稳定诊断不含远端错误正文。
- [ ] decode 完整 200 response，记录配置/返回模型、真实 usage 与 null 费用；核对 input/output/total 预算、request_id/真实 binding。共享严格领域校验通过后 candidates_validated；所有原子 unverified、pending、published=false。
- [ ] 使用 API owner 与全新 OutputRoot 保存 request.json、request-body.json、原 response.bin、仅成功 candidates.json、同事实 JSON/MD 验证快照和实际指纹。保存前后 verify source/config/output，失败按已捕获能力 discard_result，人工文件保留。
- [ ] 接线 `extract api`、help 与 native 路径/--cancel-file；实跑作者 tests/extraction_api.rs 和旧 extract fake/fixture GREEN。fresh task review 后提交。

## Task 3：独立离线审计、真实 gate 与收尾

**文件**：controller 新增 scripts/validate_extraction_api.py、doc/validation/B04-T02.md 与脱敏资产，更新 DESIGN/TESTING/ARCHITECTURE/DATA_CONTRACTS/docREADME/NEXT_STEPS；测试来源/gold沿用冻结的 fixtures/extraction-gateway-v1，不能以本次输出重写期待值。

- [ ] 独立本地 HTTP 服务记录实际 POST 数、无认证 body、结构化 Schema/限制，普通 CLI 覆盖 spec 失败矩阵；直接 SQL/文件 SHA/mtime/清单核对来源，检查双报告快照和真实产物。保留失败证据并复现修复。
- [ ] 配置/预算收到后核查用户选定模型的官方能力/当前价格规则，只运行一次指定 Span ordinal=0 的有界真实调用。保存实际原响应/返回模型/用量和 Core 核对；未经这些条件不调用、不算完成。
- [ ] 运行 cargo fmt/check/workspace test（显式 4 test threads）、Python unittest、独立 CLI、devlog/diff；fresh broad spec+quality review，无未处置重要问题。
- [ ] 实际 gate 满足才 devlog complete；否则对离线已实现/检查和缺少的 provider/endpoint/model/env ref/预算作具体记录，devlog block。普通检查通过后分步提交并 push 当前分支，核对远端 HEAD。不把 block 写成 complete，不启动依赖 T03。

## 自检

Task1 的 send/decode/request_body 三签名在 Task2 原样消费；Task1/Task2 顺序改 lib.rs，不并发编辑。Task3 用普通入口和固定作者 gold，报告只读核对而不调用生产 validator 作知识 oracle。T02 真实 gate 在 spec、任务卡、计划和账本一致；配置缺失只限制实际付费步骤，不能掩盖工程工作或改写验收。
