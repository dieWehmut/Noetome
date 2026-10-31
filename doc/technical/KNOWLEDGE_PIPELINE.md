# Noetome V1 知识处理流水线

本文件冻结完整P0算法和失败边界，供执行Agent逐项实现，不是已完成能力声明。数据类型见 [数据契约](DATA_CONTRACTS.md)，结构化模型任务见 [模型契约](MODEL_CONTRACTS.md)。所有阶段通过持久Job运行，CPU解析、API、检索并发和取消规则使用CONFIG及RUNTIME。初期fixture与本地假服务覆盖工程分支；最终真实资料、模型和Obsidian验收不能用fixture替代。

## 1 固定阶段和输入输出

| 阶段 | 输入 | 必须产生的输出 | 下游启动条件 |
| --- | --- | --- | --- |
| P00 预检 | 选中资料和ConfigRevision | immutable input manifest、运行范围、能力及预算问题 | 文件范围有效，所需能力可用；无凭据的独立离线任务仍可运行 |
| P01 导入 | 完整原件字节 | SourceVersion、原hash、ImportAttempt | 完整读取才创建版本 |
| P02 解析 | 版本、解析配置 | ParseManifest、Span、Page、FigureInventory及错误 | 局部成功可继续，失败范围有可见记录 |
| P03 全局地图 | 所有来源标题及单元清单 | 术语候选、章节粗对齐、无匹配章节清单 | 所有章节有入口，不要求均匹配 |
| P04 抽取和筛选 | 分块Span、用户目标 | Evidence、Concept/Claim/Atom候选、处置候选 | 候选引用合法且覆盖到输入单元 |
| P05 核验和融合 | 新候选、现有IR | 证据核验、条件协调、身份事件、KnowledgeSnapshot | 核心语义门禁通过，未知项隔离 |
| P06 有界研究 | 已批准目标和必要缺口 | 新来源及证据、研究队列和停止原因 | 仅已授权目标可发起检索 |
| P07 笔记规划 | sealed Snapshot、Style、锁定 | Topic、NotePlan、Placement候选 | 必要原子分配完整且结构锁定满足 |
| P08 表达规划 | Section与原子 | typed Node/Element、图表公式方案 | 必要语义全覆盖，资源有来源 |
| P09 编译 | validated Plan、真实Placement | Markdown、资源blob、ContentMapping | 确定性渲染和链接解析通过 |
| P10 校验反馈 | 文件、映射、证据 | ValidationResult/Issue、至多两轮反馈 | 仅通过本轮必需检查才可发布 |
| P11 发布 | 已核验候选及当前Vault | ChangeSet、审批、Release、四维报告 | CAS和人工保护，详见PUBLICATION |

P03等待本轮输入的目录/页面清单建立完成再创建地图；对资料级失败也生成不可解析节点。P04以后按独立主题并行，不让一处人工问题阻塞所有无依赖内容。发布分组必须是依赖闭合的连通分量；有待批准规范位置迁移时，该位置和全部入链变更同组等待。

## 2 导入和确定性解析

对本地路径先解析用户选择范围、junction和真实路径，打开只读句柄，流式计算完整SHA256并记录长度。读取时文件元数据变化或末尾复核不一致，重试完整读取最多一次后报SOURCE_CHANGED_DURING_READ，不对部分数据生成完整版本。已有同source同hash复用解析缓存；parser/normalizer/config hash变化时另建manifest。

MD/TXT严格按DATA中的UTF-8规范化。文本分段器按行扫描：围栏三或更多反引号/波浪线匹配同字符且闭合长度不少于开启长度；标题只在围栏外识别；相邻正文合为一块；空白区单列formatting；没有闭合围栏保留code并标needs_review。每个叶区间从输入字节映射生成，验证全部区间拼回规范文本逐字相同。

PDF侧车按STACK选定的pdfplumber文本和表格解析、pypdfium2页面渲染、Pillow处理及Tesseract OCR实现，界面坐标采用DATA统一规范。先获取页数、CropBox/rotation、文本块及可提取资源；每页渲染300DPI供OCR和证据裁剪。若任一页的完整渲染预计超过40百万像素，按区域平铺而不降低整个文件的文字质量；尺寸参数进入parser hash。

触发OCR条件为文本层为空、含替换/控制异常字符占可见字符超过1%、文本区域顺序无法确定或用户强制OCR。扫描页始终做页级图表检查，不依赖PDF嵌入图资源计数。Tesseract给出的置信度只决定复查优先级，不证明知识正确。native文本与OCR矛盾的关键公式/单位进入needs_review，不能用“更高置信度”直接替换。

多栏阅读顺序：优先PDF明确结构树；无可靠结构时按水平重叠聚类成列，页内先列后行并保留几何；跨列标题先于正文。表格不按普通段落阅读顺序拍平，保留单元格、合并、表头、单位和脚注。无法确定列/表格边界时使用模型视觉候选，再核对文本和区域；仍不确定保留原区域及unparsed叶单元。跨页段落/续表通过关联ID组织，原始单元身份不合并消失。

每页产出page inventory，包括已识别图表和是否存在未知区域。视觉检测不明时建立整页待审记录；原图不可辨认不允许通过“原图已保留”消除问题。

## 3 全局地图与内容分块

所有标题路径、前后章节、术语、图注和页范围进入GLOBAL_MAP任务。每个章节返回topic候选和alignment，relation取same_topic/overlap/related/unmatched。未匹配章节保留独立入口，不能从后续召回队列移除。

文本检索首版不依赖向量数据库。SQLite FTS5建立规范文本与别名索引；分词使用unicode61加预先生成的中文二元词检索列。查询按原术语、显式别名、章节术语三路召回，合并去重，按BM25后以span_id稳定排序。每个概念召回最多20个额外Span作为本批上下文；这是单次上下文上限，不是资料覆盖上限。所有未抽取Span仍必须进入普通抽取队列。

模型抽取块默认估算不超过6000输入Token，前后上下文各不超过500Token；总请求始终小于能力探测记录的上下文上限及CONFIG限额。无可靠tokenizer时按UTF-8字节数保守上界计算，不能按中英文字符平均猜测。先按章节/段落边界切分，单个超长单元按句边界切分并附同一父Span的字节范围；表格按行分块且重复表头，过程必须保留顺序索引。重叠区只供上下文，输出仅允许本块owned_span_ranges，避免重复计数。

超长输入不能静默截断；结构化JSON候选超出响应上限时降低批量重新分块，每个新子Job带原输入范围且有限分割。最小块仍不适用时记录CONTEXT_TOO_LARGE，任务阻塞该单元，保留其余工作。

## 4 抽取 筛选和证据复验

EXTRACT任务先提出原子、主张、对象、条件、限定符、证据片段和未覆盖原因。模型提供输入片段ID及原文quote，不提供可信“任意页码”；Core在相应Span中匹配quote，唯一匹配后计算字节偏移与hash。零匹配或多个匹配均需要明确offset和局部邻文重验，失败候选不能matched。

每个叶单元的处置候选只能included/excluded/duplicate/needs_review。排除允许formatting、decoration、irrelevant_anecdote、redundant_introduction、pure_exercise_form；含必要前提/新实质知识的习题不能排为pure_exercise_form。知识水平只改变解释起点。模型不确定重要性时included或needs_review；只有确定性纯空白格式可无需语义审查直接排除。

duplicate必须指向已接受的知识映射并保留该来源Evidence，不能只说“另一书有”。included至少一个原子映射；需进一步研究的单元仍记录其已有知识与缺口，不以needs_review丢弃全部可用内容。Coverage状态推进由Core根据真实实体/计划/文件映射计算，不接受模型上报“已编译”。

证据复验分三层：位置和字节可重复读取；陈述是否由该片段支持且保留条件；不同来源下对象、单位和范围是否一致。第一层完全确定性；第二三层由REVIEW候选加规则检查并保留意见，不等价于独立人工金标准。所有关键数学符号、图中箭头及来源不足事实进入专门核验。

## 5 身份解析和条件协调

候选实体召回按明确标识符、已接受别名、canonical_name和scope词汇完成。不同entity_class（如gene/protein、class/instance）禁止自动合并；同名、缩写、向量相近或模型自评分数不足以合并。允许自动接受的合并必须满足：类型相同；身份定义及上下文无矛盾；至少一条可重取证据明确同义或同指；REVIEW指出无区分性反证。缺少条件则保留两个ID及歧义问题。

同subject和predicate的Claim按以下固定顺序比较：重读解析位置→归一可验证单位→比较条件及有效时间→分类。值与条件均等价为duplicate；同对象不同属性为complement；条件明确不交集为condition_difference；时间沿革有证据为historical；同条件互斥值为conflict；条件重叠或证据无法确定为insufficient。未知条件不得当作通配符建立假冲突。

数值比较使用十进制或有理数，保留有效精度；不为消除差异偷偷四舍五入。已核实OCR错误归parse_error并触发重新解析，不能当作历史观点。不同来源票数不参与真值投票。

合并、拆分和撤销按DATA的identity_event和新Snapshot实现。每次候选Revision入库都固定前版基准hash；若并行上游已变化则重新基于最新快照计算，不覆盖旧结果。旧发布的实体解释保留可追溯性。

## 6 有边界的研究引擎

scope固定`approved_topic_ids,prerequisite_policy,goals,source_policy`及hash，prerequisite_policy从CONFIG的allow_required_prerequisites与max_hops展开。资料驱动仅可执行纠错、冲突核验和已允许前提；知识体系驱动可执行明确批准目标。模型推断为“有帮助”的新topic不自动加入scope；允许必需前提时，Core仅在已接受的prerequisite关系能以最多2跳回到已批准主题、未命中excluded_topic_ids且仍有额度时，记录派生allowed_prerequisite_ids和理由；这些是完成原goal的证据工作，不新增goal或修改分母。必要性尚无证据、超跳数或明显外延主题进入候选并说明受影响目标。

默认Brave Search REST适配，配置的检索凭据独立于模型凭据。核心发送GET到`https://api.search.brave.com/res/v1/web/search`，使用X-Subscription-Token，q由已通过scope检查的候选提供，count默认5。每目标查询默认2、唯一页面读取默认10，研究深度从0到2，全Run页面默认50；实际取CONFIG中有效值与预算更低者，配置上界分别为10查询、20结果、50页面每目标、500页面每Run。不使用自动翻页。查询、页面、Token、费用和总运行时间全部受预算约束；CONFIG用户下调立即生效，上调不得超过本轮已批准范围。

候选查询必须含目标概念和限定词，不含凭据或不必要整段私人资料。结果初筛优先原始论文、官方标准、教材出版方等符合source_policy的来源，排序不充当真值判断。搜索摘要只作为发现线索；实际GET取得并归档全文后才建立SourceVersion和Evidence。禁止根据未读页面补造引文。

页面读取仅HTTP/HTTPS；拒绝loopback、私网、链路本地、云元数据地址以及重定向到这些地址，逐次解析DNS和重定向校验，最多5次跳转。单页下载最大10MiB、120秒、仅允许配置声明的HTML/PDF/text媒体类型，不执行网页脚本。HTML去脚本、样式和导航后保存提取文本与原始响应hash；PDF走同一解析器。无法取得正文、付费墙或访问限制记unavailable，不绕过。页面中发现的新链接只允许本scope、hop≤2且仍有额度时读取。

停止理由固定goal_satisfied/no_new_reliable_evidence/marginal_gain_low/budget_exhausted/user_paused/network_restricted/blocked_config/requires_human。启用stop_on_no_new_evidence时，连续已读页面没有新增被接受Claim且达到no_new_evidence_page_limit（默认2）触发marginal_gain_low，仍未满足goal则状态stopped且目标incomplete。无法配置搜索provider时blocked_config，不假装研究完成；离线抽取与已有资料核验可继续。

模型只提出goal检查意见；Core必须核对每个goal.required_claim_types、未解决问题及证据支持，才记录satisfied。新候选由用户在结题队列接受后进入下一Run的scope，而非修改当前Run分母。

## 7 教材式笔记与表达规划

先从固定Snapshot建立topic图，按先修关系拓扑排序；并列用规范标题Unicode序及ID排序，保证可重复。循环先修关系形成一组并报告，不用循环算法无限排序。导航树每topic选一个主要父项，多重关联保留topic_member。

计划模板固定七类：overview（范围/导航/先修）；concept（定义/性质/适用条件/易混）；process（总览/步骤/调控/收支或结果）；mechanism（条件/因果链/例外）；derivation（前提/符号/分步推导/结论）；comparison（比较依据/对照/例外）；reference（查表/单位/使用范围）。不适用章节允许省略但必须rationale，不允许填空造知识。模板只是组织标准，不是文件字数配额。

NOTE_PLAN候选包含所有必需Atom分配、结构模板、每个规范概念的位置、导航/别名及拆分合并理由。优先保留原plan及锁定；已有规范位置不为风格变化移动。首次可自主规划，后续只修改影响闭包内对象。大规模移动/拆分/合并作为ChangeSet结构变更，走发布审查。

REPRESENT任务对每section选择primary形式及必要supporting形式。多对象同维属性优先table，顺序/分支过程优先ordered_list或flowchart，因果必须有对应Claim，数学优先formula，易混对比table+callout，观察图优先image。没有视觉收益时paragraph合法，不设图表数量指标。

表格和图示逐元素绑定Atom/Claim；缺失数据使用unknown原因文本，不填0。重复表达不得产生第二套权威知识内容，规范内容引用和派生表格都指向同一快照。LaTeX来自源代码时original_tex逐字保留；扫描重建原始源码字段必须null。模型输出的是图节点/边、公式表达或SVG形状候选，不是可直接执行的HTML/SVG脚本。

来源重绘顺序是语义完整前提下的Markdown、受控SVG、原图。受控SVG仅允许静态path/line/rect/circle/ellipse/polygon/polyline/text/g，颜色/坐标/尺寸范围检查，禁止script/foreignObject/event handler/外链。复杂化学空间结构、原始观测照片、无数据实验曲线默认原图；无法可靠重现的关键图保持问题，不能以漂亮的替代图通过。

## 8 编译 与视觉验证的边界

编译器消费经过校验的Typed AST：根据actual Placement生成Wikilink、标题或块锚点；转义表格竖线和特殊字符；代码围栏长度大于内容中最长同字符连续段；Properties严格类型化；资源路径为Vault相对路径。模型绝不直接输出可发布整篇Markdown，编译器不自行增加事实。

输出每个实质片段的ContentMapping，包含表格单元格、流程边及公式。流程边在Mermaid中存在不代表普通双链存在，关键知识关联另外产生正文semantic_link。嵌入图检查有向环，发现循环必须改为普通引用。

CLI运行structural检查（AST、ID、路径、引用、编码、范围）、可实现的semantic检查（单位、公式符号清单、数值推导、证据quote、条件覆盖）。CLI不引入Node/Chromium渲染器，不将语法通过当成视觉通过。Tauri WebView2使用离线打包的Mermaid/MathJax预览；B18还需真实Obsidian和关键gold逐项视觉验收。未完成的必需visual检查状态not_run且quality维度incomplete；原图回退也需确认关键内容可读。

## 9 反馈 增量和终止

P08/P10发现缺口时创建ValidationIssue并给出kind=knowledge_gap/representation_error/parse_error、证据、受影响ID和需要补充的字段。只改版面且不影响知识时修订Plan；缺事实、条件、公式或图意时返回IR修订并重新编译整个依赖闭包。禁止只修改最终Markdown来隐藏IR错误。

单问题最多2轮有新证据或明确规则修复的反馈；每个Job总尝试最多3次，包含网络明确失败、格式修复和语义修复。故同一问题的初次加2轮反馈最多9次已获预算许可的外部尝试；outcome_unknown立即冻结该请求，不属于可自动重试分支。没有新证据时不得重复推翻既有决定；超限转blocking，保留候选和完成状态。

增量输入先对比来源hash、解析器/配置/Style及当前Vault指纹。纯Style变化复用Snapshot但重算Plan；学习目标变化重算筛选与required集合；来源变化经Evidence/Claim/Atom/Plan/Node/File依赖闭包局部失效。重新抽取复用仍匹配的持久身份，不因为更换模型、路径或名称全量重编号。

对人工修改、锁定、非托管文件、外部删除和重命名先建立当前快照。任何可能改变人工内容的操作只生成候选，发布保护按PUBLICATION执行。文档与依赖分量通过后可局部发布，但全Run的四维报告不能伪造所有原范围完成。

审批候选编辑走同一闭环：用户编辑Typed Node支持的有限字段或局部采纳差异→Core判断是表达变化还是新事实→前者创建新Plan，后者将人工输入作为manual SourceVersion和待核验Claim产生新Snapshot→重新生成节点/原子/证据映射→重新校验和生成ChangeSet。旧批准绑定旧hash，编辑后立即失效。人工输入的身份记录不代表科学事实已经核实，界面保留其用户陈述与核验状态；不直接把自由Markdown写入已验证输出。

## 10 阶段验收样例

最低回归输入包含三份章节组织不同的资料、一页扫描、一个复杂表、一张无嵌入图对象的扫描图、源码公式与重建公式、同名异义概念、不同条件Claim、真正冲突及人工修改Vault。

固定断言：所有叶单元可对账；同义归并可撤销；条件差异共存；原文无图也能规划有效表/图；未知数值不变0；规范位置唯一；写作反馈改IR及所有依赖；拒绝scope扩张；无检索凭据研究为blocked_config；未知费用不释放；断点和人工冲突不损坏原文件；必需视觉未检查不能显示质量完成。

相关FR：P01至P04覆盖FR-01至08；P05至P06覆盖FR-09至14；P07覆盖FR-15至20；P08至P10覆盖FR-21至27；P11及增量覆盖FR-28至40。完整业务验收以PRD的40个AC和TESTING为准。
