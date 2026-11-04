# 有据冲突与证据不足协调契约 v3

本文件落实 B05-T03 的 FR-10、FR-26、FR-40、NFR-03、NFR-07。沿用 [v1 来源和身份核对](KNOWLEDGE_COORDINATION_CONTRACT.md)及 [v2 条件与时间](KNOWLEDGE_VARIANTS_CONTRACT.md)。本文件描述实现目标，实际完成由任务账本和验收记录证明。

## 输入及版本

普通入口仍为 `noetome knowledge coordinate <project> <case> <new-output>`。严格 Schema3 case 的 purpose 为 `explicit_identity_conflict_review`，identity 为 Schema3，字段与 v2 相同；背景仍使用 `source_context_zh_v1`。报告及 IR scope 为 `explicit_identity_conflict_review_v3`。v1/v2 的字段、分类及严格拒绝规则保持各自版本语义。

仍限定同项目 2 至 3 份各含一个原子的成功 source_review。先重验全部九件依据、真实 SQL、资源、原文、引用及背景，再分类。缺支持、错误引用、解析/资源损坏是输入失败，不包装成合法冲突或证据不足。unknown 条件或时间是有来源依据的未知状态，保留 null，不补零或伪造已核实。

## 分类与保留

原重复、互补、不同对象、完整条件变体和有据历史先后继续保留。未知条件、无法说明先后的同来源版本、不可比单位和重叠/未知时间分别为 `insufficient_evidence`，带具体原因。各原出现独立保留未知条件问题；无关的无争议原子仍可 passed。

同对象、属性、规范单位、完整明确适用范围与例外、相同已知时间下，不相等的可比较十进制值为 `true_conflict`，代码 `COORD_TRUE_CONFLICT`。比较不使用浮点、单位换算或猜测语义。精确相等但字面不同的十进制值为 `numeric_equivalent`，保留两组原始表述；不推定有效精度相同。不能比较的值为 `insufficient_evidence/COORD_VALUE_NOT_COMPARABLE`。

两种未决关系均令涉及的原出现及事实 needs_review。第三份同值支持、输入排序、显示名和来源新旧不选择胜者。全部原概念、原子、条件、例外、背景、支持、反证和限定证据按原链接保留；出处仍包含 source/version/parse/unit/range 与实际切片。

## 处理记录、计数与保存

每个原出现恰有一份 `processing_records` 项：`occurrence_id,object_id,property_id,status,issue_codes,action,reason`。action 为 `retained` 或 `retain_for_review`，reason 说明保留或未决原因；occurrence_id 定位完整来源和证据。无自动裁决或假审阅记录。

totals 沿用 v2，新增 `true_conflicts,insufficient_evidence_relations,conflicting_occurrences,insufficient_evidence_occurrences,counter_evidence_links`。前两项数 pair，随后两项数唯一原出现，可交叠；counter_evidence_links 数原 refutes 链接。evidence_links 数全部保留链接；四状态 passed/failed/needs_review/unprocessed 相加等于实际原出现数。

未决时退出 3、canonical_facts=null、不保存协调 IR；完整 JSON/Markdown、处理记录和原件照实保存。无争议项仍列在同份报告。成功仅表示本有界工程协调通过：knowledge=pending、science=not_run、research=not_requested、published=false、网络/付费调用=0。输入守卫、最终重读、新根及碰撞保护沿用 v1。

## 验收

作者原文、旧 Gold2、显式身份与完整 expected 先冻结，再构造 fake 候选。先取得普通 CLI 新行为 RED；独立核对从作者预期、真实来源和原依据重建全部结果。同步伪造 JSON/Markdown/IR/指纹仍须检测丢反证、丢历史、多数判胜、未知填零、假核实、错误成功/失败和处理记录遗漏。运行 v1/v2 回归、v3 普通入口及独立篡改矩阵、源码稳定后的完整 Rust/Python 检查和新鲜审查。工程样例不证明通用科学裁决、任意自然语言、完整多教材/PDF 协调、真实科学准确率或生产 Vault 维护。
