# retrieval-budget-v1：批准范围内的有界检索作者包

本包是一份**独立编写、可公开的**小样本，用来验证「批准范围内的有界检索与终止理由」
这一组工程契约是否真的成立。它不访问网络、不调用任何模型、不写知识库、也不碰生产 Vault。

## 用途

对应需求 FR-11、FR-12、FR-13、FR-36、FR-38、NFR-15 与验收场景 AC-31、AC-32。本包要回答四件事：

1. **授权边界**：检索计划只能包含本轮已授权的目标与必需前提；越界主题必须留在候选队列里，不能靠检索偷偷扩进来。
2. **持久预算**：每次检索都要先落账预留（次数、字节、耗时），预留即计入消耗、用完不回收；接近上限时不能各取全部剩余额度。
3. **终止理由**：七类终止理由（目标满足、无可靠新证据、边际收益不足、预算不足、用户暂停、网络受限、需要人工判断）都能从账本里找到证据支撑，并且只写一次。
4. **断点恢复**：中断后重跑必须补结算已预留的那一条，不能重复预留、重复计费。

## 成员

| 成员 | 作用 |
| --- | --- |
| `plan.json` | 宽松预算的检索计划：8 条已授权查询、5 个冻结语料文档、关闭外部检索。 |
| `plan-tight.json` | 同一计划，把预算压到 4 条查询就耗尽，用于验证「预算不足」这一终止理由。 |
| `corpus-manifest.json` | 冻结语料清单：路径、SHA256、字节数。 |
| `corpus/*.md` | 5 个自造教材样例章节，带 `#` 标题与稳定措辞，便于确定性扫描。 |
| `reasons/*.json` | 7 个终止理由输入文件，每个只有 `reason` 与 `note`。 |
| `cases.json` | 冻结的操作序列与**先验预期**（不变量、预算消耗、终止理由可用性、单元状态）。 |
| `frozen-sha256.json` | 包内全部成员的字节指纹，由 `scripts/verify_retrieval_budget.py --freeze` 生成。 |

## 先验预期怎么来的

这些数字是**先于实现**按契约推出来的，不是从产品输出回填的：

- **查询顺序**：按 `plan.json` 的 `queries` 数组顺序执行，一条查询一个尝试。
- **`q-energy`** 命中 4 条：`corpus/chapter-glycolysis.md` 里 `ATP` 与 `净生成` 各 2 处，且 `max_hits` 为 4。
- **`q-energy-repeat`** 与 `q-energy` 同主题同词项，命中键完全重复，故 `duplicates_only`、`new_hits = 0`。
- **`q-gluconeo`** 只命中「丙酮酸羧化酶」1 处 → `evidence`。
- **`q-ppp`** 命中 `NADPH` 2 处 → `evidence`。
- **`q-glyoxylate`** 的「乙醛酸循环」在 5 个文档里都不存在 → `no_hits`。
- **`q-shuttle-carnitine`** 与 **`q-shuttle-malate`** 同属 `conflict_group: shuttle`，但命中文档集合不同 → 第二条判为 `conflict`。
- **`q-external`** 标了 `requires_external: true`，而本轮 `external_retrieval: false` → 不扫描、判 `requires_external`，对应单元落 `gapped`（待核验缺口），保留而不假装补齐。
- **预算**：`plan.json` 的 8 次预留正好用满 `max_queries=8`；每次预留按最坏情况取 `max_query_bytes=65536`、`max_query_millis=1000`，故 `bytes_used = 8 × 65536 = 524288`、`millis_used = 8 × 1000 = 8000`，剩余额度为零且不回收。
- **`plan-tight.json`**：`max_total_bytes=4096`、`max_total_millis=400`，跑到第 5 条时剩余额度已不足以再预留一次，故停在第 4 条之后，`bytes_used = 4 × 1024 = 4096`；剩余 4 条查询保持 `pending`，未完成目标不被丢弃。
- **终止理由的证据判据**：`goal_met` 要求所有 `goal_subjects` 都被「已结算且有证据」的尝试覆盖且无待执行查询；`no_reliable_new_evidence` 要求至少一次 `no_hits`；`marginal_gain_insufficient` 要求至少一次 `duplicates_only`；`budget_exhausted` 要求还有待执行查询且剩余额度不足以再预留一次；`user_paused` 要求账本里存在生效中的暂停控制；`network_restricted` 要求本轮关闭外部检索且存在 `requires_external` 查询；`manual_judgment_required` 要求存在 `conflict`。

## 冻结规则

- 本包成员一经冻结不再修改；要改行为就新建 `retrieval-budget-v2`，不原地改。
- `frozen-sha256.json` 只能由脚本重新生成：`python3 scripts/verify_retrieval_budget.py --package fixtures/retrieval-budget-v1 --freeze`。
- 账本里的 `metadata.plan` 与 `metadata.corpus` 必须与这里的 `plan.json`、`corpus-manifest.json` **逐字节规范等价**，否则视为损坏。

## 边界

`scientific_status = not_run`。语料是自造教材样例，句子为了可确定性扫描而写得刻意规整；
它不证明真实教材上的检索召回率、不证明模型摘录质量，也不证明 98% 那类内容指标。
产品在编译报告里仍如实显示研究为未启用，本包只覆盖「有界检索」这一层工程契约。
