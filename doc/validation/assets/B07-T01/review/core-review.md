# B07-T01 独立源码复核第三轮：控制事务竞态修复

本轮在 finalRound04 冻结源码上做只读复核，没有修改 Rust、Cargo、fixture 或账本，没有执行新的 CLI、cargo 或模型请求。复核范围为迟到控制的写事务接纳、真实历史链、metadata before-image 保留、完成后控制 ACK、诊断 hook 与固定作者包。旧审查 `.artifacts/b07-t01-root-core-review-20261106-01/`、`.artifacts/b07-t01-root-core-review-20261107-02/` 和实际 RED 均保留，不将它们改写成最终版本通过。

结论：没有发现本轮新增阻断。之前可复现的“执行者已成功、控制器依据事务前 running 读数仍插入 pending 控制”竞态有实际 RED，修复后的同一两线程测试有实际 GREEN；独立读取原 SQLite、stdout/stderr、原始 checkpoint payload 和文件字节得到一致结果。

## 实际 RED / GREEN 原件

| 项目 | RED | GREEN |
| --- | --- | --- |
| 根目录 | `.artifacts/b07-t01-control-race-red-01-20261107/` | `.artifacts/b07-t01-control-race-green-01-20261107/` |
| 原执行 PID / 退出码 | 647396 / 101 | 647778（cargo 父进程）/ 0 |
| 原 stdout | 0 passed; 1 failed | 1 passed; 0 failed |
| 控制器结果 | `Ok(1)` | `Err(RUN_TERMINAL)` |
| 实际控制表 | 1 条 pause | 空表 |
| worker 实际状态 | 29 checkpoint、6 confirmed、每单元 attempt=1、succeeded | 29 checkpoint、6 confirmed、每单元 attempt=1、succeeded |
| publication | 唯一真实 SQL COMMIT，published | 唯一真实 SQL COMMIT，published |
| 控制前后 DB | 344064B，SHA 改变 | 344064B，逐字节相同 |

RED 固定诊断 ELF 实际重新读取 SHA `732593f53d55355dcb4dce417d36b451fdb66b7b202ba4a96d14246a6fadf336`、188020608B，与 RED `selected_binary` 相同。GREEN 原件保存的是实际 cargo 命令及原 stderr 中执行的 test driver 路径，未额外保存 selected_binary 字段；本复核不为 GREEN 补造 ELF 指纹。GREEN 三项 source before/after 实际相同，并与本轮冻结源一致。

独立 observer 比较 `actual.json.actual_race` 与真实测试生成的 `actual-run/actual-race.json` 完整对象，重新计算 before/after.sqlite SHA 并与实际 control/run.sqlite 字节相等关系核对。随后以 `mode=ro&immutable=1` 读取两份原数据库，重算 29 条原 JSON payload SHA、previous 引用、序号及 Owner RunID；实际最新事件均为 publication 的 `unit_confirmed`，状态 succeeded、ACK=0。RED 额外控制行确实存在，GREEN 没有该行。两份 compile.sqlite 的 quick_check、唯一 publication commit、Release/Run 绑定、报告原件 SHA 以及 staged/Vault Markdown 对作者 gold 的逐字节相等检查全部通过。全部被读原件读取前后 ID、mtime、长度和 SHA 相同。

测试没有构造或伪造成功 checkpoint。源码测试实际复制冻结 fixture，`start_with_hooks` 执行整个六单元流程，在真实 `publication_committed` 边界使用同步 channel 持有 worker；控制器先经过实际 reload/旧终态检查，并在开始 SQL 事务之前向另一同步 channel 发握手。测试仅在确认控制器已到此边界后释放 worker、等待其真实成功返回、只读核对 succeeded 后才释放控制器。这明确覆盖事务前读取与事务内写之间的窗口，既不靠 sleep 争取偶然调度，也没有改变预期来接受迟到插入。

## 冻结源码内的接纳与副作用顺序

`run_store.rs:534` 的生产 `request` 委托 `request_with_before_transaction(action, || {})`。诊断 closure 在合法动作检查之后、BEGIN 之前执行；真实阻塞 channel 仅由 `fixture_run.rs` 的 `#[cfg(test)]` 模块传入，生产路径是空 closure。

`run_store.rs:555` 获得 `TransactionBehavior::Immediate` 后立即用 `history(&tx, &self.metadata.owner.run_id)` 读取实际数据库，而不是复用事务前的 self.state。该函数核对真实 control chain、checkpoint 原始 payload SHA/previous/连续序号、RunID、严格 created 初始状态及后续转换；当前终态匹配 cancelled/succeeded/invalidated/failed 时直接返回 `RUN_TERMINAL`。这段 return 在 `journal_original_metadata`、控制序号分配和 INSERT 之前，因此拒绝不会先归零 metadata 或写请求。GREEN 的库逐字节相同与此排序相符。

`journal_original_metadata` 在实际写事务内读取原始 metadata JSON，先按其 UTF-8 BLOB 字节长度写 zeroblob，再恢复原始字符串，之后才写 checkpoint/control，最后统一 COMMIT。零值中间态不在独立事务提交；原文与 overflow 页 before-image 通过真正 page dirty 留在同步 journal 中。helper 的完整实际 metadata cell/local+overflow before-image provenance 门禁没有因本轮控制修复放宽。热恢复镜像仍需要实际 owner/input/context/chain/output 全部只读重验；compile 库仍要求原 snapshot 的 whole image SHA/长度/身份。未知、冷零头、外来、损坏或无 original metadata 来源的 journal 拒绝并保留。

`completed_control` 仍只允许 old/new succeeded、旧 confirmed=6、固定 diagnostic `completed before control took effect` 和严格增长的实际 ACK。history 确认 ACK 不越过真实 controls，工作事件不能冒认控制，cancel 前缀不能由 resume 跳过。控制类转换保留 units、attempts、manifest、snapshot、intent、报告和 checks。执行者在六单元确认后补 ACK；成功态 resume 在重新验证输出后仅补该 ACK，已确认单元/发布不重做。新的事务内终态拒绝避免 worker 已完成并退出后再出现无法由该 worker 处理的新 pending 控制。

## 作者包与 Git 字节属性

固定作者包共有17文件，其中16个被 frozen-sha256.json 签名；所有成员 SHA/长度与冻结清单精确相同，没有额外/缺失文件。原清单 SHA `240cf6bf13c082c24c93d92ab47a5e76cadb799beb4fe162d2665e9fbc12ccd1`。8份原 fixture 输入及第9份 `compile_expectations.rs` 作者原件，分别与当前原位置和冻结时 commit `02b55fc2897ba453275b880429da840d479e522b` 的 git 原字节再次核对，全部相等。

仅从旧作者 ALPHA/BETA `concat!` 中逐个字符串字面值重新拼合，未运行新 renderer 或 Run engine：甲容器462B、SHA `33eedf491e2ab58e5ee7d2da80c701e91bb0c77700b0212672a1ddbed2bfa376`；乙容器244B、SHA `349f06a40ea199f4d2fb2f6ebcecf61aa652a571ea18e1e81f665a11cf505b88`。两份文件都以恰好两个 LF 结束，不含 CR。`git check-attr` 对全部17文件实际返回 text unset；gold 的 `whitespace=-blank-at-eof` 只保留已签名的末尾空行，不改变字节。稳定 Markdown、知识/计划身份和分母与运行 UUID、时间、attempt 审计的区别维持作者原规则。

## 来源与实际范围

本轮保存10项源码/配置原件副本和前后 SHA，全部复核后相同。关键源为 run_store `605542d0ba32dc317da20f1c2932af84cd208bf212e17324a91213509a12cd63` /45947B，fixture_run `2c2fa5c040d85bff1ef22a94c8fd8c7a91e00e98c29df01fd83477802d353eeb` /36451B；helper、compile_store、build.rs、Cargo 与此前审查的对应原件保持。详情在 source-pins.json 和 source-after-review.json。

本审查证明上述特定源码顺序、特定真实 RED/GREEN 及作者原件完整性。最终源码绑定的全边界强制终止、暂停/取消 ACK 击杀、完整 Windows/Linux 回归和发布收束由各自实际运行证据验收，不能从此只读审查推定已经通过。科学质量保持 not_run，本轮没有访问模型/凭据/生产 Vault，没有推进 B07-T02/T03。
