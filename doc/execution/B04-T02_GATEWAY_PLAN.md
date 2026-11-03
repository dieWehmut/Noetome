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

### Task 2 实际网关修复轮：自足输入与稀疏 completed 响应

用户已通过本机窗口保存实际密钥，CurrentUser DPAPI复用0请求预检通过。首次真实POST返回HTTP200、配置/返回别名deepseek-flash和有效551输入/385输出/936总Token；原始响应遗漏error/incomplete_details，模型回复要求补充任务，并未返回候选JSON。原响应及首次请求保持独立，936已累计结算，余额9999064；不把失败算为T02真实验收。

按这次实际证据最小修复同一Task2，版本更新为`tangmao-deepseek-responses-inline-json-schema-v2`。所选input仍是一个JSON文本，但自足包含`task`（现有抽取指令加仅输出一个JSON、无Markdown）、`response_schema`（完整现有candidate_schema）和`request`（原request_id/binding/units/evidence_anchors）；顶层instructions和text.format仍保留，官方/本地input完全保持原格式。不通过剥离代码围栏或猜字段接受非JSON；Core严格领域/引文/预算保护不变。实际reasoning摘要说明effort=none不能声称服务已执行该参数。

Wire按经过精确端点/模式校验的策略分流：仅该所选策略允许显式status=completed时省略error/incomplete_details，两字段存在时仍严格校验，非null error、incomplete状态/细节、缺/错status、非法类型、重复key、工具/额外文本均不能成功。原官方/本地strict策略继续要求两字段出现。不修改原始response字节，也不从缺失usage推造0。

文件责任增加当前wire模块及Core调用处；原Task2实施者只改config/schema/wire/Core内的scoped测试，controller维护普通CLI作者测试、文档、累计记录和真实调用。先保存自足input与selected sparse-envelope的真实RED再实现/GREEN、复审；首个真实非JSON响应应从envelope失败推进到明确candidate失败而仍拒绝，完整metadata936不得丢失。通过离线相关回归和新鲜修复审查后，controller在同一累计额度内仅再执行一次有界验证请求，独立保存第二次原响应与结果，不自动循环或改写gold。

### Task 2 第二次实际失败后的 Chat 关闭思考修复轮

第二次v2真实HTTP200开始生成候选，但返回status=incomplete、incomplete_details.reason=max_output_tokens，8192输出额度中包含31255字节reasoning摘要，assistant JSON在3627字节处截断。Core保持失败；有效input1491/output8192/total9683已结算，累计10619、剩余9989381、pending0。首/第二原响应、来源和调用前gold保持原字节，不接受截断候选。

2026-11-03实际读取DeepSeek Chat Completions及Thinking Mode官方Markdown、上游Sub2API gateway路由。官方Chat参数thinking={type:disabled}明确控制非思考模式，max_tokens限制输出，response_format={type:json_object}需在prompt明确要求JSON；上游公开路由存在/v1/chat/completions。这些是精确协议候选依据，部署是否转发开关、实际模型身份和计费仍须实际响应验收；不把Responses中已观察无效的reasoning.effort=none当成功。

最小迁移已选服务的活动接口为Chat Completions，新增provider=openai_chat_completions及封闭策略tangmao-deepseek-chat-json-object-no-thinking-v3。live仅接受同一已授权origin的https://ai.1343263.xyz/v1/chat/completions与deepseek-flash；本地测试仅数字环回、显式端口、/v1/chat/completions和人工模型/凭据。provider/path不匹配、其他主机/模型/端口/查询/fragment仍请求前失败。旧openai_responses官方/本地及所选v2保持请求/解码/版本，不改变历史配置和响应。

Chat body发送model、messages（system为原完整抽取指令，user为自足task/response_schema/request JSON文本）、thinking={type:disabled}、response_format={type:json_object}、max_tokens（原max_output_tokens限额）、stream=false；字段以官方支持为界，不宣称server强制JSON Schema、不删除代码围栏、不补字段。完整严格候选Schema继续放在user文本内，Core复用现有领域/引文/版本/状态校验。实际序列化正文仍最多256KiB，输入预留仍全部UTF8正文+1024，输出8192、时间60000、响应1MiB和每次最多一次保持。

Chat wire必须严格单对象、无任何层重复key、唯一choices[0]、index=0、finish_reason=stop、assistant普通字符串content；length、content_filter/refusal、tool_calls/function_call、空/额外choice、错误role、非法类型、代码围栏/尾随JSON及候选不合契约均拒绝。允许已知reasoning_content等有正确类型的元数据，不将其当候选；不从开关请求推定思考已关闭。完整HTTP200元数据/usage独立核对，prompt_tokens/completion_tokens/total_tokens以明确映射保存为input/output/total，真实响应原字节保留；有效usage不因候选/length失败丢失，缺/错用量仍null失败。价格与底层模型仍未知或按实际返回别名。

原Task2实施者只编辑config/schema/wire/Core及必要新wire子模块的native测试，controller所有普通CLI作者测试/独立Chat本地HTTP矩阵、文档、profile/累计记录、Git及真实调用。先以新provider端点的普通CLI零预算和本地Chat行为取得真实RED，再实现/nativeGREEN。新鲜组件审查及最终完整回归后，controller保存新的Chat配置/活动profile历史，复用同一DPAPI凭据及9989381累计余额，只执行一次新的有界真实验证。两次失败完整保留，不自动循环、不改变来源/gold、不扩展其他SDK或产品阶段。

Chat新鲜组件审查另指出一项minor：同时返回prompt_cache_hit_tokens与prompt_cache_miss_tokens时，官方约定两者加总为prompt_tokens。主prompt/completion/total计量与累计预算不受此缺口影响；controller裁定在最终全仓检查前补齐该已文档化的一致性校验。只改Chat wire及一个涵盖有效/不一致/缺一项/非法值的native回归，先真实RED再GREEN；缺省仍未知、不生成缓存Token、不改任何原响应或已结算记录，之后作窄范围关闭审查。

### Task 2 Chat 实际候选包装层修复

第三次真实Chat请求HTTP200、finish_reason=stop，返回完整JSON，有效prompt1583/completion3329/total4912已结算；累计15531、余额9984469、pending0。response.message仍有10305字节reasoning_content，不能声称thinking开关已生效。候选atoms数组把Atom字段展开到了数组元素本身，缺少Schema要求的{atom,measurement}包装；因此Core严格DTO拒绝EXTRACT_API_CANDIDATES，原响应SHA6ed4b15a330aeb8d552d8bd7ef2d684345427d662cbb54b2aae4b1a5732fb04b保留。实际statement/not_applicable属于现有合法enum，不改枚举。

仅对所选Chat补充清晰的包装层提示，策略版本更新为tangmao-deepseek-chat-nested-atoms-v4。user仍恰好task/response_schema/request，原Schema、source/binding/units/anchor、system基础指令、endpoint/provider/thinking/JSON Object和全部限额保持。task明确每个atoms元素只含atom与measurement两个键，atom_id等十个Atom字段全部放在atom内，measurement只作同级对象或null；遵循原Schema，不修补实际响应、不通过展平/包裹后处理使其成功。条件留在对应事实/qualifiers/measurement.conditions，来源署名等元信息不作为学科知识；不写入gold中的具体事实作为输出模板。

原实施者只改config版本及schema提示/native测试，必要时加入授权原第三响应的严格拒绝回归；controller维护普通CLI/独立Chat验证的版本断言。提示先编译真实RED再实现，第三原响应仍必须拒绝且有效4912不得丢失。新鲜窄范围审查后运行最终相关回归，在同一累计余额内只执行一次新请求，保持三次失败、原配置与调用前gold。T02尚未complete，T03仍todo。

## Task 3: 累计预算与真实小范围验收

controller 在仓库外当前用户 Noetome 设置目录保存非秘密服务、模型、credential_env/DPAPI引用、累计10000000 Token上限和使用记录。所有后续调用先查剩余额度；已知实际用量累计，未知用量保留工程预留，不把额度每次重置。首冒烟 max_requests=1、max_output_tokens<=8192、max_response_bytes<=1MiB、timeout_ms<=60000，费用未知保持null。

设置和预算不再重复询问。密钥缺失时继续独立实现/审查，等待本机输入；不代填另一个服务的凭据，不以假服务验收真实模型。已存在的凭据正常复用，不弹新输入窗口。

使用原作者小来源 Span0 和调用前 gold，实际普通CLI一次请求，保存无密钥原响应/用量、真实调用策略、source/binding/SHA、前后来源保护与独立候选核对。运行所有相关离线检查并新鲜整分支审查，按实际成功/失败维护devlog并推送；取得真实候选之前不能启动T03。
