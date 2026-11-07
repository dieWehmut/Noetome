# structure-family-v1

一份自造的作者包，用来固定「同类结构一致性与导航复习组织」这份合同（`structure-navigation-v1`）的全部判定。所有内容都是为此格式手写的，不来自任何真实教材、论文或笔记库；它只证明规则确定、拒绝明确、链接不假，不证明内容正确。

## 成员

| 成员 | 字节 | 作用 |
| --- | --- | --- |
| `family.json` | 2132 | 主作者包：主题族「糖代谢」、5 个骨架构件、3 份笔记的概念解释与引用声明 |
| `concepts.json` | 516 | 概念表（5 个概念的正名） |
| `笔记/糖酵解.md` | 624 | 骨架完全就位的一份笔记 |
| `笔记/糖异生.md` | 525 | 顺序偏离 + 可选节省略 + 一条目标缺失链接 |
| `笔记/磷酸戊糖途径.md` | 519 | 缺必填节、多余节、空节、表格内已转义别名、一条目标缺失链接 |
| `variants/未转义表格.md` | 459 | 变体用：表格单元里写了裸 `\|` 别名 |
| `family-unescaped.json` | 2131 | 变体作者包：把 note-ppp 指向 `variants/未转义表格.md` |
| `family-duplicate.json` | 2126 | 变体作者包：两份笔记同时完整解释 `concept-g6p` |
| `family-unknown-concept.json` | 2136 | 变体作者包：note-ppp 引用概念表里没有的概念 |
| `expected.json` | 6789 | **实现验证之前手写**的先验：章节判定、链接表、概念位置、三条拒绝 |
| `cases.json` | 1427 | 端到端序列：adopt → build（比对先验）→ 二次构建字节相同 → list → status → 三条拒绝 |
| `frozen-sha256.json` | — | 冻结清单（由 `--freeze` 生成，自身不记账） |

`frozen-sha256.json` 记录其余 11 个成员的摘要与字节数；成员集合与摘要任何一处变动都会被审计器的 `check_frozen` 发现。

## 先验怎么推导出来的

先验写在实现之前，规则来自 PRD 的 FR-16/FR-17/FR-18/FR-19/FR-25 与这份合同的草稿，不是从产品输出回填的：

- **章节骨架只用 H2。** `笔记/糖酵解.md` 的 5 个 H2 正是骨架的 5 节，全部就位；`笔记/糖异生.md` 的 H2 是「定义 / 条件与例外 / 核心机制 / 易混对照」，骨架里「核心机制」排在「条件与例外」之前，所以后到的那节判 `位置偏离`；`笔记/磷酸戊糖途径.md` 少了必填的「条件与例外」与可选的「易混对照」，多了骨架没声明的「延伸阅读」，而「记忆提示」标题在、正文里一行内容都没有，所以判 `空节`——空节优先于错位判定。
- **标题数不是 H2 数。** `heading_count` 数全部标题行：糖酵解 7（H1 + 5 个 H2 + `### 补充说明`）、糖异生 6、磷酸戊糖途径 6。骨架比对仍然只看 H2，这一点在 `derivation` 里写死了，免得后来者把两个口径混成一个。
- **链接先解析目标，再查标题。** 正文里 6 条 wikilink：`笔记/磷酸戊糖途径.md#核心机制`、`笔记/糖酵解.md#定义`、`笔记/糖酵解.md#核心机制\|糖酵解核心机制`（表格内已转义）、`笔记/糖异生.md#四步绕行` 四条能解析；`[[补充说明]]` 与 `[[笔记/不存在\|某笔记]]` 的目标不在本族声明里，判 `target_missing`。表格内裸 `|` 不是「未转义的链接」，而是会截断单元格的硬错误，整包拒绝。
- **同一概念只能有一处完整解释。** `concept-g6p` 由 note-glycolysis 解释、被另外两份引用；`concept-bypass` 由 note-gluconeogenesis 解释；`concept-nadph` 由 note-ppp 解释；`concept-pfk` 没有任何笔记完整解释，note-ppp 只是引用它——这种情况下不能凭引用生成链接，所以它是 `外部引用`，不进生成链接。
- **生成链接由确定性规则算出来。** 3 份笔记导航到彼此 = 6 条，加上 3 条概念链接（note-gluconeogenesis → 糖酵解、note-ppp → 糖酵解、note-ppp → 糖异生）= 9 条，断链 0。
- **计数逐项可复算。** 章节计数 `notes 3 / skeleton 5 / sections 13 / matched 10 / misordered 1 / extra 1 / missing_required 1 / omitted_optional 2 / empty 1 / consistent_notes 1`；13 = 5 + 4 + 4，一致 1 份即糖酵解。

## 三个拒绝变体

每个变体只改动一处，用来证明单条规则会整包拒绝而不是悄悄降级：

| 变体 | 触发 | 错误码 | 退出码 |
| --- | --- | --- | --- |
| `family-unescaped.json` | 表格单元里裸 `\|` 当别名 | `STRUCT_LINK_UNESCAPED_ALIAS` | 3 |
| `family-duplicate.json` | 同一概念被两份笔记完整解释 | `STRUCT_DUPLICATE_EXPLANATION` | 3 |
| `family-unknown-concept.json` | 用了概念表里没有的概念 | `STRUCT_UNKNOWN_CONCEPT` | 3 |

## 冻结规则

```bash
python3 scripts/verify_structure_navigation.py --package fixtures/structure-family-v1 --freeze
```

`--freeze` 重写 `frozen-sha256.json`，记录每个成员的 SHA256 与字节数；重复运行必须逐字节相同（测试 `test_freeze_rewrites_the_same_bytes` 覆盖）。改任何成员都要同时更新 `expected.json` 的理由并重新冻结，否则审计会失败——这是有意的：先验不能跟着产品漂。

## 边界

- `scientific_status: not_run`：笔记正文是为了测试章节骨架和链接解析手写的短句，没有经过任何科学审校，也不能当成生化知识。
- 不含网络访问、不含模型调用：三份笔记与全部声明都是本地文件。
- 只覆盖结构一致性与链接：不判断内容质量、不判断事实正确性、不做语义等价判断，也不改写任何笔记正文。
- 只覆盖本主题族：链接只在本族声明的 3 份笔记内解析，跨库链接一律算 `target_missing`。
- 骨架键必须是 ASCII 标识符（`definition`/`mechanism`/…），骨架标题才是中文；这是产品的校验规则，不是 fixture 的取舍。
