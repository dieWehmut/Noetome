# 有界多来源知识协调契约 v1

本契约落实 [PRD](../requirements/PRD.md) 的 FR-08、FR-09、FR-10、FR-26、FR-40、NFR-03 与 NFR-07，在 B05-T01 中只实现显式身份映射、重复归并和不同属性的互补保留。来源依据沿用 [来源知识核对契约](EXTRACTION_REVIEW_CONTRACT.md)。本文件是实施规范，完成情况仍以 [账本](../../dev-log/tasks.json) 和实际验收记录为准。

## 1 范围与入口

普通 CLI 为 `noetome knowledge coordinate <project> <coordination-case> <new-output>`，动词后可加 `--` 处理以连字符开头的路径。`knowledge`、`knowledge --help` 和 `knowledge coordinate --help` 返回同一说明。参数缺失、多余、非法 OS 字符串或未知动词返回 2；解析参数不读写文件。

输入是同一个明确项目内 2 至 3 份保存的、单原子且成功的 `extract verify` 依据包。协调器重新读取来源项目、全部依据字节和身份映射，不调用模型，不读取凭据，不写来源项目或生产 Vault。输出只允许已有普通父目录下尚不存在的测试根。

v1 成功仅表示 `explicit_identity_duplicate_complement_v1` 范围内的来源依据与协调结果通过。知识保持 `pending`，科学质量 `not_run`，研究 `not_requested`，`published=false`，`network_calls=paid_calls=0`。来源时间、条件化版本、冲突裁决、身份合并撤销和一般自然语言推理分别留给后续任务；不据 mtime、文件名、运行时间、模型信心或来源多数推造事实时间及权威。

## 2 严格输入

每份 JSON 都拒绝未知字段、重复键（包括嵌入的任意 JSON 值）、缺失必填字段、不支持版本、尾随 JSON 及错误类型。声明为计数的字段只能是非负整数；声明为可空的字段仍须出现，未知写为 `null`，不能用 0 代替。输入目录只包含下列明确清单，不接管额外内容。

协调 case 包含 `coordination.json`、一个显式映射文件和 2 至 3 个普通单层依据目录。目录名及映射文件名采用已有安全相对路径规则，禁止 `/`、`\\`、`.`、`..`、Windows 保留名、尾随点或空格。输入 ID 匹配 `[a-z][a-z0-9_]{0,31}`，互不重复；两个角色不能指向同一文件或目录身份，禁止符号链接、reparse/junction 和多硬链接文件。

`coordination.json` 严格字段为：

| 字段 | 值或类型 |
| --- | --- |
| schema_version | 1 |
| purpose | `explicit_identity_duplicate_complement` |
| project_id | 来源项目的真实 UUID |
| identity_file | 映射文件的普通单层文件名 |
| inputs | 长度 2 至 3 的数组 |

每个 input 严格字段为 `input_id`、`review_bundle`、`binding`、`artifacts`。`binding` 采用完整的 `SourceBinding`，含必须出现的可空 `normalized_sha256`、`parse_id`、`manifest_sha256`；import_id 来自这个绑定。`artifacts` 的键严格为下述九个文件名，值为严格 `{sha256,byte_length}`，固定 SHA 必须与实际完整字节一致。

依据目录严格包含 `.owner.json`、`review-original.json`、`candidates-original.json`、`gold-original.json`、`decision-original.json`、`source-snapshot.json`、`knowledge-ir.json`、`verification.json`、`verification.md`。只保留声明的九个文件不意味着核对通过；仍须执行第 4 节的独立重查。

## 3 显式身份映射

映射文件严格字段为 `schema_version:1`、非空普通文本 `author`、`concepts`、`concept_mappings`、`atom_mappings`。

- `concepts` 每项为 `{canonical_id,display_name}`。canonical_id 必须是规范小写、互不重复的 UUID，display_name 是非空普通文本。显示名称不用于归并或构造 ID。
- `concept_mappings` 每项为 `{input_id,original_concept_id,canonical_id}`。每份原 IR 中每个概念恰好有一项映射，原概念不删除或改写；不允许未知、缺失、重复、跨 input 借用的引用。
- `atom_mappings` 每项为 `{input_id,original_atom_id,object_id,property_id}`。每份原 IR 中唯一原子恰好有一项映射，object_id 和 property_id 是两个不同的已声明 canonical_id，并均由该原子的概念引用映射得到。object_id 必须来自原引用中 canonical_name 或 alias 与 review.subject 规范值一致的组件，property_id 同理对应 review.property；此项只重查作者明确给出的本地角色，不按名称推断跨来源身份。v1 使用来源核对已支持的主体与属性组件概念；未提供两项可明确映射的组件引用不得猜测成对象与属性。
- 未使用的规范概念和无关映射拒绝。每项 original ID 是有界非空普通文本；映射的选择是作者的显式决定，不证明科学语义等价。

样例使用组件概念“样例 A”“速度”“质量”，两份速度输入映射到同一对象/属性，质量输入映射到同一对象和另一属性。显示名称可更改，映射数组和 input 数组可重排；canonical_id、归并关系、事实 ID 和实际证据关系必须稳定。

## 4 来源依据重验

加载依据时保留 case、父目录、映射、每个依据目录及九份文件的身份和字节守卫，直到最终结果保存后。保留全部 `SourceSession`，重新检查只读 SQLite、注册版本、原始及规范资源、Span 或 PDF parse/manifest/page/region/resource 与实际 UTF-8 范围，不只比较 SHA 或“passed”字符串。

依次重新解析四份原始输入；核对 review 选择恰好一个单位、原 gold SHA、决定为 included 且只有该原子、来源绑定、双 request_id、输入限额、解释时间与 local_http_test 标记。执行现有 `validate_gold`、`validate_candidates` 和完整 `extraction_review::assess`。只接收重新计算为 passed、恰好一个实际原子、known value 与明确单位的依据。

按实际来源重建 `source-snapshot.json`，按原候选及重算 checks 重建 `knowledge-ir.json`。使用来源核对的共同报告构造函数，从四个原角色名的实际字节指纹和七份原报告 artifact 字节指纹重建完整 `verification.json` 及其 Markdown。保存的 IR、快照、全部状态、原对象、逐项 checks、原始分母、质量分区、指纹和双报告必须与这些重建结果一致；旧规则、字段、错误行为不能因本任务放宽。

`.owner.json` 严格为 `schema_version:1`、`producer:source_review`、匹配候选的 `request_id`、`purpose:unpublished_source_review`。manifest 的 binding 必须等于重新打开的实际来源和原候选。相同内容 SHA 的不同 source_id/version_id 不能互换。

同一实际 `(project_id,source_id,version_id,parse_id,unit_id,start_byte,end_byte)` 不得以复制目录、换 input ID、改 atom ID 等方式重复计为一次新来源出现，预检拒绝 `COORD_DUPLICATE_INPUT`。不同 source_id 即使相同字节 SHA 仍保留两个真实出现位置；independent_sources 按真实 source_id 计数，不按 SHA 计数。同一来源的不同版本或解析身份不由 v1 自动裁决，保存为 needs_review。

## 5 事实归并和可定位关系

事实键严格为 JSON 数组：`[object_id,property_id,normalized_value,normalized_unit,sorted_normalized_conditions,sorted_normalized_exceptions]`。normalized 只移除 Unicode 空白；不换算单位、不数值化字符串、不把 `10.0` 等同 `10`、不折叠大小写、不删除条件和例外。条件和例外取重新核对过的原 gold 完整集合；原候选中的允许上下文仍在原对象和来源依据中完整保留。

稳定 fact_id 为 `fact-` 加上述 JSON 紧凑 UTF-8 字节的 SHA256；object_id/property_id 是作者 UUID，显示名和 input 顺序不参与键。occurrence_id 为 `occ-` 加紧凑 JSON 数组 `[project_id,source_id,version_id,parse_id,unit_id,start_byte,end_byte,original_atom_id]` 的 SHA256。原对象内容、测量、qualifiers、概念引用、来源绑定、单位定位和完整证据仍保存为每个 occurrence 的原值。

完全相同的键归为一个 fact，occurrence_ids 保留所有独立位置；重复引用不能额外贡献独立来源。每个原子的所有支持证据都保留，不以任选第一条证据替代另一输入的出现。不同 object_id 的事实分别保留；同一 object_id、不同 property_id 标为 complement。同一 object_id/property_id 的非同键事实或同 source_id 的不同版本/parse 身份，均标 `needs_review`，issue code 为 `COORD_UNHANDLED_DIFFERENCE`；原事实分组和对立依据全部保留，不产生通过的协调 IR。

每对 occurrence 有且仅有一条关系，左右 ID 按字典序排列。relation 只接受 `duplicate`、`complement`、`separate_object`、`needs_review`；前三种 issue_code 为 null，needs_review 为上述问题码。`separate_object` 仅说明作者给出不同对象，不是 FR-10 六类科学关系裁决。牵涉 needs_review 的 occurrence 和 fact 标为 needs_review，其余标 passed。成功范围没有 failed/unprocessed 的伪通过对象。

## 6 输出事实和分母

输出 owner 严格为 `{schema_version:1,producer:"knowledge_coordination",request_id:<本次 UUID>,purpose:"unpublished_coordination"}`。本次 request_id 只用于审计，不作为来源时间或事实 ID。

逐字节保存 `coordination-original.json`、`identity-original.json`，以及每份输入的九个文件，平铺名称为 `input-<input_id>--<原文件名>`。`coordination.json` 和 `coordination.md` 表达同一完整事实；stdout 是该 JSON 加一个换行。最终守卫重验成功前不得返回成功。

报告严格字段：`schema_version`、`report_kind`、`producer`、`request_id`、`scope`、`project_id`、`status`、`knowledge_status`、`published`、`scientific_quality`、`research_status`、`network_calls`、`paid_calls`、`canonical_concepts`、`inputs`、`occurrences`、`facts`、`relations`、`totals`、`artifacts`。固定值分别为 1、`knowledge_coordination`、`knowledge_coordination`、本次 UUID、`explicit_identity_duplicate_complement_v1`、实际 project_id、passed/needs_review、pending、false、not_run、not_requested、0、0。

- inputs 每项严格包含 `input_id,binding,request_id,source_review_status,source_review_checks,source_bundle_fingerprints,copied_artifacts`。status 固定 passed；checks 为重新核对的原 source_review 检查，不重新伪造“科学通过”。copied_artifacts 把原九文件名映射到实际平铺副本。
- occurrences 每项严格包含 `occurrence_id,input_id,binding,unit,original_concepts,original_atom,original_evidence,canonical_concept_ids,object_id,property_id,value,unit_value,conditions,exceptions,status,issue_codes`。unit 是完整 RequestUnit；original_atom 是完整 CandidateAtom，original_evidence 是完整支持证据列表；原概念全部保留。value/unit_value/conditions/exceptions 为核对过的原 gold 原值，canonical_concept_ids 为原子引用经显式映射得到的去重排序 ID。
- facts 每项严格包含 `fact_id,object_id,property_id,value_status,value,unit,conditions,exceptions,occurrence_ids,status,issue_codes`。value_status 固定 known，其余语义字段采用事实键的规范值；即使需要复核也保留全部事实组。
- relations 每项严格包含 `left_occurrence_id,right_occurrence_id,relation,issue_code`。
- totals 严格包含 `input_bundles,validated_input_bundles,input_atoms,retained_fact_groups,canonical_facts,source_occurrences,evidence_links,independent_sources,duplicate_occurrences,passed,failed,needs_review,unprocessed`。所有字段必填；canonical_facts 是显式可空整数，其余是整数。输入分母、事实组分母和来源/证据分母分别计算。`passed+failed+needs_review+unprocessed=input_atoms=source_occurrences`；input_atoms 来自实际重新核对的三个/两个单原子输入。duplicate_occurrences=`source_occurrences-retained_fact_groups`；它是结构性相同键冗余，不能代表科学裁决。
- passed 时 canonical_facts=retained_fact_groups；needs_review 时 canonical_facts=null，不能把未知的已协调数量写成 0。证据计数为实际每个 occurrence 支持链接数量；independent_sources 为不同真实 source_id 数量。
- artifacts 为输出当前已保存 owner、两个原输入、全部依据副本和成功 IR 的精确 `{sha256,byte_length}` 清单，不自引用 JSON/Markdown 报告。

排序按 canonical_id/input_id/occurrence_id/fact_id/关系 ID 进行，不随输入数组顺序变化。所有引用须唯一、完整，事实 occurrence_ids 分区覆盖原输入的实际原子，既不遗漏也不重复使用。

只有 status=passed 才保存 `coordination-ir.json`。其严格字段为 `schema_version:1,producer:"knowledge_coordination",revision:1,request_id,scope,project_id,knowledge_status:"pending",published:false,scientific_quality:"not_run",research_status:"not_requested",canonical_concepts,inputs,occurrences,facts,relations,totals`，与报告相应字段完全一致。需要复核退出 3，保留完整双报告、原输入和全部证据，不保存协调 IR。

## 7 限额、保护和失败

协调 manifest 和映射各最多 64 KiB，concepts/concept_mappings/atom_mappings 各最多 1024 项；身份、规则、映射、候选及 gold 的文本字段最多 1024 UTF-8 字节（input ID 与 UUID 更严格），条件与例外各最多 8 项。真实 `RequestUnit.text` 沿用单来源所选正文最多 1 MiB，不按元数据字符串截断；其 JSON 转义后仍受快照和输出单件限额约束。候选最多 1 MiB；review/gold/decision 各 64 KiB；owner 4 KiB；source-snapshot 4 MiB；IR、JSON 报告、Markdown 各 8 MiB。全部选定 case/映射/依据字节合计最多 96 MiB。总体控制 30000 ms，并在每个来源重开前后、保存前后检查；SourceSession 的已有更严格资源限额继续生效。

参数错误为 CLI_INVALID_ARGUMENT/2。case、映射和保存依据包的结构、版本、类型、未知/重复字段及额外清单均为 `COORD_SCHEMA`/2；路径为 `COORD_INPUT_PATH`/2，指纹为 `COORD_FINGERPRINT`/2，身份映射为 `COORD_IDENTITY`/2，输入限额为 `COORD_INPUT_LIMIT`/2。合法结构中的原来源依据或语义重新核对失败为 `COORD_SOURCE_REVIEW`/3，并附 input_id 和实际原因；不能将已经明确的结构错误统一重标为来源核对错误。源项目的既有错误码可以原样传播，必须明确失败。复制同一来源单位为 COORD_DUPLICATE_INPUT/2；只读输入变化为 COORD_INPUT_CHANGED/3；超时为 COORD_TIMEOUT/3。

新保存的 IR、JSON 和 Markdown 也受已有 `Artifact::create` 的 8 MiB 单件限制，超过时在该结果文件创建前拒绝 `EXTRACT_SAVE_LIMIT`/3。原输入副本遵守对应原文件限额和合计96 MiB限制，不将该保存限制改成截断输出。限额错误仍需按捕获能力撤去当前三个结果并保留原输入、owner和未知内容。

输出重叠、已有根和父目录错误复用已验收的 EXTRACT_OUTPUT_* 保护，不以字符串前缀代替真实目录身份。写入之前拒绝与项目、case、依据和映射重叠或包含它们的目标。来源、输入、哨兵、原 IR/gold 以及未知人工文件始终保留。

结果保存后再次核对输入、全部 SourceSession 和输出。失败时只可撤去当前 OutputRoot 捕获的 coordination-ir.json/coordination.json/coordination.md，不递归删除，不枚举删除未知文件，不覆盖或清理输入。owner、原输入副本和未确认内容保留。

## 8 独立验收

先冻结 [三来源作者样例](../../fixtures/knowledge-coordination-v1/README.md) 的原文、身份决定和 expected，再构造明确标记 local_http_test 的候选，通过真实 source import 和 extract verify 保存依据。现有 source import 会按完整内容 SHA 复用来源，不能假定同字节不同文件自动获得独立 ID；准备 B 时先用独特种子正文登记普通来源身份，再对该已有 source_id 显式导入冻结的相同正文，仅最后 A/B/C 的成功单原子依据参与协调。种子版本保持原样、不是第四项知识输入，也不能据此宣称真实书目独立性。第一次普通 knowledge coordinate 测试必须因外部行为缺失而 RED；不能把编译、路径或缺失依赖错误称为行为 RED。若准备不满足三个不同 source_id，应保留失败并在已提交的实现前源码快照重放修正的普通 CLI RED，明确记录其实际时间，不能倒推原准备正确。

正常样例必须得到 2 个事实、3 个来源出现、3 个不同 source_id、3 条支持证据和 1 个冗余出现，速度事实保留 a/b 两份位置，质量事实保留 c 位置。独立 Python 只读实际 SQLite/资源、冻结 gold、四原输入、保存 IR/双报告和显式映射，从事实与出现重建分母，不以 Rust 报告的 passed/计数作为 oracle。先以可接受伪报告的临时骨架运行故意破坏的行为测试观察 RED，再实现验证。

验收还包括显示名/输入顺序不换 ID、同 SHA 跨 source 错绑、缺第二来源证据、错值/单位归并、复制输入、未处理条件/单位/版本差异、可空计数伪零、未知字段/重复 JSON 键、输出碰撞/越界，以及结果保存阶段的身份/字节变化。coherent forgery 同步修改 JSON、Markdown 和指纹后仍须被独立作者事实或真实来源拒绝。常规 Rust/Python、日志、Git 检查和新鲜 task/final 审查是收尾条件；未执行的完整教材/科学质量/兼容性和性能测试保持未完成。
