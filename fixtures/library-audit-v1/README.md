# library-audit-v1 冻结包

全库链接可用性与唯一解释位置审计的公开样本。四份**自造**笔记（不含任何真实教材原文），
实现前手写 `expected.json` 作为判据，产品输出只用于核对，不用来回填先验。

## 成员

| 文件 | 字节 |
| --- | --- |
| `cases.json` | 1047 |
| `expected.json` | 8512 |
| `gold.json` | 1682 |
| `project.json` | 872 |
| `variants/broken/gold.json` | 1682 |
| `variants/broken/project.json` | 833 |
| `variants/broken/笔记/三羧酸循环.md` | 146 |
| `variants/broken/笔记/术语表.md` | 122 |
| `variants/broken/笔记/糖异生.md` | 261 |
| `variants/broken/笔记/糖酵解.md` | 531 |
| `variants/duplicate/gold.json` | 1682 |
| `variants/duplicate/project.json` | 836 |
| `variants/duplicate/笔记/三羧酸循环.md` | 146 |
| `variants/duplicate/笔记/术语表.md` | 122 |
| `variants/duplicate/笔记/糖异生.md` | 261 |
| `variants/duplicate/笔记/糖酵解.md` | 556 |
| `variants/recall/gold.json` | 1682 |
| `variants/recall/project.json` | 833 |
| `variants/recall/笔记/三羧酸循环.md` | 116 |
| `variants/recall/笔记/术语表.md` | 122 |
| `variants/recall/笔记/糖异生.md` | 261 |
| `variants/recall/笔记/糖酵解.md` | 519 |
| `笔记/三羧酸循环.md` | 146 |
| `笔记/术语表.md` | 122 |
| `笔记/糖异生.md` | 261 |
| `笔记/糖酵解.md` | 519 |

## 先验推导

- 标题数 12：糖酵解 4（`# 糖酵解`、定义、核心机制、常见混淆）、糖异生 3、三羧酸循环 2、术语表 3。
- 链接 5 条，全部落在发布集合内：
  1. 表格单元里 `[[笔记/糖酵解#定义\|糖酵解定义]]`——转义别名，`in_table` 与 `escaped_alias` 各计 1；
  2. `[[笔记/糖异生#定义]]`——纯路径；
  3. `![[笔记/三羧酸循环#定义]]`——嵌入；
  4. `[[术语表]]`——按文件名主干解析，`basis = bare_name`；
  5. `[[笔记/糖酵解#核心机制]]`——纯路径，来自另一份笔记。
- 三个概念各有一处 `> 完整解释：<概念ID>` 标记，且标记位置都是金标准声明过的原子位置，
  因此 `duplicate_concepts = 0`、`canonical_position_mismatch = 0`。
- 六个原子＝四个关键（糖酵解定义、糖酵解核心机制、糖异生定义、三羧酸循环定义）
  加两个一般（糖酵解常见混淆、糖异生与糖酵解的关系），声明标题全部真实存在，
  所以关键召回 4/4 = 100.0%、一般召回 2/2 = 100.0%。
- 基线 `verdict = pass`；三个变体各自只引入一处结构差异：
  - `variants/broken`：把一条纯路径链接的目标章节改成不存在的小节 → `broken_link` 1 条，可用率 4/5 = 80.0%；
  - `variants/duplicate`：在核心机制下再加一处完整解释标记 → `duplicate_explanation` 1 条，
    该概念三个原子全部召回失败，关键召回 2/4 = 50.0%、一般召回 1/2 = 50.0%；
  - `variants/recall`：删掉三羧酸循环的完整解释标记 → `concept_unexplained` 1 条，
    该概念的关键原子缺失，关键召回 3/4 = 75.0%。

## 冻结规则

- `frozen-sha256.json` 由审计器 `--freeze` 生成，记录除自身外每个成员的 sha256 与字节数。
- 任何成员变化都必须重跑 `--freeze` 并重新核对先验；不许只改成员不改清单。
- 变体各自是**独立完整包**（自己的 `project.json`、`gold.json` 与四份笔记），
  不是对基线做局部覆盖：否则移动一份笔记会连带打断其他按路径写的链接，把两种缺陷混在一起。

## 边界

`scientific_status = not_run`。审计只回答"链接是否能唯一解析"和"每个概念是否恰有一处完整解释"，
不判断内容对错、不评价笔记质量；所有笔记都是为本样本自造的。
