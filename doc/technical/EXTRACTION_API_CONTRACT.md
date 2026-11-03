# 单请求文本 API 抽取契约 v1

适用 B04-T02，依据 D07/D08 与 FR-09/35/36/37/38/40。沿用[离线候选契约](EXTRACTION_CONTRACT.md)的真实来源、严格领域对象和只读/新根保护；fake 的 schema/producer 不改变。新增 API 交换独立以 schema_version=1、producer=api、report_kind=text_api_extraction 标识。

## 单一接口与官方依据

当前只接 Responses 文本协议，既有离线候选为 `POST https://api.openai.com/v1/responses`。2026-11-03 用户已明确选择糖猫云 `https://ai.1343263.xyz/`、`deepseek-flash` 和累计10000000 Token，要求保存并以后复用；新增精确所选端点 `https://ai.1343263.xyz/v1/responses` 的请求策略。凭据通过本机引用使用，实际调用与网关能力仍按结果验收。其他提供商或兼容端点没有验收。原 OpenAI 官方资料实际核查日期 2026-11-03：

- [API 概览与鉴权](https://developers.openai.com/api/reference/overview)：Bearer 凭据；只读取配置指名的环境变量，不保存 Authorization、密钥值或认证错误正文到普通日志。
- [Responses create](https://developers.openai.com/api/reference/resources/responses/methods/create)：文本 input、instructions、text.format、max_output_tokens、status/output/usage。输出上限包括 reasoning，最小 16；store 默认 true，显式发送 false。service_tier 明确 default；truncation=disabled 仍可用但已标记 deprecated。
- [结构化输出](https://developers.openai.com/api/docs/guides/structured-outputs)：严格 json_schema，所有对象 additionalProperties=false、全部属性 required，可空用 null 联合；拒答和 incomplete 必须独立处理。模型能力来自配置和后续真实实测，不用一个声明推定所有模型支持。
- [错误](https://developers.openai.com/api/docs/guides/error-codes)：401/403、429 与 5xx 分别保存稳定失败分类；本任务一次请求不自动重试。
- [价格](https://developers.openai.com/api/docs/pricing)：按百万 Token 区分输入、输出和缓存，模式、上下文及地区会影响价格。未选实际模型/有效价格，不抄一个费率作为所有模型费用；价格、账单和未知付费请求数保留 null。金额预算但价格未知时调用前停止，可按 D08 使用明确 Token/次数控制。

所选服务另外实际核查[糖猫云公开入口](https://ai.1343263.xyz/)、其部署JS中的Sub2API标识、[上游gateway路由](https://github.com/Wei-Shaw/sub2api/blob/main/backend/internal/server/routes/gateway.go)、[DeepSeek Responses参考](https://api-docs.deepseek.com/api/create-response)及[Responses兼容表](https://api-docs.deepseek.com/guides/responses_api)。路由和官方JSON Schema支持是协议依据，不是已部署网关的模型调用证明。兼容表明确不支持的字段会被忽略；所选body移除background/service_tier/truncation和format.strict，配置reasoning.effort=none。费用不能套用官方费率，实际返回模型/用量单独核对。

## CLI 与配置

`noetome extract api <项目> <import-id> <config.json> <全新测试输出根> [--cancel-file <外部普通信号>]`

config.json 只读、最多 64 KiB，严格拒绝重复/额外/缺字段和未知版本。字段为 schema_version=1、provider=openai_responses、transport_mode（live 或 local_http_test）、endpoint、model、credential_env、required_input（text 或 vision）、capabilities（text/structured_output/vision）、selection、limits 和 budget。引用凭据的名称须为 2..64 位大写环境变量标识；不隐式读取默认密钥。

live 只允许上述精确 HTTPS 官方端点和用户指定的糖猫云 `/v1/responses`；糖猫云只认已选deepseek-flash，不额外允许其他模型、端口、用户信息、查询串、fragment、重定向或代理继承。local_http_test 只允许数字环回 127.0.0.1/::1、明确端口、HTTP 与 /v1/responses，模型和凭据值必须以 noetome-test- 开头；该模式只发本地假服务请求，真实模型/收费调用为 0，不作为 T02 真实验收。

required_input=vision 或缺少 text/structured_output 在请求前拒绝；不发图片或猜测 OCR。selection 明确选择已登记文本 Span 或 PDF 的已确认文本区域，最多 4 单位。limits 沿用字段，API 本项限定 max_units=1..4、max_input_bytes=1..64 KiB、max_response_bytes=1..1 MiB、max_output_tokens=16..8192、timeout_ms=1..60000；实际序列化 HTTP 请求最多 256 KiB。budget.max_requests=0..1，max_total_tokens 为明确总预算，max_cost_usd 必须出现，可空；金额为正有限十进制文本。

配置文件位于来源项目和输出之外，持有读取句柄及身份/mtime/SHA，最终重验。不扫描其父目录的私人正文。输出与整个来源项目不相交，取消信号位于来源/输出之外并不能替代 config.json；存在信号时不调用。只用明确选取的新测试来源，不写生产 Vault。

所选服务非秘密设置及累计使用记录保存到当前用户Noetome设置目录，引用NOETOME_TANGMAO_API_KEY与仓库外CurrentUser DPAPI文件；[模型服务规则](../requirements/MODEL_SERVICE.md)约束后续复用。Core仍只读所选process环境变量，外部脚本在内存解密并注入选定子进程，不给Core增加默认私钥扫描。

## 请求、预算与 native HTTP

真实 HTTP body 保存为 request-body.json。旧官方/本地策略openai-responses-strict-json-schema-v1发送 model、instructions、input（包含 request_id、SourceBinding、真实 units 和 Core 计算的完整单位证据 anchor 的 JSON 文本）、text.format={type:json_schema,name:noetome_candidates_v1,strict:true,schema}、max_output_tokens、store=false、stream=false、background=false、tools=[]、tool_choice=none、service_tier=default、truncation=disabled。

所选初始策略tangmao-deepseek-responses-json-schema-v1已在首次真实请求保留。实际网关回包省略error/incomplete_details，模型回复要求提供任务而不是候选JSON。续接策略tangmao-deepseek-responses-inline-json-schema-v2保留name/schema/输出上限与同一端点，input是自足JSON文本：task为完整抽取指令及仅输出一个JSON要求，response_schema为完整候选Schema，request为原request_id/binding/units/evidence_anchors。顶层instructions/text.format仍保留，不依赖网关实际传递这两字段；不剥代码围栏或修补非JSON。仍移除text.format.strict、background/service_tier/truncation，发送reasoning={effort:none}、store=false、stream=false、tools=[]、tool_choice=none。store/effort是客户端意图，首次回包仍有reasoning摘要，不能声称服务已经执行关闭思考或不保存；严格候选由Core检查。两种端点策略均不发送conversation、previous_response_id、搜索或文件上传；request_strategy在请求及JSON/Markdown报告逐字段保存，报告也保存实际endpoint。

Core 先序列化全部正文、提示和 Schema；input_reserved_tokens 为实际 body UTF8 字节数加 1024，reservation_policy=request_utf8_bytes_plus_1024_v1。这是保守工程预留，model_tokenizer=null，并非真实 Token 测量或提供商账单保证。请求前预留 input_reserved_tokens + max_output_tokens；不足停止。返回 usage 再核对 input<=预留、output<=指定上限、input+output=total 且 total<=总预算；违反时保留原响应并失败，不再发下一次请求。缺用量保留 null 并失败，不以 0 补齐。

用户已授权的10000000为累计额度，外部使用记录先查剩余再预留，一次调用配置不能获得新的10M；有有效usage按真实total累计，没有可核实用量则保留该尝试的工程预留和未知状态。当前第一冒烟限制一次，输出最多8192。服务/模型/预算已知，正常复用不再次索要。

evidence_anchors 的 excerpt_start_byte/end_byte 必须复制 unit.start_byte/end_byte，范围在其 locator 指向的真实文本资源内；选中非零起点 Span 或 PDF 区域不能重新从 0 计数。quote 为完整 unit.text，excerpt_sha256 是该切片字节的 SHA；resource_sha256 仍是完整来源资源 SHA，二者不混用。模型直接复制这个 anchor，Core 按原资源字节核对。

使用锁定 reqwest 0.13.5、tokio 1.54.1、futures-util 0.3.34，native TLS 校验证书与主机，不禁用验证。禁用重定向、客户端重试和自动代理，只建一次客户端/一次 POST。current-thread runtime 中用请求 future 与每 5 ms 的停止检查竞争，超时/取消丢弃请求 future 并结束 runtime，保持已收字节为 partial，不能当完整响应成功。HTTP body 有实际字节上限，断开的 Content-Length/chunked 响应明确失败。

## 响应与 Core 候选

完整原响应保存 response.bin；partial 明确 response_complete=false。HTTP 200 还须显式status=completed、一个completed assistant message的一个output_text JSON，允许独立reasoning元数据，不接受工具输出、额外文本、拒答或空响应。官方/本地strict策略仍要求error/incomplete_details出现且均为null。仅所选v2按已观察到的稀疏completed回包允许两字段省略，原字节不改写；两字段存在时仍校验null/object及内容，非null错误/不完整细节不能成功，缺/错status也不能成功。wire元数据可扩展，但所有层重复JSON key、尾随JSON、类型错误拒绝。

API 输出 JSON 严格字段为 schema_version=1、request_id、binding、concepts、evidence、atoms。SourceBinding、Concept、CandidateEvidence、CandidateAtom/Measurement 沿用领域规则；它不携带模型自封的 producer。request_id/binding 必须与 Core 请求完全一致；原子固定 required=false、verification=unverified，有真实 supports、UTF8 范围/quote/SHA 和明确未知值。API 与 fake 共用领域校验函数，API 不伪装为 fake payload。

返回模型与配置模型分别保存，不能从别名推造已验证版本。usage 的 input_tokens/output_tokens/total_tokens 必须为非负整数且加总正确；缓存和 reasoning 细项按真实返回记录，缺失保持 null，不重复计费。记录 request attempts、HTTP 状态、响应 id/模型、策略版本、源码版本/SHA、实际 body 指纹及未知计费。

完整 HTTP 200 的响应 metadata/用量独立核对，候选非法、refusal 或 incomplete 不丢弃已经验证的 id/model/usage；候选成功与用量存在是分别记录的事实。partial 或非 200 的响应不将其片段当作已验证用量，原字节/状态仍保留。确实缺失或无效用量保持 null 并区分 not_validated，不能说已解析的真实计量不存在。

在持久化 request/body/响应之前检查是否含所引用凭据（含 JSON 转义字符串）。请求包含凭据时调用前拒绝；响应回显时不保存泄密正文，仅记录已收长度/SHA 和 omitted_credential_echo，非零且没有候选。stderr/报告只用不含服务原始错误正文的稳定诊断；不记录认证头。原响应在正常无凭据回显时不改写。

JSON 字符串可能内嵌候选 JSON；凭据检查须逐层处理转义后的字符串，不能只扫描最外层字节或一次解码。完整空 HTTP 正文仍为 response_complete=true，200 的空 JSON 在语义层失败，空 401/429 保留对应 HTTP 分类；只有实际 framing/读取中断才标 partial。

## 输出与验收门槛

输出只在 config/source/path 预检后建立全新根，owner producer=api 并明确 transport_mode；request.json 保存策略、配置引用和预留，request-body.json 保存实际无认证 body，response.bin 保存安全的原响应，candidates.json 仅成功。validation.json 与 Markdown 的完整 JSON 验证快照逐字段相同，包含产物实际指纹。保存前后重查 source/config/output；失败只撤回已捕获的结果能力，未知人工文件保留，不递归清理。

成功只为 candidates_validated，knowledge_status=pending、published=false、scientific_quality=not_run；local_http_test 的标题/来源必须明确是假服务。配置/路径错误返回 2，HTTP、用量、严格候选、预算、超时/取消和变化失败返回 3；不创建 KnowledgeIR、Run、Release 或 Vault。

作者使用[T01 受控来源与调用前 gold](../../fixtures/extraction-gateway-v1/README.md)作为第一次小范围来源，选择真实 Span ordinal=0；gold 的 10 m/s、温度 20 ℃、仅适用样例 A 先于模型输出固定。普通本地 HTTP 假服务覆盖成功及配置、401/403、429、5xx、截断/非法/重复 JSON、拒答/incomplete、空输出、能力、缺/错误/超预算用量、预留/金额停止、超时/取消、凭据回显与来源/新根保护。

offline checks 和新鲜审查通过后保存实现证据；服务/模型/预算已由用户提供并保存，真实验收还需本机匹配凭据、执行一次实际有界请求并保留原响应/用量/Core核对。真实门槛未满足时只记录实际缺失条件，不把已经给出的信息重新列为阻塞，不complete、不启动依赖的T03，不把本地假服务当收费模型。
