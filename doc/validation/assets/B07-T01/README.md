# B07-T01 原始验证证据

主验收见[B07-T01](../../B07-T01.md)，机器摘要见[verification-summary.json](verification-summary.json)。本目录保存本项新测试根的原始记录；作者样例与模型、科学质量分开验收，真实模型/付费调用0。原始文件按实际SHA/长度逐字复制，64笔复制来源及前后身份见[复制原件清单](review/public-copy-manifest.json)。清单本身不证明行为通过。

## 最终证据与历史作用域

- `linux-run`、`windows-run` 是最终36项验证器的恢复矩阵，分别95通过、94通过/1 POSIX权限未运行；普通CLI为320/319次。双线程事务竞态的后续只读CLI各2次，原真实Rust驱动收据在对应ZIP内，不重复计入矩阵driver数量。
- `programs/linux-cli`、`programs/linux-driver` 保存Core05完整ELF、125文件编译/CLI测试源、实际verbose构建和12 helper测试。`windows-rust` 保存Native06完整PE/driver、源码、fmt/check/test/build及756测试原始输出。
- `linux-python` 是最终416=395通过/21平台跳过。`windows-python` 是最终Native08的477=469通过/8符号链接权限跳过，两个PowerShell引擎都实际运行；其Rust复用Native06，89/CLI125/workspace125/1864四份不同清单分别逐字相等。Native08没有再次运行Cargo。
- `review` 保留独立状态机/SQLite journal/作者检查、Core04到05仅52字节测试路径规范化差异、最终36验证器复查和精确来源作用域。复制的原README保留当时文本，不重写为最终结果；其中路径、时间、数量应按其原作用域阅读。
- `extra-invalidations` 是已确认4单元后的三次输入/身份/真实版本失效，7 CLI/3真实击杀；执行时验证器是之前35项版本，当前36相应验证函数及Linux路径未变的独立证明在`history/native06`。不并入最终95行矩阵。
- `history/native-prechecks` 保存Native01–05的准备、工具/缓存/生成schema门禁失败、真实两项helper失败和旧PE/driver原件；`history/python35` 保存真实4 ERROR/8清理异常到35项修复的原始输出；`history/native06` 保存首矩阵121 CLI/1 driver的空锁库存失败及两套实际持有锁诊断、36修复、发现缺39个pwsh测试的作用域对照。Native07假定运行时路径不存在的提前失败也保存在`windows-python`包内。

ZIP、JSON、Windows CRLF、作者Markdown结尾空行和二进制以`.gitattributes`保留原字节。所有ZIP小于100 MiB；每包manifest绑定全部成员长度、SHA，复读检查同时验证ZIP CRC。大型程序体集中存储，不从路径或哈希存在推定未保存的旧程序原件。

## 主要原始归档清单

| 原始归档 | 成员 | ZIP字节 | SHA256 |
| --- | ---: | ---: | --- |
| [extra-invalidations/originals.zip](extra-invalidations/originals.zip) | 210 | 380,732 | `c448fa27b895855ed8479a811543b94eb27c85aaec0dbc0fb3203f7da7bc92c0` |
| [history/native-prechecks/originals.zip](history/native-prechecks/originals.zip) | 344 | 57,509,639 | `bc2e3466b103c0dfe2e23a9504899bba90d77fd303162f66d6ae23dde4a8dc8a` |
| [history/native06/originals.zip](history/native06/originals.zip) | 822 | 1,075,314 | `ac77491e7205312941245955fbd877546507dbb5b6910b3129aaf0d4a16ab0f1` |
| [history/python35/originals.zip](history/python35/originals.zip) | 91 | 412,002 | `f66b85c68a60155bdd2d89ea8dc55fe83fe3fec391714fc6c928d15131f5c73c` |
| [linux-python/originals.zip](linux-python/originals.zip) | 15176 | 36,715,112 | `6b38ca6b19dd9be10f4aacf7a414aeaa28126e841e392e1f528ba4470de47f2a` |
| [linux-run/originals.zip](linux-run/originals.zip) | 3439 | 5,743,814 | `33c38adb058c94fd828235f9db1f46514b7fb2aa55bc7d5bd5c433e084782b77` |
| [programs/linux-cli/originals.zip](programs/linux-cli/originals.zip) | 43 | 33,995,704 | `e76b3fe81c08d99365dfb5b2daf3921eb58bfdd9083527e7ad6c959069491d8a` |
| [programs/linux-driver/originals.zip](programs/linux-driver/originals.zip) | 43 | 40,181,696 | `1c69c8dac48870da7b6fab023e62923c70dc4f2e4a18ee0c45955e569902476b` |
| [review/current-source/originals.zip](review/current-source/originals.zip) | 2 | 88,157 | `40a7a9acebf02032657814597c536a075eae8a2b84caabb476ef9a020577fd10` |
| [review/peer/originals.zip](review/peer/originals.zip) | 8 | 9,748 | `a3564a3ac41a181660d751bb4ceaffc4e2971c6d0b5256307e0373d1106b5797` |
| [review/readbackmanifest.zip](review/readbackmanifest.zip) | 1359 | 6,675,473 | `0669298a74736c97c4235b027ec02efcb6b6a36e13b6640f778037195a621727` |
| [windows-python/originals.zip](windows-python/originals.zip) | 101 | 15,572,317 | `97fa133eed4ce0e14b469c80fbcf1537b217b283c39c88aae7ae7198c8b92ddc` |
| [windows-run/originals.zip](windows-run/originals.zip) | 5259 | 12,460,338 | `81c732e11e24cda4a35722d6869271a7e5127fb103ef0f1210909a5a3f365a45` |
| [windows-rust/originals.zip](windows-rust/originals.zip) | 107 | 26,821,256 | `7267b68cf11e919f3c1fdbf7482b2ebab9c293f74305c54b94bec4b8fcce7772` |

## 最后独立审查补包

[最后101项报告/摘要/Git审查](review/final-report/actual.json)及[27成员原件包](review/final-report/originals.zip)另存审查首次action标签包装失败、修正结果、当前原文与Git补审，未执行新的产品命令。此ZIP139769字节、SHA `0e86eeef0925578d2d6c15ab12b8f09142014f5374f60d3201a741b40d2fabcb`；与上表14主原始包分开计，全部15 ZIP合计27031成员/237781071字节。[公开原件独立回读](review/public-evidence-independent-readback.json)、[342本地链接与原生事实终审](review/final-document-fact-review.json)、[暂存原件回读](review/git-index-originals-readback.json)分别说明实际核对作用域，不自引用未来日志或其自身文件。
