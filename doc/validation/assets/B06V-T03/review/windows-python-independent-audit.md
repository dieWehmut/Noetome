Windows 完整 Python 第三轮独立审查通过，保留 7 项明确的本机权限跳过。审查直接重读原 `python.stderr.bin`、`python.stdout.bin`、进程结果、启动脚本、源前后清单、实际 PE 与完整源 ZIP，没有仅凭汇总 `passed` 判断。

[最终原结果](../windows-python/final/result.json)实际为 `Ran 441 tests in 484.505s`、`OK (skipped=7)`，原生进程退出 0。逐条 verbose 记录独立计得 434 个 `ok`、7 个 `skipped`、0 个 ERROR/FAIL，正好组成 441。此前因 `setUpClass` 未成功执行的三个 `FlowTargetCliTests`，这轮都实际通过：外部目标重新计算回退、同步伪造 flowchart 声明拒绝、丢失可见来源条件拒绝。详细计数和原指纹见[独立审计 JSON](windows-python-independent-audit.json)。

| 实际完整运行 | 被运行测试数 | 实际 ok | skipped | 错误头（含子测/setup） | 退出码 | 源文件数 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| attempt-01 | 438 | 425 | 7 | 11 | 1 | 1916 |
| attempt-02 | 438 | 431 | 7 | 1 | 1 | 1971 |
| final attempt-03 | 441 | 434 | 7 | 0 | 0 | 1971 |

第一轮包含 6 个失败方法，其中一个方法的 5 个子测分别报错；另有一个 `setUpClass` 错误不属于 `Ran` 分母。因此 11 个错误头不能直接当作 11 个被运行测试。其准备工具错误来自漏带三个旧 collector，另有流程本机来源登记 setup 错误；第二轮 collector 齐全后只剩该 setup。前两轮原输出、清单与完整 ZIP 已与执行目录逐字节核对，见[失败历史独立记录](windows-python-independent-history.json)和[原第一轮](../windows-python/attempt-01/result.json)、[原第二轮](../windows-python/attempt-02/result.json)。第二轮工作树的 10 个文档/测试文件变化也如实保存，未把它当最终源一致通过。

第三轮冻结 1,971 个源文件；1,972 个唯一 ZIP 成员为这些原文件加 `source-before.json`，完整 CRC、SHA256、长度全部独立重算通过。`source-before`、`source-after` 和 `worktree-after` 三份原清单完全相等，每个实际快照文件及当时工作树字节另行读取核对一致。通过 `rg` 独立列出的 crates、sidecars、scripts、tests、fixtures 共 1,871 个非缓存/非生成 Schema 文件全在清单内，四个原始视觉 collector 均存在。准备工具固定 `9c28fd119634c14d2c28d24451799175fe3fa9aceaeae10566f000e52767c83f`，流程测试固定 `42b63646f045fd15c75c43cbbb48a58f7b6394b4665fcde92be95e7e4c6e54c5`。

实际启动使用 Windows PowerShell 5 的 `-NoProfile -NonInteractive` 和新的独立测试源目录，明确选择两套绝对路径解释器及 `NOETOME_TEST_BINARY`；启动脚本与 driver 均核对原字节。只在子进程移除真实 API key 环境，测试使用已有 synthetic local HTTP/DPAPI 输入。两套解释器均再次实际探测为 win32/Python 3.12.14；8 项 PDF 依赖与 Rust C 原运行时记录逐项一致，探测原输出见[运行时只读原件](windows-python-runtime-originals-03/result.json)。本次审查没有读取真实凭据、真实累计预算或发送模型请求。

实际所选普通 Windows PE 为 `D:\Noetome\target\b06v-t03-final-rust-20261106-c\noetome-selected.exe`，SHA256 `e42c534d07f66662411fc52f22561510b79ba1c63f14f8b6f419db7c4e0e3274`、25,939,968 字节；复制至第三轮新目录，单链接 `nlink=1`，复制件与 Rust C 原件在运行前后完全一致。Rust C 保守依赖 1,825 个文件中共有的 1,821 个字节全部一致；未纳入 Python 快照的 4 个是自动生成 Tauri Schema。新 Python 格式 identity 保存行和本机来源登记测试不被反推为 Rust C 原来编译过的 Python 字节。

原 stderr SHA256 为 `7144d2cac923d9e1270787272276f67bdb4e2f8b40a9a112828a29a4731fa254`（77,933 字节），stdout 为 `e376dc9510cd26520d97889cf78a4844c2543bb91ea561469b8a83b2727d3cd3`（498 字节），完整源 ZIP 为 `5cb145aed407c7fc6aba404b9fa491aa621e78dabdebe571607ffcff4ccded15`（7,690,364 字节）。父代理最终仓库归档的所有文件已再次与执行目录原字节和收据逐一核对，完整 ZIP 再读通过，见[归档独立绑定](windows-python-independent-archive-audit.json)。

7 项跳过均为当前宿主缺少符号链接/目录链接建立权限的测试：原预算 receipt 链接、知识冲突祖先链接、扫描准备来源链接、兼容准备输出祖先别名、知识表达祖先链接、扫描作者祖先链接、扫描 flat inventory 祖先链接。原 stderr 中各项理由原样保存在审计 JSON；这些跳过没有计作通过，不从其他测试推定同一路径在 Windows 已执行。此项完整 Python 不替代原生显示矩阵、科学正确率、通用 OCR、生产 Vault 人工保护或未测试平台的验收。
