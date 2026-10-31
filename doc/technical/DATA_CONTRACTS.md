# Noetome V1 完整数据契约

状态：2026年11月1日冻结的实现蓝图，尚未证明产品能力已实现。本文件与 [schema.sql](schema.sql) 一起定义所有 P0 数据；实现顺序可分阶段，实体含义不得到阶段末再重新设计。行为算法见 [知识流水线](KNOWLEDGE_PIPELINE.md)，模型边界见 [模型契约](MODEL_CONTRACTS.md)，运行及发布协议以架构和运行设计为准。

## 1 版本 身份和公共类型

数据库 `PRAGMA user_version` 是 storage schema；输入输出 JSON 的 `schema_version` 是交换协议版本；配置的 `config_schema` 是配置结构版本。这三个数字独立，当前完整数据库目标为1，交换及配置协议为1。

生产身份由 Rust core 生成 ULID 的26位大写 Crockford Base32 字符串，生成后持久化；概念、来源、版本、运行、计划、笔记、节点均不得由标题或路径推导。fixture 可用 `fx_` 前缀固定身份，仅 `producer=fixture` 允许。模型只提交本次候选内的 `tmp_` 临时ID或输入提供的引用，不能自行声明新的持久ID。持久对象引用为 `{"id":"对象ID","revision":1}`；来源版本、不可变计划、Run 等自带独立版本身份，不再附加无意义的revision。

`revision` 是同一知识身份内从1递增的整数，CAS分配，已有修订不更新。`entity` 保存类型和身份，`entity_revision` 保存修订、内容指纹、生产者、原因和时间；concept/claim/atom/relation/topic 各有类型化修订表。每个修订必须恰好具有与entity.kind一致的一行类型数据。这个跨表类型规则由入库事务验证器执行；SQLite的普通外键只保证引用存在。

JSON为UTF-8，外部对象默认拒绝未知字段、重复键、未知枚举、非有限数和超过上限的嵌套。未提供值用显式null或相应unknown枚举，不用空字符串、0或空数组替代失败。集合去重排序后散列；顺序列表保留次序。原始字节、规范文本、候选JSON、编译文件各自计算SHA-256小写64位，不相互替代。JSON规范化使用RFC8785/JCS；跨Rust与Python的fixture必须验证同值同指纹。业务小数使用十进制字符串，避免浮点序列化损失。

时间为带Z的RFC3339 UTC；UI转Asia/Shanghai或用户时区。相对路径统一`/`，禁止绝对路径、空组件、`.`、`..`、Windows保留名、ADS、尾随点空格及控制字符。`path_key`由NFC规范化、Windows大小写不敏感映射和路径组件检查生成，同计划内唯一；最终写入仍检查实际文件系统及junction，不以字符串规范化代替安全检查。

## 2 数据库建立及迁移

`schema.sql` 中 `BEGIN_SCHEMA_0` 到 `END_SCHEMA_0` 是B01初始化使用的固定子集：schema_migration、project、blob、source、source_version、import_attempt、parse_manifest、source_span。B01-T02建立该子集和project记录，其余表为空；B01-T03写文本导入数据。设置user_version=0并记录schema0块SHA256。没有模型、知识或发布能力时不创建假的Run/IR/Release。

B02-T01将现有schema0升级为完整schema1：取得项目独占运行锁，关闭工作连接，做SQLite备份并验证可读；校验schema0列、约束及迁移校验和；在一个SQLite事务内执行 `BEGIN_SCHEMA_1` 到 `END_SCHEMA_1` 的DDL、外键检查、schema_migration记录及user_version=1。失败回滚，原数据和ID保持。不能将整个建库脚本在已有库上重跑。新建测试库可直接执行完整schema.sql。

所有连接开启foreign_keys；运行模式WAL，写事务短小，迁移前checkpoint由运行设计负责。升级采用严格单调编号，不支持的更新版本只读检查后拒绝写入；`inspect`不偷偷迁移。不得以删库重建代替正式迁移。后续加字段须定义旧值转换、回填、默认值、读写兼容和失败恢复，再增加独立迁移文件；禁止修改已经应用的迁移语义。

SQL字段、NOT NULL、CHECK、主键、外键及索引以schema.sql为准。逻辑外键不能全部由SQLite表达的规则，以下明确交给同一事务中的领域验证器，测试必须覆盖失败样例。直接SQL绕过领域仓储不是受支持写入口。

## 3 项目 配置和内容寻址

| 表组 | 精确语义 |
| --- | --- |
| project | 一个数据库恰有一个project，singleton=1；state为initialized/active/archived |
| config_revision | config_id与revision联合主键，保存已解析完整配置、config_schema、指纹及创建时间；Run固定某一修订 |
| style_profile | style_id与revision，kind为minimal/detailed/custom；spec_json按CONFIG中的StyleProfile定义；reference_manifest_json只记录用户选定样例的相对描述和哈希 |
| blob | 文件内容索引，主键为原始内容SHA256，带byte_length和media_type；路径确定为objects/sha256/前两位/完整hash |
| provider_profile capability_profile | 非秘密提供商配置与能力探测记录；credential_ref是系统凭据引用，不含密钥本体 |

project.json包装固定为identity与config：identity.project_id/created_at映射project同名列，identity.storage_schema_version映射PRAGMA user_version；config.config_schema映射config_revision.config_schema，config.revision映射revision，config_id固定为project_id（每项目一个配置版本流）。配置文件内容hash与数据库该revision必须一致，冲突拒绝写入而非静默覆盖。

blob表提交前必须先完成文件安全落盘、重读指纹；同hash不同字节长度是损坏。对象数据的引用清点和GC另走可恢复维护任务，只清理无引用、非发布历史及非在途任务使用的内容。旧库风格样例不进入source/claim；提取的风格JSON才成为可复用配置。

## 4 来源 规范化与图像清单

Source是逻辑资料，SourceVersion是该资料某次完整原始字节版本。source.origin区分local、verification、research、manual。同source同原始hash复用version；不同文件同hash提示复用，但跨来源证据出处可以保留。文件无法完整读取时仅写import_attempt失败，不伪造SourceVersion。

source_version.location_json为判别联合：本地`{"kind":"local","selected_path":"本机路径","storage":"reference|copy"}`，网页`{"kind":"web","url":"https://...","final_url":"https://...","fetched_at":"...","http_status":200}`。本机selected_path只保存在项目私有库，导出改为`source_hint`和hash，不泄露无关绝对路径。reference允许raw_blob为空，但规范文本、证据裁剪和必要原图快照仍须持久化；用户选择copy才保存全部原件。失去原件不等于已有快照消失，availability和原件可用性如实报告。

文本规范化V1：严格UTF-8解码，移除开头BOM，CRLF及CR转LF，不删空白、不NFKC、不自动替换符号。原始hash在规范化前计算。PDF规范文本来自确定阅读顺序的元素文本，页间用`\f\n`分隔；记录parser_version和参数hash。同一原件重新解析产生新的parse_manifest，不原地移动旧Span的定位。

SourceSpan为叶单元或容器，payload_json中固定`coverage_role=leaf|container`。容器只提供章节关系；叶文本区间按ordinal组成规范文本的无遗漏无重叠分区，纯空白使用formatting。图片等非文本叶单元可无byte区间，必须有region或明确非分页定位。每个叶单元才拥有CoverageRecord；容器不重复计数。start/end字节是manifest规范文本的半开UTF-8区间；line为一基包含两端。text_blob为该Span的文本快照，text_sha256必须与截取结果一致。

PDF页面记录physical_page从1开始，printed_label可为空或罗马数字字符串，绝不与物理页混用。page宽高为应用页面旋转后可见CropBox的pt尺寸；原始rotation单列。span_region使用旋转后页面左上角为原点、x向右y向下的[0,1]归一化坐标，范围严格满足0≤x0<x1≤1及0≤y0<y1≤1。同Span跨页或多区域用region_no顺序保存。PDF原始坐标到规范坐标的仿射矩阵保存在parser参数中，fixture核对90/180/270度及裁切页。

span.payload_json按kind约束：heading含level；table含rows/cols和cells（row,col,rowspan,colspan,text,unit,footnote_span_ids）；formula含original_tex可空、render_tex、origin=source|reconstructed、symbol_inventory；image含figure_id；footnote/caption含referenced_span_ids；unparsed含error_code与page_id。所有引用在当前manifest中存在，续表和图注通过ID关联而非拼接丢失。

每个PDF页必须有source_page记录和inspection_status。figure_detection=uncertain/failed时进行整页视觉复查，不能从“嵌入图片数0”推断无图。每个识别出的图/表/公式图建立figure_inventory，原区域或原图以blob保存。decision为pending/retain/extract/redraw/reuse/exclude/unresolved，非pending必须有理由。element_inventory_json逐元素记录`element_id,kind,label,required,geometry,source_locator`；箭头还记录from/to、direction、sign、conditions，曲线记录axes/units/scales，照片记录比例及图注。覆盖和视觉核验分别统计。

## 5 证据和条件化知识

Evidence必须固定span_id。text定位的excerpt偏移相对于该Span文本快照，半开字节区间，重取后核对hash和UTF-8边界。region定位使用`region_no,bbox,crop_sha256,render_version`；table_cell使用`row,col,cell_sha256`；formula使用`formula_span_id,source_region,tex_sha256`（无源码时tex_sha256只描述重建结果）。verification=matched仅表示证据定位匹配，不等于事实得到支持。网页搜索摘要不能生成matched全文证据。

| 修订实体 | 结构与不变量 |
| --- | --- |
| Concept | canonical_name、entity_class、disambiguation、definition_policy；aliases独立表含语言及适用scope；名称和别名均不全局唯一 |
| Claim | subject_id、predicate、typed value、unit/precision、条件、有效时间、verification；每项实质陈述最少一条被接受的支持或可核验推导链 |
| Atom | kind、title、payload、ordered claims、ordered child atoms、concept roles；是教学编排单位，可组合多个主张，不能把整章当作不可核对字符串 |
| Relation | from_id、to_id、predicate和固定claim引用；因果、分类、调控等知识关系必须有Claim支撑，纯导航关联放topic_member |
| Topic | title、topic_type、template_key；多重归属通过topic_member，主要导航父子通过topic_edge |

模型value.kind映射SQL value_kind，其余已校验值字段映射value_json；precision映射precision_json，conditions逐行写claim_condition。候选中的既有ID必须解析到输入Snapshot固定的revision，不能解析到提交时的“最新版本”。

Claim.value_json：text=`{"text":"..."}`；number=`{"decimal":"1.25"}`；range=`{"lower":"1","upper":"2","lower_inclusive":true,"upper_inclusive":false}`；entity=`{"entity_id":"..."}`；expression=`{"latex":"...","symbols":[...]}`；unknown=`{"reason":"source_not_given|unreadable|unresolved"}`；not_applicable=`{"reason":"..."}`。unit是明确单位字符串或null，precision_json为`significant_digits`或`absolute_error`，缺失不编造。只通过已有测试的单位转换规则做精确换算，禁止模型静默纠正量纲。

claim_condition的同一Claim全部为AND；每行dimension、operator、value_json、unit。dimension例如species/tissue/temperature/pH/time/context；in的值数组表达同维OR。复杂跨维OR拆成多个Claim并由同Atom组织，不以任意自然语言藏起条件差异。缺少条件是未知，不解释成“所有条件均适用”。context_sha256来自按dimension/operator/规范值排序的条件及有效期；比较时先解开真实条件，不只比较hash。predicate进入版本化词表；不认识的词提交待审候选，不能随意把相关关系映射成causes。

Atom.payload V1：definition/statement/exception/example含`segments`；formula含`origin,original_tex,render_tex,symbols,derivation_inputs`；reaction含`reactants,products,stoichiometry,direction,conditions`；process/derivation含`ordered_child_refs,premise_refs,conclusion_refs`；comparison含`subjects,attributes,cells`；classification含`members,criterion`；causal含`from,to,sign,condition_refs`。segments只能是text、claim_ref、concept_ref、formula_ref；所有实质内容必须能归入对应Claim，模型说明性文字不能绕过证据规则。结构化输入引用同时建立SQL关联表，载荷中的ID集合必须与关联表一致。

学习属性放snapshot_member.learning_json：`target_level,importance,difficulty,learning_objectives,prerequisite_ids,misconceptions,assessable`，均针对该Run的学习目标，不宣称为永久客观难度。required属于当前快照成员，不写成永久原子属性。

## 6 知识快照 合并和冲突

knowledge_snapshot从building转sealed时，snapshot_member固定每个entity唯一revision，并计算整个快照成员及关系的规范hash。所有原子引用的Claim、子原子、概念及关系必须在同快照；父子原子、主题主要父子均不得有环。sealed版本不更新，修改任何事实或证据关联必须创建新的实体revision与快照。

实体合并不删除旧ID或改写旧证据。identity_event记录before/after快照、操作、操作者、理由、受影响映射；concept_resolution在每个快照保存source_concept到最终canonical_concept的一跳映射，禁止链和环，目标自身映射到自己。合并选择已有一个概念为目标，来源陈述仍保留original subject；展示及索引通过本快照resolution解析。撤销或拆分创建新快照及reverses_event_id，恢复逐原子/Claim归属后重算全部依赖，旧发布继续引用旧映射。

knowledge_conflict固定两条Claim修订，classification包含duplicate/complement/condition_difference/historical/conflict/insufficient/parse_error。resolution_json必须含`decision,reason,evidence_ids,policy_version,actor,resolved_at`，decision为keep_both/choose_with_counterevidence/reject_parse/propose_research/await_human。resolved不等于把两个主张都标supported；OCR错误先修正解析并产生新revision。默认历史与新观点共存时必须保留时间、条件和争议状态。

## 7 运行 预算和研究

Run固定输入manifest集合、完整配置修订、mode、producer和批准scope。mode严格使用source/research，与CONFIG的generation.mode逐值相同，分别表示资料驱动与知识体系驱动。Run状态：draft、ready、running、pause_requested、paused、cancel_requested、blocked_input、blocked_config、budget_stopped、failed、cancelled、ended。ended只表示调度结束，不能替代质量通过。Job及租约、重启恢复以运行设计为准；其状态固定queued/running/retry_wait/blocked/succeeded/failed/cancelled/skipped/uncertain。

Job的lease_generation在每次领取/回收时递增；结果提交同时CAS匹配owner和generation，过期worker不能提交。command_receipt以client_command_id唯一去重，重复ID且请求hash不同拒绝；project_event.sequence提供可重放有序状态事件，payload不含秘密。

external_attempt是模型、搜索、页面读取和Agent调用的统一可计费尝试记录，status为prepared/dispatched/succeeded/failed/outcome_unknown。字段包含请求指纹、job内序号、提供商request_id、输入/输出blob、usage以及reservation引用。原始凭据不得进入blob或usage。request_blob默认null，仅保存request_sha256；明确启用本地诊断时才保存脱敏请求，保留期遵从CONFIG。response_blob保存候选JSON及必要错误上下文，不导出隐藏推理或认证头。budget_reservation使用held/settled/released/unknown，保存预留及已知实际Token、费用和页面量。unknown仍占用预留；费用币种明确，数据库使用整数百万分之一币种单位，配置与数据库都使用max_cost_micros等micros字段，不进行隐式minor-unit换算，禁止浮点金额。

research_item固定goal、scope_sha256、scope_class、hop、required、批准状态及停止理由。accepted后范围内容变更必须创建新项；new_candidate不能required，不调度。拒绝记录按discovery_fingerprint加理由版本保存，只有实质新证据可重新提议。research_result保存检索位置、实际读取SourceVersion或不可访问原因，不能把snippet与fetched全文混为一项证据。

## 8 计划 表达和唯一位置

note_plan固定snapshot、style修订、父plan和内容hash。一个plan下的planned_note、planned_section、note_node都不可原地更新；保持note_id/section_id/node_id，产生新plan表达新版本。planned_note.path_key唯一；section和node ordinal从0开始，同父范围无重复。一个完整过程原则上同笔记，模型上下文限制由编译分块处理，不以字数阈值决定拆碎。

note_node.kind为paragraph/ordered_list/bullet_list/table/callout/formula/flowchart/sequence/timeline/image/embed，origin为fixture/source_reproduction/expression_optimization。每个节点保存表示理由及atom集合；主要形式与辅助形式由section的首节点及payload `presentation_role=primary|supporting`区分，每节恰有一个primary内容组（可包含多个连续节点共享group_id）。各类型载荷见MODEL_CONTRACTS中的RepresentationCandidate。

node_element细化图节点、边、表格单元格、符号、图例；纯label可不映射Atom，其余至少一个element_atom，且是node_atom子集。一个图表节点ID存在不意味着全部条件已展示，校验器逐实质元素及必要条件核对。派生数值需要payload中的`derivation={input_claim_refs,expression,result,unit}`，由确定性计算器重算。

canonical_placement以(plan_id,concept_id)主键保证至多一个；对快照中definition_policy=required且经resolution归一后的根概念必须恰有一个。file、heading、block三类目标分别需要note、section、node与合法block_id；section/node必须实际属于对应note。外部/排除概念可无位置，引用时显示文本或已批准外部URL，不造假Wikilink。semantic_link只存知识目标ID与display_text、reference/embed，真实路径由编译器解析。

structure_lock绑定目录路径或稳定对象ID、锁定字段、当时指纹及active状态。锁定与风格冲突时锁定优先，创建问题而非静默改结构。批量迁移新plan必须包含所有受影响链接，不能先移动目标再等待人工审批。

structure_lock.revision从1开始，修改锁定字段、撤锁或重新锁定都CAS递增，并通过audit_event保留前后值。审批指纹包含所涉及锁定的ID及revision，不能仅记录内容hash而忽略锁策略变化。

## 9 文件映射 人工变更与发布

rendered_file指向实际生成blob、plan、note以及路径；content_mapping的区间是文件UTF-8半开字节，必须在文件范围内并落在字符边界，映射到node、atom修订和可选element。映射重叠仅允许同一输出片段表达多个原子；不得映射空白或隐藏元数据冒充正文覆盖。图像映射的文件区间覆盖嵌入语法，同时element映射附属JSON将实质元素与图资源关联。

dependency_edge方向固定upstream→downstream，包含双方kind/id/revision及reason。类型仅允许source_version/manifest/span/evidence/entity/snapshot/plan/node/file/release，按类型解析引用存在性；job执行图另用job_dependency。影响分析从变化上游按边做可达闭包，无关文件内容hash应保持。

ChangeSet保存base/current/candidate三个manifest hash及自身规范hash。file_change逐项记录create/replace/move/delete、旧新路径、三个文件hash、human_affected与依赖。approval状态pending/accepted/rejected/stale，接受绑定change_set_hash和current_manifest；任何候选、原文件、锁定或依赖变化都使批准失效。人工内容或可能影响其链接的变化必须等待对应approval；拒绝的项与依赖项保持旧版本，不通过其他发布分组绕过。

用户在审批界面可编辑有限结构化候选：标题、顺序、段落segments、表格单元格、图示标签及选定差异，不接受绕过IR的整篇原始Markdown。每次保存创建新的实体revision（涉及知识时）和/或新Plan，并重建node_atom、element_atom、content_mapping与ChangeSet；旧approval一律stale，原候选保留历史。新增实质人工陈述建立origin=manual的SourceVersion/Span和Claim，证据位置可matched但事实verification仍unverified或insufficient，不能因actor=human自动获得科学supported。经核验或明确保留不确定性后，再按同一语义、覆盖、锁定和发布门禁生成新审批。

release状态staged/verified/publishing/published/recovery_required/rolled_back；release_file列出该版本一致的文件、路径、ownership及hash，不能只保存本轮修改列表。publication_step与磁盘journal同sequence_no，可恢复流程见PUBLICATION。任何重启后的未知文件状态都先恢复/人工审查，禁止直接标published。回滚也生成新的ChangeSet并重新检查人工差异。

project_head保存当前生效snapshot/plan/release及CAS revision，不按时间戳猜测最新发布。发布全部复核成功后，在提交release_file、ChangeSet applied和project_event的同一SQLite事务中更新head；历史release不变。候选Snapshot/Plan不提前替换当前已发布head。

## 10 校验 四维完成和导出

ValidationResult固定candidate manifest和validator版本。checks_json各项必须含`rule_id,category,status,required,object_refs,details`；category为structural/semantic/visual，status为passed/failed/not_run/not_applicable。CLI的AST、单位、路径检查不能代替visual；没有实际渲染核对的视觉任务保留not_run或needs_review问题。本轮必需visual未通过时quality_validation=incomplete。fixture_reviewed只说明fixture审阅，不可用作生产科学质量证据。

ValidationIssue保存规则、等级、对象、状态及解决理由。blocking不能waived；普通warning经策略记录可waived。单问题feedback_round最多2，超过仍未解决则阻塞相关产物，不能删问题求通过。

四维字段固定为source_coverage、knowledge_compilation、quality_validation、optional_research，每个值是`{status,required,total,done,reason_codes,release_id}`。前三维生产主任务必须complete且对应真正落地的发布范围；required/total/done均为非负整数，done≤total，required≤total。source total是输入来源的叶单元，失败/未知/未审定排除不算done；compilation total是本轮必要Atom，done需最终映射有效；quality total是适用必需检查，未运行不算done；research total是已批准目标，未批准候选不进入分母。没有批准研究时为not_applicable且三个计数0。还有待审批变更时说明“已发布部分范围，另有待审批”，不将全库编译记complete。

导出包含Vault、release manifest、schema_version、知识及证据索引、blob清单、来源提示与可选原件。export_bundle记录manifest hash和includes_sources；绝不导出credential_ref、密钥、机器绝对路径或请求日志。重新定位来源必须匹配原始hash，文件名相同不够；迁移时缺原件显示unavailable。

## 11 派生检索索引和候选事务门禁

source_span_search是可重建的FTS5派生索引，不是来源权威文本。text列保存Span规范文本；zh_bigrams列将连续汉字串拆成相邻二字并以空格分隔，单个汉字作为单字项。拉丁词只进入text列，由unicode61处理大小写，remove_diacritics=0；中文二字间不夹英数，数字/单位保留在text。索引版本search_v1进入检索Job缓存键。仓储在Span及其文本blob核对后，同一提交事务按span_id删除旧派生行并插入唯一FTS行，重试不得重复索引；解析manifest不可变，新增manifest新增行，查询限定本Run的manifest集合。源文本未保存时不造空索引。

B02升级创建空FTS表后，从schema0的Span文本blob重建并核对行数；索引损坏可删除派生行再重建，不改Span或Evidence。重建期间检索Job为blocked而非返回空成功。参数化MATCH查询由Core将术语分词并逐词加双引号，内部双引号转义，不允许资料原样拼接FTS操作符。查询的额外上下文上限不影响来源覆盖账本。


以下是ExtractCandidateV1中一个已绑定证据的Claim元素示例；完整Envelope与所有字段见MODEL_CONTRACTS。临时ID只存在于候选中，内容不充当科学验收材料。

```json
{
  "temp_id": "tmp_q1",
  "subject_ref": "tmp_c1",
  "predicate": "has_property",
  "value": {"kind":"text","text":"演示属性"},
  "unit": null,
  "precision": null,
  "conditions": [],
  "valid_from": null,
  "valid_until": null,
  "evidence_links": [{"evidence_ref":"fx_e1","role":"supports"}]
}
```

入库顺序：JSON/尺寸/枚举校验→输入hash与scope核对→证据重取→引用和类型校验→语义核验→分配或复用ID→写entity及revision/关联表→封印快照。一个候选失败不得留下半个Atom或伪成功Coverage。拒绝候选保存在job诊断blob，不加入已接受知识。

## 12 需求覆盖和实现检查

| PRD需求 | 主要数据落点 |
| --- | --- |
| FR-01至06 | project/source/version/import/manifest/span/page/figure/evidence/coverage/config |
| FR-07至10 | alignment/concept/alias/claim/condition/evidence/identity_event/resolution/conflict |
| FR-11至14 | run.scope/research_item/result/job/validation_issue/snapshot/dependency |
| FR-15至19 | topic/member/edge/note_plan/section/placement/link/structure_lock |
| FR-20至25 | style/node/element/atom/formula payload/figure/visual_asset/rendered_file |
| FR-26至28 | evidence/content_mapping/release/export_bundle/completion_dimension |
| FR-29至34 | dependency_edge/change_set/file_change/approval/structure_lock/release_file/publication_step |
| FR-35至40 | run/job/dependency/external_attempt/budget/provider/capability/audit/config |

必须实现负向样例：悬空Evidence、跨manifest图注、重复规范位置、路径大小写冲突、跨snapshot Claim、错误类型引用、循环子原子或redirect、未解析页却报告完整、审批hash失配、unknown预算被释放、必需visual未运行却通过。DDL执行成功仅证明建表语法；这些跨对象门禁仍须在对应实现任务验收。
