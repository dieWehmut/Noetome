# 条件与来源时间协调契约 v2

本文件落实 B05-T02 的 FR-09、FR-10、FR-26、FR-40 和 NFR-03。入口仍为 `noetome knowledge coordinate <project> <case> <new-output>`。这是 [v1 协调契约](KNOWLEDGE_COORDINATION_CONTRACT.md) 的显式版本扩展；v1 case、映射、报告、IR、错误及既有 B04 gold/source_review IR 保持原有严格规则。完成状态由 [账本](../../dev-log/tasks.json) 和 [实际验收](../validation/B05-T02.md) 记录。

## 范围与兼容

只消费同一实际项目内 2 至 3 份成功、单原子的 source_review 九件依据，沿用原 SourceSession、完整重建、身份映射和最终保存守卫。新字段只进入独立协调 Schema2，不写回旧来源核对或 gold。v1 拒绝混入 v2 字段，v2 不默认为缺失字段填值，未知版本仍拒绝。owner 协议独立保持 Schema1。

成功范围固定为 `explicit_identity_condition_time_v2`，case purpose 固定为 `explicit_identity_condition_time`。知识 `pending`、科学 `not_run`、研究 `not_requested`、`published=false`、网络及付费调用为实际 0。成功只证明有界结构、明确作者身份与实际来源匹配，不证明自然语言通用理解、科学演变、来源权威或单位等价。

## 严格输入及背景声明

case 严格字段与 v1 相同，`schema_version=2`，purpose 使用上述新固定值。每份 input 和九件依据完全沿用 v1。映射严格字段为 `schema_version:2,author,concepts,concept_mappings,atom_mappings,atom_contexts`；原四类身份字段使用 v1 的全部角色、集合唯一性和限额规则。

每个 `atom_contexts` 恰好对应一个实际输入原子，严格字段为 `input_id,original_atom_id,applicability_status,time,context_evidence_ids`。禁止缺失、重复、未知、跨 input 借用和未使用项，数组最多 1024 项。`applicability_status` 仅允许 `known,unknown,not_applicable`。`context_evidence_ids` 是最多 1024 个有界非空原支持证据 ID，必须非空、唯一，必须恰好包含该原子全部能覆盖背景与事实范围的 supports 证据；其他 supports 也原样保留。

`time` 每处严格为 `{status,precision,value}`，三个键必须出现。unknown 时 `precision=null,value=null`；known 时 precision 为 `year` 或 `day`，value 为四位 ASCII 年份 `0001..9999` 或有效公历 `YYYY-MM-DD`（含闰年检查）。禁止通过 mtime、文件名、导入/响应/运行时间或默认 0 推造 time。

本阶段实际来源使用 `source_context_zh_v1` 有界背景语法，背景必须紧邻、直接前置于原 gold.quote：

```text
适用条件：已列明；观点时间：2020年。
样例 A 的速度为 8 m/s，条件为温度 20 ℃且仅适用成年样例 A；例外为结冰时不适用。
```

适用条件值只允许 `已列明`、`未给出`、`不适用`，对应上述三个状态；观点时间只允许 `未给出`、`YYYY年` 或 `YYYY-MM-DD`。背景句以中文句号结束，随后恰一个 LF，再接原 gold.quote。原 gold.quote 在所选实际 unit 中必须恰好出现一次；背景句必须是完整一行，不能借用不相邻段落或另一个事实的时间。不同位置前缀及实际 UTF8 字节范围照实保留。背景与 gold.quote 的联合范围必须被至少一条原 supports 完整覆盖，引用 ID 集合须与作者声明一致；不会把任意证据字符串当作背景依据。

背景状态、时间逐项从 SourceSession 的实际 unit.text 重建后与作者声明比较。known 条件必须有非空、已重新核对的完整原 gold.conditions；unknown 与 not_applicable 的原 conditions 必须为空，但两个状态不相等。原条件与例外全量保留，不能用修改 gold、删例外或扩大旧受控语法取得通过。没有完整背景或对应原支持、声明与实际背景不一致、无效日期等返回 `COORD_CONTEXT/3`，不产生成功 IR；错误类型、版本、字段及枚举仍为 `COORD_SCHEMA/2`。

## 输出、身份和事实键

v2 occurrence 在 v1 的全部字段外新增必填 `context`，严格字段为 `applicability_status,time,evidence_ids,unit_id,start_byte,end_byte,quote,sha256`。context.quote 为实际背景和原 gold.quote 的联合切片；范围是所选实际 unit 的绝对 UTF8 byte 范围；sha256 是其原字节。所有 evidence_ids 指向该 occurrence.original_evidence 中的原 supports。原概念、原子、链接顺序及全部支持依据不改写。

v2 fact 在 v1 的全部字段外新增必填 `applicability_status,time`，采用重建后的原背景。稳定键为紧凑 JSON 数组：

```text
["coordination_v2",object_id,property_id,normalized_value,normalized_unit,
 applicability_status,sorted_normalized_conditions,sorted_normalized_exceptions,
 [time.status,time.precision,time.value],unknown_condition_occurrence_id_or_null]
```

时间在键中使用三元素数组，避免不同 JSON 实现的对象键序影响 ID；wire time 仍是上述严格对象。规范化仍只去 Unicode 空白，不数值化、大小写折叠或单位换算。fact_id 为 `fact-v2-` 加上述 UTF8 JSON 的 SHA256。最后一项仅在条件 unknown 时填实际 occurrence_id，其他状态为 null；所以两个未知条件不会被当作相等空集合归并。已明确 not_applicable 的同键事实可去重，仍保留每个来源。

occurrence_id 沿用 v1 的实际来源/版本/parse/unit/range/原 atom ID 键，显示名、input 顺序及背景证据数组顺序不参与身份。输入与映射数组重排不得改变分组、原出现、出处及实际计数。

原子身份由 `input_id` 与 `original_atom_id` 配对，不要求不同输入的原ID全局唯一。作者expected的每组 `input_ids` 与 `original_atom_ids` 必须等长逐项对应，前者保持唯一，后者允许不同输入重复同一原ID；每个原ID仍按对应实际输入核对，不能为通过校验改名或丢弃第二来源。

## 并列关系和未决状态

每对 occurrence 恰一条关系。v2 relation 在 v1 字段外新增必填可空 `earlier_occurrence_id,later_occurrence_id`。只有 `historical_evolution` 两者非空并指向该对原出现，其他关系两者必须 null。历史顺序只表示背景先后，不能选出权威或覆盖旧值。

按下列明确顺序分类：

1. 任一条件 unknown 时为 needs_review，问题 `COORD_UNKNOWN_CONDITION`。每个 unknown occurrence 自身需要复核，即使另一对象/属性独立。
2. 同实际 source_id 的不同 version_id 或 parse_id，若不能由两个已知且不重叠的时间范围证明先后，为 needs_review，问题 `COORD_UNHANDLED_VERSION`。
3. 对象不同为 separate_object；同对象不同属性为 complement。
4. 同对象/属性但规范单位不同，为 needs_review，问题 `COORD_UNIT_NOT_COMPARABLE`，即使温度或时间也不同。不进行推测换算。
5. 完整 v2 键相同为 duplicate。
6. 两者条件都已明确，适用三元组（状态、完整条件、完整例外）不同，为 condition_variant。保留两份数值、单位和背景；时间也不同的事实仍显示各自时间，关系先描述条件差异。
7. 相同完整适用范围和单位，两个 time 已知且范围不重叠，为 historical_evolution。year 范围是该年 1 月 1 日至 12 月 31 日，day 是该日；使用真实公历先后，不将重叠的 year/day 任意排序。
8. 其余差异为 needs_review；time 不同但不能证明先后用 `COORD_TIME_NOT_COMPARABLE`，其他差异用 `COORD_UNHANDLED_DIFFERENCE`。同条件同时间的矛盾值不在本项自动裁决。

牵涉未决关系的双方保留所有 issue_codes，去重排序；事实状态由其实际原出现决定。报告 status=needs_review 时 canonical_facts=null、退出 3、不保存协调 IR，完整双报告和来源原件仍保存。成功时不择新弃旧，所有并列事实均进入未发布 IR。

## 分母与双报告

报告和 IR 的字段以 v1 为基线，schema_version=2、scope 使用新固定值，IR revision 仍为 1。occurrences/facts/relations 使用上述严格 v2 DTO。totals 保留 v1 全部真实字段，另必填 `condition_variants,historical_relations,known_condition_occurrences,unknown_condition_occurrences,not_applicable_condition_occurrences,dated_occurrences,undated_occurrences,context_evidence_links`。

前两项是实际 pair 关系数量，不是事实或科学样本数；条件三分区和时间两分区分别相加等于 source_occurrences；context_evidence_links 为各原出现的实际唯一背景支持链接总数。原 evidence_links 包含所有原 supports。input_atoms/source_occurrences 四状态分区、重复出现、独立 source_id 和可空 canonical_facts 沿用 v1。实际无已知日期数量为 0 不等于把未知时间值写为 0。

仅成功保存 `coordination-ir.json`；原输入副本、九件依据平铺副本、owner、完整 JSON 和 Markdown 及 artifact 指纹沿用 v1 的保存范围、碰撞/链接/限额/最终守卫和仅撤去当前捕获结果的失败规则。v2 Markdown 显示三类条件及两类时间的实际数量、并列条件/历史关系数量、pending/not_run/not_requested，再嵌入完整同事实 JSON。JSON、Markdown、stdout 与成功 IR 的对应结构必须一致。

## 作者预期和验收

[新作者样例](../../fixtures/knowledge-coordination-v2/README.md) 在构造候选前冻结来源原文、旧 Gold2、v2 显式映射与独立 expected。条件温度、样例适用范围和例外不能减少；年份、有效日期、缺条件、不适用、缺时间、重叠时间、不同单位和同来源版本有独立期望。保留冻结指纹、第一次普通 CLI 行为 RED 和后续 GREEN 原件，不以编译或准备失败代替行为 RED。

独立 Python 从实际只读 SQL/资源、作者原文/expected、原九件来源核对重建全部背景、事实键、先后关系和计数，不能信任 Rust passed/计数或从结果补写作者 expected。普通 CLI 与同步修改 JSON/Markdown/IR/artifact 指纹的 coherent forgery 都覆盖删除/改写条件和例外、丢背景支持、伪造来源时间、重叠时间强排、不同单位误归并、unknown/not_applicable 混淆、默认零、遗漏原出现和身份/版本误配。v1 和 B04 回归保持，完整 Rust/Python、日志/Git、新鲜任务及最终审查通过后按用户授权普通推送。未实测真实科学/任意语义/自动换算/完整 PDF 协调与产品规模性能保持未完成。
