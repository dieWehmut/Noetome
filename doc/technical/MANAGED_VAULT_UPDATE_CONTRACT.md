# 托管测试集合的人工审批与可恢复发布契约

本契约服务 [B07-T03](../execution/B07_STEPS.md)，关联 FR-29、FR-30、FR-31、FR-32、FR-33、FR-34、NFR-07、NFR-08、NFR-14 与 AC-21/22/24/25/27/28。它复用 [B07-T01](RUN_CHECKPOINT_CONTRACT.md) 的普通文件身份守卫、[B07-T02](REQUEST_BUDGET_CONTRACT.md) 的独立 SQLite 追加账本与 sidecar 拒绝，以及既有 `case compile` 新根流程建立初始基准。实现和执行检查分别记录，真实完成状态以[任务账本](../../dev-log/tasks.json)为准；命令、字段或计划存在不算审批或发布验收。

验收作者包为 [managed-vault-update-v1](../../fixtures/managed-vault-update-v1/README.md)。它冻结两篇笔记、一个 PNG、一条链接与一份明确的操作集合；`cases.json` 与 `expected` 是独立检查器的预期，Core 不从它们生成产物。本项不接管生产 Vault、不扫描未选取目录、不接模型/网络/凭据，也不宣称完整桌面审批界面、规模或任意断电原子性。

## 范围与接口

```text
noetome vault adopt    <author.json> <尚不存在的托管根>
noetome vault plan     <托管根> <候选目录> <尚不存在的计划文件>
noetome vault approve  <托管根> <计划文件> <尚不存在的批准文件>
noetome vault publish  <托管根> <批准文件>
noetome vault status   <托管根>
noetome vault rollback <托管根> <回滚计划文件>
```

`adopt` 的父目录须已存在。六个动词都不接受 `--force` 类覆盖开关。动词后可用 `--` 分隔以 `-` 开头的路径；中文和含空格路径作为独立参数传入。`status` 只读，不写库、不修复、不发布。

## 托管根布局

```text
new-owned-root/
  control/
    owner.json          原所有权声明与初始基准指纹
    vault.sqlite        Schema1，追加式计划与发布账本
    update.lock         发布与回滚的排他锁
  baseline/             首次 adopt 时冻结的基准字节
  vault/                面向读者的新测试 Vault，允许人工编辑
  releases/<release_id>/  每次发布的候选原件与逐项日志
```

托管根只允许 `control`、`baseline`、`vault` 与已开始发布的 `releases`。未知文件、未知空目录与数据库 sidecar 一律保留原样并拒绝继续。数据库、日志与候选不放进 `vault/`。

`control/vault.sqlite` 的 `user_version=1`、`application_id=0x4e56554c`，只允许三张严格表：

| 表 | 字段与作用 |
| --- | --- |
| `metadata` | `singleton`、`schema_version`、`metadata_json`；唯一所有权、策略版本与初始基准清单 |
| `plans` | `plan_id`、`kind`、`plan_sha256`、`payload_json`、`created_at`；每次 plan/rollback 的原文与指纹 |
| `releases` | `sequence`、`release_id`、`approval_sha256`、`status`、`entries_json`、`at`；连续序号与逐项落盘日志 |

三张表均使用 SQLite DELETE journal 与 FULL synchronous。SQLite 事务与多文件写入是不同边界，恢复须分别重验。

## 三方字节

每次 `plan` 都同时保存并比较三份字节，缺一不可：

| 来源 | 含义 | 缺失时行为 |
| --- | --- | --- |
| 基准 | `baseline/` 中 adopt 时冻结的字节 | 无基准即拒绝，不推定 |
| 当前磁盘 | `vault/` 中此刻的真实字节 | 由人工新建，允许与基准不同 |
| 新候选 | `<候选目录>` 中显式列出的操作目标 | 必须有 `operations.json` |

只在 `vault/` 存在但 `baseline/` 缺失的根上不执行任何写操作，保持只读并明确报告。陌生 Vault（人工已有内容、无 `control/`）一律拒绝接管，什么都不改。

初始基准的可信性由 `adopt` 单独建立：它从作者包 `baseline/` 逐字节复制并重新读回比对，同时把作者清单的 SHA256 与复制结果的 SHA256 分别写入 `owner.json` 和 `metadata`。两份不一致时 adopt 失败且不留下托管根。

## 候选、操作与影响

候选目录必须含严格 `operations.json`（`policy=managed-vault-update-v1`、`case_id`、`summary`、`operations`）。操作只有五种：`edit`、`link`、`attach`、`rename`、`delete`。

影响按类别分别列出，不合并成一个总数：正文、结构、位置、链接、附件、删除。任何候选中出现托管集合（`author.json` 的 `notes` 与 `resources`）之外的路径，整份计划被拒绝并给出 `path-outside-managed-set`，不是跳过该条后部分执行。

链接影响单独计算：解析 `[文本](目标.md)` 后核对目标是否仍解析到托管集合中的某个路径。重命名若使既有引用无法解析，计划以 `link-would-break` 拒绝；候选必须在同一次计划里给出改写该引用的 `edit`。`delete` 还需要批准文件显式携带该操作，否则计划以 `delete-requires-explicit-flag` 拒绝。

## 审批绑定

`approve` 产出的批准文件是一个严格记录，绑定四样东西，缺一即失效：

1. 计划原文的 SHA256（候选与配置版本随之固定）；
2. 计划时记录的 `vault/` 每个受影响路径的 `FileStamp`（`identity`、`sha256`、`byte_length`、`modified`）；
3. 完整影响清单的规范序列化；
4. 托管根 `owner.json` 与 `metadata` 的指纹。

`publish` 在写入任何字节之前重新采集当前磁盘指纹。任一受影响路径与批准时不同（存在、缺失或内容/身份变化），即以 `approval-stale` 拒绝并保持 `vault/` 原样。这就是「写前再编辑令批准失效」：不需要额外的失效命令，人工编辑本身就让旧批准过期。

`approve` 本身不写 `vault/`。未批准覆盖计数在 `status` 与发布日志中恒为 0。

## 暂存与逐项落盘

`publish` 分三段，每段有独立的可验证边界：

| 段 | 动作 | 通过条件 |
| --- | --- | --- |
| stage | 把候选字节写入 `releases/<release_id>/candidate/` 并 sync | 读回字节等于候选，资源类型/尺寸可解析（PNG 走既有校验） |
| verify | 重读暂存内容，与计划的影响清单、链接解析和索引逐项对账 | 影响条目数、路径集合、链接目标全部一致 |
| commit | 先写 `entries_json` 的 pending 记录，再逐项写入 `vault/`，每项后更新账本 | 每个条目落盘后 `vault/` 读回等于候选 |

只有三段全部完成才把该 release 标为 `published`。中途终止时账本保留 `pending` 与已完成条目；`status` 报告 `needs_recovery`，`publish` 可对同一 `release_id` 继续，不重新暂存已确认条目。

## 回滚

`rollback` 不写 `vault/`，只生成一份反向计划（把已批准的条目恢复为基准字节）。该计划同样需要 `approve`，因此回滚也受「后续人工改动令批准失效」约束。反向计划不得覆盖在发布之后被人工编辑过的路径；发现冲突时以 `rollback-conflict` 停止并列出路径，不做静默强制覆盖。

## 独立验收

作者包、`cases.json` 与 `expected` 先冻结，再写实现。独立检查器（`scripts/verify_managed_vault.py`）只读 `control/vault.sqlite`、`releases/`、`vault/`、`baseline/` 与作者包，重建三方字节、影响清单、审批绑定与逐项日志，与 CLI 报告相互核对；它不调用 CLI 的判定函数。

至少覆盖：八类候选的批准/拒绝分流、未批准覆盖 0、批准后人工编辑令批准失效、陌生 Vault 只读、无基准只读、暂存失败不留部分 `vault/`、真实 SIGKILL 后恢复一致、回滚识别后续人工改动，以及 `vault/` 之外的无关文件与 `.obsidian` 配置保持原字节。完整 Rust/Python 回归通过后才更新完成日志并推送；离线结果不推为生产 Vault 保护或任意断电原子性已完成。
