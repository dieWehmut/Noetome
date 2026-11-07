# fixtures/link-migration-v1

自造的小节提升（section promotion）作者包，用于验证「把一个小节提升为独立笔记之后，引用还能不能指到同一个知识对象」。

作者包与期望表都是为本项目手写的内容；这里没有任何真实教材、真实笔记或真实链接。

## 成员

| 成员 | 字节 | 作用 |
| --- | --- | --- |
| `plan.json` | 1999 | 迁移计划：before 3 份、after 5 份、1 条提升、5 个知识对象 |
| `objects.json` | 1850 | 知识对象表：每个对象的迁移前位置、迁移后位置、跳转位置 |
| `before/笔记/糖酵解.md` | 419 | 提升前：`## 完整机制` 一节连同正文都在这里 |
| `before/笔记/糖异生.md` | 375 | 提升前：一行表格内引用（已转义别名）、一行普通引用 |
| `before/笔记/磷酸戊糖途径.md` | 181 | 提升前：无链接 |
| `after/笔记/糖酵解.md` | 291 | 提升后：原小节只剩标题 + 指向新笔记的链接 |
| `after/笔记/糖酵解-核心机制.md` | 226 | 提升出的新笔记（`created`） |
| `after/笔记/糖异生.md` | 375 | 提升后：表格引用改指新笔记，普通引用仍走旧位置（`redirect`） |
| `after/笔记/磷酸戊糖途径.md` | 181 | 与提升前逐字节相同（`unchanged`） |
| `after/附录/糖酵解.md` | 103 | 制造裸文件名 `糖酵解` 的歧义（`created`） |
| `after/variants/ambiguous.md` | 365 | 变体：用裸 `[[糖酵解#完整机制]]` 触发歧义拒绝 |
| `after/variants/missing-target.md` | 384 | 变体：指向不存在的笔记 |
| `after/variants/unescaped-table.md` | 374 | 变体：表格单元里写裸 `|` |
| `plan-ambiguous.json` | 2155 | 变体计划：把 `b-gluconeogenesis` 的对应项指向 `after/variants/ambiguous.md` |
| `plan-missing-target.json` | 2171 | 变体计划：指向 `after/variants/missing-target.md` |
| `plan-unescaped-table.json` | 2176 | 变体计划：指向 `after/variants/unescaped-table.md` |
| `plan-unsafe.json` | 1947 | 变体计划：把 `objects` 换成 `objects-unsafe.json` |
| `objects-unsafe.json` | 1875 | 变体对象表：把对象的迁移后标题改成一个不存在的标题 |
| `expected.json` | 9517 | **实现之前手写**的迁移期望表 |
| `cases.json` | 1170 | 端到端复核九步 |

## 先验推导

`expected.json` 写在实现之前，按下面的规则一条条算出来，不是产品输出的转录。

1. 链接条数：`before` 里 `糖酵解.md` 第 12 行、`糖异生.md` 第 11 行（表格内）、第 12 行，共 **3** 条；`after` 里 `糖酵解.md` 第 5 行、第 8 行、`糖异生.md` 第 11 行、第 12 行，共 **4** 条。
2. 同一对象：`[[笔记/糖酵解.md#完整机制]]`（提升前）与 `[[笔记/糖酵解-核心机制.md]]`（提升后规范位置）、`[[笔记/糖酵解.md#完整机制]]`（提升后跳转位置）必须都解析到 `obj-glycolysis-mechanism`。
3. 规范位置 vs 跳转位置：`after` 的 4 条里 3 条落在规范位置、1 条落在跳转位置（`redirect`）。
4. 被引用对象：只有 `obj-glycolysis-mechanism`（迁移前 2 条 → 迁移后 3 条）和 `obj-gluconeogenesis-bypass`（迁移前后各 1 条）被指向，其余 **3** 个对象两侧都没人指；因此 `objects_referenced = 2`、`objects_unreferenced = 3`。
5. 重写目标：`before.heading` 与 `after.heading` 不同、或落点路径不同的对象才算移位；只有 `obj-glycolysis-mechanism` 满足，且指向它的规范位置链接有 2 条（`糖酵解.md` 第 5 行、`糖异生.md` 第 11 行），所以 `rewritten_targets = 2`。
6. 笔记变更：`after/笔记/磷酸戊糖途径.md` 与 `before` 逐字节相同 → `unchanged`；`after/笔记/糖异生.md` 去掉全部 `[[...]]` 后与 `before` 相同 → `links_only`；`after/笔记/糖酵解.md` 保留原标题但正文重写 → `promoted`；`after/笔记/糖酵解-核心机制.md` 与 `after/附录/糖酵解.md` 没有对照物 → `created`，它们的 `bytes_equal`/`stripped_equal` 记 `null`（不适用），不是 `false`。
7. 断链：提升后源笔记保留原标题，所以旧链接仍然可用；期望 `broken_links = 0`。

## 冻结规则

`frozen-sha256.json` 由 `scripts/verify_link_migration.py --freeze --package fixtures/link-migration-v1` 生成。
它是审计器的自检基线：重跑 `--freeze` 必须写出完全相同的字节，任何成员被改动都必须被 `check_frozen` 发现。
包内每个成员都由本文件的手工内容构成，不存在生成脚本。

## 边界

- `scientific_status = not_run`：包里没有真实教材、没有真实 Obsidian 库、没有真实链接。
- `uses_network = false`、`uses_model = false`：冻结包与产品流程都不联网、不调用模型。
- 只覆盖「小节提升 + 引用迁移」这一条路径，不覆盖标题重命名、笔记合并、附件搬家、块引用或嵌入。
- 四条变体各只改动一处，用来证明单条规则会整包拒绝；拒绝之后不得留下任何迁移根。
