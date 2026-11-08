# 主题阅读目标与结构模板（tpl-metabolism-v1）

同一主题按阅读目标分笔记、同类主题共用结构模板的自造声明。

## 规划规模

- 模板 1 个，主题 2 个，阅读目标 6 个（每个目标对应一份笔记）
- 概念 4 个，原子 10 个；完整解释 4 处，引用 5 处、同源嵌入 1 处
- 章节 8 个（模板外章节 0 个、省略章节 10 个）；偏差 10 条、缺口 0 条

## 阅读目标与结构

| 阅读目标 | 路径 | 章节 | 原子 | 引用 |
| --- | --- | --- | --- | --- |
| 糖酵解：总览 | 代谢/糖酵解/01-goal-glycolysis-main.md | 定位 → 反应 | 2 | 1 |
| 糖酵解：能量 | 代谢/糖酵解/02-goal-glycolysis-energy.md | 反应 | 2 | 2 |
| 糖酵解：调控 | 代谢/糖酵解/03-goal-glycolysis-regulation.md | 调控 | 1 | 0 |
| 糖异生：总览 | 代谢/糖异生/01-goal-gluconeogenesis-main.md | 定位 → 反应 | 2 | 0 |
| 糖异生：能量 | 代谢/糖异生/02-goal-gluconeogenesis-energy.md | 反应 | 2 | 2 |
| 糖异生：比较 | 代谢/糖异生/03-goal-gluconeogenesis-compare.md | 定位 | 1 | 1 |

## 唯一解释位置

| 概念 | 正名 | 完整解释笔记 | 引用数 |
| --- | --- | --- | --- |
| concept-glycolysis | 糖酵解 | 代谢/糖酵解/01-goal-glycolysis-main.md | 1 |
| concept-pfk | 磷酸果糖激酶 | 代谢/糖酵解/03-goal-glycolysis-regulation.md | 2 |
| concept-gluconeogenesis | 糖异生 | 代谢/糖异生/01-goal-gluconeogenesis-main.md | 2 |
| concept-pyruvate | 丙酮酸 | 代谢/糖异生/01-goal-gluconeogenesis-main.md | 1 |

## 偏差与理由

- section_omitted（goal-glycolysis-main#调控）：省略模板章节「调控」：该章节不适用于这个阅读目标，省略是允许的。
- section_omitted（goal-glycolysis-energy#定位）：省略模板章节「定位」：该章节不适用于这个阅读目标，省略是允许的。
- section_omitted（goal-glycolysis-energy#调控）：省略模板章节「调控」：该章节不适用于这个阅读目标，省略是允许的。
- section_omitted（goal-glycolysis-regulation#定位）：省略模板章节「定位」：该章节不适用于这个阅读目标，省略是允许的。
- section_omitted（goal-glycolysis-regulation#反应）：省略模板章节「反应」：该章节不适用于这个阅读目标，省略是允许的。
- section_omitted（goal-gluconeogenesis-main#调控）：省略模板章节「调控」：该章节不适用于这个阅读目标，省略是允许的。
- section_omitted（goal-gluconeogenesis-energy#定位）：省略模板章节「定位」：该章节不适用于这个阅读目标，省略是允许的。
- section_omitted（goal-gluconeogenesis-energy#调控）：省略模板章节「调控」：该章节不适用于这个阅读目标，省略是允许的。
- section_omitted（goal-gluconeogenesis-compare#反应）：省略模板章节「反应」：该章节不适用于这个阅读目标，省略是允许的。
- section_omitted（goal-gluconeogenesis-compare#调控）：省略模板章节「调控」：该章节不适用于这个阅读目标，省略是允许的。

## 缺口

- 缺口 0 条。

## 边界

- 模板是结构规范，不是填空表：不适用的章节应当省略，声明了就必须有原子。
- 同一类型、同一层级的主题采用同一命名与章节顺序；改动顺序或加入模板外章节必须写明理由。
- 每个概念在整份规划里恰有一处完整解释，其余笔记只写引用或同源嵌入。
- 模型调用与网络请求 0 次；不生成正文、不判断内容质量与科学正确性。
