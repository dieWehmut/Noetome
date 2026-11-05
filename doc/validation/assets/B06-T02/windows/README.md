最终原生 Windows 六门已在固定提交 3977ac70cbfc6d5caeb8ed5ce20e9b5fce5907e2 的独立快照执行并通过。snapshot.json 保存 1873 个 Git blob 的指纹；native-final.json、native-commands.json 和 native-gates.json 保存实际结果、命令及同一最终 CLI 指纹。归档前后又重读快照及外部自有哨兵，均保持不变。

- cargo fmt --all --check：退出 0。
- cargo check --workspace --locked --offline：退出 0。
- cargo test --workspace --locked --offline -- --test-threads=4 --nocapture：20 个 suite，657 passed、0 failed、0 ignored。
- 原生 Python 全量 unittest：运行 205 项，203 passed、2 skipped。
- 比较表独立 audit：39 个场景、124 次普通 CLI 调用，通过。
- 流程独立 audit：16 个作者场景、41 项检查、73 次普通 CLI 调用，通过。

两项 audit 与 Rust/Python 验证绑定同一最终 noetome.exe：SHA-256 4117afb19b915b56bef501826b1666d7c2e685e9544e6804835970a44c87c169，23674880 字节。运行二进制、构建目录和完整 Git tar 留在本地，仓库仅保留相应指纹。

native-originals.zip 保存 5954 个文件和 706 个目录成员，包括全部六门原始 stdout/stderr、launcher/运行/快照回执、两项 audit 的完整作者/项目/来源/伪造产物，以及 Rust stdout 中宣布的 47 个本任务 process_case 根。两个 native-held-denials 原件共 46 行：46 次写入、46 次删除均被拒绝，错误码均为 32；两种入口的运行仍成功。2 个硬链接成员保留原字节及 links/identity 元数据，本归档范围未遇到需跳过的 symlink/reparse 成员。归档脚本在原生 Windows 用 lstat 检查，任何 symlink/reparse 仅记录元数据而不跟随。

所有 ZIP 成员均完成 CRC、SHA-256、长度和原文件逐字读回；复制进仓库后又核对整个 ZIP 指纹及全部成员。详细成员、来源路径、硬链接元数据、排除项和校验结果见 native-archive.json / archive-copy-readback.json。归档脚本 archive_native.py 只归档终态原件，不运行产品或测试。

原准备阶段 root.json 保留不覆盖；实际已测试根配置另存 root-tested.json，并且在 ZIP 的 native/root.json 中保留原字节。launcher-tool-receipt.json 保存外层 exec 的真实退出 0 与工具交付的文本片段，launcher-tool-output.txt 是相应 LF 规范化拼接，明确不声称是操作系统原始字节。每个 stage 的 .stdout.bin/.stderr.bin 则由原生 Python 直接保存。

六门本身均一次通过。后续只读 owner 退出探针第一次遗漏 PowerShell ExecutionPolicy 参数而被签名策略拒绝，原始错误和外层解析失败记录保留在 ZIP；第二次使用既定启动参数确认所有原 owner PID 已退出。第一次复制归档遇到现有准备 root.json 即停止，未覆盖它，记录在 archive-copy-attempt-1.json；随后将测试配置另名保存。这些归档/监控调整没有重跑或改写六门结果。

NTFS 专项及真实 Obsidian 图、正文和导航由另外的记录分别验收，本归档不据六门通过推定它们通过。自造来源工程检查不证明科学质量，知识仍 pending、scientific_quality=not_run、published=false。
