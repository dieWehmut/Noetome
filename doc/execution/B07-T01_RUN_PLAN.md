# B07-T01 固定流程检查点实施与验收计划

任务为[B07-T01 执行卡](B07_STEPS.md)的一个固定有限 fixture 流程，契约见[RUN_CHECKPOINT_CONTRACT.md](../technical/RUN_CHECKPOINT_CONTRACT.md)。真实状态以[任务账本](../../dev-log/tasks.json)为准；本文件规定实施顺序和验收方法，预期结果与命令示例不算实测通过。

本轮限定新测试根、两个规范 Markdown 笔记与固定六单元。无模型、HTTP、凭据、预算重试、生产 Vault、增量审批或 GUI 任务中心；不启动 B07-T02/T03，不将有界恢复推成24小时或全部知识命令能力。

## 1 先验作者与真实 RED

[run-checkpoint-v1](../../fixtures/run-checkpoint-v1/README.md) 保留原 `text-compare-v1` 的来源、IR、计划与语义 expected 原字节。外层规范 Markdown 来自此前独立 Rust 字面值，原字面文件也随作者包保存；新 Run 实现或渲染器不能生成或覆盖 expected。

固定分母为两来源、12 Span/Coverage、四必要原子、五证据、两笔记、四节点、九内容映射与两条双向章节链接。独立核对完整单位、条件、例外、未给出属性、规范位置和作者 ID；运行 UUID、时间、attempt 与控制历史分别保存，不用删除审计字段制造原件字节相等。

先保存旧普通 CLI 的实际缺入口/不能接续 RED，绑定旧可执行文件、调用前 fixture freeze SHA、完整 argv/exit/stdout/stderr 与保护前后库存。准备、路径或编译失败不能替代预期的外部行为 RED。旧失败与随后修复均保留，不拿重测覆盖历史。

## 2 有界实现

| 工作 | 最小落点 | 验收关注 |
| --- | --- | --- |
| 命令与六单元执行 | `fixture_run`、CLI parser | start/status/pause/resume/cancel；只接明确 fixture 与新自有运行根 |
| 原运行日志 | `run_model`、`run_store` | 严格 DTO、完整状态迁移、输入/版本/owner、连续 raw 链、单 worker 与持久控制前缀 |
| 输出接续 | `compile_identity`、`compile_store` | 原六表、严格 snapshot、typed prepared intent、实际身份/原件核验、发布一次提交 |
| 热 journal 前置门禁 | `run_recovery` 与 Run 接线 | 完整只读影像/所有权/输入检查后才让 SQLite 回滚；未知、无锚或损坏原件拒绝 |
| 独立验收 | `verify_run_checkpoint.py` 与测试 | 先验作者、实际 SQL/Markdown/raw 链、真实终止与恢复、同步篡改拒绝 |

文件/SQL 写入前持久 `unit_prepared`，完整文件同步或数据库 COMMIT 后保留可观测边界，再保存进度或确认。必须覆盖 output 已提交而外层记录尚未提交的窗口；不能只测试 `unit_confirmed` 后的安全场景。直接 Store API 的热 journal 拒绝与 CLI 独立前置恢复分别测，不互相冒充。

## 3 固定源码与构建来源

最终检查前冻结相关 Rust、Python、fixture 与依赖原件，保存完整成员 SHA/长度及前后 readback。先构建实际 CLI，再构建同源 Rust test driver，保存两者实际文件 SHA/长度和构建日志，核对它们实际输出的 `compiler_fingerprint` 相同。测试程序的持久 callback 只用于外部停在真实已提交边界；继续操作使用普通 CLI。

```text
cargo fmt --all --check
cargo check --workspace --locked --offline
cargo build -p noetome-cli --locked --offline
cargo test -p noetome-cli --lib --locked --offline --no-run
```

以上命令由选定平台与工具链执行，命令文本不表示已运行。构建期间不得另改 Rust 造成 CLI/driver 源指纹不同。Windows 和 Linux 分别保存来源与运行记录；不拿 ELF 或旧 PE 的文件 SHA当作当前 Windows 实际构建来源锚，也不拿 Linux 子进程结果当作 Windows 终止/文件占用保护结果。

## 4 普通生命周期与独立内容核对

先在两个互不重叠的新根执行原 `case compile` 与 `run start`，比较两篇规范 Markdown 的完整字节、稳定知识/计划 ID、内容/链接映射和报告语义。原始 DB/报告 SHA各自保存，独立 fresh run 的 UUID/时间可以不同，同一运行恢复必须保持原身份。

分别验证 `--paused` 初始状态、明确 resume、完成后重复 status/resume、已取消部分结果的保留与拒绝 resume、真实写失败后的显式接续、同根合作 worker 竞争和错误 fixture 参数保护。复用确认单元时确认次数与 attempts 不变；重新开始实际在途单元时保留新 attempt，不重新发布。

## 5 每个持久边界真实终止

先完整追踪最终 driver 的 `created`、每个 `unit_started`/`unit_prepared`/`unit_progress`/`unit_confirmed`、实际 `output_committed` 和 `publication_committed`。每条 handshake 必须绑定真实 PID、Run、已提交 journal sequence、事件/单元 occurrence 和当前编译指纹。

对追踪出的每个边界独立创建运行，实际强制终止持有中的进程，保存终止前/后原件和 exit，再用普通 `run resume` 完成。独立核对原确认前缀、重新开始的 attempts、两篇作者规范字节、每个 scope/链接、双报告与唯一 publication；第二次成功 resume 不能改写库存或增加提交。测试数量由实际 trace 决定，不写死旧程序的边界数为最终覆盖。

另在真实 `publication_committed` 持有期间保存 pause、cancel、cancel 后 pause 三种迟到请求；分别验证正常释放、publication 的 `unit_confirmed` 后但控制确认前强制终止、`completed_control` 已提交后强制终止。普通 resume 应保留六确认/succeeded/published、原单元 attempts 和原件，必要时只补完成后控制 ACK，诊断逐字为 `completed before control took effect`。无控制的 fresh trace 与这些新增有控制边界分别计数。

还需真实 SQLite cache spill 后强制终止，分别核对 control 与 compile DB 的实际 journal。恢复允许场景须证明内存原影像与原所有权/base snapshot匹配，再观察 SQLite 回滚和普通接续；错误或无锚场景须证明 DB/journal/输入/哨兵保持。仅造一个同名侧文件不算热 journal 验收。

## 6 暂停、取消、版本与现场保护

| 类别 | 必须观察的实际行为 |
| --- | --- |
| 在途暂停 | 请求响应与确认分开；2秒内无新单元调度，在途与安全落盘可见，释放后在下个单元前暂停 |
| cancel 后 pause | 完整控制前缀中取消优先；确认旧结果保留，不调度后继、不通过 resume 跳过取消 |
| 临时写失败 | 用真实平台权限/占用制造失败，保存 failed 与原件；解除已确认故障后显式 resume，不能更改 expected |
| 输入与版本 | 原文件/IR/计划/规则变化、等字节换身份、实际旧/新双 CLI 指纹不同，拒绝不兼容继续且旧确认历史保留 |
| 未知/损坏 | 缺字段/未知 Schema/枚举、raw 链错误、未知空目录/文件、截断 pending、损坏/foreign journal、links/reparse/hardlinks、owner/file/DB 替换 |
| 同步篡改 | 同步重算 checkpoint/正文/manifest/报告 SHA 后的假确认、取消 ack 跳过、正文条件/单位/链接缺失仍被拒绝 |
| 路径与范围 | 祖先、碰撞、错误根/fixture、原作者输入与外部哨兵成功和失败后均保持 |

2秒要求核对停止新调度，不能只以 pause 命令返回快替代。测试 callback 持有的在途工作与真实请求/确认时序分别保存；文件系统占用/权限和实际进程终止按平台记录，不可用或未执行明确列出。

另用真实并发控制者在事务前读取 running 后持有，允许真实六单元完成并退出，再释放控制者取得写事务；应在任何 metadata/INSERT 写入前按最新完整链拒绝终态，controls、检查点及输出均保持。该事务接纳竞态与 publication 尚在途时合法保存的迟到控制分别验收。

## 7 可复用审计入口与完整回归

选定解释器应为本轮环境明确返回的绝对路径。脚本接口可用于 Windows 或 Linux 新测试根，平台专用检查须由脚本实际结果区分；占位符须替换成已固定的实际路径与 freeze SHA。

```text
<selected-python> scripts/verify_run_checkpoint.py --cli <absolute-current-cli> --test-driver <absolute-same-source-driver> --author-pack fixtures/run-checkpoint-v1 --fixture-sha256 <actual-frozen-sha256-json-SHA> --previous-cli <absolute-earlier-B07-cli> --root <new-audit-root>
```

`--old-cli-red` 保存旧 CLI 缺接口；`--public-only`、`--interruptions-only`、`--active-controls-only`、`--publication-controls-only`、`--hot-journal-only`、`--temporary-io-only` 用于有据限定复查。各模式作用以最终脚本帮助和原结果为准，部分模式通过不等于完整验收。`--previous-cli` 必须是能建立旧 B07 Run 的实际旧程序，缺失时版本双 CLI 检查保持 `not_run`。

最终执行完整 `cargo test --workspace --locked --offline`、Python `unittest discover`、日志检查及 `git diff --check`；旧 fixture 编译与受影响旧行为按源码作用域执行或提供严格无变化复用证明。独立审阅 Core/state machine、作者核对器、原件库存、编译来源、每条边界和真实失败历史后，才由根代理完成日志与提交推送。原生 Obsidian 输出未变时本项不新增渲染兼容声明，既有 B06V 验收范围也不因检查点测试扩大。

本轮完整回归、最终源码平台行为与热 journal 接线结果仍须在主验证报告登记；本计划不预先填写 passed、最终计数或性能结论。

实际执行结果、最终源码绑定、保留的失败原件与验收边界见 [B07-T01 验证报告](../validation/B07-T01.md)。
