# 用户水平与相关性筛选契约（B10-T02）

本契约定义 Noetome 第一版「用户水平与相关性筛选的可审计记录」：作者包声明什么、
冻结根里存什么、必要性怎么由水平推导、排除为什么要登记理由并落在正文 Span 上、
必要前提缺失为什么必须整包拒绝、极简与详细两种密度为什么必须保住同一套原子与条件，
以及输出根的字节确定性从哪来。

对应需求：FR-05（相关性筛选和用户水平）、FR-20（风格配置与参考学习）、
NFR-01（来源处置透明）、AC-09（极简完整）、AC-10（水平筛选）。

## 1. 边界

- 本层只做**保留与排除的可审计记录**：把已登记的 Span 判成 kept 或 excluded，
  把已声明的原子判成 kept 或 deferred，并写出理由与落点。它**不改写来源**、
  **不删除冻结片段**、**不生成知识**、**不生成笔记正文**。
- 本层**不联网、不调用模型**。输出固定写 `model_calls: 0`、`network_requests: 0`。
- 本层**不写知识库、不写托管 Vault**，也不接管已有项目目录。`adopt` 只创建全新的筛选根。
- **排除不改变冻结输入**。被排除的行仍然逐字节留在 `control/text/<source_id>.md` 里，
  排除只体现在派生输出 `filter.json` 的 `status`/`reason`/`detail` 上。
- 用户水平只由**用户声明**决定；本层不从资料推测水平，也不提供"建议水平"。
  `level` 必须取 `introductory`、`university`、`advanced` 三者之一。

## 2. 作者包（输入侧）

`author.json` 字段：

| 字段 | 说明 |
| --- | --- |
| `policy` | 固定 `relevance-filter-v1` |
| `schema_version` | 固定 `1` |
| `producer` | 作者包固定 `fixture_author` |
| `case_id` | 标识符（ASCII 字母数字加 `-_.`） |
| `description` | 不超过 600 字符，说明本包只验证工程契约 |
| `audience` | 相对路径，指向用户水平声明 `.json` |
| `atoms` | 相对路径，指向原子清单 `.json` |
| `review` | 相对路径，指向排除登记 `.json` |
| `sources[]` | `source_id`、`path`（必须 `.md`）、`title` |
| `limits` | `max_sources`（1..=8）、`max_atoms`（1..=64）、`max_conditions`（0..=8）、`max_requirements`（0..=8） |

三份被引用声明各有自己的 `policy`：`audience` 是 `relevance-audience-v1`，
`atoms` 是 `relevance-atoms-v1`，`review` 是 `relevance-review-v1`；
三份的 `case_id` 必须与作者包一致，否则 `adopt` 直接拒绝。

用户水平声明（`audience.json`）：`level`、`declared_knowledge[]`（字符串数组）、
`purpose`（不超过 600 字符）。`declared_knowledge` 是**用户自报已掌握的前置**，
它和"来源里确实覆盖了该前置"一样被当作前提已满足——但不因此把该前置变成保留原子。

原子清单（`atoms.json`）每条：`atom_id`、`source_id`、`line`、`kind`、`statement`、
`conditions[]`、`requires[]`、`covers[]`、`levels[]`。
`conditions` 是**给读者看的散文条件**（例如「镁离子存在」），不是标识符；
`requires`/`covers`/`levels` 是标识符或枚举，越界即拒绝。

排除登记（`review.json`）：`reasons[]`（`reason` + 中文 `note`）声明本包允许的排除理由，
`exclusions[]`（`source_id`、`line`、`reason`、`detail`）逐条登记。

## 3. 冻结根布局

```
<筛选根>/
  control/
    manifest.json        所有权与摘要（canonical JSON，无尾换行）
    inputs.json          每份文档的 Span 切片清单
    author.json          冻结的作者声明（canonical JSON，无尾换行）
    audience.json        冻结的用户水平声明（原始字节）
    atoms.json           冻结的原子清单（原始字节）
    review.json          冻结的排除登记（原始字节）
    text/
      <source_id>.md     冻结的规范化正文（逐字节不可变）
```

顶层只允许 `control/`；`control/` 只允许上列七项；`text/` 恰好是「文档数」个文件。
任何多余条目按 `RELEVANCE_INTEGRITY` 拒绝且不修改任何字节。

`manifest.json` 记录 `root_identity`、`parent_identity`、`control_identity`、
`manifest_identity`、`inputs_identity` 用于检测目录被替换，并记录
`author_sha256`、`audience_sha256`、`atoms_sha256`、`review_sha256`、`inputs_sha256`
五份摘要与每份文档的 `normalized_sha256`。打开时逐项复核。

**注意一处字节细节**：`author.json` 落盘时用规范 JSON 且**不带尾换行**，
因为 `strict` 会把读到的字节与重新规范化的结果逐字节比对；多一个换行会让此后每次
`open` 都报"非规范编码"。派生输出里的 `filter.json`/`densities.json`/`report.json`/
`manifest.json` 则各带一个尾换行，审计器 `strict_load` 允许且只允许这一个换行。

## 4. 水平与必要性判定

`level_index`：`introductory` = 0、`university` = 1、`advanced` = 2。

原子对当前用户**必要**的判据只有一条，且刻意不对称：

```
required(atom) = ∃ lv ∈ atom.levels : level_index(lv) <= level_index(用户水平)
```

即"低阶原子在高水平仍然必要"——大学水平需要高中水平的定义，反过来不成立。
于是 `levels: ["university","advanced"]` 的原子对 `university` 必要，
对 `introductory` 不必要（可暂缓）。

`required` 为真却被排除 → `RELEVANCE_REQUIRED_EXCLUDED`（exit 3），
文案必须点出原子、来源、行号、排除理由，并说明"必要原子不能因为「极简」或「超纲」被删除"。
**这条是硬拒绝，不是告警**：FR-05 要求必要的定义、条件、例外、单位、公式与理解前提
不能仅因"基础"或"极简"被删除，AC-09/AC-10 都以此为准。

## 5. 排除理由的登记与落点

三步校验，任一步不过即整包拒绝：

1. **理由必须在登记表内**。`reason` 不在 `review.reasons` 里 → `RELEVANCE_ARGUMENT`（exit 2），
   文案列出允许集合。自由文本理由会让"为什么排除"变成不可比对的东西，所以不允许。
2. **落点必须是真实正文 Span**。`(source_id, line)` 必须落在某个 `kind = "text"` 的 Span 内。
   行号超出文件行数 → `RELEVANCE_ARGUMENT`；落在标题或空行（`heading`/`formatting`）上
   → `RELEVANCE_ARGUMENT`，文案说明"只有正文片段可以被排除或承载原子，标题与空行属于结构"。
3. **同一行不得重复排除**。

标题永远 `kept`：章节结构不是可选散文，排除一个标题只会让下游失去定位。

## 6. 必要前提规则

`requires` 是原子依赖的前置知识标识符。判据：

```
provided = { c : 某个 kept 原子的 covers 含 c } ∪ 用户 declared_knowledge
∀ kept 原子 a, ∀ need ∈ a.requires : need ∈ provided
```

任一不满足 → `RELEVANCE_PREREQUISITE_MISSING`（exit 3），
文案点出原子、缺失前置，并写明"来源没有提供、用户也没有声明掌握；
补足前提或降低目标后再筛选，不能静默省略"。

**为什么是拒绝而不是跳过原子**：静默丢掉一个缺前提的原子，会让笔记看起来完整却实际缺少
理解链条；这与 FR-05「不确定的重要性决策保留或标为待复核」方向相反。要求调用方补足前提
或降低目标，是唯一能保持"缺口可见"的做法。

## 7. 双密度合同

同一份已核对原子集派生两种密度：

| 密度 | 每原子 `expansion` | 其余字段 |
| --- | --- | --- |
| `minimal` | 全为 `null` | 完全相同 |
| `detailed` | 全为一句展开文本 | 完全相同 |

`expansion` 形如
`糖酵解在细胞质中把一分子葡萄糖变成两分子丙酮酸。适用条件：无附加条件。前置知识：无。来源：doc-basics 第 2 行。`

一致性判据 `comparable(density)` 只取 `(atom_id, conditions, requires)` 的三元组序列，
两者必须相等，`report.unchanged_across_densities` 记录结果。
`report.densities` 是 `{minimal: {atoms, conditions}, detailed: {atoms, conditions}}`，
两键计数必须相同——**极简与详细只在表达与展开程度上不同，原子与条件集合不减少**，
这正是 AC-09 的可复算形式。

## 8. 输出根与字节确定性

```
<输出根>/
  filter.json      逐 Span 与逐原子的判定（含理由与落点）
  densities.json   两种密度的完整内容
  report.json      计数、理由分布、两密度计数、零调用声明
  report.md        人读记录
  manifest.json    输出清单（四文件摘要 + 计数）
```

**同一冻结输入的两次构建必须逐字节相同**，包括 `manifest.json`。
输出清单因此**不含任何墙钟时间戳**（`created_at` 出现在冻结根的清单里，用于记录 adopt 时刻，
但绝不进入输出清单）。判定顺序固定：先派生四个文件，回读 `report.json` 拿计数，最后写 `manifest.json`。

输出根必须尚不存在，也不能位于筛选根内部或包含筛选根。构建失败回滚整个输出根。

## 9. 所有权与身份校验

`adopt` 在创建根之前先 `canonicalize` 父目录并取身份；**先做一次完整的 dry 判定**
（逐来源分段 + 完整 `decide`），失败即返回且不留根——这样"前提缺失""必要原子被排除"
这类语义拒绝不会在磁盘上留下半成品。创建后立刻取新根身份写进清单；
`adopt_into` 失败且根身份仍是刚创建的那个，则删除整个根。

对**陌生目录**（不存在 `control/`）报 `RELEVANCE_UNKNOWN_ROOT`（exit 3），不是 IO 错误，
与 `research`、`retrieve`、`revision`、`outline` 四类根的行为一致。

## 10. CLI 动词与错误码

| 动词 | 作用 |
| --- | --- |
| `relevance adopt <author.json> <根>` | 冻结作者包与三份声明，建筛选根；根必须尚不存在 |
| `relevance build <根> <输出根>` | 从冻结输入派生五个文件 |
| `relevance list <根>` | 机器可读摘要（audience、limits、documents、filter、minimal、detailed、report） |
| `relevance status <根>` | 人读中文摘要（所有权、水平、每来源保留/排除、理由分布、原子、两密度） |

| 错误码 | exit | 含义 |
| --- | --- | --- |
| `RELEVANCE_ARGUMENT` | 2 | 参数无效、根已存在、未登记理由、行号落不到正文 Span、重复排除、字段越界 |
| `RELEVANCE_INTEGRITY` | 3 | 摘要不符、布局多余条目、身份被替换、非规范 JSON |
| `RELEVANCE_UNKNOWN_ROOT` | 3 | 目标不是已建立的筛选根 |
| `RELEVANCE_PREREQUISITE_MISSING` | 3 | 必要前提既未被保留原子覆盖，也未被用户声明 |
| `RELEVANCE_REQUIRED_EXCLUDED` | 3 | 当前水平需要的原子被排除 |
| `PROJECT_IO_ERROR` | 3 | 文件系统层失败（父目录不存在等） |

## 11. 本轮明确不做

- 不推测用户水平、不生成水平建议、不自动放宽或收紧必要性判据。
- 不做语义相似度、实体识别或自动判定"这条是不是轶事"——排除必须由人登记理由。
- 不改写、不删除来源正文；被排除的行仍逐字节保留在冻结根里。
- 不生成笔记正文、不排版、不合并章节（排版属于展示层，章节结构属于 B10-T01/B10-T03）。
- 不支持 PDF/DOCX 直接输入；正文规范化属于上游职责。
- 不写项目库与托管 Vault，不接管生产目录；不做增量筛选（每次全量重算）。
- 不判断科学正确性：筛选成功只说明保留/排除决策可复算、理由可追溯、必要前提未缺失。
