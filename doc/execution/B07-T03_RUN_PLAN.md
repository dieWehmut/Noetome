# B07-T03 执行方案：明确托管测试集合的人工审批与可恢复发布

任务状态只由 [任务账本](../../dev-log/tasks.json) 维护；本文件是执行细节，不构成验收。契约见[托管测试集合的人工审批与可恢复发布契约](../technical/MANAGED_VAULT_UPDATE_CONTRACT.md)，作者包见 [managed-vault-update-v1](../../fixtures/managed-vault-update-v1/README.md)。

## 单一可见结果

一个新的小型托管测试集合（两篇笔记、一个 PNG、一条链接、五种操作）在人工编辑后提出更新时：审批绑定当前字节，未批准覆盖为 0；发布中断后恢复为可核对的一致版本或明确待恢复；回滚识别发布后的人工改动。

## 先冻结、再实现

1. 作者包 `fixtures/managed-vault-update-v1/` 与 `cases.json` 的八类预期先冻结，Core 不从 `expected` 生成任何产物。
2. 先记录真实 RED：`noetome vault` 命令组不存在、不产生任何根（原件 `assets/B07-T03/red-vault-absent.txt`）。
3. 再定义契约（`doc/technical/MANAGED_VAULT_UPDATE_CONTRACT.md`），之后写实现。

## 实现切分

| 模块 | 职责 |
| --- | --- |
| `crates/noetome-cli/src/vault_model.rs` | 作者包/候选/操作的严格解析、`analyze()` 影响与条目计算、链接解析、影响规范序列化 |
| `crates/noetome-cli/src/vault_store.rs` | 托管根布局与所有权、`owner.json`、`control/vault.sqlite`（Schema1）、`adopt`、`status` |
| `crates/noetome-cli/src/vault_update.rs` | `plan`、`approve`、`publish`（stage/verify/commit 与恢复）、`rollback` |
| `crates/noetome-cli/src/lib.rs`、`main.rs` | `vault` 命令组六个动词的解析与分派 |
| `scripts/verify_managed_vault.py` | 独立检查器：只读磁盘与 sqlite，重算三方字节/影响/审批绑定/逐项日志 |

六动词（不含 `--force` 类覆盖开关；`status` 只读）：

```text
noetome vault adopt    <author.json> <尚不存在的托管根>
noetome vault plan     <托管根> <候选目录> <尚不存在的计划文件>
noetome vault approve  <托管根> <计划文件> <尚不存在的批准文件>
noetome vault publish  <托管根> <批准文件>
noetome vault status   <托管根>
noetome vault rollback <托管根> <回滚计划文件>
```

## 验证矩阵（必须实际运行）

| 项 | 期望 |
| --- | --- |
| adopt | 新根建立，`baseline/` 与 `vault/` 逐字节等于作者包；父目录不存在/root 已存在被拒绝且不留根 |
| 八类候选 | `cases.json` 的 approvable 全部可批准并发布；拒绝类返回对应 code，且不产生计划文件、不写 vault |
| 批准绑定 | 批准后人工改同一文件 → `publish` 以 `approval-stale` 拒绝，vault 原字节不变 |
| 未批准覆盖 | 恒为 0（`status` 与发布日志） |
| 陌生/无关内容 | `foreign/`、`.obsidian` 配置、无关附件字节前后不变 |
| 无基准 | 只有 `vault/` 没有 `baseline/` 的根只读，不写任何字节 |
| 发布中断 | commit 中途真实 SIGKILL → `status` 报 `needs_recovery`；再 `publish` 继续到一致版本 |
| 回滚 | 发布后人工编辑 → `rollback` 以 `rollback-conflict` 列出路径，不静默覆盖 |
| 独立审计 | `scripts/verify_managed_vault.py` 与 CLI 报告互相核对一致 |
| 回归 | `cargo fmt --all --check`、`cargo check -p noetome-cli`、`cargo test -p noetome-cli`；Python 套件按 B07-T02 记录的锁定运行时与 UTF-8 模式运行 |

## 明确不在本卡范围

不接管生产 Vault、不扫描未选取目录、不接模型/网络/凭据、不提供桌面审批界面；不宣称千页/万原子、3000 笔记/200 人工改动、24 小时或任意断电原子性已通过。