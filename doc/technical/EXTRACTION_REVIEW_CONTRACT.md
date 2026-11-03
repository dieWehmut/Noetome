# B04-T03 有界来源知识核对契约

本契约为已保存的API候选增加离线 `extract verify` 入口。它只核对明确选中的一个来源单位和一条作者测量基准，输出新的测试根；不改来源项目SQL、原候选、调用响应、固定gold或生产Vault。本项关联FR-02/04/09/35/40和NFR-03/07/08，不能充当正式200原子、98%准确率、人工科学评审或完整知识覆盖。

## 1 输入及来源

命令为 `noetome extract verify <project> <import-id> <review-case> <new-output>`，支持在路径参数之前加 `--`，参数解析不访问文件。核对过程不读取模型配置、凭据、累计预算，不发网络请求。复用 `SourceSession` 取得实际文本Span或PDF区域、SourceBinding、解析状态与资源内UTF8范围。来源/parse/manifest的身份及SHA全部精确相等，同SHA跨来源不得互换。

`review-case` 是已存在的普通目录。仅显式读取四份文件：`review.json` 和它声明的candidate/gold/decision文件，不扫描其他私人文件内容。所有输入以同一有界读取句柄和父目录身份守卫持有到最后重查，拒绝链接、硬链接、重复身份、文件变化和输出与来源/输入重叠。

`review.json` Schema1字段全部必填：

```json
{
  "schema_version": 1,
  "purpose": "single_measurement_source_review",
  "selection": [{"kind": "text", "ordinal": 0}],
  "candidate_file": "candidates.json",
  "gold_file": "gold-before.json",
  "gold_sha256": "实际gold原始字节SHA256",
  "decision_file": "decision.json",
  "subject": "样例 A",
  "property": "速度",
  "allowed_contexts": ["在本夹具的样例 A 中"],
  "reviewer": "automated_source_review",
  "interpretation_timing": "after_model_response"
}
```

文件名限普通单层相对文件名；三个输入互不相同，也不能指向review.json。selection恰一项。候选输入最多1 MiB，gold/review/decision各最多64 KiB，整体各类候选实体最多1024。subject/property/context是非空明文，条件/例外每类最多8项，每项最多1024 UTF8字节。固定超时30秒，输入来源最多1 MiB。未知/重复/多余/缺失字段、尾随JSON、未知版本/枚举必须拒绝。interpretation_timing只允许after_model_response或before_synthetic_response；后者仅用于明确local_http_test工程输入的调用前作者规则。

candidate原样读取T02保存的包装：schema_version=1、producer=api、transport_mode=live或local_http_test、request_id、binding、payload。payload继续使用未放宽的ApiCandidates。外层和payload的request/binding及实际SourceSession必须一致；候选保持required=false、verification=unverified和正revision。synthetic local_http_test只标记工程输入，不能冒充真实模型。

gold Schema1原封使用调用前的 `gold-before.json`：schema_version、author、producer=fake、scientific_quality=not_run、concept_name、atom_content、value、unit、conditions、quote、verification=unverified、required、knowledge_status=pending、published=false。Schema1表示这份已声明基准无单独例外项，不能改文件添加字段。为独立作者例外/未知值工程样例允许gold Schema2：保持上述字段，另必填exceptions数组与value_status（known/unknown）；value和unit必填可空。已知数值必须非空，未知数值必须null并保持需复核。两个gold版本分别严格解析，不默认为缺失字段填值。

decision Schema1仅有schema_version、unit_id、disposition（included/excluded/needs_review/pending）、atom_ids、exclusion_reason（必填可空）。这份建议映射与gold独立：已纳入的一条基准必须映射恰一个实际候选；required=false不等于允许排除。excluded必须有明文理由，但本契约的一条测量基准没有允许排除的情况；有理由也不能把知识遗漏变成正确排除。pending/needs_review如实记未处理/需复核，不产生已通过IR。

## 2 判据及当前真实候选

原gold先存在于模型调用之前，事实、值、单位、条件及quote只来自它；新解释规则是调用后的自动来源核对，报告必须明示此时间和主体，不能称为新的调用前人工金标准。gold的实际字节SHA钉在review.json中，保存副本也逐字节相同。额外上下文与概念组合规则不会改写gold或候选。

每项gold事实先与真实所选来源核对：quote必须是实际UTF8文本内的非空连续切片，value/unit/每个条件和例外出现在该quote内；concept_name经仅去空白处理后必须等于subject+property。subject/property应被原文支持。显式允许的context必须逐字出现在原quote中，去空白后严格等于“在本夹具的subject中”，只提供该subject的重复范围；不允许任意来源片段充当新增条件。基准内容和实际quote自身也必须符合下述同一受控语法、完整条件/例外与精确值绑定，否则输入基准无效，不把它用于评分；不能凭value10是source110的子串就通过。仅unknown作者样例的原quote另允许声明context加“，property未知”，候选正文仍须subject的property未知。

概念组允许一个已关联的compound concept等于gold.concept_name，或同一原子的关联概念/别名分别完整提供subject和property。额外概念/别名必须来自支持证据中的明文，并且关联合法。计划名称存在但原子未关联时失败。当前第六候选的“样例 A”与“速度”关联组合符合原gold“样例 A 速度”；“夹具”和“温度”只作为来源支持的附加概念记录。

数值、单位、条件分别核对。measurement.conditions必须完整保留gold条件，允许额外出现review明确声明的同subject上下文；除此以外的额外条件失败。qualifiers必须包含每个条件、例外和单位，只允许同一声明上下文。集合不得重复。真实第六候选多出的“在本夹具的样例 A 中”因此须单独显示为允许上下文，而不能使两个必要条件消失或强行修改原gold。

事实正文采用显式 `controlled_measurement_zh_v1` 语法：仅去Unicode空白，fact为`subject的property为valueunit`，或`allowed_context，property为valueunit`；随后为条件段（分隔符逗号/中文逗号/分号/中文分号/句号，前缀“条件为”或“条件是”），条件之间只接受“且”、“、”、“，”及“，且”；可选末尾中文句号。Schema2非空例外另需“；例外为”段，以同样条件连接符逐项列出。条件/例外比较完整集合，不接受重复、遗漏、额外断言、否定或数值变更。未知值对应value_status=unknown、value=null、正文“subject的property未知”，不自动判为已知事实。超出这份语法的自然语言视为核对失败并指出需要复核，不能据此声称一般科学错误。本项不是通用自然语言推理器。

每条事实至少一份有效supports证据，证据必须精确绑定实际unit/range/quote/SHA且包含gold quote对应的事实范围。只引用来源末尾工程声明、改写quote或正确quote配错误事实都失败。共用严格IR规则仍覆盖引用、实体ID/revision/明文/已知与未知测量；不通过减少gold、改原文、填空值或放宽ApiCandidates修复失败。

## 3 输出、分母和保护

新根owner明确producer=source_review、purpose=unpublished_source_review。原candidate、gold、review、decision的字节副本以固定输出文件名保存，来源快照保存实际binding与完整所选units。JSON/Markdown核对报告包含origin、规则/主体/解释时间、实际输入指纹、来源范围、逐对象错误、核对项目、分母、知识状态、scientific_quality=not_run、research_status=not_requested、published=false、network_calls=0及实际产物指纹。Markdown嵌入与JSON相同的完整事实快照，两份都保存重读。

expected_atoms固定1；passed/failed/needs_review/unprocessed互斥且相加等于1。candidate_atoms来自实际候选长度；未预期及语义重复的候选单独计数定位，不藏入分母。非法或重复JSON键使实际数量不可确认时保留null，不能用Value的最后同名键恢复为零。条件/单位/例外等类别的分母来自gold实际条目；空例外分母0且not_applicable，不能宣称例外正确率100%。实际已核对的类别失败不被整体pending/needs_review状态改为未处理或复核；首候选的质量按输入数组位置取其实际检查，不能因重复ID借用其他候选的通过项。额外源支持概念数与必要概念组数量分别记录。排除审查来自实际decision，本范围excluded恒不通过；遗漏为unprocessed并返回失败。源needs_review或unknown值不能升级为passed。

只有来源/严格候选/事实/覆盖全部通过且无review/pending，才写 `knowledge-ir.json`。IR使用独立Schema1、producer=source_review、revision=1、原请求/来源绑定、原概念/原子/证据及核对记录；不冒充fixture IR或持久Run/Release。原子verification仍unverified、required=false，IR的source_match=passed只表示本范围基准与来源一致。IR保存重读后核对原内容和指纹；失败报告存在时不得残留成功IR。

输入和来源最终守卫必须在报告与IR保存之后再次核对。出现变化只删除当前句柄捕获的IR/报告结果，保留原输入副本和未知现场；不枚举删除未知文件，不递归删除失败目录。已有输出根、嵌入来源/输入的输出、链接/硬链接、输出未知文件/身份变化均拒绝。测试用内部阶段回调不暴露到普通CLI或环境变量。

## 4 必须实际验证

先保存未接线CLI的真实RED，再覆盖删除条件/单位/例外、数值及否定事实变更、正确quote配错误事实、只引无关文本、UTF8/引文SHA/来源或parse版本误配、未关联概念、遗漏和新ID语义重复原子、错误排除、unknown与源needs_review。RED设置错误和产品失败分别记录。例外/未知/非零Span/PDF只用清楚标记的独立工程fixture，不改真实第六原响应或gold。

独立Python验证器不导入Rust校验器或渲染器，不信任报告自己的readonly/数量/通过声称：直接读取只读SQLite、实际资源/候选/gold、计算SHA/UTF8切片，重建必要事实、分母与双报告；包含来源/输入/新输出保护的真实普通CLI故障矩阵。真实第六候选直接使用原live根，原gold/source/六次响应/模型服务预算前后逐项相等；本任务付费调用0。保存失败、修复及新鲜审查，运行实际Rust/Python/日志/diff检查后才complete。
