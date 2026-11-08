# 全库链接可用性与唯一解释位置审计合同

本文定义 `noetome audit` 的可复算口径，对应需求 FR-17（唯一规范解释位置）、NFR-05（内部链接唯一解析、本轮发布范围内有效率 100%、断链零）、NFR-02（金标准原子召回：关键原子 100%、一般原子至少 98%）与验收 AC-17（同一概念被多个主题引用时仅一处完整解释，其余为引用或同源嵌入）。

审计只回答结构问题：一条链接能否唯一解析到目标位置、一个概念在发布集合里是否恰有一处完整解释、金标准里声明过的原子能否在声明的标题下找到。它不判断内容对不对，也不判断讲解好不好。

## 1 边界

- 只读：`adopt` 冻结输入，`build` 写一个新的输出根，`list`/`status` 只重算不写盘。
- 不联网、不调用模型：报告的 `model_calls` 与 `network_requests` 恒为 0，审计器逐项核对这两个字段。
- 不写项目、不写 Vault：输出根只含派生报告。
- **不判断内容质量与科学正确性**；"链接可用"不等于"引用得当"，"召回"只表示声明过的原子位置存在。
- 有缺口时仍然写出完整报告，只把退出码置为 3——审计的产出就是结论本身，把缺口藏进拒绝里，缺口就永远不会被逐条列名。

## 2 工程声明与金标准

工程声明 `project.json`（13 字段以内，全部 `deny_unknown_fields`）：

| 字段 | 含义 |
| --- | --- |
| `policy` / `schema_version` / `producer` | 固定为 `library-audit-v1` / `1` / `fixture_author` |
| `case_id` / `description` | 案例标识与用途说明 |
| `gold` | 金标准清单的相对路径 |
| `notes[]` | `{note_id, path, title}`，`path` 必须以 `.md` 结尾且在包根下 |
| `limits` | `max_notes` 1..=16、`max_atoms` 1..=512、`max_links` 1..=4096 |

金标准 `gold.json`（policy `library-gold-v1`）：`concepts[]` 为 `{concept_id, canonical}`，`atoms[]` 为 `{atom_id, concept_id, key, note_id, heading}`。`key` 标记关键原子；每个原子必须指向已声明的 `note_id`，`heading` 必须是该笔记里真实存在的标题。

## 3 冻结根布局

```
<审计根>/
  control/
    gold.json          金标准原文（逐字节）
    inputs.json        规范化 JSON，无尾换行
    manifest.json      规范化 JSON，无尾换行
    notes/             每份笔记一份冻结副本（逐字节）
    project.json       工程声明原文（逐字节，保留来源缩进）
```

- 顶层只允许 `control/`；`control/` 恰好这五项。
- `manifest.json` 15 个字段，含六处 identity（`root_identity`/`parent_identity`/`control_identity`/`manifest_identity`/`notes_identity`/`inputs_identity`）与三份摘要（`project_sha256`/`gold_sha256`/`inputs_sha256`）。
- 打开时逐项复核：字段集合、policy/producer/schema、`root` 等于现场规范化路径、六处 identity、三份摘要、`inputs.notes` 每条的 `sha256` 与 `byte_length`、冻结副本集合与清单集合完全相等（禁止符号链接）。
- `adopt` 先按全部规则预演一次判定（只拦声明错误），任何一条不过就不建根；建根失败且根身份未变时删除半成品。

## 4 标题与章节归属

- 标题判定必须与 `crates/noetome-cli/src/text.rs` 的 `heading()` 一致：缩进不超过 3 个空格、`#` 数 1..=6、`#` 之后必须是行尾或空白。`#没有空格` 不是标题。
- **一个标题拥有从它的行到下一个标题前一行的全部内容**（包括空行与正文）。只把标题行算作本节，会让"标题提到但正文没讲"永远不可证伪，也会让标记与链接的归属错位。

## 5 完整解释标记

- 完整解释只认行首的显式标记：`> 完整解释：<概念ID>`。散文里出现"完整解释"四个字不算声明。
- 标记归属"最后一个 `line ≤ 标记行 ≤ end_line` 的标题"。
- 标记里的概念 ID 必须是金标准已登记的概念，否则整包拒绝（`AUDIT_ARGUMENT`）。
- 同一概念出现两处及以上标记即记 `duplicate_explanation`；一处都没有记 `concept_unexplained`；两者都判 findings。

## 6 链接解析

- 识别 `[[目标]]`、`[[目标#标题]]`、`[[目标|别名]]`、`![[目标]]`（嵌入）。
- 别名分隔符优先找 `\|`（表格里必须转义）；表格行（`trim_start` 后以 `|` 开头）里别名前的竖线没有转义即整包拒绝——否则表格单元会被提前切断。
- 目标去掉尾部 `.md`；标题取 `#` 之后非空部分。
- 笔记解析顺序：先按路径、去 `.md` 的路径或 `note_id` 精确匹配（`exact_path`）；否则若目标不含 `/`，按文件名主干匹配，唯一命中记 `bare_name`，多个命中记 `duplicate_note_name`（歧义），零个记 `path_missing`（断链）。
- 标题解析：**只有恰好一个同名标题才算可用**。零个记 `heading_missing`（断链），多个记 `duplicate_heading`（歧义）。取第一个匹配会让"同名标题"这类真实歧义悄无声息地通过。
- 判定三态：`resolved` / `broken` / `ambiguous`。断链与歧义都判 findings，`links_resolved / links` 即可用率；NFR-05 要求发布范围内可用率 100%。

## 7 原子召回与唯一解释位置

- 召回条件＝概念恰有一处完整解释标记，**且**该原子声明的标题在它声明的笔记里真实存在。原子的位置是"这条知识应当落在哪里"，不必等于概念的完整解释位置。
- 完整解释标记还必须落在金标准声明过原子的位置上（同 `concept_id` + `note_id` + `heading`），否则记 `canonical_position_mismatch`——否则"随便找个地方宣布一句话"就能让唯一性检查通过。
- 关键原子必须 100% 召回；一般原子允许低于 100%，但不得低于 98%（`GENERAL_RECALL_PERCENT`）；不达标记 `recall_below_threshold`。
- 缺口逐条列名（`atom_missing` / `concept_unexplained` / `duplicate_explanation`），不用百分比掩盖。

## 8 口径与不适用

每项比率都是 `Ratio{numerator, denominator, applicable, display, note}`：分母为 0 时 `applicable=false`、`display="不适用"`，不得写成 0% 或 100%。报告固定给出三个比率（链接可用、关键原子召回、一般原子召回）与 20 项计数。

## 9 输出根与字节确定性

输出根恰好五个文件：`links.json`、`explanations.json`、`report.json`、`report.md`、`manifest.json`。

- 输出清单 12 个字段，**不含 `created_at`**：同一冻结输入的两次构建必须字节相同。
- `report.md` 固定六节：结论、口径、链接明细、唯一解释位置、缺口、边界。
- 有缺口时五个文件照常写全，退出码 3；只有声明错误（未知标记概念、表格别名未转义、链接超上限）才在写盘前拒绝。

## 10 CLI 动词与错误码

```
noetome audit adopt <工程声明> <审计根>
noetome audit build <审计根> <输出根>
noetome audit list <审计根>
noetome audit status <审计根>
```

| 错误码 | 退出码 | 含义 |
| --- | --- | --- |
| `AUDIT_ARGUMENT` | 2 | 参数或声明不合法 |
| `AUDIT_INTEGRITY` | 3 | 冻结根与清单不一致、被篡改或目录不可访问 |
| `AUDIT_UNKNOWN_ROOT` | 3 | 目标不是已建立的审计根（陌生目录保持只读） |
| `AUDIT_FINDINGS` | 3 | 审计完成但存在缺口，报告已写出并逐条列名 |

## 11 本轮明确不做

1. 不做语义链接：只认路径、文件名与标题，不做别名推断或向量相似度。
2. 不判断内容正确性、讲解质量与引用是否得当。
3. 不自动修复链接或移动文件。
4. 不审计 Vault 之外的外部链接与网络可达性。
5. 不做跨语言路径归一化（只按字面与组件比较）。
6. 不生成知识、不合并笔记、不改写正文。
7. 不接入 GUI，也不做增量审计（每次针对一个冻结的发布集合）。
