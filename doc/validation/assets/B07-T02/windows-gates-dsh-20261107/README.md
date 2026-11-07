# B07-T02 Windows 门禁补测原件（2026-11-07，DSH 会话）

本目录保存 2026-11-07 由 DSH 会话在 Windows 一侧实际执行的门禁输出。此前作者已封存 Linux 全套证据，Windows 的
`813` / `Python` / 两个 PowerShell 引擎仍为未闭合项，本目录是对这三项的实际运行记录。

## 环境

| 项 | 值 |
| --- | --- |
| 源码 | `git archive HEAD`（HEAD=`c91bc26`）解包到 `D:\noetome-wsl-head`，16306 文件 |
| Windows 壳 | 本机 Windows（PowerShell 5.1.26100 内置） |
| Rust | cargo 1.98.1 / rustc 1.98.1 |
| 锁定 Python | `D:\Noetome\.artifacts\b03-t02-python312\Scripts\python.exe`（CPython 3.12.14、pdfplumber 0.11.9、pdfminer.six 20251230） |
| PowerShell 7 | 便携版 7.6.9，GitHub release zip 解压到 `D:\tools\pwsh7`（原本机器上没有 pwsh 7） |
| 测试环境变量 | `NOETOME_TEST_PYTHON` 与 `NOETOME_PDF_PYTHON` 都指向上面的 3.12.14 解释器 |

## 实际结果

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| fmt | `cargo fmt --all --check` | exit 0 |
| check | `cargo check --workspace` | exit 0，1 条 dead_code 警告，1m09s |
| Rust 计数 | `cargo test --workspace -- --list` | **813** 项（与作者口径 `Windows813` 相同） |
| Rust 执行 | `cargo test --workspace --no-fail-fast -- --test-threads=1` | **811 通过 / 2 失败**（详见 `windows-rust-813.txt`） |
| Python | `PYTHONUTF8=1 <3.12.14> -X utf8 -m unittest discover -s tests -p "test_*.py"` | **Ran 503 / OK (skipped=12)**，431.474s，exit 0（见 `windows-python.txt`） |
| PowerShell | 同一 Python 套件内的 `test_api_credential_scripts`（按 `shutil.which` 动态生成引擎类） | powershell 与 pwsh 各 16 个 test 方法全过，两引擎 32 项 54.671s（见 `windows-powershell.txt`） |

## 计数差额（不解释为通过）

- 作者口径 `Python527`：本机实际发现 **503** 项。`tests/*.py` 静态 `def test_` 为 489 个；凭据模块的 16 个 `def` 在
  两个引擎都在时展开成 2 个类，动态发现值即 503。差 24 项无法在本机复现，未计入通过。
- 作者口径 `两 PowerShell 各 39`：本机每个引擎实际 16 个 test 方法；39 应为断言/检查计数，不是方法数。

## 两个失败项（时间敏感，非本任务改动引入）

1. `sidecar_process::tests::review_broken_pipe_notifies_controller_and_reaps_live_tree`
   - 整套串行运行时实测 `broken_pipe_latency_ms=3694 error=SIDECAR_TIMEOUT: runtime limit`，即测试自身的 4000ms
     期限先用尽，断言 `SIDECAR_IO`（`tests/support/sidecar_tests.rs:781`）失败。
   - 隔离运行 4 次全部通过（0.41s / 0.47s / 0.52s），其中一次打印 `broken_pipe_latency_ms=11 error=SIDECAR_IO: 管道已结束。 (os error 109)`。
2. `pdf_source_model::tests::source_cache_races::owner_bytes_are_guarded_without_an_acquired_pdf_version`
   - 只在整套串行运行中出现；隔离运行 3 次全部通过（2.87s / 2.90s / 2.90s）。

两项都是 `#[cfg(windows)]` 的期限/竞争型用例，失败只出现在 555 项 lib 套件连续跑完的末尾，单独运行稳定通过。
本轮改动（`request_budget_store.rs` 的 checked 事务、`request_run.rs`）不在其调用路径上；本目录保留原始行以便复核。
