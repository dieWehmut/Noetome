# 小节提升与链接迁移契约（B11-T01）

本契约定义 Noetome 第一版「小节提升为独立笔记与链接批量更新」：迁移计划声明什么、
冻结根里存什么、引用为什么必须按知识对象而不是文件名核对、重定向位置为什么必须保留、
四种笔记变更各自怎么核对，以及什么情况下整包拒绝而不写出任何报告。

对应需求：FR-18（链接解析与位置迁移）、FR-17（唯一规范解释位置）、
AC-18（小节提升后的引用解析）、NFR-05（链接可用率）。

## 1. 边界

- 本层只做**迁移核对与报告**：把迁移前后的两侧正文各扫一遍，把每条 wikilink 解析到
  知识对象，再把两侧的引用归属、身份保持与断链算清楚。它**不改写作者包**、
  **不搬动文件**、**不重命名笔记**、**不生成知识内容**。
- 本层**不联网、不调用模型**。输出固定写 `model_calls: 0`、`network_requests: 0`。
- 本层**不写知识库、不写托管 Vault**，也不接管已有目录。`adopt` 只创建全新的迁移根。
- **断链是硬条件**：迁移后仍有任何一条断链，`build` 直接拒绝写报告。报告里写零断链，
  就必须真的零断链。
- **不判断内容质量**：本层只核对引用落到哪个对象、笔记有没有被意外改写，
  不评价笔记写得好不好、科学上对不对。

## 2. 迁移计划（输入侧）

`plan.json` 字段：

| 字段 | 说明 |
| --- | --- |
| `policy` | 固定 `link-migration-v1` |
| `schema_version` | 固定 `1` |
| `producer` | 作者包固定 `fixture_author` |
| `case_id` | 标识符（ASCII 字母数字加 `-_.`） |
| `description` | 不超过 600 字符，说明本包只验证工程契约 |
| `before_dir` | 相对路径，迁移前正文目录（如 `before`） |
| `after_dir` | 相对路径，迁移后正文目录（如 `after`） |
| `objects` | 相对路径，指向知识对象表 `.json` |
| `before[]` | `note_id`、`path`（必须 `.md`，允许中文路径段）、`title` |
| `after[]` | 同上，另加 `counterpart` 与 `change` |
| `promotions[]` | `promotion_id`、`from_note`、`from_heading`、`to_note` |
| `limits` | `max_notes`（1..=8）、`max_objects`（1..=8）、`max_links`（1..=32）、`max_promotions`（0..=4） |

`change` 只能是 `unchanged`、`links_only`、`promoted`、`created` 之一：

| 变更 | 含义 | 必须声明的字段 |
| --- | --- | --- |
| `unchanged` | 迁移前后逐字节相同 | `counterpart` |
| `links_only` | 只有链接目标变了，正文其余部分不变 | `counterpart` |
| `promoted` | 某节被提升成独立笔记，原笔记保留标题作重定向 | `counterpart` |
| `created` | 新增笔记，迁移前不存在 | 不允许 `counterpart` |

`before[]` 里的笔记**不允许**带 `counterpart` 或 `change`——迁移前没有"参考对象"这回事。
`promotions[]` 的 `from_note` 必须在 `before[]` 里，`to_note` 必须在 `after[]` 里。
`after[]` 里非 `created` 的笔记，`counterpart` 必须能在 `before[]` 里找到。

知识对象表（`objects.json`，`policy` 固定 `link-objects-v1`）：
`case_id` 必须与计划一致，每个对象有 `object_id`、`kind`（`concept`/`atom` 等标签）、
`term`（中文可读名）、`before` 位置、`after` 位置、`redirects[]`。
位置是 `{note_id, heading}`；`heading` 为 `null` 表示"整篇笔记就是该对象的位置"。

## 3. 冻结根布局

```
<迁移根>/
  control/
    manifest.json      所有权、身份、计划与对象表与输入清单的摘要
    inputs.json        两侧每篇笔记的路径、标题、变更类型、摘要、字节数、片段数
    plan.json          迁移计划原文（逐字节冻结）
    objects.json       知识对象表原文（逐字节冻结）
    before/
      <note_id>.md     迁移前正文（逐字节冻结）
    after/
      <note_id>.md     迁移后正文（逐字节冻结）
```

顶层只允许 `control/` 一项；`control/` 恰好这 6 项，不多不少。任何多余条目、
符号链接、目录替换都判 `MIGRATE_INTEGRITY`。

**尾换行是有区别的**：`manifest.json`、`inputs.json` 由 `strict` 逐字节比对 canonical JSON，
所以**不带**尾换行；`plan.json`、`objects.json`、`before/*.md`、`after/*.md` 是作者包原文，
保持原样。输出根里的派生记录则相反——五个文件都带一个尾换行（见第 8 节）。

## 4. 知识对象位置必须真实存在

迁移计划声明的每个位置都要在对应笔记里**真的找到那个标题**：

- `before` 位置：在 `before` 侧的笔记里找标题。找不到 → `MIGRATE_OBJECT_MISSING`
  （文案「知识对象 … 声明的位置 … 在笔记 … 里不存在这个标题；位置必须真实存在，不能靠猜。」）。
- `after` 位置：在 `after` 侧的笔记里找标题。`heading: null` 表示整篇，不做标题查找。
- `redirects[]`：每个跳转位置按 `after` 侧查标题，找不到同样 `MIGRATE_OBJECT_MISSING`。

这条检查跑在**扫链接之前**：连"对象在哪"都说不清楚，后续的引用归属就没有意义。

## 5. 引用解析：先落到对象，再比较身份

正文里的 `[[...]]` 按行扫描，同一行可以有多条。内部按 `目标#标题|别名` 拆：

- **别名竖线**：如果这条链接在表格行里（`trim_start` 以 `|` 开头），裸 `|` 会截断单元格，
  整包拒绝 `MIGRATE_UNESCAPED_ALIAS`；必须写成 `\|`。
- **目标解析**：先按 `path` 精确匹配；失败则补 `.md`、再按路径尾段或 `note.title` 匹配。
  0 条 → `MIGRATE_TARGET_MISSING`，多条 → `MIGRATE_AMBIGUOUS_TARGET`。
- **标题校验**：目标解析成功后，若带 `#标题`，该标题必须在目标笔记的**全部标题**里存在，
  否则 `MIGRATE_TARGET_MISSING`。
- **落到对象**：把 `(note_id, heading)` 合成位置键，在对象表的规范位置与跳转位置里查。
  查不到 → `MIGRATE_OBJECT_MISSING`（这条链接指到了一处没有知识对象的位置）。
- **匹配类型**：命中规范位置记 `canonical`，命中跳转位置记 `redirect`。
  两者都算指向同一对象——这正是"小节提升后旧链接不该断"的表达方式。

**引用按知识对象ID核对，不是按文件名核对**：文件名只是位置，对象ID才是身份。
迁移把一个对象从"某笔记的某节"搬到"另一篇笔记的整篇"，ID 不变，引用就不该丢。

## 6. 身份保持与四种变更的核对

**身份保持**：一个对象只要在迁移前被引用过（`referenced_before`），迁移后就必须仍然被引用
（`referenced_after`）。否则记一条身份违规，`build` 报 `MIGRATE_INTEGRITY`
（文案「迁移后这些知识对象不再被任何引用指向：…。迁移不能丢引用。」）。

"未被引用"的口径是**两侧都没人指向**：只在迁移前被引用也算被引用过，
所以 `objects_unreferenced` 不会把"迁移后才没人引用"的对象算成无关。

**四种变更各自有判据**：

| 变更 | 判据 |
| --- | --- |
| `unchanged` | 两侧正文逐字节相等 |
| `links_only` | 剥掉所有 `[[...]]` 之后逐字节相等 |
| `promoted` | 提升来源里本来就有该标题；迁移后**仍保留**该标题作重定向位置 |
| `created` | 只计数，没有对照物 |

`promoted` 的两条方向必须说清楚：来源里没有这个标题 → `MIGRATE_ARGUMENT`
（计划声明的提升内容不存在）；迁移后把标题整个删掉 → 也是 `MIGRATE_ARGUMENT`
（提升必须保留原标题作为重定向位置，否则旧链接会立刻断掉）。**删标题不是提升**。

`created` 没有对照物，`bytes_equal`、`stripped_equal`、`promotion_satisfied` 一律记 `null`
（不适用），**不是 `false`**——写 `false` 会让读者以为"核对过且不相等"。

## 7. 计数口径

`report.json` 的 `counts` 一共 20 项，逐项可复算：

```
notes_before / notes_after        两侧笔记篇数（来自计划）
objects                           对象表条数
links_before / links_after        两侧扫出的 wikilink 条数
resolved_before / resolved_after  状态为 resolved 的条数（两侧都必须是全部）
canonical_links_after             after 侧命中规范位置的条数
redirect_links_after              after 侧命中跳转位置的条数
broken_links                      after 侧非 resolved 的条数（必须为 0）
table_links_before/after          表格行里的链接条数
escaped_alias_before/after        带转义别名的链接条数
objects_referenced/objects_unreferenced   被引用/两侧都没人指向的对象数
identity_violations               迁移后丢失引用的对象数（必须为 0）
unchanged_notes/created_notes/promoted_notes   按变更类型的笔记数
rewritten_targets                 after 侧命中"对象位置变了"的规范链接条数
```

`rewritten_targets` 只数 **after 侧的规范链接**，且只在对象**位置真的变了**时计入——
位置比较用的是**解析后的路径与标题**，不是 `note_id`（`note_id` 在两侧本来就不同前缀，
拿它比较会把每个对象都算成"移动过"）。

## 8. 输出根与字节确定性

```
<输出根>/
  links.json        两侧每条链接的解析记录与 12 项链接计数
  objects.json      每个知识对象的归属、引用数、身份保持与 5 项对象计数
  report.json       20 项全量计数、每篇 after 笔记的核对结论、零调用声明
  report.md         人读报告：计数 / 知识对象身份 / 迁移后引用 / 笔记核对 / 边界
  manifest.json     输出清单：四份派生的摘要与计数
```

**输出清单不含时间戳**，同一冻结输入的两次构建必须字节相同。派生四文件各带一个尾换行。

`report.md` 固定五节；`## 笔记核对` 里的"是/否/—"对应 `true`/`false`/`null`，
"—"表示不适用而不是"没核对"。

## 9. 所有权与身份校验

`manifest.json` 记 `root`、`root_identity`、`parent_identity`、`control_identity`、
`manifest_identity`、`inputs_identity` 六处身份，以及 `plan_sha256`、`objects_sha256`、
`inputs_sha256` 三份摘要与逐篇 `documents[]` 摘要。`open` 每次只读都重新核对：
身份变了、摘要不符、笔记文件多一个少一个，都判 `MIGRATE_INTEGRITY`。

`adopt` **先在内存里把整份判定跑一遍**，通过才建根；建根过程中失败且根身份未变则删掉半成品。
语义拒绝（歧义、目标缺失、未转义别名、位置不存在、身份丢失、断链）**一律不留根**。

## 10. CLI 动词与错误码

| 动词 | 作用 |
| --- | --- |
| `migrate adopt <plan.json> <尚不存在的迁移根>` | 核对计划与两侧字节，预演一次判定，全部通过才建冻结根 |
| `migrate build <迁移根> <尚不存在的输出根>` | 从冻结输入重算并写出五文件输出根 |
| `migrate list <迁移根>` | 机器可读 JSON：计划摘要、对象、规范/跳转条数、报告计数 |
| `migrate status <迁移根>` | 中文摘要：逐篇 after 笔记核对结论、引用身份、链接与断链计数 |

| 错误码 | 退出码 | 何时 |
| --- | --- | --- |
| `MIGRATE_ARGUMENT` | 2 | 参数、路径、字段取值不合法；目标已存在；提升来源没有该标题；提升后标题被删；输出根与迁移根互相包含 |
| `MIGRATE_INTEGRITY` | 3 | 冻结根布局/身份/摘要不符；迁移后丢失引用；迁移后仍有断链 |
| `MIGRATE_UNKNOWN_ROOT` | 3 | 目录不存在或不是迁移根（缺 `control/`），保持只读 |
| `MIGRATE_AMBIGUOUS_TARGET` | 3 | 链接目标对应多篇笔记，必须写完整路径 |
| `MIGRATE_TARGET_MISSING` | 3 | 链接目标或 `#标题` 不存在 |
| `MIGRATE_UNESCAPED_ALIAS` | 3 | 表格行里写了裸 `|` 别名，必须写成 `\|` |
| `MIGRATE_OBJECT_MISSING` | 3 | 声明的对象位置在笔记里不存在，或链接指到没有对象的位置 |

## 11. 本轮明确不做

1. 不执行真实文件搬迁：`adopt`/`build` 只读作者包，两侧正文都来自冻结输入。
2. 不解析嵌入（`![[...]]`）与块引用（`#^block`）；只处理文件/标题链接。
3. 不做冲突合并：迁移后出现两条指同一对象但内容不同的链接，本层只记录，不裁决。
4. 不判断内容质量与科学正确性。
5. 不实现锁定与增量更新：每次 `build` 都从冻结输入整体重算。
6. 不写知识库与托管 Vault。
7. 未接桌面 GUI：`migrate` 只在 CLI 可用。
