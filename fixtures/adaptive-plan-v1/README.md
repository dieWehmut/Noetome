# 教材式自适应规划冻结包（adaptive-plan-v1）

自造的规划声明与**实现前手写**的期望，用于验证 `noetome plan` 的确定性口径，
不含任何真实教材原文。声明只描述主题、阅读目标、概念关系、反应链与必要原子；
本阶段不生成知识、不写正文、不判断内容质量。

## 成员

| 成员 | 作用 |
| --- | --- |
| `project.json` | 基线声明：2 主题、6 阅读目标、4 概念、4 关系、1 条反应链、8 个原子 |
| `project-cycle.json` | 变体：追加 `concept-pfk → concept-glycolysis` 前置边，使前置关系成环 |
| `project-split.json` | 变体：删掉反应链的 `split_reason`，链被拆到两份笔记却没有理由 |
| `project-unplaced.json` | 变体：把 `concept-gluconeogenesis` 的原子全挪到对照目标，主目标不再承载完整解释 |
| `project-argument.json` | 变体：把 `atom-pfk-allosteric` 挪进另一个主题的阅读目标（跨主题搬原子） |
| `expected.json` | 实现前手写的期望：计数、笔记与文件夹路径、每个原子的规划位置、教学顺序、三条告警与四条拒绝 |
| `cases.json` | 复现序列（adopt → build → list → status）与四个变体、边界声明 |
| `frozen-sha256.json` | 冻结清单（由审计器 `--freeze` 生成，禁止手改） |

## 先验怎么来的

1. 前置边共 3 条：glycolysis→pfk、glycolysis→gluconeogenesis、pyruvate→gluconeogenesis；
   入度为 0 的只有 glycolysis，之后 pfk 与 pyruvate 同时就绪，按声明序取 pfk，最后 gluconeogenesis。
   因此教学顺序是 `糖酵解 → 磷酸果糖激酶 → 丙酮酸 → 糖异生`。
2. 每个主题的主目标排第一份笔记，其余目标按教学序位排序，编号从 `01` 递增。
   2 个主题 × 3 个目标 = 6 份笔记、2 个文件夹。
3. 8 个原子全部声明了目标，其中 3 个落在非主目标：`atom-glycolysis-payoff`（能量）、
   `atom-pfk-regulation`（调控）、`atom-gluconeogenesis-compare`（对照）。
   这些概念的完整解释在主笔记里，其余笔记只放一个链接，因此跨笔记引用 3 处。
4. `chain-glycolysis` 的 3 个步骤分布在主目标与能量目标两份笔记里，声明了
   `chain_too_long`，所以拆分计数 1，并且这条链必须被记录下来。
5. `topic-gluconeogenesis` 的 energy 目标没有任何原子，期望只产生一条
   `goal_without_atoms` 告警——空目标是被允许的，但必须可见。
6. 四个变体各自只违反一条规则：成环、无理由拆链、完整解释不在主目标、跨主题搬原子。

## 冻结与复现

```bash
python3 scripts/verify_adaptive_plan.py --package fixtures/adaptive-plan-v1 --freeze
python3 scripts/verify_adaptive_plan.py --package fixtures/adaptive-plan-v1
```

冻结清单覆盖包内除自身以外的全部成员；任何字节改动都会被 `frozen-digest:<成员>` 报出。

## 边界

- `scientific_status: not_run`：这里的声明与期望只证明规划口径可复算，不证明生化内容正确。
- 不含笔记正文、不含 Vault、不联网、不调用模型。
- 规划只决定"知识放在哪个文件"，知识图谱的多重归属在此只保留一个主要位置，其余用链接表达。
