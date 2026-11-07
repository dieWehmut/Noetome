# 相关性筛选记录（rlv-glycolysis-v1）

用户水平：university；学习目的：准备生化考试里的糖代谢计算题

## 计数

- 来源 3 份、片段 25 个（正文片段 10 个）
- 保留正文片段 4 个、排除 6 个
- 原子 6 个：保留 5、暂缓 1
- 保留条件 3 条

## 排除理由分布

| 理由 | 条数 |
| --- | --- |
| beyond_target_level | 2 |
| decoration | 1 |
| duplicate_introduction | 1 |
| irrelevant_anecdote | 1 |
| pure_exercise | 1 |

## 排除明细

| 来源 | 行 | 理由 | 说明 |
| --- | --- | --- | --- |
| doc-basics | 13-13 | duplicate_introduction | 第 2-3 行已完整给出同一结论，这里只是重复一遍。 |
| doc-anecdote | 2-3 | irrelevant_anecdote | 手摇离心机的实验室旧事与糖酵解知识无关。 |
| doc-anecdote | 6-6 | decoration | 「生命最古老的记忆」只是修辞，不承载可核验结论。 |
| doc-anecdote | 9-9 | pure_exercise | 画环式结构是纯练习形式，正文未给出答案。 |
| doc-overshoot | 2-3 | beyond_target_level | 单分子 FRET 的微秒构象集合属于前沿方法学，大学水平不需要。 |
| doc-overshoot | 6-6 | beyond_target_level | 不同实验室相反的别构常数属于未决争议，超出声明水平。 |

## 保留原子与两种密度

| 原子 | 状态 | 条件数 | 极简原子数 | 详细原子数 |
| --- | --- | --- | --- | --- |
| （合计） | - | 3 | 5 | 5 |
| atom-glycolysis-net | kept | 0 | - | - |
| atom-glycolysis-anaerobic | kept | 1 | - | - |
| atom-hexokinase-ring | kept | 0 | - | - |
| atom-magnesium | kept | 1 | - | - |
| atom-lactate | kept | 1 | - | - |
| atom-pfk-fret | deferred | 0 | - | - |

## 边界

- 两种密度的原子与条件完全一致：是。
- 排除只写进筛选记录，冻结来源的每个片段都保留在 `control/` 里，没有被删除或改写。
- 模型调用与网络请求：0 次；本阶段不判断科学正确性，只做可审计的保留/排除决策。
