# outline-alignment-v1：章节粗对齐与全局概念清单作者包

## 用途

验证 FR-07 结构识别与本轮新增的结构层契约的四件事：

1. 章节粗对齐只依赖标题行与标题编号，不做语义相似度、不做实体识别、不合并章节。
2. 章节编号缺陷（缺编号、重复编号、层级跳级、编号深度与父节不一致）能被按位置识别成警告，而不是被静默修正。
3. 全局概念清单只认作者包声明的词表与别名；只出现在图注或附录的概念、标题提到但正文没展开的概念都要单列。
4. 同一冻结输入的两次构建必须字节相同，且 `model_calls` 与 `network_requests` 恒为 0。

## 成员

| 成员 | 职责 |
| --- | --- |
| `author.json` | 作者包声明：policy、case_id、概念词表路径、四份文档、上限 |
| `concepts.json` | 概念词表：5 个概念与各自别名 |
| `documents/glycolysis.md` | 编号完全规范的三级章节（0 警告基线） |
| `documents/numbering-defects.md` | 缺编号、跳级、重复编号、编号深度不匹配 |
| `documents/caption-appendix.md` | 概念只出现在图注与附录；标题提到但正文没展开 |
| `documents/plain-note.md` | 完全没有标题的笔记 |
| `cases.json` | 冻结的核心序列与包边界 |
| `expected.json` | 先验判据：计数、每文档对齐数、概念索引、九条警告的位置 |
| `frozen-sha256.json` | 由审计脚本 `--freeze` 生成的成员摘要清单 |

全部资料都是为验证工程契约自造的短文本，不含任何真实教材原文、个人信息或凭据。

## 先验预期的推导依据

`expected.json` 写在实现验证之前，不从未经独立推导的产品输出回填。推导规则：

- **章节数 15** = doc-glycolysis 4（H1 + 2×H2 + 1×H3）、doc-numbering 7（H1 + H2 + H4 + 3×H2 + H2 + H3）、doc-appendix 4、doc-plain 0。
- **已对齐 10 / 未对齐 5**：只有 `numbering-defects.md` 里的五节（无编号、跳级、三个重复编号、深度不匹配的子节）算未对齐；其余已对齐。
- **概念出现 12 次**：`糖酵解净能量收支`/别名 2 次、`糖异生绕行反应`/别名 2 次、`丙酮酸脱氢酶复合体` 2 次、`三羧酸循环`/`TCA循环` 4 次、`NADPH` 2 次。词表外词汇（如"糖原"）不计。
- **警告 9 条**，七类各 1 条、`duplicate_number` 3 条：
  - `heading_without_number`：`numbering-defects.md` 第 3 行"无编号小节"；
  - `level_jump`：第 7 行 H2 → H4；
  - `duplicate_number`：第 7、11、15 行的 `4.1`；
  - `number_depth_mismatch`：第 23 行 `5.1.1` 三段，父节编号只有一段；
  - `concept_only_in_caption_or_appendix`：`caption-appendix.md` 第 3 行 NADPH；
  - `heading_concept_missing_from_body`：`caption-appendix.md` 第 13 行标题提到丙酮酸脱氢酶复合体，本节正文没有；
  - `no_headings`：`plain-note.md` 第 1 行。
- **"只出现在图注或附录的 1 个"** 指 `nadph`：它的每一次出现都在图注或附录上下文里。

## 冻结规则

- 本目录的成员一经冻结不得原地修改；需要改样例必须新建 `outline-alignment-v2`。
- `frozen-sha256.json` 只能由 `scripts/verify_outline_alignment.py --freeze` 生成，不允许手写。
- 审计脚本会核对成员集合与每个成员的字节摘要；任何漂移都判失败。

## 边界

- `scientific_status` 为 `not_run`：本包不证明任何科学正确率，也不证明真实教材的章节结构能被同样处理。
- 不联网、不调用模型、不写项目数据库、不写生产 Vault。
- 章节对齐结果只描述"标题编号与层级是否自洽"，不判断章节内容是否正确。
- 概念清单只按词表与别名做字面匹配，不具备语义相似度或实体识别能力。
