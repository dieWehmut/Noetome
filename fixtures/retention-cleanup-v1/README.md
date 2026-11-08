# retention-cleanup-v1

保留与清理策略的冻结输入。全部内容为自造数据：没有真实教材原文、没有真实运行日志，也不含任何凭据。
本包只证明「保留与清理计划」的判定与字节确定性；它不证明真实存储清理、已发布版本退役或恢复演练。

## 成员

| 文件 | 作用 |
| --- | --- |
| `project.json` | 基线声明：2 个可清理类别、5 条存储条目、1 个已发布版本 |
| `project-protected.json` | 变体：把 cache 保留期缩到 10 天，使受保护条目过期 |
| `project-unknown.json` | 变体：`log-1` 引用未声明的类别 |
| `project-missing.json` | 变体：`rel-1` 声明了不存在的必需条目 `index-2` |
| `project-duplicate.json` | 变体：两条条目都用编号 `log-1` |
| `expected.json` | 实现之前手写的判据：计数、逐条动作、受保护条目、影响说明、四个变体的拒绝码 |
| `cases.json` | 调用序列与变体期望 |
| `frozen-sha256.json` | 冻结清单（由 `--freeze` 生成） |

## 先验是怎么推导出来的

- `log-1`：`run-logs` 保留 14 天，年龄 21 天 → 超过保留期 → 删除；它带 `recovery`，所以记 1 条恢复能力丧失。
- `log-2`：`run-logs` 保留 14 天，年龄 3 天 → 未超过保留期 → 保留。
- `cache-1`：`cache` 保留 30 天，年龄 45 天 → 超过保留期 → 删除。
- `index-1`：年龄 12 天未过期，且是 `rel-1` 的必需索引 → 保留，理由「已发布版本仍需引用」。
- `trace-1`：年龄 20 天未过期，且是 `rel-1` 的必需追溯 → 保留，理由同上。
- `deleted_bytes = 400 + 9000 = 9400`；`kept_bytes = 300 + 1200 + 700 = 2200`；`deleted + kept = 5`。
- `trace-parse` 原本 3 条（`log-1`、`log-2`、`trace-1`），删掉 `log-1` 后剩 2 条。
- 四个变体各破坏一条规则，且**都不留根**：保留期缩短到 10 天会让受保护条目过期（`RETENTION_PROTECTED`，整条拒绝）、条目引用未声明类别、必需条目不存在、条目编号重复。

## 冻结与复现

```bash
PYTHONUTF8=1 python3 scripts/verify_retention_cleanup.py --package fixtures/retention-cleanup-v1 --freeze
PYTHONUTF8=1 python3 scripts/verify_retention_cleanup.py --package fixtures/retention-cleanup-v1
python3 -m unittest tests.test_verify_retention_cleanup
```

## 边界

- `scientific_status: not_run`：与科学正确性无关。
- 只生成计划，**不删除任何文件**：真正的清理由调用方按计划执行。
- 不联网、不调用模型；模型调用与网络请求在两个产物里都恒为 0。
- 不含凭据；声明里出现疑似凭据字样会整包拒绝（判定在 `scripts/verify_retention_cleanup.py` 与产品里各自独立实现）。
