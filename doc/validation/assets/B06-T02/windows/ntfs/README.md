NTFS 专项实际通过，绑定源码 `3977ac70cbfc6d5caeb8ed5ce20e9b5fce5907e2` 和最终 CLI `4117afb19b915b56bef501826b1666d7c2e685e9544e6804835970a44c87c169`。

实际 35 条子命令：29 次普通 CLI、4 次独立 Python 核对、2 次 junction 创建；25 个路径接纳场景按预期拒绝，原输入和外部哨兵保持逐字不变。两处自身 junction 已按身份验证移除，四类硬链接别名逐项移除，原始目标保留。

目录 symlink 创建返回 WinError 1314，8 个计划场景未执行，未计入通过数。见 [实际终态](ntfs-final.json)、[命令清单](commands.json) 和 [不可用原件](symlink-unavailable.json)。这不代替 Rust 持有期测试或真实 Obsidian 验证。

首次驱动在 preflight 因 Cargo build 可执行文件有两个硬链接而退出，尚未创建 probe 根或调用 CLI；第二次改用字节相同、单链接的独立副本后通过。两个版本的驱动与两轮原流都在归档内。native 快照 repo、原 build 可执行文件均未改变。

[originals.zip](originals.zip) 保存本专项完整输入/输出、每条 argv/stdout/stderr/receipt、前后指纹、路径与清理原件以及独立外部哨兵。可执行文件不入库，其真实指纹、执行路径和链接计数见 [复制记录](cli-copy.json)。[归档清单](archive-manifest.json) 给出每项原路径、SHA-256、长度和 CRC32；[实际读回验证](final-readback.json) 记录逐条解压读回与原件比较结果。
