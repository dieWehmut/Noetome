# 教材式自适应规划合同（adaptive-plan-v1）

对应需求：**FR-15 教材式自适应规划**（P0），结构约束取自 **FR-07 概念关系与多重归属**。
本阶段只决定"知识放在哪个文件"，不生成知识、不写正文、不判断内容质量与科学正确性。

## 1 边界

- 规划输入只有一份声明：主题、阅读目标、概念、概念关系、反应链与必要原子。
- 读声明、算规划、写派生结果；不联网、不调用模型、不写项目与 Vault。
- 知识图谱允许多重归属，但**文件树只选一个主要位置**，其余联系用链接表达。
- 不平均分配字数：过程完整性、论证完整性与人的查找习惯优先于篇幅均衡。
- 文件数量与篇幅不设机械硬上限；本阶段也不生成任何正文。

## 2 规划声明

顶层 13 个字段，全部 `deny_unknown_fields`：

| 字段 | 说明 |
| --- | --- |
| `policy` / `schema_version` / `producer` | 必须是 `adaptive-plan-v1` / `1` / `fixture_author` |
| `case_id` | 声明身份，标识符格式 |
| `description` | 人类可读描述，最长 600 字符 |
| `folder_root` | 输出区的相对根，例如 `代谢` |
| `topics` | 1..=4 个主题，每个主题声明 `topic_id`、`title`、`folder`、`concepts`、`goals`、`primary_goal` |
| `goals` | 1..=8 个阅读目标，`kind` ∈ `overview`/`reaction`/`regulation`/`energy`/`comparison` |
| `concepts` | 1..=64 个概念，`concept_id` 唯一 |
| `relations` | `from`/`to`/`kind`，`kind` ∈ `prerequisite`/`related` |
| `chains` | 反应链，`chain_id` 唯一，可选 `split_reason` |
| `atoms` | 必要原子，声明 `concept_id`、`goal_id`、可选 `chain_id` + `step` |
| `limits` | 五个上限，必须与声明规模相容 |

**每个概念只能属于一个主题**。跨主题的概念不是靠搬原子表达，而是靠 `related` 关系与笔记之间的链接表达。

## 3 冻结根布局

```
<root>/control/project.json   声明的原文（保留缩进与尾换行）
<root>/control/inputs.json    规范编码，无尾换行
<root>/control/manifest.json  规范编码，无尾换行
```

顶层只允许 `control/`，`control/` 恰三项。清单 13 个字段，含 `root`/`parent`/`control`/`manifest`/`inputs` 五处身份与
`project_sha256`、`inputs_sha256`。`adopt` 先完整预演一次规划（成环、拆链、原子无处安放），全部通过才建根；
失败时若根身份未变则删除已建目录，不留半个根。

## 4 教学顺序

教学顺序由 `prerequisite` 边决定，`related` 不参与排序：

1. 算出每个概念的入度，入度为 0 的概念就绪；
2. 同层就绪的概念**按声明顺序**取，不按字母序；
3. 取走一个概念后更新其后继的入度；
4. 若仍有概念未排入，说明前置关系成环，整份声明以 `PLAN_PREREQUISITE_CYCLE` 拒绝。

排序结果写进 `plan.json` 的 `teaching_order`，并在 `status`、`report.md` 与 `adopt` 摘要里原样出现——
**摘要里的教学顺序必须是真实顺序，不能拿某一份笔记的标题充数**。

## 5 笔记与文件夹规划

- 文件夹：`<folder_root>/<topic.folder>`，每个主题恰好一个。
- 笔记：`<folder_root>/<topic.folder>/<NN>-<goal_id>.md`，`NN` 从 `01` 递增。
- 主目标永远排 `01`；其余目标按"所辖原子中最小的教学序位"排序，序位相同时按目标声明顺序。
- 每个必要原子恰有一个规划位置：`plan.json` 的 `atoms` 按阅读顺序（笔记顺序、笔记内位置）排列，
  便于人工核对，而不是按声明顺序或字母序。
- 没有原子的目标仍会生成笔记，并产生一条 `goal_without_atoms` 提醒——空目标允许存在，但必须可见。

## 6 多重归属与跨笔记引用

当一个概念在自己的主题里出现在非主目标的笔记中，该笔记不复制解释，只放一条指向主笔记的链接：

```
{ "concept_id": "...", "target_note": "<主笔记路径>", "display_text": "<概念正名>" }
```

同一笔记内同一个概念只记一次链接。跨主题搬原子是声明错误（`PLAN_ARGUMENT`）：
"跨主题只能建立链接，不能搬走原子"。

## 7 反应链与学生可读性

- 一条链的原子必须按 `step` 从 1 连续编号，缺号或重号都拒绝。
- 链的原子落在多份笔记里时，链上**必须**声明 `split_reason`，取值只能是
  `chain_too_long`（单份笔记装不下）、`topic_boundary`（跨越主题边界）、`user_lock`（用户锁定位置）。
- 没有理由就把一条连续反应拆成大量单句文件，是最典型的退化形态，因此直接以 `PLAN_CHAIN_SPLIT` 拒绝。
- 有理由的拆分会计入 `counts.chains_split` 并逐条记录在 `report.json` 的 `chains` 里，
  理由是**可检查的声明**，不是事后解释。

## 8 必要原子必须落在主目标

每个概念至少要有一个原子落在其主题的 `primary_goal` 里，否则该概念在文件树里没有完整解释的位置，
以 `PLAN_UNPLACED` 拒绝。这一条是"每个必要原子都有规划位置"的加强版：
位置不仅存在，而且必须是**说清楚这个概念的那一份笔记**。

## 9 输出与字节确定性

输出根恰五个文件：

| 文件 | 内容 |
| --- | --- |
| `plan.json` | `policy`、`plan_version`、`case_id`、`folder_root`、`teaching_order`、`folders`、`notes`、`atoms` |
| `report.json` | 计数（10 项）、反应链、提醒、零模型调用与零网络请求 |
| `report.md` | 教学顺序、阅读目标与位置、反应链、提醒、边界 |
| `manifest.json` | 输出清单：四个文件摘要、计数、`source_inputs_sha256` |
| — | 输出根不得已存在，也不得与规划根互相包含 |

输出清单**不含 `created_at`**。同一冻结输入的两次构建必须字节相同；这是唯一能让"规划没有被随机因素影响"变成可复算事实的做法。

## 10 命令与错误码

| 命令 | 作用 |
| --- | --- |
| `noetome plan adopt <声明> <规划根>` | 预演并冻结声明，只写 `control/` |
| `noetome plan build <规划根> <输出根>` | 由冻结声明派生规划与报告 |
| `noetome plan list <规划根>` | 只读输出 16 个字段的 JSON 摘要 |
| `noetome plan status <规划根>` | 只读输出人类可读摘要（含教学顺序与逐笔记原子数） |

| 错误码 | 退出码 | 触发条件 |
| --- | --- | --- |
| `PLAN_ARGUMENT` | 2 | 声明结构非法、跨主题搬原子、步骤不连续、输出根已存在 |
| `PLAN_INTEGRITY` | 3 | 清单/输入被改写、`control/` 出现多余条目 |
| `PLAN_UNKNOWN_ROOT` | 3 | 目录不存在或不是规划根（缺少 `control/`） |
| `PLAN_CHAIN_SPLIT` | 3 | 连续反应链被拆开却没有可检查的理由 |
| `PLAN_PREREQUISITE_CYCLE` | 3 | 前置关系成环，无法排出教学顺序 |
| `PLAN_UNPLACED` | 3 | 概念在主阅读目标里没有完整解释位置 |

## 11 本轮明确不做

1. 不生成任何正文，也不判断内容质量与科学正确性。
2. 不做语义相似度或实体识别：主题、概念、关系全部由声明给出。
3. 不支持增量重排：每次 `build` 都从冻结声明重新规划。
4. 不写 Vault、不移动已有笔记、不改动用户目录。
5. 不做字数与篇幅均衡；文件数量也不设机械上限。
6. 不实现"多阅读目标共用同一份笔记"的模板合并（留给后续阅读目标与模板任务）。
7. 不覆盖锁定位置与规划变更的局部影响（留给后续锁定与变更任务）。
