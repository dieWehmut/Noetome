# 有据主动表达契约 v1

本文件定义 B06-T01 的 FR-21、FR-23、FR-25、FR-26、NFR-03、NFR-06 实施范围，实际完成以任务账本与验收原件为准。沿用[有界协调 v1](KNOWLEDGE_COORDINATION_CONTRACT.md)、[条件/时间 v2](KNOWLEDGE_VARIANTS_CONTRACT.md)和[冲突 v3](KNOWLEDGE_CONFLICTS_CONTRACT.md)。

## 输入与范围

入口为 `noetome knowledge present <project> <coordination-case> <coordination-output> <new-output>`，动词后可使用 `--` 传入以连字符开头的路径。解析参数不读写文件。只接受同一明确项目的既有 2 至 3 份单原子 source_review 及当前严格 v1/v2/v3 协调输出，不新增模型、研究或生产发布。

重新持有原 case、身份映射、全部九件×输入数的来源依据、真实 SQL/资源与保存的协调输出。独立重算原来源核对及对应版本协调，再完整比较原件、IR、JSON/Markdown、全部计数/处理记录和真实文件清单。不能只信 `status=passed` 或保存的指纹。成功保存输出精确库存为 24 或 33 件；合法未决输入为 23 或 32 件，没有成功 IR，阻断表达成功。

## 表示选择与计划

新计划使用严格 schema 1，固定 `producer=knowledge_presentation`、`scope=active_presentation_v1`、`representation_origin=deterministic_planner`，绑定实际协调 schema、IR 字节指纹、项目及本次 request_id。与 fixture v1 分开，不改变其 producer、验证枚举或发布数据库。

至少两个明确对象共享一个实际 property_id 时选择比较表；单对象或无共享属性时使用正文，保存规则、共享属性及选择理由。对象、属性、笔记与行列按稳定身份定位；显示名来自已核对的明确身份。相同属性下多条事实完整保留，不按多数、时间新旧或显示顺序选胜者，也不换算单位或产生数值判断。

计划包含对象/属性标签、完整事实组、逐对象/属性单元格与结构化笔记节点。实质标签及内容保存 concept_ids、fact_ids、occurrence_ids；原 atom_id 使用 input_id 限定。缺失单元格使用独立 missing 状态、空事实集合和“选定事实未给出”，不推定整个来源没有该知识，不填零，不与 applicability 的 not_applicable 混淆。

值、单位、完整条件、例外及有据时间均进入实际内容范围。v1 未声明的独立背景保持 null/版本范围，不补 known；v2/v3 保留背景状态和原时间精度。原子内容、概念、source/version/parse/unit/range、全部 supports/refutes/qualifies 链接与首次链接顺序的唯一证据切片进入可追溯索引；v1/v2 也从原已核对候选取得全部链接，不只复制旧 occurrence 的 supports。

## 新测试 Vault 与映射

标签只声明身份：对象标签使用空 fact_ids、完整对象 occurrence_ids 与该 concept_id；属性表头绑定完整属性出现与概念。事实单元格及段落才声明需要逐项显示的 fact_ids。概念可跨不同事实分别承担对象或属性，两个角色集合分别去重，不能据交集拒绝合法协调。

新输出本身是尚不存在的平面测试 Vault：`知识比较.md`、稳定 `对象-<object-id>.md`、`plan.json`、`presentation.json`、`presentation.md`及隐藏的原件副本。对象详情使用固定“知识明细”标题，表内链接通过结构化目标生成；复杂显示名作为转义正文，固定链接别名避免分隔符歧义。正文保留知识内容，完整出处保存在内部索引。

对象笔记的显示标题沿用来源允许的有限明文，换行以受控 `<br>`、制表符以实体保留，不额外拒绝合法身份名称。链接小节仍为固定单行“知识明细”，显示标题不参与链接目标推断。

计划与渲染映射分别保存 note_id、section_id、node_id、row_id、column_id、kind、事实/原出现/概念引用和 UTF8 半开字节范围。链接 token 范围与正文范围分开，不能借别名补足丢失单位或条件。有限 Markdown 明文按既有转义规则处理，结构换行可使用受控 `<br>`；源字面标记不能变成隐藏内容。

输入、已有目标、祖先链接/reparse、硬链接、未知文件及碰撞保护沿用持有目录/文件能力。写入后重读笔记、计划、双报告及精确清单，再复验全部来源与输入。失败仅清理本次捕获且仍可证明身份/字节的结果；原件、未知新增内容与外部哨兵保留，不递归接管旧目录。

小节定位由实际文件与 section_id 共同限定，不同对象笔记可各有 details 小节。清理优先处理已捕获双报告；某个已变更结果不能使其余未变更结果的清理提前停止，错误仍返回且变更文件保留。

## 报告与验收

双报告固定 knowledge=pending、science=not_run、research=not_requested、published=false、network_calls=paid_calls=0。成功表示有界表达预览通过，包含实际来源/协调/计划/渲染/保存检查和真实计数。合法未决协调输入返回 3，可保存无计划、无笔记的失败双报告；读取或路径准入失败在建立输出前拒绝。

作者无图原文、Gold2、身份及独立原子/单元格 expected 在调用前冻结；普通 CLI 先取得真实 RED。独立 Python 从来源和作者预期重建协调与表示，重读实际 Markdown，拒绝同步修改报告/映射/指纹后仍存在的错行列、删条件/单位/例外、丢版本/依据、缺失填零或假成功。必须实跑完整回归、路径保护、独立审查以及记录版本的干净 Obsidian 表格、正文、中文路径与表内链接。工程作者样例不证明科学准确率、通用自然语言规划或生产 Vault 发布。
