# 单次 Responses API 与审计协议

本入口对应 B04-T02，只支持用户选定的 `https://ai.1343263.xyz/v1/responses` 和请求模型 `deepseek-flash`。它采用 OpenAI Responses 的结构化文本格式；官方格式文档不证明第三方网关或其模型别名兼容。真实成功以独立冒烟结果为准，离线合成响应不作为真实调用证据。

## 文档依据与固定政策

2026-11-02 实际读取官方 [Responses create](https://developers.openai.com/api/reference/resources/responses/methods/create.md) 与 [Structured Outputs](https://developers.openai.com/api/docs/guides/structured-outputs.md)。Markdown 原文分别为 535450、145408 字节，SHA256 分别为 `3201c1cc5350a3283e290d40d541a6eb55ee8a47bea0e1734b6708234ff3f46c`、`468c24cc11b89e5167d76c9ee7224d96754a29251aeb8eb481d24177bf6fdd17`。服务公开首页自称“糖猫云 - AI API Gateway”，未提供可据以核实模型别名的公开契约。

HTTP body 固定 `model`、完整 units JSON 文本及 instructions，使用 `text.format={type:json_schema,name:noetome_candidates,strict:true,schema:…}`。默认规范文本最多8192 UTF8字节、域请求和 HTTP body 分别最多65536字节、响应最多524288字节、输出最多1024 Token、一次生成最长30秒。配置可以收紧政策，不能提高上限。`store/background/stream/parallel_tool_calls=false`、`truncation=disabled`、`tools=[]`、`tool_choice=none`，无搜索、自动重试、重定向或接口/模型回退。

总预算由用户明确设置。客户端准入预留65536输入 Token 加配置的输出上限，记录依据 `http_body_utf8_byte_cap_plus_max_output_tokens_client_policy`；默认预留66560，低于预留即拒绝。这是保守客户端政策，不是所选模型的 tokenizer 测量或未知服务的计费保证。实际返回的 input/output/total 另行记录；费用未知保持 `unknown` 和 null，不记为零。

## CLI 与明确授权

```text
noetome api preflight PROJECT VERSION CONFIG AUTH NEW_OUTPUT --python ABSOLUTE_PYTHON
noetome api extract   PROJECT VERSION CONFIG AUTH NEW_OUTPUT --python ABSOLUTE_PYTHON
noetome api inspect   PROJECT OUTPUT
```

`preflight` 只读核对完整来源、配置、授权、预算、输出路径和选定运行时，不读取密钥、不创建输出或启动进程。`extract` 准入后才解析 `env:NOETOME_AI134_KEY`。`inspect` 用原生 Rust 只读重建保存的事实，不读取密钥或启动进程。无密钥 CLI 参数。错误准入退出2；完成但无合法候选退出3；合法候选退出0。无报告的中断现场保留，不自动接管或重发。

配置是所有字段必需、拒绝未知/重复/位置数组的命名对象：

```json
{
  "schema_version": 1,
  "protocol": "noetome-api-config-v1",
  "provider_id": "ai-1343263-responses",
  "endpoint": "https://ai.1343263.xyz/v1/responses",
  "model": "deepseek-flash",
  "credential_ref": "env:NOETOME_AI134_KEY",
  "policy": {
    "max_input_bytes": 8192,
    "max_response_bytes": 524288,
    "max_output_tokens": 1024,
    "timeout_ms": 30000,
    "max_calls": 1,
    "retries": 0,
    "search_enabled": false,
    "tools_enabled": false
  },
  "total_token_budget": 10000000
}
```

授权对象精确绑定 `schema_version=1`、`protocol=noetome-api-authorized-source-v1`、UUIDv4 `authorization_id`、原配置字节 `configuration_sha256`、相同 endpoint/model/credential_ref、`budget_total_tokens`、`max_calls=1`、`allow_externalization=true`、实际 `FullTextSummary` 和 RFC3339 UTC `authorized_at`。来源 summary 必须来自已登记完整源预检，含 project/source/version/revision、raw/normalized SHA与字节数、normalization/parser版本、全Span及证据Span计数。配置、授权和来源任何绑定不符均拒绝，不能用一个允许布尔值替代明确资料范围。

用户授权的纸灯箱R7原文为1208字节，SHA256 `5aa51f5816ec39ac4eefd7a8299def081d33a2277a8fdddb66243b4a741c7591`。独立必要原子清单在调用前冻结，不发送给模型；与模型输出的完整内容差异属于T03验收。

## 完整输入与候选

Core 持续保留已登记快照、配置、授权、Python、adapter、schema和instructions的原始文件身份、单链接和祖先目录守卫，并在调用前后复核字节/SHA。Formatting单元随完整文本输入；最终候选证据只许非Formatting真实单元。选定原件移走后使用登记快照，不扫描未登记私人资料。

模型紧凑输出为 `{candidates:[{unit_ordinal,statement,qualifiers}]}`。statement须为指定单元中唯一连续原文；adapter及原生重读各自计算UTF8半开区间、实际span UUID、片段SHA。请求与 canonical Outcome 继续采用[候选协议v1](MODEL_CANDIDATE_PROTOCOL.md)，响应须通过其严格入口，Core形成独立 `CandidateIr`，producer为Model、verification始终unverified。JSON合法、引用匹配和模型自信都不授予科学正确或旧fixture发布资格。

HTTP完整 envelope 只允许已实现的 Responses 字段；unknown/duplicate、不一致状态、工具调用、refusal、无效用量或无唯一完成文本会分类失败。允许官方 reasoning 项作为已验证但不参与候选的输出。记录请求模型与实际返回模型两项，不将别名推断为已确认模型版本。usage数字须为u64、非bool/float，total须与已知input+output一致；未报告保持null。incomplete或输出超限不生成成功候选。

## 瞬时凭据与本地输入

`scripts/invoke_api_with_key.ps1` 是开发辅助窗口，使用PowerShell7/WinForms的masked输入。它先核对并锁定实际CLI的DOS/PE格式标记、SHA256和单链接；此检查不表示验证Authenticode签名。Cargo构建产物可能与deps目录共享硬链接，应复制到新的普通文件再核对原构建SHA，不能放宽单链接约束。参数不含密钥。新输出根与状态文件须位于同一个已有空普通目录，不能接管旧现场。

用户在本机窗口输入的值只进入这一个Core子进程的环境（另保留SystemRoot），不会注册系统环境变量或保存密钥。按钮单次门闩防止重复启动；窗口排空并丢弃Core stdout/stderr，状态文件只有固定phase/PID/exit_code/error_code。普通读取共享冲突仅重试本地状态发布，不能重新启动Core或API。字符串清理属于尽力缩短生命周期，不宣称进程内存可完全清零。

隐藏PowerShell宿主不等于隐藏输入框：Form的Shown事件只检查并显示本进程自己的窗口，console保持隐藏。状态原子替换遇到Win32错误1175时，依据[微软契约](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-replacefilew)和两份文件的实际原生身份复核，才能使用现有有界本地重试；外来临时文件保留、外来状态拒绝，其他部分替换错误不泛化为忙碌。该重试不会新增API请求。

Core给provider的环境仍仅SystemRoot，凭据经既有版本1三管道stdin帧交接，不在argv、认证头审计、文件或报告中保存。adapter和Core两层拒绝原始及JSON转义/嵌套/前缀/截断内容中的凭据回显；转义解码有限深度，超过检查深度拒绝保存。未知编码不作全覆盖宣称。stderr内容、凭据及其派生指纹不进入API报告。

## 独占持久根与远端事实

API根使用独立schema2、`noetome-api-single-v1`，与旧CandidateReport v1分离，不迁移旧根。精确库存为 owner.json、config.json、authorization.json、request.json、reservation.json、attempt.json、http-request.json、http-observation.json、http-response.bin、response.json、candidate.json、report.json；http-observation/raw/response/candidate按实际结果可缺失，已发布完成现场必须有report。无report的中断现场保留，但不能当已完成报告重读。普通JSON最多64KiB，raw/canonical/candidate最多512KiB。拒绝未知文件、链接、junction、重叠项目或已有根。report通过独占临时文件最后发布；保留持续文件和目录守卫。

在provider执行前写入attempt marker，初始远端状态为remote_unknown。HTTP body离线生成进程也不拥有密钥，不访问网络。provider显式连接后只调用一次POST；receipt的 `not_sent` 表示连接/发送前失败，`sent_once` 表示已收到完整HTTP响应，`remote_unknown` 表示发送/读取未能确认。PID及进程成功不作为发送次数证据；错误HTTP状态仍可能计费。`confirmed_http_posts` 依receipt记0/1/null，离线私有callback强制execution_kind=offline_test、真实HTTP请求数0。

report记录源/config/auth/runtime资产指纹、原域wire指纹、无认证头HTTP body、实际安全raw和canonical指纹、reservation、进程事实、receipt、实际模型/用量、明确未知费用、候选身份/成功与固定分类失败。拒绝保存不能确认安全的raw，未知发送结果不重试。

安全raw与合法候选分别判断：完整有界UTF8、严格JSON的重复键/非有限数检查及凭据扫描通过后，原始JSON可作为失败诊断保存；envelope含未知/缺失/不合格字段时仍拒绝候选，并保持model/ID/usage未知。未通过安全检查的字节仍不保存。首次旧adapter将两项判断合并，导致其已丢弃的失败响应无法恢复；历史报告按已审阅的固定资产指纹及原始raw策略重读，不允许任意历史hash，也不执行旧路径。

`inspect`从受保护API库存读取配置/授权副本和实际调用者选定的来源项目，原生核对固定body字段/schema/指令/完整units的语义及raw/canonical与IR关系。extract现场实际body与离线body精确按原字节比较，保存SHA始终绑定保存原字节；reader允许JSON字段顺序和无意义空白差异，不强制单一JSON序列化。报告中的原配置/授权/Python/adapter路径仅为来源记录，不被打开或执行。原配置文件归档或移走不阻止重读。指纹和交叉约束证明当前记录相互一致，不宣称能发现所有外部锚点连同记录一起重写，或代替服务端签名。

## 验证边界

普通回归使用新增的[合成Responses样例](../../tests/fixtures/responses-api-synthetic-v1/README.md)、本地connection boundary和受控私有callback，不读取真实密钥或调用真实服务。API Core测试显式选择 `NOETOME_TEST_API_PYTHON`；本机运行时绝对路径不写入仓库配置。行为红/绿、独立审查、全workspace/Python及devlog检查与实际冒烟分别保存。真实密钥未输入、请求未发生或响应不合格时，T02不得complete。科学质量、PDF模型能力、批量/长任务和生产Vault由后续任务分别验收。
