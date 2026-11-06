# B06V-T03 全量回归发现的原缓存并发测试就绪修复

Windows首次冻结源A轮完整workspace测试真实exit101：CLI lib467通过、1失败，未继续剩余workspace。失败test为 `pdf_source_model::tests::source_cache_races::unknown_source_cache_entry_added_during_raw_read_is_rejected`，工作线程报 `no live raw guard observed`，主线程join随后失败；该轮不算全量通过。其余mode1/2/3有原始共享违例观测，但不能替代缺失的mode0注入。

[原失败和诊断原件](source-cache-race-failure-originals/manifest.json)先保存旧源SHA `45dc7f4309f445567ef72ffa75cfb1d1bd56c2f639bd36048513610fd995c3e0`、原stderr/stdout/receipt。对实际旧unit PE作独立复制后，三次只运行失败用例的诊断均真实观察mode0Applied并通过；这支持“线程启动未同步，错过观测窗口”的定位，不用三次诊断掩盖原全量失败。

本任务主执行者明确授权最小测试同步修复，仅修改 `crates/noetome-cli/tests/support/source_cache_race_tests.rs`，没有修改生产文件读取/所有权代码或扩大产品能力。工作线程先配置原raw路径的Windows共享探针，并进入循环，然后通过零容量channel通知ready；调用方收到ready才release并开始 `validate_snapshots`。原5秒观测deadline从release握手之后开始，不包括未调度就绪的时间。

保留16MiB原输入、既有1ms探针间隔、Win32错误32必须实际出现的门禁、mode0/1真正写入、mode2/3实际Applied或Blocked处理、owner原字节/身份重查和最终source-cache完整清单断言。就绪失败或仍未观测实际guard依然失败；没有增加等待延时或接受未注入。新源SHA为 `2702b9dfb0d782cdf6262a98bb68e98099f2650ef963633e4e3dcafcfadfca7a`，10,304字节。

B轮独立新冻结source、cache invalidate及fmt均通过，4线程四项raw-cache限定测试全部通过，原stderr实际为mode0Applied、mode1Applied、mode2Blocked、mode3Blocked，耗时6.05秒。随后C轮再新建1,915文件快照，从fmt/check/prebuild到完整workspace及最终build全部actual exit0，726passed/0failed/0ignored；完整运行也实际观察四种mode，所有冻结源及外部哨兵前后相等。

完整A/B/C源、原命令/输出/PE、失败/限定验证与最终全量归档见[Windows Rust记录](../windows-rust/README.md)和[独立重建审计](../windows-rust/audit-result.json)。这个修复只使原并发保护测试先建立观察者就绪顺序；它不建立B07用户任务恢复、生产Vault人工保护或任意未来调度场景的额外完成声明。
