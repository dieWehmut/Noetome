# 章节对齐与全局概念清单（outl-glycolysis-v1）

## 章节对齐

- 文档 4 份；章节 15 个；已对齐 10、未对齐 5。

| 文档 | 标题 | 章节 | 已对齐 | 未对齐 |
| --- | --- | --- | --- | --- |
| doc-glycolysis | 糖代谢（编号规范） | 4 | 4 | 0 |
| doc-numbering | 糖代谢补充（编号有缺陷） | 7 | 2 | 5 |
| doc-appendix | 术语与图注（只出现在图注或附录） | 4 | 4 | 0 |
| doc-plain | 无标题笔记 | 0 | 0 | 0 |

## 全局概念清单

- 概念 5 个；出现 12 次。

| 概念 | 正名 | 出现次数 | 文档数 | 首次出现 | 图注或附录 |
| --- | --- | --- | --- | --- | --- |
| glycolysis-net | 糖酵解净能量收支 | 2 | 1 | doc-glycolysis:5 | 0 |
| gluconeogenesis-bypass | 糖异生绕行反应 | 2 | 1 | doc-glycolysis:11 | 0 |
| pdc | 丙酮酸脱氢酶复合体 | 2 | 2 | doc-numbering:9 | 0 |
| tca | 三羧酸循环 | 4 | 2 | doc-numbering:5 | 0 |
| nadph | NADPH | 2 | 1 | doc-appendix:3 | 2 |

## 对齐警告

- concept_only_in_caption_or_appendix：1 条
- duplicate_number：3 条
- heading_concept_missing_from_body：1 条
- heading_without_number：1 条
- level_jump：1 条
- no_headings：1 条
- number_depth_mismatch：1 条

| 类型 | 文档 | 行 | 说明 |
| --- | --- | --- | --- |
| concept_only_in_caption_or_appendix | doc-appendix | 3 | 概念 nadph 只出现在图注或附录里，正文没有展开。 |
| heading_concept_missing_from_body | doc-appendix | 13 | 标题提到概念 pdc，但本节正文没有再次出现：标题与正文不一致。 |
| heading_without_number | doc-numbering | 3 | 章节「无编号小节」没有编号。 |
| duplicate_number | doc-numbering | 7 | 编号 4.1 在同一文档内出现多次。 |
| level_jump | doc-numbering | 7 | 标题层级从 H2 跳到 H4：父节 无编号小节 下缺少中间层。 |
| duplicate_number | doc-numbering | 11 | 编号 4.1 在同一文档内出现多次。 |
| duplicate_number | doc-numbering | 15 | 编号 4.1 在同一文档内出现多次。 |
| number_depth_mismatch | doc-numbering | 23 | 编号 5.1.1 有 3 段，但父节编号只有 1 段（期望 2 段）。 |
| no_headings | doc-plain | 1 | 文档没有任何标题：无法做章节粗对齐，内容全部留在根节点外。 |

## 边界

- 本报告由冻结文档与已登记 Span 确定性推导，零模型调用、零网络请求。
- 章节身份来自标题编号；概念身份来自作者包声明的词表与别名，不做语义相似度或实体识别。
- 未在词表中的词不会进入清单；警告只描述位置问题，不修改来源、不生成知识。
