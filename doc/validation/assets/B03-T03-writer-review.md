# B03-T03 Writer 测试子任务报告

Status: DONE。当前仓库唯一新增/编辑文件为 `crates/noetome-cli/tests/support/pdf_source_write_tests.rs`；生产实现、任务账本、标准、Git提交/推送由主执行者负责。本子任务没有提交或推送。独立负控仅修改 compiler-temp 中的副本。

最终模块 SHA256：`52c2da6c3f011378e318c426c1ad15ec0b102ba1b00be544041b8ab6ee8d49dd`。使用冻结873字节 single-page.pdf，SHA256 `1a8bbfe535892b74e2ec1e0fdb6f99320d98e3492441f5f8ae339a57644fc457`，实际锁定 Python/runtime 和真实 PDF adapter。旧MD预期为标题[0,18)、空行formatting[18,19)、正文[19,43)，明确保留3段及原始/规范字节。

## 实际行为与结果

- 7阶段受控Err：Prepared、VersionPublished、ParsePublished、DatabasePublished、ConfigPublished、Validated、Cleanup。每次必须实际到达对应hook、返回PDF_TEST_ABORT、清理已知新树和marker、逐字节恢复cfg/db及旧快照、保留旧SQL四表行/报告；同一项目随后真实PDF重试成功。
- Version/Parse两个发布阶段各注入未知human.keep：实际拒绝成功、保留未知字节、marker/operation/原配置与数据库备份，source/project inspect只读拒绝；独立新项目随后成功。
- Version/Parse两个发布阶段各施加Windows share_mode(1)真实deny-delete句柄：回滚无法清理时保留明确待恢复现场及备份，句柄释放后仍拒绝隐式接管，独立新项目随后成功。
- 5阶段真实Rust测试子进程中断：ready先在pending中sync/写完/sync/关闭，再rename；保留200ms空暂存窗口。父进程核对完整receipt及真实PID、try_wait存活后才kill/wait，确认退出、原SQL历史/快照/备份、inspect bytes/mtime只读及独立重试。五个实际PID事实已保存于 interruption-facts.json。
- 共4个行为组、16个阶段场景，另1个无专用测试env正常返回的子进程入口；该入口不计作独立故障场景。

完整模块实跑 `cargo_local.ps1 test -p noetome-cli --lib source_write::pdf_tests --locked --offline --verbose -- --nocapture`：5通过、0失败、0忽略，测试10.20s/命令13.472s；日志 stdout.log/stderr.log、receipt.json、当时模块 module-before-run.rs（SHA13c989f2...）完整保留。最后仅将原有marker缺失断言提前以避免故障打印整份SQLite字节，全部断言保留；最终SHA52c2...对应受影响7阶段测试在精确源码副本中编译0、测试1通过0失败，详见下述正控。单文件rustfmt --check及独立逐行尾空白检查通过。

## 红绿来源与负控

初轮 `.artifacts/b03-t03-pdf-writer-first-ac91836939f642e9b33b5694748b2ecf` 的4失败来自我漏算旧MD空白formatting，属于测试oracle错误，明确不计产品RED；原源码、输出与exit101保留。修正后首轮故障矩阵5/5绿，证据 `.artifacts/b03-t03-pdf-writer-faults-b0e4210f01844413bdc28ae88d5d2ed3`。这是已功能化候选之后的测试，不冒充实现前RED。

真实敏感性负控位于 `D:\Noetome\target\compiler-temp\noetome-pdf-writer-mutant-dcd9b00a5fe8404aaf6e5878a4b644f1`。复用实际Cargo rustc argv及现有离线依赖，在独立冻结源码副本：

- positive-diagnostic：最终未变模块，原始Writer，编译exit0，已知Err七阶段测试exit0。
- negative-diagnostic：测试逐字节不变，仅将副本Writer::abort改为finished+Ok，省略恢复与owned cleanup；编译exit0，测试exit101，实际错误为“a known PDF abort must remove its owned writing marker”。
- 精确argv、二进制SHA、完整输出、变异差异、原文件指纹见 diagnostic-receipt.json、diagnostic-snapshot-hashes.json、writer-abort-diagnostic.diff及两个对应目录。共享Writer SHA前后均b8665d52745e5136962f2a00f3130894c589180ff8ecf6d09fa382739e56786f，未被本负控修改。旧的完整树差异负控与准备时旧SHA前置检查中止记录也保留。

本模块覆盖上述Windows Writer故障和一个冻结文本PDF；整项CLI、迁移矩阵与最终workspace验收由主执行者汇总，本次未重跑全部workspace。
