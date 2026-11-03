# B04-T02 已选网关与凭据续接计划

> 执行者：使用 superpowers:subagent-driven-development；这是 B04-T02 内部续接，不重新派发已经验收的原计划 Task 1/2，不启动 B04-T03。controller 维护账本和 Git，每项实现后由新鲜 reviewer 核对 spec 与质量，最后审查全部续接差异。

**目标**：保存用户已经授权的服务和累计预算，提供本机遮蔽密钥输入及复用入口，以所选网关完成一次小范围真实抽取。

**已授权输入**：OpenAI 接口、`https://ai.1343263.xyz/`、`deepseek-flash`、累计 `10000000` Token。用户要求查找匹配凭据并保存设置，之后复用；另一个网关的 Codex 凭据没有转发给该服务。本轮未发现匹配环境变量。

**基线**：`d1a46df397d58b2f4fedd5b3aecd8a67dcebad03`，分支 `codex/b04-t02-configured-gateway`。两个旧 `docs/Noetome-PRD-v1.0.*` 删除是用户改动，不暂存。所有分工只属于当前唯一 in_progress B04-T02，不修改任务状态、不自行提交、不调用真实服务、不派发子代理。

**依据**：D07/D08、[原 API 契约](../technical/EXTRACTION_API_CONTRACT.md)、[原离线验收](../validation/B04-T02.md)。2026-11-03 实际读取网关公开首页、部署 JS、Sub2API gateway.go 和 DeepSeek Responses API/Guide。Responses 与 JSON Schema 已有官方依据；网关实际模型路由、计量及收费仍须实测，价格保持未知。

## Task 1: 本机遮蔽凭据保存与复用脚本

**文件责任**：新建 `scripts/set_api_credential.ps1`、`scripts/invoke_api_credential.ps1`，以及针对它们的 `tests/test_api_credential_scripts.py`。只编辑这些文件，controller 负责模型设置文件、文档及真实调用。不得打开任何实际用户密钥文件或读取 Codex 认证值。

**保存接口**：`set_api_credential.ps1 -CredentialFile <外部绝对路径> -ServiceUrl <HTTPS origin> -Model <标识> [-Dialog] [-FromEnvironment <明确变量>]`。没有明文 key 参数；默认 Read-Host -AsSecureString，Dialog 使用原生 WPF PasswordBox，显示服务和模型、保存/取消按钮。只在操作者点击保存后保存，Enter 不能绕过检查。FromEnvironment 供明确选中环境变量及离线 public synthetic key 测试；从不枚举环境或默认密钥。

**保存保护**：Windows CurrentUser DPAPI（ConvertFrom-SecureString 默认模式），仓库之外的普通文件；拒绝仓库内/相对路径、任何已有目的文件及 reparse point 祖先。正常创建新文件，不覆盖旧凭据；加密内容不可混入 key 值、服务配置、认证头、stdout/stderr 或错误正文。输入须为非空 ASCII、无控制/空白、最多8192字节。Dialog 不返回 PasswordBox 内容，取消没有凭据文件。保存完成只输出无敏感值的状态及引用路径。保持 PowerShell 5.1/7 Windows 可用，失败用稳定诊断。

**复用接口**：`invoke_api_credential.ps1 -Binary <CLI> -Project <选定项目> -ImportId <ID> -Config <配置> -OutputRoot <全新根> -CredentialFile <DPAPI文件> [-CancelFile <信号>]`。只读明确 config 和 credential；由 config.credential_env 选择2..64位大写变量。解密值仅保存在当前进程内存和子 CLI 环境，不成为命令行参数或临时明文文件；调用既有 `extract api`，保留CLI退出码。finally 还原原环境变量，ZeroFreeBSTR；拒绝仓库内凭据及链接，不自行读私人目录或发起 HTTP。本脚本不自动重试、循环或新增批量。

**验收**：先真实失败再实现，用 public `noetome-test-...` 密钥验证 DPAPI 加密文件不是原文、解密后只断言相等不打印值、已有文件/仓库内/非法输入停止，明确环境变量注入选定子入口且没有 key 值出现在参数或输出、退出码和环境恢复。WPF 使用真实系统加载/窗口入口检查，不把 headless 断言称为用户实际输入。测试不依赖本机固定运行时，Windows 外明确 skip。不产生真实模型请求。

## Task 2: 已选网关的 Responses 请求策略

**文件责任**：现有 extraction_api_config/schema/Core 和相关 scoped native tests。保持 reqwest/TLS、来源/配置/新根保护、严格 wire 与候选领域校验。controller 编写独立外部行为审计和文档。

**端点**：保留已有官方 Responses 离线候选，增加用户明确选定的 `https://ai.1343263.xyz/v1/responses`；后者只认已选 `deepseek-flash`，不宽泛允许其他服务、模型、端口、userinfo、查询、fragment 或路径。local_http_test 的数字环回及 public synthetic 凭据/模型规则保持。

**请求**：按 DeepSeek 当前 Responses 文档及兼容表，沿用 input/instructions/text.format 的 json_schema/name/schema、max_output_tokens、stream=false、tools=[]、tool_choice=none；所选策略 reasoning.effort=none。仅发送官方已核查支持或明确忽略但安全的字段，不能将不受支持的 strict 标记当作服务器保证；Core 继续拒绝非法 schema/重复 key/未知候选/错误来源。旧官方策略保持其 strict JSON Schema 字段。

**审计**：request.json 与 validation JSON/Markdown都记录明确 request_strategy 版本；配置模型与实际返回模型继续分开。格式策略不改变计量定义、输入预留、null/unknown、一次POST、无重试、凭据回显或来源保护语义。先保存所选端点/模型与策略的真实 RED，再最小实现，所有 scoped native tests、普通CLI与独立旧API/fake回归实跑。

**精确裁定**：所选策略 `request_strategy=tangmao-deepseek-responses-json-schema-v1`；旧官方/本地假服务策略 `openai-responses-strict-json-schema-v1`。两个保存快照都包含策略，validation也包含endpoint。所选body保留store=false表达客户端意图，移除background/service_tier/truncation和text.format.strict，添加reasoning.effort=none；不改变name/schema/input/证据anchor。controller 已保存普通CLI 1通过/1失败的真实RED于该计划工作区gateway-cli-red.json，作者普通CLI测试文件由controller负责，不由worker编辑。

## Task 3: 累计预算与真实小范围验收

controller 在仓库外当前用户 Noetome 设置目录保存非秘密服务、模型、credential_env/DPAPI引用、累计10000000 Token上限和使用记录。所有后续调用先查剩余额度；已知实际用量累计，未知用量保留工程预留，不把额度每次重置。首冒烟 max_requests=1、max_output_tokens<=8192、max_response_bytes<=1MiB、timeout_ms<=60000，费用未知保持null。

设置和预算不再重复询问。密钥缺失时继续独立实现/审查，等待本机输入；不代填另一个服务的凭据，不以假服务验收真实模型。已存在的凭据正常复用，不弹新输入窗口。

使用原作者小来源 Span0 和调用前 gold，实际普通CLI一次请求，保存无密钥原响应/用量、真实调用策略、source/binding/SHA、前后来源保护与独立候选核对。运行所有相关离线检查并新鲜整分支审查，按实际成功/失败维护devlog并推送；取得真实候选之前不能启动T03。
