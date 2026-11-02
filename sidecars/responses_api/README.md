# 单一 Responses 适配器

`adapter.py` 是 B04-T02 的 Python 标准库适配器，仅选择以下配置：

| 配置 | 固定值 |
| --- | --- |
| ProviderIdentity.producer | `model` |
| provider_id | `ai-1343263-responses` |
| adapter_version | `responses-stdlib-v1` |
| requested_model | `deepseek-flash` |
| HTTP endpoint | `https://ai.1343263.xyz/v1/responses` |
| 接口核对日期 | 2026-11-02 |

接口字段依据 [Responses create 官方文档](https://developers.openai.com/api/reference/resources/responses/methods/create)及[Structured Outputs 官方文档](https://developers.openai.com/api/docs/guides/structured-outputs)。采用 `text.format={type:json_schema,name:noetome_candidates,strict:true,schema:...}`，无需 SDK 或第三方 Python 依赖。官方协议字段一致不证明该转发服务已兼容；实际结果另由真实冒烟记录。

固定 schema 和指令分别存为同目录 `schema.json`、`instructions.txt`，由 Python 只从 `Path(__file__).resolve().parent` 读取，Core 原生只读核对可通过 `include_str!` 使用同一资产。两文件均无尾部 LF；Python 不调用 strip/rstrip、不改动指令文字，schema 以原字段顺序严格解析。schema 为 360 字节、SHA `0421cac7d779e48c5102472e3304e7746e72fe6c15f83b6640ae407e3087e662`；instructions 为 723 字节、SHA `d43a64004c039cb02a16918081f7a993ee17a48c82257a8899a02fbc69ea38d1`。Core 运行时须守卫并记录 Python、adapter 和这两项资产的身份与 SHA；不能只核对 adapter.py。提取后的 HTTP body 与冻结 synthetic fixture 字节保持完全相同。

## 输入与发送

生产入口由 Core 在受守卫的新工作目录中启动：

```text
<selected-python> -B <absolute-adapter.py>
```

不接收 endpoint、模型、密钥或 fake 参数。唯一 stdin 是既有瞬时凭据帧：magic `NOETOME-CREDENTIAL-STDIN\0`，依次为小端 u32 的版本 1、凭据长度、原始请求长度，再跟随凭据和**原请求 wire 原字节**。凭据为 1..4096 UTF8 字节，拒绝 NUL/CR/LF、坏 UTF8、截断及额外字节。适配器不查询环境变量、argv、密钥文件、临时密钥或代理。Core 负责外发授权、预算、来源登记重验、凭据引用解析和 Windows 进程总时限；此脚本不能代替收费准入。

原请求保持 `noetome-model-single-v1`、schema_version 1，严格拒绝重复、未知、缺失字段、位置数组、错误整数类型和坏 Unicode。完整 `SourceSlice.units` 从 offset 0 连续覆盖，ordinal 从 0 连续，每段 SHA 和拼接后的 normalized SHA 都要匹配。Formatting 片段保留在完整输入中；Core 最终候选验证负责只允许非 Formatting 的真实证据。

HTTP `input` 是一个 JSON 字符串，包含全部 `{ordinal,text}` units；没有另一份完整正文。指令要求抽取全部显式规则，逐项保留条件、单位、例外和缺失值规则，复制所选 unit 中唯一、完整的连续原文表达；不向模型发送独立 expected 清单，不人为限制几条 atoms。模型只返回 `{candidates:[{unit_ordinal,statement,qualifiers}]}`，不生成 UUID、SHA、偏移、审阅状态或输出路径。

政策可收紧但不可放宽：完整输入 <=8192 UTF8 字节，原请求及无认证 HTTP body 各 <=65536 字节，完整 HTTP/规范响应 <=524288 字节，输出 <=1024 Token，执行 <=30000ms，max_calls=1，retries=0。发送字段还固定 `store=false`、`background=false`、`stream=false`、`truncation=disabled`、`parallel_tool_calls=false`、`tools=[]`、`tool_choice=none`。默认 SSL 验证开启，先显式 connect，再最多一次 POST；不跟随重定向，不自动重试。stdio 宿主另外约束 30 秒墙钟及进程树清理，脚本通过每次读写前的剩余 deadline 约束 socket；同步 DNS、OS 和证书加载的完整墙钟边界由宿主承担。

## 解码与保存

只有一个 completed assistant message 中的单个 `output_text` 可生成候选。已知 Responses reasoning item 按命名字段和 summary/content 形状验证、扫描后忽略为候选。工具、检索、function/web output、refusal、未知 output、非完整状态和不合法结构都不生成成功候选。模型返回的实际 `model` 写入 actual_model，不用请求 alias 替代。

每条 statement 必须是指定 unit 中唯一的非空原文连续片段，适配器本地计算局部 UTF8 start/end、真实 span_id 与 SHA，候选 ordinal 按数组次序分配。此检查只证明字节证据和结构，不能证明句意、条件完整、科学正确或全部必要原子覆盖；这些仍需 B04-T03 独立审阅。合法候选保持 `unverified`。

原始 JSON 使用 object_pairs_hook 拒绝重复键，保存前检查裸字节及解码后的字符串键和值，包括 `output_text` 内嵌 JSON 的转义回显。失败不保存远端异常文本、凭据、认证头、受抑制响应的长度或 SHA；stderr 始终为空。Python 缓冲和标准库内部副本没有全面 RAM 擦除保证。

全局 float 解析拒绝非有限数值（包括 `1e999`），官方 temperature/top_p 字段仍接受有限浮点，Token/ordinal 等计数字段只接受 u64 整数。凭据扫描遍历所有字符串，不依赖 JSON 起始前缀、完整对象或闭合引号；quoted scalar、多层编码、代码围栏、任意文本前缀和未闭合 JSON 中的有效转义片段均逐层解码检查。最多 8 层，每层合并合法 Unicode surrogate pair；超深仍有可解码转义时固定拒绝保存。普通文本中的无效自然反斜杠保持原样，不以 JSON 解析失败跳过回显扫描。

只在 cwd 独占创建三个固定文件，已有同名文件拒绝调用、不覆盖：

- `http-request.json`：无 Authorization 的实际 HTTP body。
- `http-observation.json`：下面的严格、脱敏事实对象，nullable 字段总是显式 null。
- `http-response.bin`：仅完整、严格可读、无凭据回显的原始 HTTP JSON。语法/协议/回显/截断/超限失败时不保存；合法 incomplete 或 HTTP error envelope 可以保留供审计。

stdout 只有合法 T01Outcome，不增加原始 HTTP response 等额外字段。usage 为 `provider_reported`；输入/输出未知时显式 null；cost 固定 unknown，amount_microunits/currency=null，basis=not_available。实际 token 数字必须为 u64，拒绝 bool、浮点、负数及不一致 total；已报告输出超限不生成 canonical。incomplete/max_output_tokens 等保留合法 actual_model/usage，failure=capability_insufficient，canonical_response_available=false。

Observation 精确必需字段为：schema_version=1、protocol=`noetome-responses-observation-v1`、request_sha256（原 domain wire）、http_request_sha256、endpoint、http_state、http_status、response_id、actual_model、failure、raw_response_available、canonical_response_available、usage_input_tokens、usage_output_tokens、usage_total_tokens。http_state 为 not_sent（connect 前失败）、remote_unknown（send/read 失败或未收全）、sent_once（完整响应已收到）。本地 PID、进程创建成功和 canonical 缺失都不代表远端未计费。failure 为 null 或 authentication/rate_limit/timeout/capability_insufficient/remote_unknown/invalid_response/response_limit。

## 只读离线入口

```text
<selected-python> -B <absolute-adapter.py> --offline-body <request.json>
<selected-python> -B <absolute-adapter.py> --offline-decode <request.json> <http-response.bin> <http-status>
```

这两种模式不访问网络、不查找凭据、不创建或修改文件。前者 stdout 重建同一无认证 HTTP body，后者 stdout 重建同一 T01Outcome，用于显式离线协议核对；Core extract 只使用受守卫的 body 模式，Core inspect 在 Rust 中重建，不启动任何子进程。失败 exit 2 且 stdout/stderr 为空；成功 exit 0，stdout 为 JSON 原字节，无另加换行。完整 UTF8 wire 的空白与顺序继续参与 request_sha256，不重新序列化域请求。

## 离线验证

`tests/test_responses_adapter.py` 使用纯自造文本、合成凭据、可注入 connection boundary 和本地 127.0.0.1 HTTPServer；生产 CLI 没有注入入口。新增 [synthetic fixture](../../tests/fixtures/responses-api-synthetic-v1/README.md) 与旧 B04-T01 fixtures 分开。行为红绿日志位于 `.artifacts/B04-T02-response-*.log`；普通 Python 回归无真实 provider 请求、密钥或费用。该适配器和离线样例不证明真实 API 已成功、PDF/视觉/批量兼容、知识质量或正式基准通过。
