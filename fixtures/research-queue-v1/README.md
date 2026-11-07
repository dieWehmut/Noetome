# research-queue-v1 作者包

本包是 Noetome 研究候选队列（B08-T01）的冻结测试资料。**全部内容为项目自造的虚构教材样例**，可公开提交，不含用户生产笔记、真实教材原文或任何凭据。

## 用途

验证 FR-11 / FR-12 / FR-13 的四件事：

1. 授权边界：`objective.authorized_targets` 与 `required_prerequisites` 冻结成本轮必做集合，越界观测只能进候选队列。
2. 候选持久化：每个候选按 FR-13 的八个字段（主题、发现原因、与当前目标关系、可能收益、来源线索、预估代价、依赖、状态）落盘。
3. 重复发现归并与拒绝记忆：同一主题再次被发现会追加证据；已拒绝主题无新理由的重提被挡回并计数，带新理由可重入。
4. 队列操作零检索零模型调用：所有动词只读写本地根与输入文件，不联网、不调用模型。

## 成员

| 文件 | 职责 |
| --- | --- |
| `author.json` | 作者包：policy、case_id、本轮目标、授权目标/必需前提、限额。`observations` 与 `decisions` 故意为空：adopt 只冻结目标，不预先入队。 |
| `round-01-observations.json` | 第 1 轮观测：2 个授权目标 + 1 个必需前提 + 2 个越界主题，全部首次入队。 |
| `round-02-decisions.json` | 第 2 轮决定：接受两个授权目标，拒绝乳酸穿梭，延后脂肪酸 β 氧化。 |
| `round-03-observations.json` | 第 3 轮观测：授权目标补充证据；乳酸穿梭**不给 revisit_reason**（应被挡回并计数）；新增两个越界主题。 |
| `round-04-observations.json` | 第 4 轮观测：乳酸穿梭带真正的 `revisit_reason`（应回到 proposed）。 |
| `round-05-decisions.json` | 第 5 轮决定：磷酸戊糖并入脂肪酸 β 氧化（merge）、糖原合成拒绝、乳酸穿梭改范围说明（revise_scope）。 |
| `cases.json` | 冻结的调用序列与先验预期。 |
| `frozen-sha256.json` | 除自身外所有成员的 SHA256 与字节长度，作为整包冻结锚。 |
| `originals/` | 被 `source_leads` 引用的自造教材样例，只作静态线索。 |

## 先验预期从哪来

`cases.json` 的 `expected` 是**按合同手算的判据**，不是从产品输出、SQL 或 report 回填的：

- `must_do` 恒等于 `author.json` 的授权目标 + 必需前提，候选无论被接受与否都不会改变它。
- `observations` = 10（5 + 4 + 1），`decisions` = 7（4 + 3）。
- `blocked_repeats` = 1：只有第 3 轮的乳酸穿梭既无 revisit_reason、也未带新理由。
- `reopenings` = 1：第 4 轮的乳酸穿梭带了与既有拒绝理由、既有证据都不同的新理由。
- `merges` = 1：磷酸戊糖并入脂肪酸 β 氧化，证据随之转移，故脂肪酸 β 氧化的证据数为 2。
- `queue_only` 是 `out_of_scope` 且未合并的候选，故磷酸戊糖不出现。

## 冻结规则

- 本包成员一经冻结不得修改；任何改动必须新建 `research-queue-v2` 并在 README 说明差异。
- `frozen-sha256.json` 需在其余成员全部定稿后由 `scripts/verify_research_queue.py --freeze` 重新生成，并在人工核对后提交。
- 引用锚点（`#net-energy` 等）只用于同文件内定位，不声明任何外部来源。

## 边界

- `scientific_status` 为 `not_run`：本包不证明事实准确率、知识召回率或正式基准通过。
- 不访问网络、不调用模型、不写生产 Vault；所有操作只发生在调用方给出的新测试根内。
- 产品当前仍把编译报告的研究维度显示为"未启用研究"；本包不改变该结论。
