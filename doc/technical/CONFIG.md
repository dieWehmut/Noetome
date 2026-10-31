# Noetome 完整产品配置契约

本文件固定 CLI 与桌面端共用的配置字段、默认值、校验和执行权限。它与 [架构](ARCHITECTURE.md)、[运行时](RUNTIME_PROTOCOL.md)、[数据契约](DATA_CONTRACTS.md)、[模型协议](MODEL_CONTRACTS.md) 共同构成实现契约。参数是已选择的产品基线，可通过版本化配置调整；人工保护、真实状态、来源追溯和范围授权不得被配置关闭。

当前文件定义应实现的行为，不表示配置模块或完整产品已经可用。尚未实现的能力必须返回能力错误，不能忽略配置后输出成功。

## 1 文件层级与解析规则

项目根的 project.json 保存项目身份及完整 ProjectConfig；state/noetome.sqlite3 保存配置版本、执行快照和状态。用户设置保存在应用数据目录的 settings.json，仅用于新项目默认值、提供商目录和界面偏好。确切应用目录按 [架构](ARCHITECTURE.md) 确定，不将本机绝对路径写入仓库。

有效配置由内置默认值、用户设置、新项目选择、已保存项目配置和本次显式覆盖依次合并。已存在项目不随用户默认值变化；每次运行合并后的结果先校验，再保存不可变快照和 SHA256。桌面端不另存一份影响行为的隐藏配置。CLI 的覆盖参数与桌面表单使用同一验证和保存接口。

- config_schema 固定为整数1，与 SQLite 的 storage_schema_version 分离。字段新增或语义改变须有迁移说明和有效、无效样例。
- JSON 使用 UTF-8，未知字段和未知枚举拒绝，重复键拒绝。用户输入值不合法时返回字段路径、错误码和允许范围，不静默换成默认值。
- 默认值只补缺失字段；显式 null 仅对表中允许的字段有效。列表默认替换，不隐式拼接；对象按已声明字段合并。
- 整数使用明确单位；金额使用对应货币的百万分之一单位micros保存，不使用二进制浮点计算预算。比率范围为0至1。
- 文件路径存储为结构化路径或项目相对路径；读取外部来源与写入目录分别授权。输出相对路径不得逃出已选 Vault。
- 已运行任务继续使用原快照。用户改配置时创建新 revision，预览受影响阶段；不在执行一半时改变模型、预算或知识范围。

## 2 项目与资料

下表字段位于 project.json 的 config 对象；identity 独立保存 project_id、created_at、storage_schema_version，不可通过普通配置覆盖。

| 字段 | 类型 | 默认值 | 校验与行为 |
| --- | --- | --- | --- |
| config_schema | integer | 1 | 只接受已支持版本 |
| revision | positive integer | 1 | 有效配置内容变化时递增，由core分配；重复保存相同内容不递增 |
| project.display_name | string | 新项目 | 去除首尾空白后1至120字符；不作为ID |
| project.subject | string | 空字符串 | init允许空；真实生成前必须明确主学科 |
| project.language | enum | zh-CN | zh-CN、en；决定输出语言，不改变事实 |
| project.learning_goals | string array | 理解、复习 | 非空、去重；生成前冻结 |
| project.target_level.mode | enum | infer | infer、explicit |
| project.target_level.value | string or null | null | explicit时必填；infer保留推测和依据供查看 |
| project.prior_knowledge | string array | 空列表 | 用户声明；不从学历直接推导全部掌握 |
| sources.storage_mode | enum | reference | reference、copy；copy只复制选定来源至项目对象区 |
| sources.allowed_formats | enum array | md、txt、pdf | 不支持的适配器必须显式拒绝；不能假装已支持PDF |
| sources.follow_directory_links | boolean | false | 不遍历指向选择目录之外的符号链接或junction |
| sources.detect_changes | boolean | true | 运行前复核来源指纹，原路径变化不覆盖旧证据 |
| sources.extract_exercise_knowledge | boolean | true | 提取独有实质知识，不导入完整题库 |
| sources.exclude_pure_decoration | boolean | true | 保留处理和排除记录 |
| sources.on_uncertain_relevance | enum | retain | retain、review；不得提供静默删除不确定内容的值 |
| sources.selected_version_ids | string array | 空列表 | 启动时至少1个有效来源；研究续跑可复用上轮来源集合 |

reference 模式保存原文件引用和必要解析快照；原文件失联时使用已验证快照或报告缺失。copy 模式保存原始字节副本及指纹。二者均不修改原文件。复制策略不等于允许向服务商上传。

## 3 生成范围与研究

| 字段 | 类型 | 默认值 | 校验与行为 |
| --- | --- | --- | --- |
| generation.mode | enum | source | source、research |
| generation.depth | enum | deep | essential、standard、deep、custom |
| generation.depth_goals | string array | 空列表 | custom必填；其他模式由项目目标生成可见的本轮目标集 |
| generation.scope.topic_ids | string array | 空列表 | source以选定资料为边界；research启动前必须有有效主题集合 |
| generation.scope.excluded_topic_ids | string array | 空列表 | 与纳入集合不得冲突；影响必要前提时生成配置问题 |
| generation.scope.allow_required_prerequisites | boolean | true | 只补理解已授权主题必需的前提，保存理由和证据 |
| generation.scope.new_topic_policy | enum | queue_for_user | 唯一值；外延新主题记录候选，结题时交用户决定 |
| research.search_mode | enum | disabled | disabled、verify_only、approved_scope |
| research.search_provider_id | string or null | null | 外部检索启用时指向可用检索提供商 |
| research.queries_per_goal | integer | 2 | 1至10；同目标去重 |
| research.results_per_query | integer | 5 | 1至20；结果仍需读取证据 |
| research.pages_per_goal | integer | 10 | 1至50 |
| research.max_hops | integer | 2 | 0至2；跳数不授予新主题范围 |
| research.pages_per_run | integer | 50 | 1至500；与预算中更低的限制共同生效 |
| research.concurrent_requests | integer | 2 | 1至4；同时受总Job并发限制 |
| research.allowed_domains | string array | 空列表 | 空表示不附加域名筛选，不表示自动批准所有内容 |
| research.blocked_domains | string array | 空列表 | 拒绝优先于允许；域名标准化后比较 |
| research.preferred_source_types | enum array | primary、reference、review | primary、reference、review、official；用于排序，不当作事实真伪标签 |
| research.stop_on_no_new_evidence | boolean | true | 按下列连续已读页面计数停止，仍须如实说明剩余缺口 |
| research.no_new_evidence_page_limit | integer | 2 | 1至10；连续已读页面无新增被接受Claim时达到边际收益停止条件 |
| research.max_revision_rounds_per_issue | integer | 2 | 1至2，初始尝试为round0；到限升级问题，不能循环自我重写 |

关闭外部检索只禁止网络搜寻新证据。若使用远程模型，所选资料片段仍可能通过模型API发送；其权限由第7节单独控制。完全离线要求解析、模型和所有启用能力均在本地可用。

研究目标来自已保存的主题和范围快照。模型只能提出查询或候选，不得更改上述配置、扩大域名权限或增加预算。未经批准的候选不计入本轮目标分母。原范围内已知必要知识缺口仍阻止相应完成状态，不能因研究停止而变成“已完整”。

## 4 风格与结构

| 字段 | 类型 | 默认值 | 校验与行为 |
| --- | --- | --- | --- |
| style.preset | enum | detailed | minimal、detailed、custom |
| style.instructions | string | 空字符串 | 最多20000字符；与硬约束冲突时返回问题 |
| style.reference_note_paths | path array | 空列表 | 只读明确选取笔记；不默认导入其知识 |
| style.reference_transfer | boolean | false | 必须另有目的地和来源范围授权后才可外发样例 |
| style.terminology | enum | zh_with_original | zh_with_original、zh、original |
| style.explanation_density | enum | detailed | compact、balanced、detailed；不改变必要原子集合 |
| style.prefer_tables | boolean | true | 可比较属性优先表格，不强制所有章节表格化 |
| style.prefer_diagrams | boolean | true | 关系和顺序值得可视化时主动选图 |
| style.callouts | boolean | true | 适用于提示、例外、争议；正文仍保持重要限定 |
| structure.navigation | enum | textbook | textbook为V1唯一值 |
| structure.canonical_explanation | enum | single | single为唯一值；可以有上下文短事实及同源嵌入 |
| structure.max_note_words | positive integer or null | null | 不设内容量硬上限；非null仅作拆分建议，不能截断 |
| structure.max_heading_depth | integer | 4 | 2至6；超过时重规划，不删除内容 |
| structure.large_restructure | enum | review | review、disabled；不得设自动覆盖人工结构 |
| structure.lock_ids | string array | 空列表 | 引用受锁目录、笔记、章节和规范位置；不存在ID需报错 |
| structure.keep_existing_paths | boolean | true | 增量默认局部修订，改变路径需进入影响分析 |

style.preset 是建立配置的模板，保存时展开为显式字段。后续改变某项风格显示为 custom，不让预设在后台覆盖用户修改。结构锁定和人工内容来源以数据库为准，配置只引用锁定对象；撤锁操作必须明确作用范围并留事件。

供模型候选与数据库引用的StyleProfile只包含preset、instructions、terminology、explanation_density、prefer_tables、prefer_diagrams和callouts七项，类型与上表同名字段一致。reference_note_paths和reference_transfer属于用户配置权限，不允许模型在profile中生成或修改；引用样例的指纹由Core从已选范围另存。

## 5 图表 数学与Obsidian

| 字段 | 类型 | 默认值 | 校验与行为 |
| --- | --- | --- | --- |
| representation.active_planning | boolean | true | 原文无图也可主动用表格、流程图 |
| representation.priority | fixed enum array | markdown、svg、original | 固定顺序；语义准确和重要信息完整为前置条件 |
| representation.inspect_every_image | boolean | true | 只允许true，无法检测图片边界时做页面级清点 |
| representation.allow_original_fallback | boolean | true | 无法可靠重绘时回退原图；原图不清也须阻断或复核 |
| representation.allow_unverified_numeric_redraw | boolean | false | 只允许false；无原数据不伪造实验曲线 |
| representation.max_diagram_nodes | integer | 40 | 5至200；超过时拆图或SVG，不能删节点凑数 |
| representation.max_table_columns | integer | 8 | 2至20；超过时拆表或横向滚动，不能删属性 |
| math.preserve_source_tex | boolean | true | 只允许true；原始源码与兼容表达分别保存 |
| math.uncertain_symbol_policy | enum | review | review为唯一值 |
| math.renderer | enum | mathjax | V1固定；源码公式和扫描重建分别验证 |
| obsidian.link_style | enum | wikilink | V1固定，编译器决定文件、标题或块目标 |
| obsidian.emit_properties | boolean | true | 仅输出声明的轻量字段 |
| obsidian.property_fields | enum array | id、aliases、tags | 不包含完整证据、密钥、绝对来源路径 |
| obsidian.asset_directory | relative path | assets | 位于Vault内部，不以插件设置重定向 |
| obsidian.source_display | enum | sidecar | sidecar为V1默认且唯一自动生成策略，正文不插逐句出处 |
| obsidian.compatibility_profile | string | bundled | 对应应用支持矩阵；发布记录实际测试版本 |

渲染能力和目标版本不能由用户填一个较新版本就自动获得。编译器对当前支持图型、公式宏和链接语法做能力检测；不支持时使用已验证等价表达、SVG或原图，并保留原因。

## 6 解析器与任务资源

| 字段 | 类型 | 默认值 | 校验与行为 |
| --- | --- | --- | --- |
| parser.pdf_text | enum | pdfplumber | 具体版本以STACK和锁文件为准 |
| parser.pdf_raster | enum | pypdfium2 | 不调用用户随意安装的替代解析二进制 |
| parser.ocr.engine | enum | tesseract | 本地OCR；不能代替视觉知识理解 |
| parser.ocr.languages | string array | chi_sim、eng | 必须匹配已安装且校验的语言资源 |
| parser.ocr.dpi | integer | 300 | 150至600；性能和识别效果记录于测试 |
| parser.ocr.mode | enum | auto | auto、always、disabled；disabled仍不能跳过图片清点 |
| parser.max_parallel_documents | integer | 1 | 1至4 |
| parser.page_timeout_seconds | integer | 120 | 10至1800；超时记录该页，不假定整份成功 |
| parser.max_frame_bytes | positive integer | 16777216 | 16MiB协议帧上限，资源以有边界的对象引用传递 |
| parser.max_file_bytes | positive integer | 536870912 | 超限明确拒绝并建议分批；不只解析前一部分 |
| parser.minimum_free_bytes | positive integer | 2147483648 | 启动预检及执行中监测，达到阈值安全停止写入 |
| execution.preset | enum | quality | quality、balanced、efficient |
| execution.max_concurrent_jobs | integer | 4 | 1至8，仍受模型和解析器各自限额 |
| execution.max_attempts | integer | 3 | 1至3，包含首次；outcome_unknown不得自动重试 |
| execution.retry_base_seconds | integer | 2 | 1至60，指数退避并遵守可验证Retry-After |
| execution.retry_max_seconds | integer | 60 | 仅限制本地计算退避；更长Retry-After需等待其时限或受预算阻断，不得截短 |
| execution.connect_timeout_seconds | integer | 10 | 1至120 |
| execution.page_fetch_timeout_seconds | integer | 120 | 10至1800，抓取失败保留原因 |
| execution.model_timeout_seconds | integer | 300 | 10至1800，网络未知结果需单独处理 |
| execution.job_lease_seconds | integer | 60 | 30至600，必须大于心跳间隔的3倍 |
| execution.heartbeat_seconds | integer | 10 | 1至60，租约过期按恢复规则处理 |
| execution.checkpoint_after_job | boolean | true | 只允许true |
| execution.auto_resume_after_restart | boolean | false | 重启后恢复状态，未启用时不自动恢复收费任务 |

执行预设只影响推理和复核轮数：quality 对高风险事实与图表执行独立复核；balanced 按风险补复核；efficient 减少重复模型调用并优先缓存。覆盖、来源、链接、人工保护和真实状态检查不可减少。预设展开后的具体检查集写入运行快照，不由模型随意决定。

## 7 权限与模型提供商

权限配置授予“能否使用某类数据和目的地”，预算规定“最多用多少”，二者独立。新增目的地、扩大文件范围、改为发送原图或风格样例时需要新的明确配置；同一已保存授权范围内不逐次弹窗。

| 字段 | 类型 | 默认值 | 校验与行为 |
| --- | --- | --- | --- |
| permissions.read_roots | canonical path array | 项目及已选资料路径 | 运行前复核边界；不自动扩大到用户主目录 |
| permissions.write_roots | canonical path array | 项目工作区 | Vault写入另需在output中选定且通过管理范围检查 |
| permissions.provider_transfer | enum | disabled | disabled、selected_content |
| permissions.provider_ids | string array | 空列表 | 允许数据发送到的具体提供商ID |
| permissions.transfer_kinds | enum array | text、structured | text、structured、image、style_sample；image和style_sample需显式选取 |
| permissions.search_requests | boolean | false | 仅授权检索查询；不代表整个来源可上传 |
| permissions.external_page_fetch | boolean | false | 只用于批准检索结果及其范围内证据；拦截本机和内网地址 |
| permissions.diagnostics_transfer | boolean | false | 诊断导出与发送单独选择，不跟随模型权限 |

模型提供商目录 settings.models.providers 以 provider_id 为键，项目配置只引用ID和路由。每次运行将所有用到的非秘密提供商字段、能力探测及价格记录一并解析为快照，纳入effective_config_hash；目录后来修改不能改变已运行任务。每项提供商字段如下。

| 字段 | 类型 | 默认值或约束 |
| --- | --- | --- |
| provider_id | stable string | 用户创建时生成，不随显示名变化 |
| display_name | string | 可读名称，不含密钥 |
| kind | enum | fake、openai_compatible、agent、local；V1首发真实API仅openai_compatible，未实现的kind明确拒绝 |
| base_url | absolute URL or null | fake为null；openai_compatible保存明确API根地址，官方示例为https://api.openai.com/v1 |
| credential_ref | string or null | 系统凭据存储中的引用；配置不保存API密钥正文 |
| model | string or null | 默认null；真实运行必须明确已验证可用的模型标识 |
| capability_profile_id | string or null | 引用能力探测记录，含image_input及JSON能力；不能仅信用户勾选 |
| max_context_tokens | positive integer or null | 默认null；配置或探测得到，未知时拒绝需要Token上界的真实运行 |
| max_output_tokens | positive integer | 默认4096，必须不超过能力及预算 |
| requests_per_minute | positive integer | 默认30，可调；服务商限流更低时服从其限额 |
| concurrent_requests | integer | 默认2，1至8 |
| timeout_seconds | integer | 默认300，10至1800 |
| pricing | object or null | null表示费用未知；非null按下述精确定义，不把未知缓存计价当成免费 |

pricing对象包含currency（ISO货币字符串）、input_micros_per_million和output_micros_per_million（非负整数）、cached_input_micros_per_million和cache_write_micros_per_million（非负整数或null）、effective_at（带时区时间戳）、source_note（非空说明）。null缓存价格表示不能按缓存优惠预留，按可证明的最高适用价格处理；无法确定上界而启用了金额限额时阻断派发，即使同时有Token限额也不绕过。每次结算保存实际价格快照，不追溯改写历史费用。

检索目录 settings.research.search_providers 独立于模型目录；research.search_provider_id引用下表记录。没有真实目录项或凭据时保持disabled，默认不创建可调用的收费配置。

| 检索提供商字段 | 类型 | 默认值与校验 |
| --- | --- | --- |
| search_provider_id | stable string | 创建时生成；与模型provider_id属于不同命名空间 |
| display_name | string | 非空可读名称 |
| kind | enum | brave、fake；fake仅用于fixture与本地测试 |
| base_url | absolute URL or null | brave固定https://api.search.brave.com/res/v1/web/search；fake为null |
| credential_ref | string or null | 默认null；brave调用前必需，保存在系统凭据存储 |
| requests_per_minute | positive integer | 30；服务商更低限流优先 |
| concurrent_requests | integer | 2，1至4；同时受research和总Job限额约束 |
| query_cost_micros | nonnegative integer or null | null；未知价格不按零费用计入金额预算 |
| currency | ISO currency string | USD；有价格时必须与金额预算同币种或有固定转换依据 |

首发Brave请求、认证头及响应字段由MODEL_CONTRACTS固定；不让模型设置base_url或headers。网页抓取不使用检索或模型认证信息。搜索请求数、页面数和发送的数据类别分别检查已有权限和预算。

HTTP只允许已显式选定的回环本地模型地址；远程地址必须HTTPS。禁止用户名密码型URL、URL中的密钥、未校验TLS和模型返回的新endpoint。重定向到另一authority时不携带认证信息，外部搜索页面抓取禁止继承模型认证头。密钥通过Windows安全存储在core侧解析，桌面WebView、日志、报告、Vault和导出均只看到遮盖状态。

models.mode为fake、api、agent或local，默认fake。models.routes.extractor、models.routes.reviewer、models.routes.planner均为非空provider_id字符串，默认fixture；内置fixture目录项kind=fake，model和credential_ref为null。mode=fake只允许producer=fixture的显式样例流程，普通真实生成必须blocked_config，不准静默输出fake知识。mode=api要求所有使用路由均为openai_compatible；agent/local首发返回CONFIG_UNSUPPORTED。

extractor负责实体、原子及图像结构抽取，视觉任务要求image_input；reviewer负责证据核验和冲突候选分类；planner负责笔记、表达、风格和研究查询候选。同一provider可以多路复用，但不因此成为独立人工证据。缺少对应能力时返回blocked_config。

首发openai_compatible使用明确base_url加相对chat/completions路径，stream=false。结构响应优先strict_json_schema，否则使用json_object加本地严格校验；不支持JSON能力时阻断。Responses和Anthropic原生不属于首发兼容声明。网络重试、格式修复和语义修复共同计入最多3次总尝试，不能各自重试3次。具体HTTP协议及检索提供商契约以 [模型协议](MODEL_CONTRACTS.md) 为唯一来源。

## 8 预算与计费

| 字段 | 类型 | 默认值 | 校验与行为 |
| --- | --- | --- | --- |
| budget.max_total_tokens | positive integer or null | 1000000 | 输入、输出及可获得的额外token共同计数 |
| budget.max_cost_micros | positive integer or null | null | 对应货币百万分之一单位；启用时所有收费路由须有可靠价格/用量上界并换算至同一明确币种，否则blocked_config |
| budget.currency | ISO currency string | USD | 只影响货币预算，不擅自查询或使用浮动汇率 |
| budget.max_wall_seconds | positive integer or null | 28800 | 8小时模板值；24小时测试使用单独配置 |
| budget.max_model_requests | positive integer or null | 2000 | 包含重试；未知结果也占请求次数 |
| budget.max_search_queries | positive integer or null | 100 | 受研究范围和每目标限制共同约束 |
| budget.max_fetched_pages | positive integer or null | 50 | 成功和已发生费用的失败均记录 |
| budget.allow_unlimited | boolean | false | 所有限额null时必须显式true，并在启动摘要突出显示 |
| budget.stop_policy | enum | stop_new_dispatch | 唯一值；已有请求继续等待或进入未知结果处理 |

默认限额是已选择的运行模板值，不授权本轮开发Agent产生真实费用。真实API回归必须有已存在的明确调用配置和预算；缺少时记录 blocked_external，并继续不依赖它的离线任务。

调度前预留最大可确认用量，成功后按实际返回结算。ModelAttempt 为 outcome_unknown 时对应预留保持 unknown 并继续占用，不释放后立刻重复请求。已知未受理可释放；所有重复结算按请求身份幂等。不同价格币种没有固定换算依据时不提供跨币种费用总额。价格或换算未知时，只有金额限额明确为null且另有可执行的token、请求或时间限制，才可运行并注明费用未知；不能一边保留金额上限一边忽略它。

到限停止新派发并进入 budget_stopped，不把停止记为完成。在途请求、服务商隐藏计费和最终账单存在差异时同时显示已结算、已预留、未知费用；不得承诺应用的费用上限等同于服务商绝对不超额。

## 9 审批 发布与保留

| 字段 | 类型 | 默认值 | 校验与行为 |
| --- | --- | --- | --- |
| review.mode | enum | important | automatic、important、strict |
| review.manual_edits | enum | always | always为唯一值，不能关闭 |
| review.fact_conflict | enum | preserve_evolution | preserve_evolution、prefer_source、review_each |
| review.preferred_source_ids | string array | 空列表 | prefer_source时必填；仍保留反证与条件 |
| review.locked_structure | enum | always | always为唯一值 |
| review.stop_entire_run | boolean | false | 默认隔离受影响对象及依赖，其他任务继续 |
| output.vault_path | canonical path or null | null | 生成发布前必填；新目录或已登记管理范围 |
| output.mode | enum | new_vault | new_vault、incremental；后者必须存在基准及管理登记 |
| output.auto_publish_verified | boolean | true | 仅无阻断问题且不需审批的变更可发布 |
| output.preserve_unmanaged | boolean | true | 只允许true |
| output.export_provenance | boolean | true | 独立索引，默认不打包原始资料 |
| output.include_source_copies | boolean | false | 导出时单独选择，对不许可的来源拒绝打包 |
| retention.cache_days | nonnegative integer | 30 | 0禁用缓存留存；不能清掉已发布追溯必需对象 |
| retention.diagnostic_days | nonnegative integer | 30 | 默认不含完整原文和认证信息 |
| retention.release_snapshots | positive integer | 10 | 当前、受引用和未结审批涉及版本不自动删除 |
| retention.keep_source_evidence | boolean | true | 只允许true；项目明确删除走独立有说明的操作 |
| retention.cleanup_mode | enum | preview_then_apply | 清理先产生清单，不能按年龄直接删仍有引用的对象 |

automatic 仅自动处理L1及允许带标记的L2；所有人工影响、关键不确定公式、非法结构及不可恢复数据问题仍隔离。审批绑定 current、base、candidate 清单和change_set指纹，配置变更或文件再次编辑使旧审批失效。完整发布与回滚算法以 [发布方案](PUBLICATION.md) 为准。

## 10 桌面偏好与后台行为

以下字段位于用户 settings.json，不影响知识内容；CLI不读取它们来改变领域行为。

| 字段 | 类型 | 默认值 | 行为 |
| --- | --- | --- | --- |
| ui.language | enum | zh-CN | zh-CN、en |
| ui.theme | enum | system | system、light、dark |
| ui.scale_percent | integer | 100 | 80至200；不能隐藏主要操作 |
| ui.close_behavior | enum | ask_once | ask_once、tray、pause_and_exit |
| ui.minimize_to_tray | boolean | false | 仅主窗口关闭/最小化行为，不建立系统服务 |
| ui.notifications | enum | actionable | actionable、all、none |
| ui.confirm_expensive_start | boolean | true | 第一次明确配置摘要；已授权相同范围不逐请求重复 |

每项目由按需worker承载运行。选择托盘继续时桌面托盘宿主保留，worker继续；选择暂停退出时停止新派发、等待在途与落盘、关闭worker后退出。超过30秒仍有请求则显示“仍在等待”，提供继续等待或明确强制退出；强退后请求结果可为未知，不能显示安全完成。操作系统关机按最佳努力记录恢复状态，不宣称能保证无限等待。无OS服务、无默认登录自启动，不自动改变休眠策略。

## 11 校验顺序与实现验收

加载依次执行JSON结构、版本、字段类型、枚举与范围、跨字段约束、路径和来源范围、提供商与能力、预算可执行性、发布管理范围检查。错误使用 CONFIG_SCHEMA、CONFIG_VALUE、CONFIG_SCOPE、CONFIG_PERMISSION、CONFIG_PROVIDER、CONFIG_BUDGET、CONFIG_PATH、CONFIG_UNSUPPORTED 等稳定码，并附 field_path。尚未提供的凭据只在实际调用前阻断依赖任务，纯本地项目查看和离线测试仍可进行。

至少验证：旧版本拒绝或迁移；未知字段报错；CLI与桌面保存后获得相同effective_config_hash；检索关闭但模型传输开启时摘要正确；权限关闭不能靠较大预算绕过；null价格不是零；手工保护无法关闭；研究范围不会因高预算扩大；相同配置重复保存不产生无意义revision；运行中修改只影响新快照；配置与导出中没有密钥明文。

首个配置实现可以只激活当前任务需要的能力，必须完整校验已声明字段并显式返回尚未实现能力。全产品字段不意味着首个CLI任务应立刻创建所有执行模块。命令接口由 [运行时](RUNTIME_PROTOCOL.md) 固定，测试和发布门禁分别见 [测试标准](../standards/TESTING.md) 与 [发布标准](../standards/RELEASE.md)。
