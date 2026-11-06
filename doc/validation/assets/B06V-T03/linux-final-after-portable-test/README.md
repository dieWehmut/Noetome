# 最终 portable flow test 之后的 Linux 完整 Python

本目录保存测试 setup 修复后的第四份独立最终 Linux 快照。实际 **380 run = 361 passed + 19 明确 Windows 平台 skipped，0 failed、0 error，exit0**；unittest 175.940s，完整实际子进程 181.182s。解释器 `/tmp/noetome-b05-t03-python/bin/python` 3.12.14；固定 ELF SHA `038f106a483945b31594009598decec882bc19d53c73ffc1b462c778b793bfe4` 的两个普通字节副本实际运行，`target/compiler-temp` 为普通目录。

最终 flow 测试 SHA `42b63646f045fd15c75c43cbbb48a58f7b6394b4665fcde92be95e7e4c6e54c5`。新源、原作者、三个视觉冻结包和四原 collector 共 1,971 文件加两份 ELF 指纹，在运行前后逐值完全相等；当次相关 Python/CLI 生产源码工作区子集也完全相等。根代理随后单独改 VISUAL 契约的检查顺序文字，不属于当次源码相等或产品实现声明。

原始 [实际检查](actual-checks.json)、[full receipt](full-python-receipt.json)、[stdout](full-python-stdout.bin)、[stderr](full-python-stderr.bin)、[source-before](source-before.json) 和 [source-after](source-after.json) 均可直接读取。旧三轮失败/适用范围继续保留于 [原目录](../linux-final/README.md)。

| 原件包 | 成员 | Bytes | SHA256 |
| --- | ---: | ---: | --- |
| [source-originals.zip](source-originals.zip) | 1,979 | 71,338,036 | `6604b9dca47d560d52cfaa820824187bf6cece3295f49e0a7e26a25dc2843b28` |
| [final-run-originals.zip](final-run-originals.zip) | 13,131 | 59,368,398 | `752e141671396c80245630890e920e350284ebac79b447f2ae07e7b7e79357d1` |

两包共 15,110 成员，CRC 和每个成员 SHA/长度实际读回通过；完整清单见 [source manifest](source-originals-manifest.json) 和 [run manifest](final-run-originals-manifest.json)。源包额外留存原 flow 测试字节，未覆盖旧 expected 或数据库。

五条既有视觉审计仍复用实际 **317 CLI（table125/flow73/formula33/image55/scan31）**，精确执行作用域相等见 [five-audit-reuse-binding.json](five-audit-reuse-binding.json)。该 scope 不把旧 flow test 源当作新测试证据；新测试已经本轮全套重跑。此处仅记录实际使用的 ELF 字节和 Python 行为，**没有推断旧 ELF 的编译源码**；当前能力到实际构建源码的严格补证另见 [强 WindowsC PE 当前30审计](../review/current-final-pe/README.md)。

真实模型/付费调用为 0，外部累计 33,185 ledger 未读取或修改。没有在本分工新增原生应用捕获或 Linux 完整 Rust workspace 验收。Windows Python 的真实单独运行由根代理保存。
