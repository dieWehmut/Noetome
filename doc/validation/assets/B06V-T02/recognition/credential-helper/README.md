# 扫描 API 凭据与累计额度包装脚本

[调用脚本](../../../../../../scripts/invoke_scan_api_credential.ps1)只启动一个选定的 Windows 普通 CLI，将已有凭据传给该子进程，并在同一外部累计账本中预留和结算。真实凭据、账本与 API 调用状态依据[实际缺失预检](../vision-api-preflight/missing-current-state.json)；这里的 Windows 测试全部使用公开 synthetic key、独立临时额度账本和无网络代码的 Python fake child。它们不证明真实视觉识别或当前用户余额。

调用参数为 `Binary`、`CaseDirectory`、`ApiConfig`、`Profile`、`OutputRoot`，以及可选的 `CredentialFile`、`UsageLedgerFile`。指定 DPAPI 文件时只在内存解密；未指定时只复用配置中明确的进程环境变量。Live 严格限定已授权的端点、`deepseek-flash` 和 `NOETOME_TANGMAO_API_KEY`。脚本不创建缺失的累计账本，不读取其他默认认证文件，不安装依赖，不启动密钥窗口，不重试请求。

累计账本沿用既有八字段 Schema1：`schema_version`、`approved_total_tokens`、`accounted_tokens`、`pending_reserved_tokens`、`remaining_tokens`、`model_attempts`、`records`、`updated_at`。上限为同一已授权的 10,000,000 Token；逐条 confirmed 之和必须等于 accounted，尚未结算记录的 reservation 之和必须等于 pending，remaining 必须等于上限减 accounted 和 pending。重复 JSON 键、大小写别名与 Unicode 转义别名均拒绝。旧记录保持原字段和数值，不从历史剩余额度初始化当前账户。

配置、账本和 stdout 使用同一个保持 JSON 字符串的解析入口。支持 `DateKind` 的 PowerShell 使用 `ConvertFrom-Json -DateKind String`；实测 Windows PowerShell 5.1 原生保持日期字符串。缺少字符串保护能力的旧 Core 解释器在解析时失败，不能自动将旧日期转型后重新序列化。最终原件读取曾捕获 pwsh 7.6.5 把历史 `.0500710Z` 转为 `.050071Z`，严格历史相等测试的旧实现 RED 保留在 `windows-run-l-timestamp-red`；修复不采用时间语义等价放宽。参数行为依据 [Microsoft 官方文档](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.utility/convertfrom-json?view=powershell-7.5)。

包装脚本采用 `bounded_request_total_cap`，预留本次 API 配置的 `budget.max_total_tokens`。Core 采用 `request_utf8_bytes_plus_1024_v1`，计算实际请求 UTF-8 字节数加 1024 加输出 Token 上限。正确关系是 `Core reservation <= helper cap <= before remaining`；两种预留不要求相等。子 CLI 报告 network_attempts=0 时归还预留；有有效实际 usage 时累计 total_tokens，即使语义 CLI 返回失败；无法核实用量时完整保留 pending。

每个 Live 请求在同一明确外部账本目录以 CreateNew 保存以下原件，其中 `P = <UsageLedgerFile>.scan-<invocation_id>`：

| 阶段 | 收据 | 精确账本副本 | 发生时点 |
| --- | --- | --- | --- |
| before | `P.before.json` | `P.before-ledger.json` | 读取并验证原账本后、任何预留写入前 |
| reserved | `P.reserved.json` | `P.reserved-ledger.json` | 追加预留记录并 Flush 后、启动子 CLI 前 |
| launch | `P.launch.json` | `P.launch-ledger.json` | 实际选定子进程已启动，记录 PID 和开始时间 |
| settled | `P.settled.json` | `P.settled-ledger.json` | 读取子 CLI 输出并写入有效结算或未知状态后 |

原始 `scan-before-<id>.json` 备份也保留。账本通过持有的同一文件句柄写入和 Flush，所选文件及全部祖先在调用期间持有不可删除保护；文件拒绝重解析点和多硬链接，累计账本拒绝第二个并发写入者。写入不是原子替换；中断留下的预留和备份需要明确核查，脚本不会自动恢复或重置账户。

每份收据含以下必填字段：`schema_version=1`、`kind=scan_api_credential_invocation`、`invocation_id`、`phase`、`recorded_at`、`usage_ledger_reference`、`ledger_snapshot`、`selected_binary`、`selected_config`、`case_directory`、`profile`、`output_root`、`reservation`、`launch`、`result`、`settlement`。三个文件指纹对象均含 `path/sha256/byte_length`，直接从持有文件读取实际字节。`reservation` 含 `policy/max_total_tokens`，`launch` 为 null 或实际 `pid/started_at`。

结算收据的 `result` 含 `cli_exit_code/stdout/stderr`；每个流含 `original_sha256/original_byte_length/omission_reason/saved_copy`，saved_copy 是 `path/sha256/byte_length`。正常无秘密输出保存精确 UTF-8 字节。检测到凭据回显时省略整条流，仅保存 `[redacted]` 及原字节 SHA/长度，omission_reason 为 `credential_echo_redacted`，不能将该标记当原件。输出直接向标准流写 UTF-8 字节，避免 PowerShell 5.1 的实测 GB2312 控制台编码改变报告，也不改变父进程控制台编码。`settlement` 为 null 或 `record_status/confirmed_total_tokens/network_attempts`；未启动或未完成的调用可能没有后续阶段收据，不得据此虚构已结算。

回显检测采用 `bounded_recursive_json_echo_v1`，至多解码 32 轮 JSON 转义；每轮之前和最后一轮之后均检查原凭据及同规则归一化凭据。后者覆盖凭据本身含字面 `\n`、`\"` 或 `\u0065`，而其他部分使用更深编码的情况。仍有更深可解码层、归一化凭据为空或过短等歧义时保守省略整个非空流；普通无回显输出仍保留原始字节。只做逐字符 regex 替换的旧实现曾漏过嵌套 Unicode，首轮整流修复又漏过特殊凭据的混合深度，两个真实双 PowerShell RED 和原错误持久化收据均保留在 `windows-run-h-echo-red`、`windows-run-j-special-red`。最终修复实测覆盖普通/Unicode/嵌套/混合回显、31/32/33/40 层边界和特殊凭据，未改旧工具或实际用户账户。

独立预算核对应以实际收据、账本副本、保存 stdout 的完整 Core report 和独立捕获的 SHA 锚点推导。收据元数据字符串本身不能证明当前账户来源。所选 binary/config SHA、四路径、同 invocation、实际网络次数、usage、响应 SHA、先前记录及 Core reservation 都需要交叉核对。

Windows 原件分别位于 `windows-run-a` 至后续真实运行目录；[验证收据](validation-receipt.json)列出每次失败、修复、运行时及所有复制字节的 SHA。公开测试临时目录由 harness 清理，复制前原件保持精确字节；各 fixture 的 `artifact-relocation.json` 明确原路径与镜像目录。没有复制 DPAPI 文件。原 setter 与旧文本调用 helper 均通过 `git show HEAD` 实际字节核对，保持不变。

当前状态恢复应按实际信息推进：用户提供明确迁移位置后只核对那几个选定文件；存在的账本先检查原字节、所有历史记录、pending 与余额，再复用。若只缺凭据，可由用户通过既有遮蔽输入入口 CreateNew 保存一次，额度账本保持原件。若账本确实丢失，先由用户确认最后 30,097 Token 之后的新增调用和未结算预留，再形成带原快照 SHA、来源和未知项的恢复候选；没有这项确认就保持当前额度未知，不自动落盘为零消费或新的 10M。此脚本没有额度初始化或恢复接口。

API 能力依据和原始官方文档副本见[官方核对记录](../official-capability-check.json)。DeepSeek 官方声明支持 Vision；糖猫路由的实际返回模型、价格和底层权重版本仍需分别记录，不从官方模型表推定网关收费或不可变权重。12 次本地 pix2tex 失败与所有源码、依赖和权重清单继续保留在[本地实验历史](../local-pix2tex-experiment/)。
