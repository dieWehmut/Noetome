# journal-audit-v1：可定位事件的审计日志夹具

这是 `journal` 动词（FR-40 / NFR-15）的冻结夹具。**全部内容是自造的**：没有真实教材、没有真实笔记、没有真实模型调用，`scientific_status` 恒为 `not_run`。它证明的是「日志的四层定位与排序口径可独立复算」，不是「抽取质量合格」。

## 成员

| 成员 | 作用 |
| --- | --- |
| `project.json` | 主声明：2 来源、2 知识对象、2 受影响文件、2 任务、6 事件 |
| `project-no-source.json` | 变体：`task-parse` 不声明任何来源 → `JOURNAL_ARGUMENT` |
| `project-duplicate-at.json` | 变体：同一任务两条事件都用 `at = 2` → `JOURNAL_ARGUMENT` |
| `project-secret.json` | 变体：来源描述里出现 `password=hunter2` → `JOURNAL_SECRET_MARKER` |
| `project-foreign-file.json` | 变体：`task-compile` 声明了属于别的对象的文件 → `JOURNAL_ARGUMENT` |
| `expected.json` | 实现前手写的先验（计数、排序、任务表、失败表、严重级、四条拒绝、7 条推导） |
| `cases.json` | 冻结序列与五个期望（含未知根 `JOURNAL_UNKNOWN_ROOT`） |
| `frozen-sha256.json` | 本目录全部成员的摘要（由审计器 `--freeze` 生成） |

## 先验怎么来的

1. **计数**：`task-parse` 有 e1..e4（started、failed、retried、resolved），`task-compile` 有 e5、e6（started、failed）。所以 started 2、failed 2、retried 1、resolved 1，合计 6 条事件、2 个任务。
2. **排序**：三张表（任务、失败、事件）统一按**任务 id 再按序号**排序。`task-compile` < `task-parse`，所以 e5、e6 与 `task-compile` 排在 `task-parse` 之前。这条不是实现细节：如果任务表按声明顺序、事件表按 id 排序，同一份日志会有两种说法，重排声明就会悄悄改变读者看到的东西。
3. **失败定位的一条 = 四层**：每条失败记录都带 `sources`、`objects`、`files`（取自所属任务），加上 `task_id` 与 `task_title`。定位不到这四层的失败在声明阶段就被拒绝——这正是 `project-no-source.json` 与 `project-foreign-file.json` 覆盖的场景。
4. **`followed_by_action`**：e2 之后同任务有 `retried`（e3）与 `resolved`（e4），所以为 `true`；e6 之后 `task-compile` 再无事件，所以为 `false`。没有这个标志，读者分不清「我们重试了」和「我们放弃了」。
5. **未处理失败 = 1**：就是 `followed_by_action` 为 `false` 的失败事件数。
6. **任务结果**：取该任务最后一条事件的 `kind`。`task-parse` 收在 `resolved`，`task-compile` 收在 `failed`。
7. **四个变体各违反一条规则**，且全部在建根之前被拒绝：`adopt` 会先做一次完整预演，失败时连根目录都不会创建（实测每个变体的 `left=0`）。

## 复现

```bash
W=$(mktemp -d /tmp/journal-XXXX)
./target/debug/noetome journal adopt fixtures/journal-audit-v1/project.json "$W/root"
./target/debug/noetome journal build "$W/root" "$W/out"
./target/debug/noetome journal build "$W/root" "$W/out2"      # 两次构建应字节相同
python3 scripts/verify_audit_journal.py "$W/root" --output "$W/out" --package fixtures/journal-audit-v1
python3 scripts/verify_audit_journal.py --package fixtures/journal-audit-v1   # 只审夹具
python3 scripts/verify_audit_journal.py --package fixtures/journal-audit-v1 --freeze
```

## 边界

- `scientific_status: not_run`：四个变体只证明规则被拦下，不证明真实任务会以这种方式失败。
- 合成：事件文本是手写的，不对应任何真实运行的日志。
- 不联网、不调用模型、不写项目与 Vault：`adopt` 只写 `control/`，`build` 只写输出根。
- 本夹具不覆盖：真实崩溃恢复（FR-35）、真实在途请求状态显示、日志轮转与保留策略（属 B14-T03）。
