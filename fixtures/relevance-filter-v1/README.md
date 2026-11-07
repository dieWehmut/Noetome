# 用户水平与相关性筛选冻结包（B10-T02）

本目录是一份**自造**作者包，用于验证 `relevance adopt|build|list|status` 的工程契约：
用户水平如何决定原子必要性、排除为什么必须登记理由并落在正文 Span 上、
必要前提缺失为什么整包拒绝、极简与详细两种密度为什么必须保住同一套原子与条件。

它**不证明**真实教材的筛选正确率：三份来源都是为覆盖判据而写的短文本，
`cases.json` 的 `boundaries.scientific_status` 明确写 `not_run`。

## 成员

| 成员 | 字节 | 作用 |
| --- | --- | --- |
| `author.json` | 874 | 主作者包：声明三份来源、三份被引用声明与 limits |
| `audience.json` | 241 | 用户水平 `university`，自报已掌握 `glucose-ring-structure` |
| `atoms.json` | 2836 | 六个原子，带 `conditions`/`requires`/`covers`/`levels` |
| `review.json` | 1888 | 五条允许理由 + 六条排除登记 |
| `documents/basics.md` | 466 | 13 行正文，含定义、条件、前提、误解与一条重复介绍 |
| `documents/anecdote.md` | 278 | 9 行，含轶事、装饰性引言与纯练习 |
| `documents/overshoot.md` | 295 | 6 行前沿方法学与未决争议 |
| `expected.json` | 2927 | **实现前手写**的保留/排除期望与四条拒绝清单 |
| `cases.json` | 1192 | 八步执行序列 + 边界声明 |
| `frozen-sha256.json` | 2635 | 审计器 `--freeze` 生成的成员摘要 |
| `audience-insufficient.json` | 221 | 变体：`declared_knowledge` 为空 |
| `audience-advanced.json` | 240 | 变体：水平改 `advanced` |
| `author-insufficient.json` | 899 | 变体包：指向空自报名单 → 必须报必要前提缺失 |
| `author-advanced.json` | 886 | 变体包：高水平会把 `atom-pfk-fret` 变成必要却被排除 |
| `author-unknown-reason.json` | 823 | 变体包：排除理由写自造词 `boring` |
| `review-unknown-reason.json` | 884 | 上者的排除登记 |
| `author-unregistered-reason.json` | 843 | 变体包：理由表去掉 `pure_exercise` 却仍用它排除 |
| `review-unregistered-reason.json` | 793 | 上者的排除登记 |

## 期望判据从哪来（先验推导，不是从产品输出回填）

`expected.json` 在写实现之前手写，逐项推导如下：

- **片段与正文片段**：`basics.md` 12 个 Span（4 个标题、5 段正文、3 段空行）、
  `anecdote.md` 8 个、`overshoot.md` 5 个，合计 25；其中 `kind = text` 的分别是 5、3、2，合计 10。
- **排除 6 条**：`doc-basics:13`（重复介绍）、`doc-anecdote:2`（无关轶事）、
  `doc-anecdote:6`（纯装饰）、`doc-anecdote:9`（纯练习）、`doc-overshoot:2` 与
  `doc-overshoot:6`（明显超纲），理由分布因此是
  `duplicate_introduction 1 / irrelevant_anecdote 1 / decoration 1 / pure_exercise 1 / beyond_target_level 2`。
- **保留 4 条正文片段**：10 个正文 Span 减去 6 条排除（`doc-anecdote:2` 与 `doc-overshoot:2`
  各自排掉一个跨两行的正文 Span，不会再重复计数）。
- **原子 6 个、保留 5、暂缓 1**：`atom-pfk-fret` 只在 `advanced` 必要，
  `university` 下排除后按规则暂缓而非拒绝；其余五个在 `university` 下必要。
- **条件 3 条**：`atom-glycolysis-anaerobic`、`atom-magnesium`、`atom-lactate` 各一条，
  另三个原子无附加条件。
- **两密度都是 5 个原子 / 3 条条件**：`minimal` 与 `detailed` 的差异只允许在 `expansion` 上。
- **四条拒绝**：空自报 → `RELEVANCE_PREREQUISITE_MISSING`（exit 3）；
  高水平下排除必需原子 → `RELEVANCE_REQUIRED_EXCLUDED`（exit 3）；
  自造理由与未登记理由 → `RELEVANCE_ARGUMENT`（exit 2）。

## 冻结规则

- 除 `frozen-sha256.json` 之外的每个文件都进入清单，逐文件记 `sha256` 与 `byte_length`；
  任何成员被改动、增删，审计器都会报 `frozen-digests` 或 `frozen-set`。
- 需要更新冻结包时，先改内容，再跑
  `python3 scripts/verify_relevance_filter.py --package fixtures/relevance-filter-v1 --freeze`
  重写清单；不要手工编辑清单。
- 三份来源都是自造内容，不引用任何真实教材段落。

## 边界

`scientific_status`：`not_run`。本包零网络、零模型调用，不写项目库、不写托管 Vault。
它能证明的是"同一冻结输入下的筛选结果可复算、理由可追溯、拒绝语义明确"，
不能证明真实教材上的筛选正确率，也不能证明原子陈述在科学上正确。
