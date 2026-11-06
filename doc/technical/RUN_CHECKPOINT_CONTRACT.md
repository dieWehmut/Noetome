# 有限 fixture 流程的运行检查点契约

本契约服务 [B07-T01](../execution/B07_STEPS.md) 的固定六单元 CLI 流程，关联 FR-35、FR-40、NFR-08、NFR-13 与 AC-26。它复用既有 fixture 的来源、知识、计划、有限 Markdown 渲染和独立语义核对；只写明确新建的测试运行根。实现和执行检查分别记录，真实完成状态以[任务账本](../../dev-log/tasks.json)为准，命令、字段或计划存在不算恢复验收。

验收作者包为 [run-checkpoint-v1](../../fixtures/run-checkpoint-v1/README.md)。其中 `fixture/` 保留原作者输入，外层 `work-graph.json`、`expected.json` 与规范 Markdown 是独立检查器的预期。Core 接收明确选取的合法 fixture，采用内置六单元顺序，不从外部文件加载调度图，也不从外层 expected 生成产物。作者固定的概念、原子、笔记、章节与节点身份不能因恢复重新生成。

本项不接模型、HTTP、凭据或预算重试，不修改来源项目 Schema1/2/3，不接管旧 `case compile` 根或生产 Vault。跨平台搬移运行、全部知识命令恢复、24小时任务、睡眠恢复、通用调度及增量审批发布分别验收。现有桌面关闭/托盘设置不因此接入后台任务。

## 命令与新根

```text
noetome run start <fixture-root> <new-run-root> [--paused]
noetome run status <owned-run-root>
noetome run pause <owned-run-root>
noetome run resume <same-fixture-root> <owned-run-root>
noetome run cancel <owned-run-root>
```

`start` 完整预检来源和 fixture 契约后创建运行。目标根必须不存在、父目录已经存在，输入与输出不得互为祖先；沿用原路径、大小写碰撞、链接/reparse 和 Windows 不安全组件保护。`--paused` 只创建初始暂停运行。动词后可用 `--` 分隔以 `-` 开头的路径；中文和含空格路径作为独立参数传入。

`resume` 只接续具有同一输入、所有权、策略与编译指纹的运行；不会通过重试重建未知现场。`status` 核对已保存的运行和实际输出，不调度单元、不写检查点、不执行数据库恢复。`pause`/`cancel` 保存控制请求，响应中的请求序号与执行者已确认序号分开。请求已保存不等于在途单元已落盘或进程已退出。

## 两个数据库各自负责什么

```text
new-run-root/
  control/
    owner.json                     原运行所有权声明
    worker.lock                    合作执行者的排他锁
    run.sqlite                     Schema1，调度与控制的持久记录
  output/                          来源单元开始创建，尚未开始时可不存在
    state/compile.sqlite           原六表 Schema1，最终发布依据
    staging/vault/<note-paths>     本次候选原件
    vault/<note-paths>             本次新测试 Vault
    reports/validation.json        固定 typed 验证事实
    reports/validation.md          同一事实的可读报告
```

运行根只允许 `control` 及已开始的 `output`；控制目录的常规完整清单只有 `owner.json`、`worker.lock` 与 `run.sqlite`。未知文件、未知空目录和数据库侧文件不能按名称猜测归属后接管。数据库、日志和未发布候选不放入阅读 Vault。

`control/run.sqlite` 的 `user_version=1`、`application_id=0x4e52554e`，只允许三张严格表：

| 表 | 字段与作用 |
| --- | --- |
| `metadata` | `singleton`、`schema_version`、`metadata_json`；唯一原所有权、编译上下文和完整选取输入库存 |
| `checkpoints` | `sequence`、`previous_sha256`、`payload_json`、`payload_sha256`；连续序号、原始 JSON 字节指纹及前一记录引用 |
| `controls` | `sequence`、`run_id`、`action`、`requested_at`；连续的 `pause`/`cancel` 请求，不覆盖历史 |

`output/state/compile.sqlite` 保留 [B02-T03 编译契约](DATA_CONTRACTS.md)：`application_id=0x4e434d50`，`compile_metadata`、`runs`、`releases`、`coverage_records`、`rendered_files`、`publication_commits` 六表及 Schema1 原含义不变。外层日志与该库使用同一真实 Run/Release UUID，但承担不同责任；外层 `succeeded` 或报告 `validated` 不能替代发布事务。

两个库均使用 SQLite DELETE journal 与 FULL synchronous。SQLite 事务和多文件写入是不同边界；恢复须分别重验，不能把其中一个库的状态复制为另一个库的成功结果。

## 持久字段与版本

运行 DTO、嵌套 snapshot/intent 与身份字段拒绝未知字段和未知枚举。允许 `null` 的 `output_snapshot`、`pending_intent`、`validation_report`、`diagnostic` 仍为必填项，缺失不能当作 `null`。Run、检查点与控制记录的审计时间保留真实 RFC3339 UTC 值；输入文件的 `FileStamp.modified` 则保存自 Unix epoch 起的纳秒数十进制字符串。单元与控制顺序由持久序号确定，不能用时间或文件修改时间代替内容指纹。

| 对象 | 严格字段与不变量 |
| --- | --- |
| `Owner` | `schema_version`、`producer=fixture_run`、`run_id`、`release_id`、`root`、`case_root`、根/父级/control/数据库/锁/fixture 的实际身份、`compiler_fingerprint`、`policy_version` |
| `RunMetadata` | `owner`、`owner_file_identity`、原 `CompileContext`、选取输入的 `inputs`；数据库中的 owner 与磁盘原 `owner.json` 字节和身份均重验 |
| `FileStamp` | `identity`、`sha256`、`byte_length`、`modified`；后者为实际文件 mtime 自 Unix epoch 起的纳秒数十进制字符串，作为附加观察，不代替 SHA |
| `WorkUnit` | `unit_id`、`dependencies`、`state`、`attempts`、`output_paths`；已确认前缀不改写，后继只能在依赖确认后开始 |
| `RunState` | `schema_version`、`status`、六个 `units`、`acknowledged_control`、`checks`、`manifest`、`output_snapshot`、`pending_intent`、`validation_report`、`diagnostic` |
| `Checkpoint` | `schema_version`、`sequence`、`event`、`at`、`run_id`、完整 `state`；序号连续、原 payload 指纹匹配，并逐项检查状态迁移 |

持久身份来自普通文件/目录的实际打开句柄：Unix 使用 device/inode，Windows 使用 volume/index；文件多硬链接、符号链接或 reparse 拒绝。完整原 Run/Release UUID、输入 SHA、作用域、IR/计划修订号与规则版本都必须一致。

编译指纹由 [build.rs](../../crates/noetome-cli/build.rs) 在构建时计算，绑定 CLI 的 Rust 源码、build script、CLI/workspace Cargo 与 lock、实际 `rustc --version --verbose`、目标平台、构建 profile 与编译选项。它不同于版本显示字符串，也不同于可执行文件 SHA。不同平台或构建方式可能得到不同指纹，本契约不承诺把运行目录搬到另一程序、路径或平台继续。

输入库存包含 `case.json`、其明确指定的 IR、NotePlan、语义 expected 和全部选取来源。修改输入字节、文件身份、修改时间、规则/计划版本或编译指纹会拒绝不兼容恢复，保留旧确认历史；兼容条件未满足时不局部迁移或重新封装旧结果为成功。新版本需新建运行。

## 固定六单元与状态

策略版本为 `fixture-six-unit-v1`，工作单元固定为下表顺序，每个单元依赖其直接前驱。

| 单元 | 实际职责 | 确认所需依据 |
| --- | --- | --- |
| `source_contract` | 来源、覆盖、知识、计划、语义 expected 与可渲染性预检，建立独立输出库 | 同一输入守卫、真实检查与完整初始化 snapshot |
| `render` | 在 `staging/vault` 写有限 Markdown，登记 manifest/映射/链接 | 完整源派生字节、实际暂存清单及 staged SQL |
| `stage_verification` | 重读暂存内容与来源/计划/语义 expected 对账 | 必要原子、单位/条件/例外、scope、规范位置和链接逐项通过 |
| `vault_readback` | 只写新自有 Vault，再次重读完整内容 | 暂存/Vault 字节与语义核对、verified SQL |
| `reports` | 保存 JSON/Markdown 两份验证事实 | 同一已持久 typed report、实际文件字节与双报告一致 |
| `publication` | 核验全部依据后一次提交原编译库 | 唯一 publication 记录、Run/Release/Coverage 同事务完成 |

运行状态为 `queued`、`running`、`paused`、`cancelled`、`succeeded`、`invalidated`、`failed`；单元状态为 `pending`、`running`、`confirmed`。`failed` 保存真实错误和最后安全状态，只有显式且完整重验的 `resume` 可重新开始在途单元；不是每类失败都可恢复。`cancelled`、`invalidated` 不可继续。已成功运行的重复 `resume` 核对结果，不追加尝试、不重新发布；若有完成前保存但尚未确认的迟到控制，只追加严格的完成后控制确认。

每个实际开始增加本单元 `attempts`，中断后重新开始在途单元可以增加次数。确认必须对应已开始的当前单元，且一个逻辑单元只确认一次；确认前缀的状态、身份、输出路径和尝试数不因恢复改写。复用仍会重读已保存证据及文件，不等于对旧成功标记免检。

控制请求通过事务保存，有自己的序号和真实时间。执行者检查完整未确认请求前缀，`cancel` 优先于后来的 `pause`。工作事件不能自行增加控制确认序号；暂停/取消事件必须对应新请求的有效动作，`resume` 不能确认或跳过取消请求。暂停要求在2秒内停止调度新单元；在途单元可继续到安全落盘边界，状态与确认序号必须保留其实际进度。取消保留已经确认的结果，不调度尚未开始的后继单元；它不是删除工作区或回滚已确认数据的命令，已经提交的 publication 也不能被控制请求撤销。

新控制的接纳条件在取得 `BEGIN IMMEDIATE` 写事务后重读完整实际检查点链，并在任何 metadata 归零或 INSERT 前拒绝 cancelled/succeeded/invalidated/failed。事务前读取 running 不足以授权随后插入：若执行者已完成，迟到新请求应拒绝且库字节不变，不能留下完成态的新 pending 控制。

在 publication 已提交而单元确认尚未提交时收到的 `pause`、`cancel` 或 `cancel` 后 `pause`，保留请求原序号。六单元确认并成为 `succeeded` 后，执行者以 `completed_control` 确认未处理前缀，诊断固定为 `completed before control took effect`；单元、attempts、manifest、报告及唯一 publication 保持。若进程恰在 publication 确认后、此控制确认前被终止，显式成功态 `resume` 可补同一控制确认；不得追加工作尝试或将已发布结果反写为 cancelled。控制响应保留请求前真实只读 phase，不能因保存请求而伪报已暂停。

## 文件与 SQL 提交窗口

`StoreSnapshot` 只记录 `schema_version`、`context`、实际 `root`/父级/目录身份、文件与数据库的身份/SHA/长度、`phase`、`failure_release_status`、`manifest`、`published`。正文保存在原件文件中，不把完整正文写进 snapshot。阶段为 `planned`、`staged`、`verified`、`failed`、`published`；每次接续仍执行原 Store 的 schema、完整记录、精确树与 manifest 核验。

在任何当前单元文件或阶段写入之前，外层先持久保存 `unit_prepared`。其 `StoreIntent` 只有 `phase`、完整 `manifest`、待写文件 `files`；每份 `FileExpectation` 必须有 SHA 与长度。恢复时从当前来源渲染或从已持久报告重新算完整文件字节，与 sealed intent 精确相等，不能仅复用 intent 自述指纹。

原 snapshot 已确认的目录、文件和数据库身份必须相同，文件 SHA/长度必须相同。只有 intent 明确声明的完整 pending 文件及其必要祖先目录可以纳入；未知 extra、链接、等字节身份替换和截断文件拒绝并保留。新 pending 文件在首次身份记录前以严格 source-derived 全字节和明确路径为准；不能声称一个从未记录过的文件身份已经有旧身份锚。

| 中断窗口 | 允许的接续依据 |
| --- | --- |
| `unit_prepared` 后尚未创建输出根 | 同一 Run/Release 和空 planned intent，可创建原约定的新 `output` 根 |
| 输出初始化完整提交，尚未保存 snapshot | 无旧 snapshot 的空 planned intent，只接纳完整同 ctx、六表与精确初始目录；半初始化保留拒绝 |
| 完整文件 `sync_all` 后，外层进度记录前 | 旧 snapshot 全部原身份保留，已存在 pending 文件等于重新派生的完整字节；缺失的声明文件可继续独占写入 |
| staged/verified SQL COMMIT 后，外层记录前 | 同一数据库身份与一个已预定阶段转换；完整六表记录、manifest 及实际文件共同重验 |
| publication COMMIT 后，外层确认前 | 同 ctx/manifest、双报告原字节和 SHA、唯一 publication 记录及完整 terminal SQL；只能重验并确认，不再次提交 |

已存在 pending 文件通过核对时不会被替换成新文件。未知或不完整现场没有通用 `--force`、吞错恢复或自动清理路径。

## SQLite 热 journal 的独立前置门禁

默认 `Store::resume` 与 `resume_with_intent` 拒绝任何待恢复侧文件，immutable 核验不触发 SQLite 回滚。有限 Run 的显式恢复可以另走只读前置门禁；helper 自身不改写、删除 journal，也不授权未知现场。

前置门禁须验证普通单硬链接文件及精确 `-journal` 路径、锁定 SQLite 页/扇区/记录/checksum、完整有限库存，先在内存重建回滚前影像。`control/run.sqlite` 的影像还必须经过原 schema/history/transition 检查，绑定同一 owner、owner 文件身份、Run/Release、完整 fixture 输入和编译上下文。原 metadata 单例的完整 SQLite record，包括 local payload 与所有 overflow 页，必须来自已同步 before-image，且逐字等于独立解码的完整 UTF-8 元数据；只保存 header、部分字段或共享页不能证明归属。控制库每次写事务先在事务内等长度归零再恢复原 metadata JSON，使原页进入 journal，最终已提交原 JSON 保持不变。`compile.sqlite` 的影像必须同时匹配既存 base snapshot 的原 DB 身份及**整份影像 SHA/长度**；没有 base snapshot 的 bootstrap 热 journal 不接管。

全部只读验证通过、合作 worker 排他锁有效后，只有 SQLite 自身可执行原库的正常回滚。回滚结束再次完整核验 journal 消失、原库身份、原 snapshot 与输入/输出，才允许继续调度。`status` 不执行此过程。冷零首 header、截断/损坏/未知 journal、super-journal、foreign Run、链接/替换、输入或版本变更均保留拒绝。结构有效的 journal 和稀疏 checksum 本身不是所有权证明。

这里定义接线与验收所需的边界；该门禁的真实中断及平台检查须按[本项执行计划](../execution/B07-T01_RUN_PLAN.md)保存最终源码绑定的原件。不能从旧程序的中断通过推定新增门禁通过，也不能从有限 SIGKILL/TerminateProcess 测试推出任意指令、存储设备故障或断电下多文件原子恢复。

## 发布、错误与独立证据

最终发布依据保持原编译事务：插入唯一 `publication_commits`，绑定双报告与 compact manifest SHA，同时写 Run.succeeded/finished、Release.published 和全部 Coverage.validated。事务完成之后 stdout 传输失败不会反写为未发布；再次接续必须保留已提交结果。

成功 stdout 使用 Schema1、`producer=fixture_run`，保留 operation、真实 Run/Release、`run_status`、检查点序号、确认单元数、完整工作单元、`release_status`、控制请求/确认序号、输入 SHA、编译指纹、实际路径、持久报告与诊断。科学质量固定 `not_run`，实际模型/付费调用为0。语法错误返回2；输入/所有权/恢复或数据库错误按稳定 `RUN_*`、`COMPILE_*`、`PROJECT_*` 代码返回实际非零。正常暂停可返回0；取消、失效或失败状态不能包装为成功。

在 publication 已 COMMIT、外层确认尚未提交的窗口，`status` 可在完整只读重验后观察到 `release_status=published`，同时保留日志中真实的 `running`、五个确认单元和在途 publication。它不为对齐显示而改写检查点；显式恢复完成原单元确认后才显示日志成功。

独立检查器读取原作者 Markdown/语义 expected、真实 SQLite、完整 raw checkpoint 链和文件字节，核对每个确认及每份原件。原始 JSON/Markdown/DB、stdout/stderr、PID/强制终止、源与二进制指纹、哨兵和失败历史分别保留。状态库中的连续 SHA 链能发现不一致，但不是对本地全盘改写者的秘密认证；即使同步更改 payload、输出、指纹和报告，来源/作者语义、身份和状态机仍须独立核验。
