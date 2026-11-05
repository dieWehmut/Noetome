# B06-T03 Windows 完整回归与公式 CLI

在 D:/Noetome/target/b06-t03-source-d791e08c047f 的新源码副本中，使用实际 Windows CPython 3.12.14、已有 D:/Noetome Cargo 缓存及 scripts/cargo_local.ps1 执行。未改旧 Windows 项目源码、权限、Noetome 安装或生产 Vault。fake Python 和锁定 PDF Python 均按旧验收环境显式选择。

真实最终结果：workspace fmt/check/test、CLI build 和公式独立 audit 均出口 0；完整 Rust 670 passed、0 failed、0 ignored。公式审计包含 15 作者 case、29 次普通 CLI、56 个工程场景；Windows 上未执行的 Unix symlink 矩阵另有 not_run 条目，不计为通过。实际 runtime、命令、退出、CLI 指纹和原始输出见 [运行时](native-runtime.json)、[所有命令](native-commands.json)及[最终统计](native-final.json)。

第一次 full test 的实际编译出口 101：新副本遗漏了四份既有 B04 API 响应 include_bytes 测试原件。保留第一轮 stderr 与 receipt，补齐明确被测试引用的原件并追加 snapshot history；没有修改 Rust 源码或判据。单元 ProcessCase 没有 CARGO_BIN_EXE，故先真实 build 当前 CLI，再同字节复制到新副本 target/debug/noetome.exe；[复制记录](unit-test-cli-copy.json)明确记录该 prebuild 程序。随后完整 fmt/check/test/build/audit 通过；Cargo 正常重链接会改变 PE 指纹，所有调用均有各自真实程序指纹，没有声称全部测试调用与最终程序逐字节相同。

最终 Windows CLI SHA `8b062568e44014bcc6fa79deba26e563a98649273e0192cf08026069e58598ab`；新公式 audit 使用该最终程序的独立稳定副本。所有产品与测试输入保持同一冻结字节。487 个全部输入与当前工作树最终逐字节一致；测试后仅同步 README 的用户文档，保存同步前快照、旧 README 及历史，不为纯文档重复完整 Rust 回归。对照见 [最终 source 比较](final-source-comparison.json)、[实际测试时 snapshot](source-snapshot-tested-before-readme-sync.json)和[当前 snapshot](source-snapshot.json)。

[完整回归/audit 原件 ZIP](native-regression-originals.zip)保存 702 份原始文件及 manifest，已逐条解压读回并核对长度/SHA；[原件清单](native-regression-originals-manifest.json)。[源码 ZIP](source-snapshot-originals.zip)包含全部 487 个当前源/测试输入、测试前 snapshot 及 README 原件，全部源件也已实际读回核验。测试临时根和构建缓存保留在本地。科学质量、通用 OCR、一般图像重绘、生产维护、桌面重装及整个发布兼容矩阵不由这些工程检查推定。
