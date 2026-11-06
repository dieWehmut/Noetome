# 最终 Windows 完整回归

在全新 `D:/Noetome/target/b06v-t01-source-final-20261106b/repo` 副本中实际完成 workspace fmt/check/test、CLI build、Python full 和表格/流程/公式/image 四项普通 CLI 独立 audit。所有必需命令最终退出 0；真实 Rust 为 688 passed、0 failed、0 ignored。Python full 实际 256 tests，为 254 passed、0 failed、2 skipped，跳过项没有计为通过。结果与原始命令指纹见[最终结果](native-final.json)、[命令记录](native-commands.json)和[运行时](native-runtime.json)。

Windows CPython 3.12.14，fake Python 也实测为 3.12.14，PDF 八项依赖均与 runtime-lock 逐版本相同。最终 PE SHA256 为 `ed54203ae6a6a7eaac499045b3a221843fa7c03bf249d2902c4580031ba1dc12`，25182720 字节；同一 PE 的 immutable 副本实际用于[17 场原生验收](../native/final-summary.json)。未重装 Noetome 或 Obsidian，未改旧 Windows checkout 或生产 Vault。

四项 audit 实际结果：

| 范围 | 原作者 | 普通 CLI | 工程场景 |
| --- | ---: | ---: | ---: |
| 主动表格/正文 | 不合并为新作者计数 | 124 | 39 |
| 有据流程 | 16 | 73 | 41 |
| 公式 | 15 | 29 | 56 |
| 图像 | 12 | 51 | 88 |

Python 两个 skipped 的准确名称和理由见[python-skips.json](python-skips.json)：conflicts ancestor symlink 用例明确要求另一 Windows reparse 环境；presentation 的目录 symbolic link 实际不可用。为识别非 verbose 全套中的动态 skip，只另跑了两个含 skipTest 的边界方法，实际一个 hardlink 通过、一个 symlink skipped。没有把这些操作或 Windows Rust 的 junction/持有句柄通过替代为 Python symlink 用例通过。

源首先固定 544 份当前产品、fixture、test、script 和明确引用的历史原件。流程 audit 第一次在发出任何 CLI 前发现其 source-pins 所需的既有 PROCESS_FLOW_CONTRACT.md 未复制，实际 exit 1；保留了原 snapshot 和失败文件，仅补入这个原字节标准并追加 history，成为 545 份清单。产品、fixtures、tests、scripts 均未变化，成功的 Rust/Python/表格没有重复；之后流程第二次、公式和 image audit 全部通过。[源快照](source-snapshot.json)和[最终逐字节对照](final-source-comparison.json)记录 545 份全部与当前工作树相同，旧 Windows 项目仍仅保留原有两份旧 docs 删除。

更早的[candidate1](candidate-1/README.md)保留共用 target 对旧 B06-T03 lib/PE 误报 fresh 的失败。本轮实际执行了只针对选定缓存的 `cargo clean -p noetome-cli`，保留依赖缓存；真实重建后首先实际检查 `knowledge image --help` 才进入完整 Rust。没有修改源语义来规避编译错误。

结果汇总 helper 最初只识别没有 skip 的 OK 文本；修正为明确记录实际 skip 后，通过 limited boundary probe 确认准确原因，再只重新汇总已完成检查。两次汇总断言失败没有改变实际成功的测试或重跑全套，原始 helper 和修改后 helper 都保留在运行原件中。

原件归档保存源码、stdout/stderr、收据、所有四项 audit 与实际 Rust/Python 测试临时根。WSL 读取真实 NTFS junction 的 readlink 元数据发生 EIO，前两次归档留下各自源码包；第三次 WSL 尝试使用明确自有路径的 lexical Windows drive 转换和选定 Windows readlink 只取得 alias 元数据，不读取目标内容。可重复的 PE/编译对象和调试二进制不放入 ZIP，保留其指纹；测试 alias 也按元数据保存。

第三次 WSL 归档已压入 14211 份原件后在普通文本 open 中出现 9P 等待，仅中断自有归档进程并保留 partial ZIP。最终直接用选定 Windows CPython 读取 D: 自有根，完整归档并逐成员读回 547 份源码/config 和 37625 份运行原件；没有重跑测试，也没有跟随 junction 目标。三个中间归档目录保留技术失败。成功产物为[源码 ZIP](final/source-originals.zip)、[运行 ZIP](final/regression-originals.zip)及各自[源码清单](final/source-originals-manifest.json)、[运行清单](final/regression-originals-manifest.json)。所有成功文件复制到仓库后再次逐文件比较原字节，[复制核对](final/archive-copy-verification.json)。

科学质量、一般图像理解/OCR、生产维护、最低/当前稳定版矩阵均未运行，不从这份工程回归推定。
