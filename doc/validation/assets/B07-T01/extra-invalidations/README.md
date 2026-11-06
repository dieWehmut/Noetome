# B07-T01 已确认四单元后的实际依赖失效补验证

仅新的三个 fixture copies/Run。固定 Core05 CLI e7194fa4，Core05 driver 77c7043d，第三案固定 Core04 driver f62c5172；完整程序本体在此前独立程序包，不在本 ZIP 重复。

正式 attempt02 共 7 次 ordinary CLI、3 个实际 driver。每次在 reports/unit_prepared 且 4 confirmed 后，对自己启动的实际 PID SIGKILL 并 wait（exit -9），随后分别追加有效 UTF-8 源字节、同 SHA/长度/mtime 的新 FileID replacement、实际 Core04→Core05 编译指纹变化。三案均实际 invalidated。旧23条 SQL checkpoint 的完整原元组逐项完全保持，只追加1条失效 checkpoint；4确认和旧attempt、Owner/原 metadata、已有 output 完整库存保持，publication SQL=0，重复 resume 全 Run 树相等。

不编辑 checkpoint/payload/Owner/version。源 old/new 全字节及文件ID/SHA/长度/mtime、原始 SQL 与输出树、每次 argv/PID/stdout/stderr、实际握手及 kill 原件完整收录。Recorder 核对执行前后生产/检查器源不变，实际模型/付费调用0，不证明科学质量或生产恢复。

第一次包装器因 verify_fixture_compile import 路径缺失，在调用任何 CLI/driver 之前退出；其脚本及物理 stdout/stderr 同时保留。此基础设施失败不是产品恢复 RED，attempt02 修正仅包装器 sys.path，没有更改冻结公共 source。

ZIP 逐 member CRC/SHA/length 回读及原文件ID/mtime/bytes前后全部通过。任何符号链接只记录自身 target，不跟随到外部。
