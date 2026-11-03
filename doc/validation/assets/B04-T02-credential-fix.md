# Task 1 fix round 1 report

DONE for the assigned focused fix. The independent P2 hard-link finding is fixed and the fresh scoped Windows regression passes. This does not accept B04-T02's real API invocation or operator-input work.

## Binding inputs and changes

Read `task-1-review.md` first and reread the original `task-1-brief.md`. The binding reviewer requirement is to reject hardlinked selected credential files through an open read guard, inspect only that selected handle, and continue reading bounded DPAPI ciphertext from that same guard. No alias enumeration is needed or performed.

Edited only:

- `scripts/invoke_api_credential.ps1`: adds a narrowly scoped Windows native handle query using `GetFileInformationByHandle` and `SafeFileHandle`. The explicitly selected file is opened for read with `FileShare.Read`; the selected handle must report exactly one hard link and ordinary-file attributes. Link/error failure occurs under the existing `credential-invoke: path-rejected` diagnostic before decryption. The maximum ciphertext length check now uses the guard's `Length`, and a leave-open `StreamReader` reads from this same guard; there is no credential pathname reopen. Reader and guard are disposed in finally, including rejection/failure paths. Native quoting and parent-environment injection code are unchanged.
- `tests/test_api_credential_scripts.py`: adds an actual external public-synthetic DPAPI hard-link fixture using `os.link`, confirms `st_nlink == 2`, and selects both original and alias names. The local synthetic child writes a public marker if actually launched, independently of stdout. The regression requires no marker, exit 2, exactly the stable path rejection diagnostic, empty stdout and unchanged ciphertext bytes. Final test resets only its owned marker between the selected names. A permanent real-child native argument regression also preserves the reviewer's embedded quote/backslash/trailing backslash cases.

The setter was not changed: its existing `CreateNew` and no-existing-destination behavior already refuses pre-existing destinations. No ledger/state changes, Git commit/push, unrelated user deletions, other source edits or broad directory cleanup were performed. This report and the fresh raw command/tool evidence below are the requested local plan artifacts.

## Actual fresh RED and GREEN

Working directory: `D:\Noetome`. Each run redirects real command output directly into its own plan workspace file as it is produced. These files are fresh executions from this fix round, not reconstructed historical logs.

RED command, run after adding the regression and before modifying production:

```powershell
& 'C:/Users/Lyinn/.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe' -X utf8 -m unittest discover -s tests -p test_api_credential_scripts.py -k hardlinked -v *> '.superpowers/sdd/B04-T02_GATEWAY_PLAN/task-1-fix-red.txt'; $credentialTestExit = $LASTEXITCODE; Get-Content -LiteralPath '.superpowers/sdd/B04-T02_GATEWAY_PLAN/task-1-fix-red.txt'; 'test-exit=' + $credentialTestExit; exit $credentialTestExit
```

Actual RED: **2 tests, 4 failing selected-name subcases, 2.964s, exit 1**. Both Windows PowerShell and PowerShell 7 reached the public child marker for the hardlinked credential. The initial test did not reset the marker before its second alias selection; that detail was strengthened before final full GREEN. The first selected name independently demonstrated an actual child launch in each engine, and the raw initial output is retained without rewriting it. The fresh review independently already demonstrated alias-name launch on both engines.

Focused GREEN after the minimal production guard:

```powershell
& 'C:/Users/Lyinn/.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe' -X utf8 -m unittest discover -s tests -p test_api_credential_scripts.py -k hardlinked -v *> '.superpowers/sdd/B04-T02_GATEWAY_PLAN/task-1-fix-green-focused.txt'; $credentialTestExit = $LASTEXITCODE; Get-Content -LiteralPath '.superpowers/sdd/B04-T02_GATEWAY_PLAN/task-1-fix-green-focused.txt'; 'test-exit=' + $credentialTestExit; exit $credentialTestExit
```

Actual focused GREEN: **2 tests, 0 failures/skips, 3.356s, exit 0**. Both selected names now stop with the stable path diagnostic before the child creates a marker.

Full fresh scoped GREEN, after making marker checks independent per selected name and adding the native argument regression:

```powershell
& 'C:/Users/Lyinn/.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe' -X utf8 -m unittest discover -s tests -p test_api_credential_scripts.py -v *> '.superpowers/sdd/B04-T02_GATEWAY_PLAN/task-1-fix-green-full.txt'; $credentialTestExit = $LASTEXITCODE; Get-Content -LiteralPath '.superpowers/sdd/B04-T02_GATEWAY_PLAN/task-1-fix-green-full.txt'; 'test-exit=' + $credentialTestExit; exit $credentialTestExit
```

Actual full GREEN: **32 tests, 0 failures, 0 skips, 42.232s, exit 0**. Sixteen tests each on installed Windows PowerShell and PowerShell 7. Ordinary single-link DPAPI reuse succeeds, while actual two-link files stop before child execution for both selected names. Retained tests pass for DPAPI roundtrip/secrecy, ciphertext corruption, configured environment names, static junction rejection, existing/repository/relative/ADS protections, printable ASCII bounds, no-key arguments/output redaction, controlled exit 23, unchanged parent environment including launch failure, and unshown real WPF controls/invalid handler. The new quote/backslash regression passes on both engines.

Additional commands actually run:

```powershell
git diff --check -- scripts/invoke_api_credential.ps1 tests/test_api_credential_scripts.py
git diff --stat -- scripts/set_api_credential.ps1 scripts/invoke_api_credential.ps1 tests/test_api_credential_scripts.py
Get-FileHash -Algorithm SHA256 -LiteralPath scripts/invoke_api_credential.ps1,tests/test_api_credential_scripts.py,'.superpowers/sdd/B04-T02_GATEWAY_PLAN/task-1-fix-red.txt','.superpowers/sdd/B04-T02_GATEWAY_PLAN/task-1-fix-green-focused.txt','.superpowers/sdd/B04-T02_GATEWAY_PLAN/task-1-fix-green-full.txt' | ConvertTo-Json -Compress
```

Diff check returned exit 0. Git emitted a normal LF-to-CRLF working-copy warning for the invoker. Scoped diff stat showed only two assigned files, 89 insertions and 2 deletions. No changes to the setter.

## Fresh evidence and SHA256

| File | SHA256 |
| --- | --- |
| `scripts/invoke_api_credential.ps1` | `C71022C74449CCC51C213613020071BB735E208A4D3EB6AECB83D788EE3DE504` |
| `tests/test_api_credential_scripts.py` | `BFEDF2BD046C7F8F900ADF14F7A68F8643007A6E43AEBDAAEB3BB05A6E7381C6` |
| `task-1-fix-red.txt` | `9894DFBD8249502A418E9DAF33D1F35AE57ADF63F9BB560265907BE5CBEC632A` |
| `task-1-fix-green-focused.txt` | `6EB7E76C6EFE7E1B63243A258BA65517D4F2303041C348664609BE45AF33F6DD` |
| `task-1-fix-green-full.txt` | `697FB29A21328920C94E6635E6B2AE51A64AB6399DB3FB7FDA71A346CDF11DF6` |

The raw RED command output was written directly at execution. Focused and full GREEN additionally retain actual tool-return JSON objects, saved immediately after their corresponding tool calls:

- `task-1-fix-green-focused-tool.json`
- `task-1-fix-green-full-start-tool.json` (session returned while the directly redirected command was still running)
- `task-1-fix-green-full-result-tool.json` (actual completion output and exit result)

## Self review and boundaries

The `BY_HANDLE_FILE_INFORMATION` interop layout consists of the documented thirteen sequential DWORD fields (attributes, three FILETIME pairs, volume serial, size pair, link count and file index pair). The native signature uses a SafeFileHandle and marshals BOOL. Each installed Windows engine actually exercised it. Checking link count from the selected file's handle catches hard links without searching for their names or reading directories. The guard excludes write/delete sharing, the ciphertext size is checked on that guard, and the reader never reopens the credential by pathname. Existing ordinary-file success checks prevent an implementation that simply rejects every credential from passing this regression.

The script retains static selected-path/ancestor reparse checks, and additionally rejects directory/reparse attributes reported for the opened file. This focused fix does not claim a general defense against adversarial concurrent replacement of ancestors or later creation of aliases. The review expressly required a static selected-handle hard-link rejection and did not demand a general ancestor race defense.

Only fresh public-synthetic DPAPI fixtures and a local public child entry were used. No actual saved key, credential profile, Codex authentication, authorization header, private directory or remote service was accessed. No real operator dialog or HTTP/model request was launched. No credential values were put into arguments, raw outputs, reports or plaintext files; the test fixture retains only its pre-existing public synthetic code value and generated DPAPI ciphertext. Tests cleaned up only their owned temporary fixtures/marker/junction.

No outstanding concern blocks fresh controller review of this delta. Full workspace acceptance, Git/log/document updates, operator input and actual provider behavior remain controller work.
