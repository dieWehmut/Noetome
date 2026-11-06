# Flow target CLI 测试的平台本地来源修复

Windows 完整 Python 的 `FlowTargetCliTests.setUpClass` 实际收到 `SOURCE_INVALID_RECORD`。原冻结 `verification-inputs/flow-branch/project/project.sqlite` 保存 Linux 绝对 `import_path`；Windows Core 拒绝它符合现有来源边界。原数据库、Core 和独立 expected 均未改。

唯一代码修复为 `tests/test_verify_process_flow.py` 的 class setup：在全新临时根执行普通 `project init`、`source import` 和 `knowledge process-review`，复制同一冻结 branch 原文/gold/profile，再通过独立 `actual_source` 与 `verify_review` 检查。原三项 target fallback、同步假 flowchart 和可见条件篡改断言保持原样。最终测试为 **31,744 bytes / SHA256 `42b63646f045fd15c75c43cbbb48a58f7b6394b4665fcde92be95e7e4c6e54c5`**。

实际 fresh focused **17 tests / 17 pass / 0 skip/error/fail**，包含 3 个 target CLI 测试、6 个严格 renderer Schema 测试和 8 个 registry 测试。4 次实际普通 CLI 与 1 次独立 Python check 的 stdout/stderr/receipt，以及新公开测试项目/真实 SQLite、case/review 和篡改副本，见 [focused-originals.zip](focused-originals.zip) 和 [实际统计](focused-actual-checks.json)。17 测试实际执行时只改变临时目录清理以保留原件，没有改变断言或 subprocess 行为。

修改前原测试、branch 原文/gold/expected/profile 与整个冻结 verification inputs 共 46 成员封存于 [before-originals.zip](before-originals.zip)，SHA `ae945c93cfe146bedf52394d04769d8eddfbdd1f15bf443979d5f791b2d17d44`；focused 原件 90 成员 ZIP SHA `2c477048905d7605ed93bdc871bc1e840fe57bf38f2900b4f493fe920d10d409`。两包均实际通过 CRC 与逐成员 SHA/长度读回。Windows01 原全套 result、stdout/stderr、source-before 也在本目录原字节保存。

[after-inputs-check.json](after-inputs-check.json) 逐项确认原 45 份冻结输入未变；[new-source-pins.json](new-source-pins.json) 记录最终十份 Python 源，只有这个测试 setup 与旧十份固定源不同，其他九份 exact。最终新源码完整 Linux **380=361 pass+19 明确平台 skip/0 error/fail** 已重新运行，见 [Linux04](../../linux-final-after-portable-test/README.md)。Windows 完整运行由根代理单独记录，未从 Linux 结果推定。
