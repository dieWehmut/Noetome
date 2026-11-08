# locked-change-v1

锁定目录、文件名、章节与顺序之后，新增资料只影响声明范围内的笔记。

## 成员

| 成员 | 说明 |
| --- | --- |
| `project.json` | 基准声明：5 份笔记、3 条变更、3 条锁定、1 条重构 |
| `project-lock-file.json` | 文件锁撞上改名提案，变更转候选 |
| `project-lock-dir.json` | 目录锁撞上移出该目录的提案，变更转候选 |
| `project-canonical.json` | 规范位置锁撞上重构移动，硬拒并保留旧位置 |
| `project-missed.json` | 触动概念但漏声明依赖它的笔记，记 `missed_dependency` |
| `project-unexpected.json` | 声明了既不共享概念也不同主题的笔记，记 `unexpected_touch` |
| `project-unsafe-split.json` | 拆分目标没有原子，硬拒（空壳笔记不是拆分） |
| `expected.json` | 实现前手写的判据：计数、已应用清单、未受影响指纹、六变体结论 |
| `cases.json` | 包级元数据与执行顺序 |
| `frozen-sha256.json` | 冻结清单（`--freeze` 生成，重跑必须字节不变） |

## 先验怎么来的

1. 5 份笔记、3 条变更、3 条锁定、1 条重构由声明直接读出。
2. `chg-gly` 提出新路径，但锁只有 file/section/order 三条，且 n-gly 不在文件锁里、章节顺序与位置都没变，所以允许应用。
3. `chg-pfk` 与 `chg-gng` 没有提案，路径与章节保持原样，文件锁与目录锁都不触发。
4. `r-tca-move` 只移动 n-tca 并列出 1 处引用变化，所以 `reference_changes = 1`。
5. n-pyr 不在任何变更或重构里，所以 `unchanged = 1`，指纹取声明值。
6. 三个变更各自只触动本主题的一个概念，且引用该概念的笔记都已声明，所以 `findings = 0`。

## 冻结与复现

```bash
python3 scripts/verify_locked_change.py --freeze --package fixtures/locked-change-v1
python3 scripts/verify_locked_change.py <变更根> --output <输出根> --package fixtures/locked-change-v1
```

## 边界

自造声明与指纹，不含真实教材原文；只验证锁定保持与影响范围，不判断内容质量与科学正确性（`scientific_status: not_run`）。不写项目、不写 Vault、零模型、零网络。
