# B02-T02 fixture契约校验实施方案

目标来自[任务卡](NEXT_STEPS.md)、[数据契约](../technical/DATA_CONTRACTS.md)和[测试标准](../standards/TESTING.md)。按已授权基线新增只读`case verify <case-root>`；不创建Run、项目数据库、Release、Vault或GUI入口。默认使用自造虚构对象和明确声明的fixture，不能作为模型或科学金标准。

## 架构与接口

- `fixture_model.rs`：Serde严格类型，全部版本固定1，producer枚举只接受fixture，未知字段/枚举拒绝。fixture ID为1–128字节ASCII标识，首字符字母/数字，其余字母/数字/`_`/`-`，不自动按名称生成。
- `fixture_input.rs`：只读取manifest声明的文件，复用project路径/普通文件/Windows读守卫；总原始输入64 MiB，单JSON最多4 MiB。持有守卫至验收结束，读前后核对身份、长度与mtime；拒绝路径穿越、绝对/驱动器/ADS、reparse/junction及大小写路径冲突。复用text::analyze，生成实际Span并逐字段核对声明清单。
- `fixture_knowledge.rs`：`validate_knowledge(&LoadedCase) -> Vec<Issue>`与`validate_coverage(&LoadedCase) -> Vec<Issue>`，核对IR身份、来源、Evidence、Atom及Coverage。独立模块测试先观察可编译stub断言失败，再实现。
- `fixture_plan.rs`：`validate_plan(&LoadedCase) -> (Vec<Issue>, Vec<ResolvedTarget>)`与`validate_expected(&LoadedCase) -> Vec<Issue>`，核对节点显示映射、规范位置、语义链接及独立预期。段落/列表项/表格单元格是独立映射范围，不借其他单元格补单位。
- `fixture.rs`：公共`verify(&Path) -> Result<CommandOutput, ProjectError>`，组织五项必需检查、实际计数和JSON；语义失败输出failed JSON并退出3，无法读取/反序列化的输入失败走稳定诊断及2/3退出码。
- `lib.rs/main.rs`：case帮助和纯参数解析，桌面render仍只help/version。`tests/fixture.rs`调用实际CLI；`fixtures/text-compare-v1`保存完整可审计自造包；主执行者维护文档/证据/账本及提交。

`LoadedCase`公开crate内字段：`manifest: Manifest`、`ir: KnowledgeIr`、`plan: PlanPackage`、`expected: ExpectedPackage`、`spans: BTreeMap<String, ActualSpan>`、`source_issues: Vec<Issue>`、`ir_sha256/plan_sha256/expected_sha256/case_sha256: String`。`ActualSpan`含source_id、version_id、`declared: DeclaredSpan`和实际规范区间`text: String`。只要实际来源清单不匹配，其他必需检查为not_run，不从错误边界切片。`LoadedCase::check_unchanged()`在完成JSON前复核全部守卫。

`Issue`字段为`rule_id`、`object_refs`、`message`。`ResolvedTarget`为concept_id、note_id、section_id、output_path、heading。顶层报告schema_version=1、producer=fixture、case_id、validator_version、passed、checks、counts、issues、resolved_targets和四文件SHA；checks含sources/coverage/knowledge/note_plan/expected及明确not_run的rendering/scientific_quality。passed只由五项必需契约检查决定，不宣称实际输出或科学核验。

## JSON v1具体结构

所有下述字段必填，包括可空字段；集合顺序有意义的地方保持输入顺序。实体ID在同类清单内唯一，Span/node/section ID在各自全包范围唯一。JSON类型定义与本节一致，必要内容/标题/理由非空，revision正整数。

**case.json / Manifest**：schema_version、producer、case_id、sources、ir_file、note_plan_file、expected_file。至少一份来源；最多32份。每个来源：source_id、version_id、relative_path、media_type、content_sha256、normalized_sha256、normalization_version、parser_version、spans。media_type为text/plain或text/markdown，路径后缀与其匹配，规则版本为utf8-newlines-v1与blocks-v1。DeclaredSpan：span_id、ordinal、kind、start_byte、end_byte、start_line、end_line、text_sha256、parse_status，与B02-T01实际草稿严格相符。

**ir.json / KnowledgeIr**：schema_version、producer、revision、coverage_run_id、sources、concepts、evidence、atoms、coverage。coverage_run_id是fixture内固定审计作用域，不代表本轮执行创建了Run。来源引用：source_id、version_id、content_sha256、normalized_sha256，与manifest集合双向一致。

- Concept：concept_id、revision、canonical_name、aliases、needs_full_explanation。名称/别名可重名，链接仅按ID。
- Evidence：evidence_id、source_span_id、excerpt_start_byte、excerpt_end_byte、excerpt_sha256。范围相对于实际Span，非空、UTF8字符边界且SHA由真实切片计算。
- Atom：atom_id、revision、kind、concept_ids、content、qualifiers、evidence_links、required、verification、value_status。kind为definition/statement/comparison_value；content为纯文字，qualifiers有序字符串列表，不强迫无条件定义虚构单位。EvidenceLink为evidence_id、relation（supports/refutes/qualifies）。verification为fixture_reviewed/unverified，value_status为known/unknown/not_applicable；required只能known且fixture_reviewed，必须至少一条有效supports，不能只靠反证。
- Coverage：run_id、span_id、disposition、atom_ids、required_atom_ids、duplicate_of_span_id、exclusion_reason、decision_source、policy_version、stage。disposition保留pending/included/excluded/duplicate/failed/needs_review，stage保留registered/mapped/planned/rendered/validated。本项只允许已映射mapped或planned，不伪造rendered/validated；policy_version为fixture-policy-v1。decision_source为fixture_review或deterministic_format。每个实际Span恰一条记录；included非空atom映射及当前位置supports，required_atom_ids恰为其中required子集。excluded映射空、有非空理由、无duplicate引用，确定性排除仅适用于formatting。duplicate需同完整Span SHA，禁止自环/环，最终included且映射一致，每个重复位置仍有自己的支持证据。其他处置/未复核parse_status不通过。

**note-plan.json / PlanPackage**：schema_version、producer、revision、knowledge_revision、knowledge_sha256、notes、placements。knowledge_sha256对完整ir.json原始UTF8字节计算，不能只签字段子集。

- Note：note_id、revision、title、output_path、sections；Section：section_id、heading、ordered_nodes。至少一份笔记/非空小节，output_path是普通`/`相对`.md`，Windows大小写路径碰撞拒绝；同笔记heading忽略首尾空白及大小写后唯一。标题/heading不接受换行、`[]#|^`等改变wiki标题解释的字符；嵌入、块链接不在v1支持范围。
- Placement：concept_id、note_id、section_id、canonical；needs_full_explanation概念恰一处规范位置，其他概念可没有；已有位置需有相关原子显示映射。SemanticLink：target_concept_id、display_text，须解析到唯一已有规范位置，不按同名搜索。
- 所有Node共同字段node_id、representation_origin（只fixture）、atom_ids、kind。paragraph增加text、links；ordered_list增加items，每个ListItem为item_id、text、atom_ids、links；table增加columns、rows，Column为column_id、label，Row为row_id、cells，Cell为kind（label/content/missing）、text、atom_ids、links。表格矩形，每行对应列数；content单元格非空实质映射，label/missing不带atom或links，missing明确写未给出或不适用。Node的atom_ids与实际子项映射集合相等，重复引用不能隐藏漏映射。
- 每条段落、列表项或实质单元格映射须在其本身显示对应Atom.content和所有qualifiers，不借另一个单元格、标签或节点的字符串补齐。所有required原子至少有一次完整显示；只保留ID、空文本或遗漏单位不能通过。

**expected.json / ExpectedPackage**：schema_version、producer、case_id、required_atoms、evidence、coverage、canonical_placements、required_links、tables。此文件由fixture作者独立声明，验证器不能重写它；审阅只验证结构/源片段，不宣称人类科学金标准。evidence固定已审阅的全部Evidence ID、实际Span、局部范围及SHA，与IR逐字段对账，支持合法partial只在事先审阅预期匹配时通过。

- ExpectedAtom：atom_id、kind、concept_ids、content、qualifiers、evidence_span_ids。集合非空，恰与IR.required集合相等；正文/类型/概念/限定精确匹配，支持证据位置集合需包含明确预期位置。required=false漂移、unknown、unverified、删单位等都失败。
- ExpectedCoverage：span_id、disposition、atom_ids、required_atom_ids、duplicate_of_span_id、exclusion_reason、decision_source，与全部实际Span及IR逐项对账，任意非空伪排除理由不足以通过。
- ExpectedPlacement：concept_id、note_id、section_id、atom_ids，与全部canonical位置集合相等且独立指定的解释原子必须在该规范位置完整显示。不能用同概念的其他数值原子冒充移走的定义。ExpectedLink：from_node_id、target_concept_id、display_text；每项必须在实际该节点的语义引用中出现。
- ExpectedTable：node_id、columns、rows（复用Column/Row/Cell严格结构），恰与所有实际table节点对应并逐字段一致。标签、缺失状态、行列、每个单元格kind/text/atom映射均独立固定，不能改成label或把数据移到别行规避校验。

## 执行与验收

1. fixture/实际CLI测试先落盘，运行旧CLI观察case verify尚不存在的断言失败；纯模块先可编译stub再观察行为红。
2. 先完成严格类型和守卫加载，再独立实现Coverage/Knowledge及NotePlan/expected；参数/报告由主执行者接线。每个代理仅改约定文件，不改账本、不提交、不推进B02-T03。
3. 正常fixture至少两个相关概念/笔记、比较表、中文路径、重复位置与纯空白排除；原子及必要条件/单位/例外用虚构值，原文/预期可逐字复核。
4. 用命名输入变异验证缺原子/证据、仅反证、漏/重复Coverage、自环/循环重复、伪排除、旧完整IR SHA、required漂移、内容/单位删改、空或错误表格映射、重名标题、非法路径/未来Schema/未知字段/Unicode范围及实际Windows占用/junction。expected不随变异更新，除仅为隔离规则更新IR SHA的测试明确说明。
5. 主执行者独立运行有效与失败包，核对stdout JSON、退出码、文件SHA/mtime/清单和外部哨兵；运行Rust工作区/前端/Python日志/文档链接/diff检查，独立审查后完成日志、小步提交推送并核对远端。真实输出/Vault/模型质量仍not_run。

当前规则是当前任务的可审查实现选择；它落实已定基线，不改变产品后续真实抽取、引用迁移和完整发布目标。

## 实际审查后收紧

首轮代码审查与主执行者复现发现证据缩为一个字、HTML/注释隐藏正文、原文手写wiki链接、规范位置移走定义仍误通过。修正前病例与旧expected SHA保留在独立审查证据，不将它们计为成功验收。补充上述evidence和canonical atom约束；文本显示v1拒绝原始HTML/comment、Obsidian隐藏注释、手写wiki/嵌入及未经语义声明的Markdown链接，所有段落/列表/单元格及标签/别名都受同一明文规则约束。输出路径另拒绝`#[]^`等当前不支持的字面wiki地址控制字符，case输入根仍允许正常选取的中文/空格及普通路径字符。保持数学`x < 2`等普通明文，不引入完整Markdown解析器或实际renderer。

只读JSON Schema产物为`schemas/fixture-v1.json`，与Rust输入类型和有效/非法包一起核对；Schema结构验证不代替五项交叉语义检查。
