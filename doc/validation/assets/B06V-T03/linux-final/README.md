# B06V-T03 最终 Linux Python 与五条视觉独立审计

本轮完整 Linux Python 在最终源码新快照上实际运行 **380 项：361 passed、19 平台 skipped、0 failed、0 error**。五条既有视觉独立审计实际 **317 次普通 CLI** 全部通过；原始 stdout/stderr、逐调用 receipt、作者/expected/产物副本和源/CLI 前后指纹均已保存。此前两轮的设置失败及源码差异原件一并保留。

选定解释器为 `/tmp/noetome-b05-t03-python/bin/python`，实际 CPython **3.12.14**。最终 ELF 原件为 `.artifacts/b06v-t03-knowledge-final-build/noetome`，**151,148,440 bytes / SHA256 `038f106a483945b31594009598decec882bc19d53c73ffc1b462c778b793bfe4`**。快照 `target/debug/noetome` 和 `target/debug/noetome.exe` 均为这份 ELF 的独立普通文件副本；这里的 `.exe` 后缀是 Linux 测试使用的字节别名。`target/compiler-temp` 为实际普通目录。

## 实际检查

| 检查 | 实际结果 | 范围 |
| --- | --- | --- |
| 完整 Linux Python | 380 run，361 pass，19 skip，0 fail/error；unittest 196.345s，实际子进程 203.623s | 最终 prepare `9c28fd…`、当前全部 Python 源和最终 ELF |
| table | 125 CLI / 39 场景，passed | 原作者、真实来源项目、比较表和中文路径链接 |
| flow | 73 CLI / 41 场景，passed | 来源核对、节点/关系、明确方向/文字回退及同步造假拒绝 |
| formula | 33 CLI / 59 场景，passed | 原 TeX/作者语义、MathJax/原图选择及同步造假拒绝 |
| image | 55 CLI / 91 场景，passed | 原 PNG、有限 SVG/原图选择、空间/标签/方向及同步造假拒绝 |
| scan | 31 CLI，passed | 5 作者目标展示、20 补充 HTTP 故障、6 输入故障、20 篡改拒绝；实际 21 次数字环回 synthetic HTTP |

每次审计普通 CLI 的 stdout/stderr 都已与原 receipt 或其文件库存 SHA/长度逐项读回；scan 的 31 调用/62 流额外见 [独立读回](scan-command-stream-readback.json)。125+73+33+55+31 的实际总计为 **317**，没有套用历史汇总数。非零退出是各阻断和输入拒绝场景的预期行为，审计入口自身全部退出 0。

19 项 skip 的原始逐测试名称和原因保存在 [full-python-receipt.json](full-python-receipt.json)：16 项要求 Windows PowerShell/CurrentUser DPAPI，另有 Windows 文件共享、Windows creation/change time 和实际 Windows DPAPI/file sharing 各 1 项。Node 存在，本轮原 collector 相关 Python/Node 检查实际运行。

最终源快照从旧 611 路径清单扩充为 **1,971 个源/作者/输入文件，1,973 个总指纹项**，完整复制当前 scripts/tests、所有公开 fixtures、原 13 pack、flow extensions、corrected-v2 和独立 capability registry。四份原 native collector 全部显式纳入：

- `doc/validation/assets/B06-T01/obsidian/capture_obsidian.mjs`
- `doc/validation/assets/B06-T02/obsidian/capture_obsidian.mjs`
- `doc/validation/assets/B06-T03/native/capture_formula.mjs`
- `doc/validation/assets/B06V-T01/native/capture_image.mjs`

[source-before.json](source-before.json) 的 SHA256 为 **`4ae87369f5f3fb282ad2f4c0822c3059ea88acdcd9adc3bd2b206f5747d4121e`**。它与 [source-after.json](source-after.json) 逐值相等；最终相关 Python/CLI 生产源码子集与工作区前后也相等。正在独立修改的 Rust cache test helper 不属于这项工作区相等断言；本记录没有把 Linux 完整 Rust workspace 算作通过。

## 三轮原件与有限复用

第一轮把快照源 chmod444，旧 `shutil.copytree` 将模式继承到新造假副本，造成 16 项 full-unit `PermissionError`，结果为 380 run / 345 pass / 16 error / 19 skip。formula、image、scan 审计在相同模式问题处中断；没有修改产品或独立判据。原失败栈、权限、输入及产物都在旧轮原件包中。

第二轮恢复普通复制权限，完整 Python 380/361/19 和五审计均实际通过。但已确认固定的 prepare 随后因根代理授权新增唯一 `result.identity=identity;` FORMAT 元数据行，工作区 SHA 从 `b22fc9c…` 变为最终 `9c28fd…`。因此第二轮 aggregate 诚实记录为 failed，不把它称为最终源码的完整通过。

第三轮重新创建最终源码快照并完整运行 Python，全部通过。第二轮与第三轮**唯一源字节差是这份 prepare 文件**；五审计从不导入或执行该准备器。其余 **1,972 指纹项**，包括全部独立 verifier/helper、CLI 生产源码、作者/expected、原 native collector 和两份最终 ELF 字节别名，严格相同。因此复用第二轮五审计的原始结果；[audit-source-reuse-binding.json](audit-source-reuse-binding.json) 保存逐文件绑定，[prepare-only-source-delta.patch](prepare-only-source-delta.patch) 保存精确唯一改动。第二轮 whole-file prepare 与整套 Python 的最终源码相等不在复用声明中。

## 封存原件

| 原件包 | 成员 | ZIP 原始长度 | 完整 ZIP SHA256 |
| --- | --- | --- | --- |
| [source-originals.zip](source-originals.zip) | 1,979 | 71,347,352 | `5da7b0a4045226584823e96e68563f765be5a3985f53e60188f8804a40b2d04d` |
| earlier-run-originals.zip（以下原字节分段） | 39,356 | 164,587,460 | `8ef8daa340d532bacbcddaf15adcbf0d639654466cab56294b8690cfdf608bab` |
| [final-run-originals.zip](final-run-originals.zip) | 13,133 | 59,387,463 | `de07c87b87948b131d7059aceb965cda5a88a114d2bf34ffd8b89c3b68f95eb9` |

旧轮 ZIP 按原字节保存为 [part-001](earlier-run-originals.zip.part-001)（90,000,000 bytes）和 [part-002](earlier-run-originals.zip.part-002)（74,587,460 bytes）；按 [earlier-run-originals-parts.json](earlier-run-originals-parts.json) 的顺序连接即可得到上述未改变的完整 ZIP，连接 SHA/长度已经独立读回验证。完整本地原 ZIP 仍保留在 `.artifacts/b06v-t03-linux-final-03-20261106/earlier-run-originals.zip`。

三包共 **54,468 成员**，均实际通过 ZIP CRC 与每个成员 SHA256/长度读回。清单分别见 [source manifest](source-originals-manifest.json)、[earlier manifest](earlier-run-originals-manifest.json)、[final manifest](final-run-originals-manifest.json)。显式测试链接以链接目标文字和原类型/模式保存，不跟随到外部内容。源包包含最终 `repo/` 原字节和旧 prepare 原件；旧两轮的完整源清单指向相同的最终源字节，仅用这份旧 prepare 替换即可还原其字节集合，权限设置另由原执行器/receipt 留存。

重现时将源 ZIP 解压到新的测试根，赋予两份 ELF 执行权限，并先创建普通 `repo/target/compiler-temp` 目录。保存的运行 receipt 给出实际六个入口和绝对解释器/CLI 路径；重现者应显式选择本机可用解释器及新的输出根。原执行器也已在源包 `metadata/run-final-linux-validation.py` 中保存。所有测试输出都进入新快照的 `target/` 或新 synthetic 临时根。

[actual-checks.json](actual-checks.json) 为本轮总证据，SHA256 **`dd8345fe2d8c3c9d823ceae95fca0465e25a1e8aa511b4ed334c22f7d9483a3b`**。独立 scope 和失败历史见 [earlier-rounds-and-final-scope.json](earlier-rounds-and-final-scope.json)；完整 final stdout/stderr 原件另可直接读取 [stdout](full-python-stdout.bin) 和 [stderr](full-python-stderr.bin)。

进程环境仅继承执行所需的系统设置，真实 key/proxy/服务 ledger 环境均未继承。scan 只用明确 public synthetic credential 和数字环回 HTTP，**real_model_calls=0、paid_calls=0**；外部真实累计 33,185 ledger 未请求、未读取、未修改，UnknownConfidence 保留。本次没有新增实际 Obsidian capture、Windows Python 或 Linux 完整 Rust 验收，也没有证明科学正确率、通用 OCR 或生产 Vault 人工保护；这些范围与根代理的独立原生/平台证据分开记录。
