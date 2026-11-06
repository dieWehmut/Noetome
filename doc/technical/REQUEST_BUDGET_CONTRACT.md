# 有界 synthetic 请求运行与持久预算契约

适用 B07-T02，关联 FR-36、FR-38、D08、AC-29/30，依赖已验收的[有限检查点](RUN_CHECKPOINT_CONTRACT.md)。本合同定义本项实现与验收范围，实际进度由[账本](../../dev-log/tasks.json)维护；接口、计划或测试代码存在不等于已经验收。

## 范围与接口

只复用既有 `extraction_api` 的一次 Responses 请求、严格来源/候选校验和原件保存。新入口仅接 `local_http_test`、`openai_responses`、数字环回地址和 `noetome-test-` 人工模型/凭据。既有普通 API/Chat/live 路径、外部 DPAPI 封装及真实累计10000000额度不变。本项不读取或迁移真实额度账本，不接真实模型、GUI、生产Vault或新的模型SDK。

```text
noetome run api init <authority-config.json> <尚不存在的预算根>
noetome run api start <run-spec.json> <预算根> <尚不存在的运行根> [--paused]
noetome run api resume <同一run-spec.json> <预算根> <运行根>
noetome run api status <预算根> <运行根>
```

初始根父目录须存在。参数路径分别确认普通祖先、真实身份和单链接；配置/来源/预算根/运行根不得碰撞。只读status不请求、不结算、不修复SQLite。来源由显式选定的新测试项目/import/Span组成，不扫描未选私人目录。

Authority配置严格字段为 schema_version=1、producer=synthetic_api_budget、global_budget、opening。预算包含 max_requests、max_total_tokens、必填nullable的max_cost_usd；本项价格未知，金额上限不为null则停止。opening分别保留accounted_tokens、held_tokens、dispatched_requests、held_requests，来自明确synthetic作者输入，只在第一次初始化写入。已有根拒绝init，新Run不能获得新的opening或全局额度。

Run配置严格字段为 schema_version=1、producer=synthetic_api_run、run_budget、concurrency、max_elapsed_ms、retry、units。最多4个单元，concurrency为1或2；每个单元有 unit_id、project、import_id、api_config、max_attempts、max_retry_tokens、max_elapsed_ms。unit_id仅1..32字节ASCII字母数字及`_-`，max_attempts为1..4；运行时限1..60000ms，单元时限不超过运行时限。retry包含base_delay_ms=1..5000和max_delay_ms=base..10000。旧单次ApiConfig仍限制max_requests为0或1。

## 权威账本与原子预留

新预算根采用独立Schema1/application_id `0x4e425544`，`control/budget.sqlite`只有固定metadata与追加events。metadata固定authority、opening、上限、owner、编译来源；events保存原始JSON、序号和前一SHA。投影余额和状态均从完整有界事件链重建，不接受调用者或数据库汇总counter。未知表/字段/schema、错序或非法迁移、碰撞/替换/sidecar现场保留拒绝，不使用新账本清零绕过。

所有预留和发送许可在`BEGIN IMMEDIATE`取得后重新读取完整当前链，核对Run、单元、终态、期限、输入和余额，再提交。每个工作线程独立打开同一authority；HTTP、候选验证和退避均在事务外。Run可共享authority，旧Run的用量和承诺始终进入全局计算。

每次实际请求正文继续采用旧算法：`R = body_utf8_bytes + 1024 + max_output_tokens`。它是工程预留，tokenizer和价格未知，不是服务商账单上限保证。所有加减与用量加总检查溢出。各scope的已占用量为已确认实际用量加未决承诺；请求数分别保留已派发和待派发预留。Run/global的Token与次数均满足时才允许新的请求，attempt_no>=2还受单元独立retry Token上限约束。

Prepared封存已保存的请求/body、原目录与文件身份，但不占预算；Reserved占Token/请求slot。Denied保留Prepared，重复Denied不扩张历史；若其他在途结算释放额度，可在原期限内再检查同一计划。没有可释放在途额度后明确预算停止，保留未完成工作。成功结算释放Token差额，已派发请求次数不退回。

## attempt身份、发送和结算

attempt身份为原Run/WorkUnit/attempt_no/原API生成的request_id四元组。请求UUID、来源绑定、配置/策略、实际正文SHA/长度、预留和原件位置先验固定。不能以相同响应SHA合并两个不同请求，也不能通过重启改写旧attempt身份或回退逻辑attempt号。

| 状态 | Token/请求承诺 | 恢复条件 |
| --- | --- | --- |
| Prepared | 无 | 原worker失去能力后可明确NotSent；不收养未知目录 |
| Reserved | 完整R和一个slot | 尚无MaySend可按持久协议明确NotSent释放 |
| MaySend | 完整R，视为可能已发送 | 发送许可只能取得一次，不能幂等再次发HTTP |
| Unknown | 原R及已派发请求保留 | 无可信用量；只接受同一身份后续确证，不自动退款 |
| Known | 真实Token及已派发次数 | 同一完整receipt重复交付只结算一次 |
| NotSent | 无网络用量和slot | 只接受协议证明或安全最终report的network_attempts=0 |

MaySend提交在实际发送前。之后即使进程在真正HTTP前退出，也保守保留未知；不靠退出状态、超时或非200推定未计费。相同key重复结算要求全部核算字段及原件一致；不同key即使响应相同仍分别计量。合法200用量与候选成功分开，坏候选也结算已验证用量。缺/错用量、非200或partial保持null/Unknown。实际用量超过预留也原样入账并停止相关Run/全局的新请求，不截断数字来维持表面上限。

## 传输原件与恢复

仅新run-hooks路径在旧OutputRoot中增加transport-receipt.json；普通旧API Noop维持原库存。该严格记录绑定request_id、http_status/http_request_id、response_complete、network_attempts、response SHA/长度、response_file、error_code、omission_reason与网络elapsed_ms，schema_version=1、producer=synthetic_api_transport_receipt。nullable字段必须出现；凭据回显保持省略原文，不保存认证头。

safe response.bin与transport记录共同sync后才提供`response_original_sync`边界。不因正文能解析JSON就推定HTTP framing已完成。最终validation.json回调位于原API全部source/config/output复验之后；若提前终止，只有原transport明确完整HTTP200且原件、策略和身份通过复核，才能复用旧wire/Core校验生成明确的recovery-validation.json。原request/response不修改，也不再次发送同一attempt。原response单独存在而缺framing证明则继续Unknown。

Run根、来源与配置原FileStamp以及实际编译指纹均首次固定。输入/配置/版本失配或未知现场停止新调度，原Known和Unknown全局历史仍保留，不为新Run释放。当前不做跨平台搬移、authority迁移或任意断电修复；严格sidecar拒绝与可恢复边界各自实测。

## 重试、期限和隔离

实际HTTP白名单只有RATE_LIMIT、SERVER、TIMEOUT、API_TRANSPORT、TRUNCATED。退避为`min(base * 2^(attempt_no-1), max)`，固定规则不宣称遵从未保存的Retry-After。retry_at_utc先持久保存，恢复不提前发新请求。Run和单元绝对期限第一次登记，不通过重启重置；取得发送许可时再次检查，网络时间也受旧适配器边界约束。

REQUEST_INTERRUPTED另为显式resume恢复分类：仅旧worker已失去发送能力、没有可信final receipt、MaySend已保守Unknown、当前身份/来源通过复核时允许新attempt，旧R保留。这不把有最终report的非法wire、候选或输出错误泛化成retry。

认证、其他HTTP、非法wire/候选、无效用量、上下文/能力或来源失败按固定非retry策略处理，不修JSON或截断输入。次数/retry预算/期限耗尽的单元隔离，不依赖它的单元只在Run/global仍允许时继续。预算停止不表示候选或知识完成；本项输出始终published=false、knowledge=pending、scientific_quality=not_run。

## 独立验收

作者原件、故障策略、预算expected先冻结。旧CLI零请求的正文5238B、输入6262、输出上限2048确定R8310，近限額9009使真实700结算后只余8309。独立检查器从实际request/response、transport、SQLite事件和原件重建预算，与报告相互核对。

至少覆盖两个并发单元与跨Run上限、非零opening/既有消费、新Run不得重置、未知保留、同key重复结算、不同key相同SHA、合法用量坏候选、白名单重试/有限退避/期限、失败隔离以及真实OS终止后恢复。保存原请求/响应、强制终止与wait、服务器真实接收时序和完整Rust/Python回归。只在实际验收后更新完成日志和推送，离线结果不推为真实网关恢复或供应商费用绝对保证。
