# diagnostics-export-v1：带范围与内容预览的诊断导出夹具

这是 `diagnostics` 动词（FR-40 / NFR-08）的冻结夹具。**全部内容是自造的**：没有真实教材、没有真实日志、没有真实模型调用，`scientific_status` 恒为 `not_run`。它证明的是「勾选决定导出内容、未勾选如实记为未导出、绝对路径与凭据整包拒绝、清单不带时间戳」这几条口径可独立复算，不是「诊断包在真实事故里够用」。

## 成员

| 成员 | 作用 |
| --- | --- |
| `project.json` | 主声明：3 个范围类别（2 非敏感 + 1 敏感）、4 条条目、只勾选前两个类别、未记录敏感同意 |
| `project-consent.json` | 变体：把敏感类别 `source-body` 也勾上，但仍未记录同意 → `DIAGNOSTICS_CONSENT` |
| `project-absolute.json` | 变体：预览里粘进 `/home/…` 绝对路径 → `DIAGNOSTICS_ABSOLUTE_PATH` |
| `project-secret.json` | 变体：预览里出现 `password=hunter2` → `DIAGNOSTICS_SECRET_MARKER` |
| `project-unknown.json` | 变体：勾选了不存在的类别 `network-capture` → `DIAGNOSTICS_ARGUMENT` |
| `project-empty.json` | 变体：`run-logs` 没有任何条目 → `DIAGNOSTICS_ARGUMENT` |
| `expected.json` | 实现前手写的先验（13 项计数、三条类别、三条条目、五条拒绝、6 条推导） |
| `cases.json` | 冻结序列、五个变体各自的期望码与退出码、边界 |
| `frozen-sha256.json` | 本目录全部成员的摘要（由审计器 `--freeze` 生成） |

## 先验怎么来的

1. **条目与字节**：`journal-events` 有 `ev-1`(120) 与 `ev-2`(180) 共 2 条 300 字节；`run-logs` 有 `log-1`(400) 共 1 条 400 字节；`source-body` 有 `src-1`(9000) 共 1 条。合计 4 条条目。
2. **已导出 = 勾选的两个类别**：3 条条目、700 字节（120 + 180 + 400）。
3. **未导出 = 剩下那个类别**：1 个类别、9000 字节。未勾选的类别必须如实标为未导出，导出条目为 0 —— 这正是「不静默补全」的可检查形式。
4. **`sensitive_included` = 0**：三个类别里唯一的敏感类别是 `source-body`，它没有被勾选，所以即使有同意也不会进包。同意只在「勾选了敏感类别」时才是前置条件，这是两件独立的事。
5. **`sensitive_categories` = 1**：敏感类别的总数，与有没有勾选无关；它让读者一眼看出这个包本可以包含多少敏感材料。
6. **五个变体各破坏一条规则**，且都在建根之前被拒绝（`adopt` 先做一次完整预演，实测每个变体的 `left=0`）：缺同意（exit 3）、绝对路径（exit 3）、凭据字样（exit 3）、勾选不存在的类别（exit 2）、空类别（exit 2）。
7. **绝对路径扫描不是只看开头**：`project-absolute.json` 把路径粘在句子中间（`见 /home/lyinn/… 下的样例。`），仍然必须被拦下 —— 一旦漏检，诊断包会把机器布局一起带出去。
8. **清单不带时间戳**：输出清单里没有 `created_at`，同一冻结声明的两次导出逐字节相同；`adopt` 写下的 `control/manifest.json` 才有 `created_at`（它是所有权的记录，不参与确定性比较）。

## 复现

```bash
W=$(mktemp -d /tmp/diag-XXXX)
./target/debug/noetome diagnostics adopt fixtures/diagnostics-export-v1/project.json "$W/root"
./target/debug/noetome diagnostics build "$W/root" "$W/out"
./target/debug/noetome diagnostics build "$W/root" "$W/out2"     # 两次导出应字节相同
python3 scripts/verify_diagnostics_export.py "$W/root" --output "$W/out" --package fixtures/diagnostics-export-v1
python3 scripts/verify_diagnostics_export.py --package fixtures/diagnostics-export-v1   # 只审夹具
python3 scripts/verify_diagnostics_export.py --package fixtures/diagnostics-export-v1 --freeze
```

## 边界

- `scientific_status: not_run`：五个变体只证明规则被拦下，不证明真实事故里现场就长这样。
- 合成：预览文本是手写的，不对应任何真实运行产生的日志。
- 不联网、不调用模型、不写项目与 Vault：`adopt` 只写 `control/`，`build` 只写输出根。
- 预览不是原文：声明里写的是短摘要，本夹具不含任何正文材料。
- 本夹具不覆盖：真实崩溃现场的采集、真实日志轮转、存储保留与清理策略（属 B14-T03）。
