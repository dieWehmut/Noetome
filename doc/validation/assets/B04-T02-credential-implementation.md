# Task 1 implementation report

Status: DONE for the assigned local implementation and offline checks. Operator dialog entry and real API invocation remain controller responsibilities.

## Files changed

- `scripts/set_api_credential.ps1`: explicit external credential destination, HTTPS origin/model display, default masked `Read-Host`, optional native WPF `PasswordBox`, and explicit selected environment-variable input. Validates 1..8192 printable ASCII characters without spaces or controls. Stores only CurrentUser DPAPI output using default `ConvertFrom-SecureString`; uses `FileMode.CreateNew`, refuses existing files, repository paths, relative/device/UNC/ADS paths, reserved aliases, missing parents and reparse ancestors. Fixed failure diagnostics disclose no exception body or input. Success gives only status and reference path.
- `scripts/invoke_api_credential.ps1`: explicitly selected configuration and DPAPI credential; configuration selects a 2..64-character uppercase environment identifier. Validates decrypted credential before launch. Launches exactly one native `extract api <project> <import-id> <config> <output-root> [--cancel-file <signal>]` command. The decrypted value goes only into the selected child environment, never arguments or temporary plaintext files. Redirects both output streams, reads them concurrently to avoid pipe deadlock, removes exact credential echoes before forwarding output, and preserves the child exit code. Finally clears the child environment entry, plaintext/output references, SecureString and BSTR. The parent's environment is never mutated; controller explicitly approved this stronger preservation outcome.
- `tests/test_api_credential_scripts.py`: independent Windows process tests, generated for available Windows PowerShell and PowerShell 7 engines; outside Windows or without those engines explicitly skips. Runtime path is `sys.executable`, not a fixed local path. All inputs are public synthetic values. Temporary artifacts reside in newly created external test roots.
- This required local handoff report.

No task-state, ledger, documentation, Git commit/push or unrelated source edits were made by this agent. The pre-existing deletions `docs/Noetome-PRD-v1.0.md` and `.html` remain untouched. Other working-tree changes visible at the end belong to concurrent controller/delegated work.

## Commands and observed results

All unittest runs used this exact command from `D:\Noetome`:

```powershell
& 'C:/Users/Lyinn/.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe' -X utf8 -m unittest discover -s tests -p test_api_credential_scripts.py -v
```

Execution history is retained, rather than describing intermediate failures as passes:

| Run | Real result | Meaning |
| --- | --- | --- |
| RED before either production script existed | 24 tests, 14 failures, 14.481s, exit 1 | Positive DPAPI/create, reuse, upper-bound and WPF tests failed because the scripts/factory were missing. Negative tests initially passed on missing script; final tests add diagnostic assertions to avoid accepting that as protection evidence. |
| First implementation | 24 tests, 8 failures, 24.904s, exit 1 | PowerShell 5.1 could not load Security module; PowerShell 7 also exposed test harness mistakes in `-Command` exit propagation and absent-versus-empty environment semantics. |
| First harness correction | 24 tests, 6 failures, 25.564s, exit 1 | PowerShell 7 passed; Windows PowerShell still failed module loading. |
| Corrected inherited module-path casing | 24 tests, 0 failures, 0 skips, 30.366s, exit 0 | Both engines passed. Python's environment mapping retained uppercase `PSMODULEPATH`; case-sensitive removal of `PSModulePath` had missed it. Tests now remove only that engine-specific variable, case-insensitively, allowing each engine to establish its own standard module paths. No repository runtime path was hardcoded. |
| Final expanded checks after captured WPF validator correction | 28 tests, 0 failures, 0 skips, 34.156s, exit 0 | Fourteen tests each on Windows PowerShell 5.1 and PowerShell 7. Includes fixed diagnostics, launch-failure environment preservation and actual WPF save-handler invalid-input behavior. |

Additional checks:

```powershell
git diff --check
git diff --check -- scripts/set_api_credential.ps1 scripts/invoke_api_credential.ps1 tests/test_api_credential_scripts.py
Get-FileHash -Algorithm SHA256 -LiteralPath scripts/set_api_credential.ps1,scripts/invoke_api_credential.ps1,tests/test_api_credential_scripts.py | ConvertTo-Json -Compress
powershell -NoLogo -NoProfile -NonInteractive -Command '$PSVersionTable.PSVersion.ToString()'
$PSVersionTable.PSVersion.ToString()
git status --short
```

Both diff checks returned exit 0. The global check emitted a normal LF/CRLF warning for the concurrent `crates/noetome-cli/tests/extraction_api.rs` modification. Engine versions observed: Windows PowerShell `5.1.26100.7920`; PowerShell `7.6.5`. Final status showed the three assigned implementation/test files as untracked, both unrelated user deletions intact, and other controller changes.

## Artifact evidence and tested behavior

Final implementation SHA256:

| File | SHA256 |
| --- | --- |
| `scripts/set_api_credential.ps1` | `99001ED7A90266480440B753659965DFED875D85B055821EC76CCABBD0B3C0CB` |
| `scripts/invoke_api_credential.ps1` | `EEA8034DBF773AD488F0ED2846E0E5185BBD4EE12F0F3D36C09DFB9E6B2B0B9D` |
| `tests/test_api_credential_scripts.py` | `D3BD41A3638A815E39B8727010907287657E0D10223D263142239031FE5D8A83` |

The process tests assert actual external DPAPI file creation, ciphertext excludes input bytes, and Windows decryption equals the public synthetic input without printing that input. Existing sentinel bytes remain unchanged on rejection. Repository/relative/ADS/missing-parent destinations stop. Empty, whitespace/control, Unicode and 8193-character inputs stop without files; 8192 printable ASCII characters succeed. Non-HTTPS/path/query/userinfo origins and conflicting dialog/environment input stop. Real Windows directory junction ancestors stop in both scripts.

For reuse, an external generated local Python child entry receives the exact literal expected API positional arguments and optional cancel-file arguments. It compares its selected environment value to the independently supplied public test input and reports only a Boolean. It deliberately echoes its public input to both output streams; the wrapper forwards only `[redacted]`. Controlled exit 23 is preserved. Parent environment remains identical when previously set, empty/absent, and when the executable launch fails. Invalid environment identifiers, corrupt synthetic DPAPI content, repository credentials and directory credentials stop before the selected child runs.

WPF checks load the actual installed PresentationFramework/PresentationCore/WindowsBase assemblies, create a real Window and PasswordBox through the production factory, verify Save is not a default Enter action and Cancel has cancel semantics, and exercise the actual production save handler against empty and whitespace-containing public input. Those invalid clicks do not populate secure state and give an ordinary validation message. Tests close the unshown window. No `ShowDialog` operator window was launched by this agent, and no actual user input, valid modal save or modal cancel is claimed.

Temporary synthetic artifacts were removed only through their owning test temporary-directory cleanup (and removal of the tests' own newly created junction). No ignored repository directories were cleaned. No actual user credential file, Codex authentication value, saved production credential or remote service was accessed. No real model/network request was made.

## Self review and limits

- Scripts use Windows PowerShell 5.1-compatible APIs; both installed engines actually ran the production save/invoke behavior. Native argument quoting implements Windows CRT backslash/quote escaping; real paths with spaces and the optional cancellation path were verified.
- Secrets are never interpolated into diagnostics. Setter validation inspects a temporary BSTR and always zero-frees it. Dialog uses `SecurePassword`, never returns `PasswordBox.Password`; the validator scriptblock is explicitly captured for the handler's closure. Invoker clears BSTR and SecureString in finally; immutable .NET strings are dereferenced but cannot promise an immediate garbage-collection memory wipe.
- Parent environment preservation is achieved by changing only `ProcessStartInfo.EnvironmentVariables`, avoiding any concurrent exposure from temporary parent mutation. The child receives no new credential argument.
- `CreateNew` enforces no-overwrite even on destination races; path reparse checks run again immediately before creation. This is ordinary selected local-path protection, not an OS-handle-level guarantee against a hostile process racing replacement of ancestors. Tests cover actual static junctions, not adversarial concurrent path replacement.
- Output is buffered in memory until the bounded CLI exits so exact credential echoes can be redacted across pipe chunks. The wrapper does not add retries, batch loops, independent networking or another timeout policy; the existing selected CLI/config owns bounded API behavior. It is not a general sandbox for arbitrary untrusted binaries.
- WPF modal valid save/cancel and actual operator entry are still to be performed by controller. DPAPI portability to other Windows users/machines, Unix behavior (explicitly unsupported), production credential reuse and real provider behavior were not tested or claimed here. Controller owns full workspace regression, task ledger completion, docs, commits and remote push.

No remaining concern blocks controller review of this implementation. Use a Windows STA PowerShell process for the actual WPF entry after review.
