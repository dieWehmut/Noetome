# Worker CLI与运行协议

本文件固定V1运行和失败语义，实体字段以[DATA_CONTRACTS](DATA_CONTRACTS.md)/[schema.sql](schema.sql)为准，默认值以[CONFIG](CONFIG.md)为准。协议版本为1；本轮仅设计，未宣称代码或跨进程测试通过。

## 1 Worker生命周期与单写者

`noetome worker --project <dir>`是持久任务宿主。CLI/桌面先尝试连接，未运行则使用确定的已安装CLI绝对路径启动隐藏进程，再握手。两个客户端竞态启动时只有取得`state/worker.lock`独占OS句柄者服务；另一个等待并连接既有worker，不删除锁文件推断进程失效。

一个worker只打开一个project；数据库写入均通过worker串行事务。不同项目可同时运行，但相同Vault由`.noetome`发布锁互斥，归属冲突拒绝接管。worker锁覆盖迁移、普通运行、发布与恢复，不能每个Job自行打开第二个写者。

桌面关闭提供“托盘继续”或“安全暂停并退出”：前者worker继续且托盘显示实况；后者发pause，等待可恢复检查点后关UI。CLI退出/客户端断线不等于取消Run。没有活动Run、客户端或待恢复发布时，worker空闲60秒退出；持久paused/blocked任务允许进程退出，重启默认不自动恢复执行。操作系统注销/关机只尽力检查点，启动必须恢复账本，不能依赖一定收到退出事件。

Python解析器是worker管理的子进程，放入带kill-on-close的Windows Job Object；stdout只用于协议，stderr脱敏日志。worker异常退出时终止该解析进程，孤儿不能继续写资源。发布关键步骤不放Python进程执行。

## 2 本地IPC与消息帧

Windows命名管道名称：`\\.\pipe\Noetome.<用户SID的SHA256前16位>.<project-id>`。DACL只允许当前用户及SYSTEM，设置拒绝远程客户端；客户端验证服务端进程身份、项目ID及二进制协议版本。V1不以随机TCP端口、浏览器CORS或路径保密替代权限。

每帧为4字节little-endian无符号长度 + UTF-8 JSON，长度为JSON字节数，最大16MiB。拒绝零长度、超限、无效UTF-8、重复JSON键、非有限数字、未知协议主版本；关闭违规连接并记录错误。大正文、图片、PDF只传对象引用，不能挤入消息帧。标准JSON-RPC 2.0 request/response/notification封装；id为字符串，一连接不得有重复在途id。

首帧必须`system.hello`，params含protocol_version=1、client_kind=cli|desktop|parser、client_version、project_id；返回server_version、storage_schema_version、capabilities、event_sequence。主版本不一致拒绝运行；新可选字段必须在声明能力内，不能静默忽略关键审批字段。

写命令params含`client_command_id` ULID。`command_receipt`的request_sha256、被接受的操作实体/Job与初始事件必须在同一事务持久化，之后才返回accepted；不能先单独写收据再丢失待执行操作。同ID同hash返回既有结果及实际状态，同ID异hash报`COMMAND_ID_REUSED`。响应丢失先查询收据，不换ID重复副作用。并发请求以实体revision和期望状态CAS，禁止后到请求覆盖新状态；文件副作用仍经发布journal恢复。

响应result含command_id、accepted_at、实体id/revision及实际状态；“accepted”不代表任务完成。错误data固定code、category、retryable、entity_id可空、details（脱敏），不直接返回堆栈或密钥。

`project.events`从after_sequence订阅`project_event`持久事件；事务提交才递增并广播。客户端去重、断线按sequence重放，发现缺口主动拉取快照；UI不能按推测补进度。每连接待发事件队列上限128帧，超限断开让其重连补读，不拖慢worker写入。

## 3 命令接口

CLI总格式`noetome <group> <action> --project <dir> [--json]`；`--json`向stdout只写单个JSON结果，进度/诊断走stderr。长任务默认返回Run ID，`--wait`订阅直到停止并给最终报告。帮助及version不得启动worker。

| CLI | IPC method | 行为 |
| --- | --- | --- |
| project init --path <dir> | 初始化在worker前执行 | 新建空项目、schema0与配置，现有非空目录拒绝覆盖 |
| project inspect | project.inspect | 只读状态与兼容性，不自动迁移 |
| project migrate | project.migrate | 独占备份后逐版本迁移 |
| config show / set --file <json> | config.get / config.update | 全量验证、revision CAS；密钥单独凭据入口 |
| source add <paths...> | source.add | 显式选取文件，导入账本 |
| source list / inspect --id <id> | source.list / source.inspect | 来源/页/Span/失败状态 |
| source remove --id <id> | source.remove | 从未来选择移除，不删原件/历史 |
| run start --mode source\|research | run.start | 固定来源、目标、配置、预算，返回Run ID |
| run status --id <id> | run.status | 快照含完成维度及阻塞原因 |
| run pause / resume / cancel --id <id> | run.pause / run.resume / run.cancel | 持久状态转换 |
| run retry --id <id> | run.retry | 终态创建关联旧Run的新Run，复用合格缓存，不改旧历史 |
| run resolve --attempt <id> --decision <choice> | run.resolve_attempt | 处理未知外调，见第7节 |
| report show --run <id> [--format json\|md] | report.get | 从实际快照生成JSON或Markdown，不触发知识再生成；桌面用自身安全视图展示 |
| changes list / inspect --id <id> | changes.list / changes.inspect | 显示B/C/N、影响和审批指纹 |
| changes edit --id <id> --file <candidate-json> | changes.edit | 有限typed候选编辑，新revision与manual来源，重验后旧审批过期 |
| changes approve / reject --id <id> --expected-hash <hash> | changes.approve / changes.reject | 审批CAS，不能通配批准所有未来变化 |
| vault publish --change-set <id> | vault.publish | 执行已合格且批准的闭包 |
| vault recover [--release <id>] | vault.recover | 核对journal与磁盘后安全恢复 |
| vault rollback --release <id> | vault.rollback | 建立逆向ChangeSet，仍经过人工保护 |
| export create --release <id> --output <dir> | export.create | 新目录可迁移包，校验脱敏及引用完整 |
| credential set / remove --provider <id> | credential.set / credential.remove | 秘密从交互输入读取，不接受命令行明文参数 |
| doctor | system.doctor | 版本、路径、磁盘、parser/API能力诊断；联网探测须显式选项 |
| fixture run --case <fixture-dir> --project <temporary-project> --output <new-dir> | 内部同领域服务 | 仅开发离线夹具，producer=fixture，拒绝生产Vault |

CLI准确参数在对应任务建立clap定义和帮助快照；上表命令名、状态、副作用与退出码不可临时更换。桌面按钮调用同名IPC；不存在第二套UI直写API。扩展Agent能力只预留gateway kind，不提供V1外部任意命令执行接口。

| 退出码 | 固定含义 |
| --- | --- |
| 0 | 同步操作成功；非wait仅表示已接受；wait需完整达到本次目标 |
| 1 | 内部/IO/网络终结故障，报告包含根因 |
| 2 | 参数、配置或输入前置条件不合法，尚未接受执行 |
| 3 | 已接受但暂停、取消、预算停止、待输入/审批、质量不完整或部分发布 |
| 130 | 当前CLI被Ctrl+C中断；未表示worker Run已取消 |

`--wait`中的Ctrl+C默认仅解除客户端等待，stderr显示Run ID与恢复命令；用户要取消须显式run cancel。fixture前台无共享worker时中断取消自身临时作业。

## 4 Run状态机

Run固定输入manifest、mode、config revision、预算及批准范围；同一Run中不能悄悄换模型、来源或风格。更改这些内容创建新Run并关联旧Run；仅暂停/恢复和显式未知请求处置写同一运行历史。

| 当前状态 | 允许后继及条件 |
| --- | --- |
| draft | ready（输入/配置/预算合法）；cancelled |
| ready | running（取得调度权）；cancelled |
| running | pause_requested、cancel_requested、blocked_input、blocked_config、budget_stopped、failed、ended |
| pause_requested | paused（全部在途作业已检查点或转uncertain）；cancel_requested |
| paused | ready（显式resume且前置条件仍成立）；cancelled |
| cancel_requested | cancelled（本地活动已停止、外部未知已记账） |
| blocked_input / blocked_config | ready（问题已显式解决并复验原配置）；cancelled |
| budget_stopped | ready（仅在原预算内核销未知占用后足够）；cancelled |
| failed / cancelled / ended | 无后继；retry创建新Run |

预算上限或配置改变须新Run，不能以resume清空已用量。`ended`表示可调度工作已结束；completion_dimension及Release决定是否完整成功。部分批准发布、未解决质量项可以ended但CLI返回3。

pause立即停止领取新Job，发协作取消给可中止作业；持久化当前结果后才paused。cancel不删除已有知识、对象或已发布Release；发布正在不可分步骤时先完成该步骤并记录journal，再暂停/恢复，不在文件移动中间直接退出。

## 5 Job 调度 租约与幂等

Job状态固定`queued/running/retry_wait/blocked/succeeded/failed/cancelled/skipped/uncertain`。依赖DAG不得成环，全部前置succeeded或明确允许的skipped才可领取。required任务不能因依赖失败自动标skipped；写blocked并保留分母。

领取在短SQLite事务中执行：比较queued或到期retry_wait、Run状态、并发/预算；写running、lease_owner、lease_expires_at，递增lease_generation及attempt_no。每次结果提交必须匹配job_id、owner和generation；旧worker/线程即使晚返回也无权写入。心跳和租约按CONFIG，所有写连接开启foreign_keys、WAL、busy_timeout=5000ms。

重启发现running租约失效：纯本地可重做Job回queued；外部已dispatched转uncertain并保持预算；发布Job按journal恢复，不能一般重试。系统时钟回拨不得延长已失联租约，活进程用单调时钟判耗时，持久租约结合worker持有OS锁及boot/session身份判断。

本地成功输出先安全落盘、hash校验，再一次事务记录结果、对象引用、事件及Job succeeded。相同idempotency_key在同语义输入下只允许一个有效成功结果；缓存命中也记录来源Job和验证版本。失败事务不留半套IR。

并发固定默认总4、parser1、模型API2、检索2，所有资源信号量由worker管理且总上限同时适用。最大尝试3包括首次、网络重试及结构修复。明确可重试错误采用2秒指数退避、最大本地60秒；Retry-After更长必须等到其时限或预算停止，不能截短。结构修复也消耗API请求/Token预算，不开独立无限循环。

## 6 预算预留

调度外部请求前，在同一事务创建`budget_reservation(held)`和`external_attempt(prepared)`。Token预留=可确定的输入上界+配置的输出上限；不支持可靠上限则拒绝受严格预算控制的请求。请求数量也先占用。货币用整数currency micros，同Run单币种；价格未知时max_cost_micros只能null，并明确费用不保证。配置默认值见CONFIG。

可用额度=上限−已结算消耗−held−unknown；不得为重试借用未释放预留。返回可信usage后settled，保留原预留与实际量并记录差异；超出预估立即停止新请求并报告，不能伪称能强制提供商不超账。仅能证明未发送或服务商明确未计费时released。unknown持续占用全部预留。

active wall累计running/pause_requested/cancel_requested中的执行时间，不计显式paused；跨重启累计，不因新进程清零。心跳丢失区间保守计到最后租约到期，单调时钟防回拨。达到任一预算停止新请求，已发出调用结果仍接收和记账，unknown不消失。

## 7 外部调用未知结果恢复

外部调用包括模型、搜索、抓取。必须先把prepared改dispatched并持久化，然后才进行网络发送；持久化失败不发送。请求ID、provider_request_id可空、payload hash、idempotency key、attempt_no、reservation ID均可追溯，密钥及完整私有正文不进入普通日志。

- prepared且证明未开始发送：可释放预算并重排。
- dispatched后超时、连接中断、进程崩溃：`outcome_unknown`，Job uncertain，Run blocked_input；不能假设失败、自动重发或释放预算。
- 收到完整成功响应：先保存校验过的原始响应对象，再做候选校验；业务Schema失败不是网络未知，仍结算可确认usage。
- 明确错误响应：external_attempt failed；仅提供商文档/响应明确可安全重试才退避重试，费用未知仍保留unknown预留。
- 提供商支持请求查询或已验证幂等键时，恢复可查询同请求或重用同键；能力profile未经实测不能臆测支持。

`run resolve`选择固定为`wait`（继续保留，等待查询结果）、`abandon`（丢弃该候选，unknown预留仍按上界占预算，actual费用保持null）、`reissue`（用户明确接受可能重复收费，建立新attempt并重新占预算）。abandon解除等待该结果的交互阻塞，但对应required工作仍失败/不完整；不能把保守预算占用展示为提供商确认的实际费用。不能用“重试所有”替代未知收费确认；实际请求若晚到，只关联原attempt做去重，不重复导入。

## 8 Parser协议与失败隔离

Parser使用相同帧封装及hello，方法`parser.capabilities`、`parser.parse`、`parser.cancel`、`parser.shutdown`。parse参数含job_id、lease_generation、source_version_id、只读来源路径、原始hash、允许输出根、页范围及版本化参数。必须校验收到的hash，禁止网络访问；使用OS受限子进程和最小继承句柄，不声称Python进程天然安全沙箱。

每页输出page/Span/region/figure候选及相对资源manifest；worker重新校验路径、大小、hash和Schema后才导入objects，拒绝`..`、绝对路径、reparse及写出工作根。页超时按CONFIG，单页失败记录unparsed后继续其他页，最终报告缺页；协议损坏或进程失败使该解析attempt失败，不伪造完整manifest。

cancel后5秒无响应则终止Job Object，记录已验证页和未完成页；恢复从固定原件hash及parser版本的未完成页开始，不将两次不同版本解析拼成同manifest。解析密码从短期进程内输入传递，不保存日志；无法解密blocked_input。

## 9 观察 自动测试与更新

日志使用tracing JSON，至少run_id/job_id/attempt/phase/error_code/duration；默认不记录私有正文、密钥及完整请求。报告记录状态转移、预算已用/预留/未知、重试次数、阻塞原因和候选/发布指纹。UI展示真实事件，没有随机百分比、无依据ETA或把开始当成功。

自动UI固定Tauri官方Windows WebDriver路径：tauri-driver + 与WebView2运行时匹配的Microsoft Edge WebDriver + WebdriverIO9，均为开发测试依赖，绝不装进产品运行时。实现接口与隔离/采证要求见[UI_AUTOMATION](UI_AUTOMATION.md)；B15/B16落地，B18保留实际Obsidian验证。

升级在无活动Run且无未恢复发布时退出worker，备份及迁移见[PUBLICATION](PUBLICATION.md)。协议/库新主版本拒绝旧客户端操作，不能静默降级。安装器不边覆盖正在发布的worker边继续升级；恢复失败时保留原二进制和项目数据供诊断。
