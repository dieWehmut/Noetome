# 真实 API 前的本地来源与凭据边界

本文记录 B04-T02 已实现的本地部分：完整短文本预检和瞬时凭据交接。当前尚未选择提供商或模型，没有真实 API 请求、预算预留或资料外发授权；B04-T02 尚未完成，实际状态以[任务账本](../../dev-log/tasks.json)为准。收费调用和独立真实冒烟仍须完成[执行卡 T02](../execution/NEXT_STEPS.md#b04-t02-单次小文本真实api候选抽取)的验收。

实现入口为 [`model_preflight.rs`](../../crates/noetome-cli/src/model_preflight.rs) 和 [`credential_handoff.rs`](../../crates/noetome-cli/src/credential_handoff.rs)。二者各自提供本地能力，目前没有把完整来源、配置、授权、预算与真实 adapter 串成收费执行入口。旧[候选协议 v1](MODEL_CANDIDATE_PROTOCOL.md)、fixture IR、Compiler Schema1 和项目来源 Schema3 保持原义。

## 完整短文本预检

`prepare_whole_text(project, version_id)` 只准备明确项目内已登记的一个 MD/TXT 版本。它读取项目配置、数据库和持久 raw/normalized 快照，不重新读取原选取文件，也不扫描未登记资料。共用的项目整体一致性检查会复核同一项目内其他已登记的文本及 PDF 快照；其中任何快照损坏都可能使此次预检失败。原文件移动后，登记快照仍可重读；未知或错误版本不回退到另一来源。PDF 返回能力不符，不启动 PDF 解析器。

上限针对**完整规范 UTF8**，为 `1..8192` 字节；raw 保留来源登记的 `64 MiB`（67,108,864 字节）上限。规范化沿用 `utf8-newlines-v1`：先严格解码完整 raw，移除至多一个开头 BOM，将 CRLF 和 CR 转为 LF，不替换坏 UTF8、不裁剪空白、不静默截断。仅有空白或 Formatting、无可用正文的来源拒绝。

例如 `BOM + "x\r\n" × 4095 + "xx"` 的 raw 为 **12290** 字节，完整规范文本为 **8192** 字节，预检接受并保留两份不同的 SHA。规范文本为 8193 字节时整份拒绝，不产出前 8192 字节预览。raw 的长度不是本轮模型文本长度，8KiB 也不是完整 JSON 请求或凭据帧的长度限制。

`PreparedText` 持有三个有明确区别的视图：

| 入口 | 当前含义 |
| --- | --- |
| `text()` | 完整规范文本，包含所有 Formatting 和尾部空白字节 |
| `units()` | 按登记顺序排列的全部真实 Span；拼接后逐字节等于 `text()` |
| `evidence_source()` | 原候选 `SourceSlice`，仅含真实、非 Formatting、非空白 Span，保留原 ordinal 和全局半开区间 |

完整 `units` 必须从偏移 0 连续覆盖到规范文本末尾：每个 Span 已 parsed，来源版本、ordinal、`blocks-v1` parser、UTF8 边界和实际片段 SHA 全部匹配。Formatting 仍参与完整输入覆盖；它没有被伪造成可引用正文。`evidence_source()` 的非 Formatting 子集不能代替完整选取正文；它的存在也不证明真实 adapter 已发送完整文本。

`PreparedText` 生存期内持续持有配置、数据库、raw/normalized 文件及祖先目录守卫。`verify()` 先核对目录/文件身份、文件长度和修改时间，再从同一登记版本重新获取并比较摘要、全文、全部 units 和 evidence，最后复核两组守卫。raw SHA、规范快照字节、规范 SHA、数据库状态和完整覆盖也在重新获取时核对。返回后仍保留守卫直到该对象释放；Windows 写入、删除和目录替换保护不能用一次读取前检查替代。预检不承诺同步 OS 调用或整个函数的严格墙钟时限。

`summary()` 只提供下面的命名必需字段，不包含正文或本地路径：

- `schema_version=1`；`project_id`、`source_id`、`version_id` 和正数 `revision`。
- `raw_sha256`、`raw_byte_length`、`normalized_sha256`、`normalized_byte_length`。
- `normalization_version=utf8-newlines-v1`、`parser_version=blocks-v1`。
- `span_count`（全部 Span）及 `evidence_span_count`（非 Formatting 子集）。

摘要解码拒绝未知、重复、缺失字段和位置数组；UUID、SHA、版本、长度及计数必须合法。正文缺失、未 ready、坏 UTF8、能力不符、超限或登记快照变化均失败，保留项目和来源现场，不自动修复。

## CLI 可见行为

实际命令由 [`lib.rs`](../../crates/noetome-cli/src/lib.rs) 的参数解析和 [`main.rs`](../../crates/noetome-cli/src/main.rs) 分派：

```text
noetome knowledge preflight <项目路径> <version-id>
noetome knowledge preflight -- <项目路径> <version-id>
noetome knowledge preflight --help
```

两个位置参数须分别传入，中文和空格路径作为一个参数；以 `-` 开头时使用动词后的 `--`。这里使用来源导入结果的 `version_id`，不使用 `import_id` 或候选根。当前没有 provider、model、credential、budget 或真实生成命令参数。

成功退出 0，stdout 为带换行的 JSON，stderr 为空。顶层固定字段为：

| 字段 | 实际值 |
| --- | --- |
| `schema_version` | `1` |
| `result` | `"source_ready"` |
| `api_status` | `"not_called"` |
| `externalization_authorized` | `false` |
| `source` | 前述完整 `FullTextSummary` 对象 |

`source_ready` 只表示登记文本通过本地检查；`not_called` 表示该入口没有调用 API；`externalization_authorized: false` 明确表示未获外发授权，实际输出没有名为 `unauthorized` 的字符串状态。它不读取凭据，不创建候选根、Run、报告或 Vault 文件，也不输出正文。

参数错误或来源预检失败退出 2，stdout 为空，stderr 只输出固定错误码/参数说明；来源层固定错误可透传。超限不返回部分正文。序列化或 stdout 写入失败使用一般错误退出，不把输出失败当作来源或 API 成功。

## 瞬时凭据交接

`TransientCredential` 是不序列化、不 Clone、不格式化的有界 UTF8 字节容器。长度为 `1..4096` 字节，拒绝 NUL、CR、LF 和非法 UTF8。`new()` 接受调用者明确提供的临时字节；`from_env_ref()` 只解析显式 `env:NAME`，不枚举环境、不查找文件或自动选择密钥。NAME 长度为 `1..128` ASCII 字节，首字符为字母或 `_`，后续可加数字；引用错误、缺失和非法值分别返回固定凭据错误码。

`from_env_ref()` 的调用顺序仍由上层负责：须先完成配置、来源、预算准入，才解析引用。当前 CLI 预检完全不调用它；该库入口本身不证明这些准入已经完成。

`credential_handoff::run()` 接收明确绝对可执行文件、argv、原始 `ExtractionRequest` 及其 wire、工作目录、临时凭据和取消信号。启动前严格验证原请求政策，限制原 wire 为 65536 字节，并严格解码 wire 与请求对象逐项相等；拒绝未知/重复字段及数组对象。原始 JSON 空白和键顺序保留，不重序列化成另一份请求。

凭据与请求原字节、严格解码后的字符串**键和值**、program/workspace/argv 以及唯一将传递的 `SystemRoot` 作回显检查。命中时返回固定 `MODEL_CREDENTIAL_ECHO`，不启动子进程；无效请求、路径和启动前取消同样不建立子进程。JSON Value 仅用于已严格解码 DTO 的回显检查，不能先把重复字段折叠后当作严格解码。

凭据仅通过私有 stdin 帧交给一个本地进程，按以下顺序写入后关闭 stdin：

| 帧部分 | 字节格式 |
| --- | --- |
| magic | `b"NOETOME-CREDENTIAL-STDIN\0"` |
| frame version | `u32` 小端，固定为 1 |
| credential length | `u32` 小端，1..4096 |
| original wire length | `u32` 小端，至多 65536 |
| credential | 恰好对应长度的原凭据字节 |
| original wire | 恰好对应长度、未修改的原请求字节 |

这份运行时帧和它的长度/SHA 不成为保存的模型事实。响应的 `request_sha256` 仍绑定**原 wire SHA**，不绑定带凭据帧或重新编码的请求。65536 字节原 wire 上限没有因私有帧开销而放宽。

交接复用现有 [`sidecar_process.rs`](../../crates/noetome-cli/src/sidecar_process.rs) 和 Windows Job 边界：子进程显式环境只有 `SystemRoot`，凭据不进 argv、环境或临时文件；只继承所需三根管道句柄。Job 在恢复进程执行前绑定，保留关闭即清理、整树退出核对、并行 stdout/stderr 排空、stdin 阻塞时限和取消处理。响应上限取原请求政策，stderr 最多捕获 65536 字节，执行时限取政策（至多 30 秒），清理另受 2 秒边界约束。当前有限实现不能据此宣称其他平台或任意 API 已兼容。

## 审计、抑制与用量语义

只有完整、无回显且经 `decode_auditable_outcome()` 严格确认协议、身份、原请求 SHA、命名字段和用量格式的 stdout 才能离开交接边界。stdout 同时检查原字节和解码后的键值，转义凭据、未知/重复字段或截断响应不会借 JSON 解码绕过。该审计与最终候选验证分开：可审计但证据错误、候选语义不合法或输出 Token 超限的响应仍保留已报告用量，调用端随后须执行严格候选验证，不能把它当作合法 CandidateIr。

`SensitiveAttempt` 返回固定状态、可选本地 PID、耗时、固定 failure code 和两个 suppression 标记；它不提供被抑制管道的正文、前缀、诊断、捕获长度或 SHA：

| 当前结果 | stdout | stderr | outcome/用量 |
| --- | --- | --- | --- |
| 完整、严格可审计且无回显的响应 | `stdout_suppressed=false`，有 response 和该完整 response SHA | 始终 `stderr_suppressed=true`，无正文/长度/SHA | 有已审计 outcome；不等于候选验收通过 |
| 启动失败、取消、超时、超限、传输错误或响应审计/回显失败 | `stdout_suppressed=true`，response 和 response SHA 均为空 | 始终抑制 | outcome 为空，用量不能补造为 0 |

响应审计失败时本地状态仍可能为 `delivered` 且有 PID；这表示管道交付已经发生，不表示候选或远端生成成功。`not_called`、`timeout`、`cancelled`、`response_limit`、`transport_failed`、`remote_unknown` 等同样首先描述本地 attempt。PID 和进程创建次数都不能作为真实 API 请求数或实际扣费依据。远端是否接受请求、是否计费和未返回用量尚待唯一真实 adapter 记录；未知必须保留 unknown 语义，不能从本地失败推断零费用。

所有 stderr 都抑制，包括干净成功响应伴随的凭据回显 stderr；无效 stdout 也不保留其字节数/SHA。这样不会把短凭据的长度或可枚举摘要变成报告旁路。当前不把 `SensitiveAttempt` 强接旧 v1 `CandidateReport`：旧报告包含 stderr 长度/SHA、本地调用事实和存储约束，不能用占位值或 PID 冒充本轮真实 API 审计。新的收费事实保存与只读重载仍未实现。

凭据容器、拥有的 stdin writer 副本、已取得的部分 stderr/错误诊断和审计失败的 stdout 在指定路径用 `fill(0)` 尽力清理。这里只承诺这些指定 owned 缓冲的 best-effort 清理：传输错误时未交付的 reader Vec、serde 临时 String/Value 等仍是常规 drop，没有全面副本清除。该措施不承诺优化器无法消除清理，不清除调用者原始字节、环境/OS 副本，也不提供所有 RAM 或内存 dump 防护；材料中的操作指令仍只是文本，不授予路径、预算或外发权限。

## 验证入口与当前未完成项

离线行为由 `model_preflight.rs` 内的单元测试、[`tests/model_preflight.rs`](../../crates/noetome-cli/tests/model_preflight.rs) 的真实 CLI 检查及 [`tests/credential_handoff.rs`](../../crates/noetome-cli/tests/credential_handoff.rs) 的 private fake 子进程检查覆盖。用例包括完整 Unicode/Formatting/尾部、8192 与 8193、raw 12290→规范 8192、原件移动、快照篡改、持续 Windows 守卫、严格摘要，以及固定帧、仅 SystemRoot、回显/转义/重复字段、阻塞 stdin、超时/取消和独立 native observer 的实际退出核对。fake 只使用显式合成凭据，不读取已有真实密钥。用例存在不等于检查已运行；本轮实际命令与结果须另存验证记录。

T02 仍缺：按官方文档选定并实现唯一 provider/model adapter；配置与授权指纹及明确完整正文外发范围；单次预算/Token/时限的执行准入；真实远端状态、实际调用和 usage 或明确未知；兼容抑制规则的保存/只读重载；单独显式启用且有脱敏证据的真实冒烟。缺配置/授权/预算时不能收费执行，未跑实际冒烟不能完成任务。本地预检和 fake 进程检查不证明 PDF 模型抽取、科学正确率、全部结构化 API、视觉、批量或长任务能力，也不启动 B04-T03 或生产 Vault 发布。
