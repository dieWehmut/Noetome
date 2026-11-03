# B04-T02 Task 1 fix round 1 independent review

Review date: 2026-11-03. **SPEC: PASS. QUALITY: PASS. The original P2 hard-link finding is closed. No new actionable finding was found in the scoped fix.**

The production wrapper now checks the explicitly selected credential through its open read handle, requires exactly one link and ordinary-file attributes, and reads the bounded ciphertext through the same FileStream. Fresh reviewer tests on both installed Windows engines verify rejection before child launch and successful ordinary reuse. This verdict accepts the Task 1 fix against the binding brief and previous review; it does not accept real operator input, a real provider request, or completion of B04-T02.

## Scope and immutable identity

Binding inputs read: `task-1-brief.md`, the original `task-1-review.md`, and `task-1-fix-report.md` in `.superpowers/sdd/B04-T02_GATEWAY_PLAN/`.

Immutable package: `.superpowers/sdd/B04-T02_GATEWAY_PLAN/review-6e61705..cd01771.diff`.

Reviewed range: `6e61705ce577da692d5dccd62b5a20de4baaabbe..cd01771459d0ae0f55ad8758fa8bd80f95f54512`.

Head observed by `git rev-parse HEAD`: `cd01771459d0ae0f55ad8758fa8bd80f95f54512`. Changed implementation files are only `scripts/invoke_api_credential.ps1` and `tests/test_api_credential_scripts.py`; the remaining eight changed files are review/fix evidence. The setter has the same blob and bytes as the original review. Exact-head comparison for the three production/test files returned exit 0. Range formatting check returned exit 0.

| Inspected input | SHA256 |
| --- | --- |
| Binding brief | `4FC3C59E0261D7AB1A0E558282EFF23766D81094EDEDE49BEBF6D3BFF7EBA510` |
| Original review | `0CDEEB986607408B9A8E651544439E37B639E27E8F5B6238EB17BEB10D855B40` |
| Worker fix report | `16A3D42EBCE34B0B5904CA5BE1AC1F3BB4ED462E4D8A230F50CB82E191F3A966` |
| Immutable fix package | `B19A3EA96953B33AE0CDA5223C2E563A86D8278B8BC44B66A6D6ACA128178868` |
| `scripts/set_api_credential.ps1` | `99001ED7A90266480440B753659965DFED875D85B055821EC76CCABBD0B3C0CB` |
| `scripts/invoke_api_credential.ps1` | `C71022C74449CCC51C213613020071BB735E208A4D3EB6AECB83D788EE3DE504` |
| `tests/test_api_credential_scripts.py` | `BFEDF2BD046C7F8F900ADF14F7A68F8643007A6E43AEBDAAEB3BB05A6E7381C6` |
| Archived RED JSON | `D14C8E548199C14322E9AE1CB4196648F352E88456B150B76F6B55A76C4E3555` |
| Archived full GREEN text | `697FB29A21328920C94E6635E6B2AE51A64AB6399DB3FB7FDA71A346CDF11DF6` |
| Archived full GREEN completion tool JSON | `4757999F87DB78DD58A2F2CC9A5925CB896AB0ECDB1EB50C0820C34FB6757416` |

The inspected head's Git blobs are `ddcc72723d3e175cd5e5bc1916e6508f8d5e1df5` (invoker), `a7001b5a502baa4a9d32c7772a9cbdee64415aa8` (tests), and unchanged `a1dbbfb153e5f393c27904830eed04d96a83ab3d` (setter).

Current STATUS/TODO, the B04 task definition/history, NEXT_STEPS and doc/README were reread for this round. Previously read task standards and the original binding brief remain applicable. Uncommitted controller documentation, native author tests, generated ledger files and the existing user deletions in `docs/` are outside this fix. They were not changed by this reviewer.

## P2 closure and quality checks

The old finding required a selected-file link-count guard before decryption, without enumerating aliases or reopening the credential pathname. All parts are implemented:

- `Open-CredentialReadGuard` at invoker line 46 uses a small Windows interop class. Its sequential thirteen-DWORD layout correctly places attributes, three FILETIME pairs, volume serial, size pair, link count and file index pair. `GetFileInformationByHandle` takes a SafeFileHandle and marshals BOOL; failure is caught under the existing non-sensitive `path-rejected` diagnostic.
- Lines 71–72 require `Links == 1` and exclude directory/reparse attributes reported for the selected handle. This checks the actual opened file rather than trusting only its pathname. Existing selected-path and static ancestor reparse checks remain.
- Lines 79–87 open the selected path using read access and `FileShare.Read`, inspect that handle, and dispose it on rejection. Other write/delete sharing is excluded while that selected guard is held.
- Lines 103 and 110–112 retain the guard, check its `Length <= 65536`, and construct a leave-open StreamReader over that same FileStream. Credential bytes are not reopened by pathname. The unchanged config read is a separate explicitly selected input.
- Lines 159–160 dispose the StreamReader and guard in finally on normal and failure exits. Existing child-environment, BSTR, SecureString and process cleanup remains in that finally block. The parent's environment is still never mutated, which satisfies the binding restoration ruling.

The new hard-link test creates only external public synthetic DPAPI material, verifies an actual two-link file, and independently checks both selected names. It clears only its owned public child marker before each selected-name subcase and requires no marker, exit 2, exactly `credential-invoke: path-rejected`, empty stdout and unchanged ciphertext bytes. This closes the original coverage gap without allowing universal rejection to pass: the ordinary reuse test still launches the selected public Python child, checks the environment, receives the exact argument list, redacts both streams and preserves exit 23.

The permanent native-argument regression matches the original review's quote/backslash examples. It passed on both engines. Invalid configuration, ciphertext corruption and launch-failure paths also passed the focused fresh run. This exercised the guarded read on successful decoding and on failures after opening, while code inspection verifies finally ownership and disposal.

No unrelated change or new actionable issue was found. The original review's passing findings for setter DPAPI/no-overwrite, printable ASCII validation, masking, static paths, environment selection and stable diagnostics remain applicable. Unchanged areas were not needlessly retested in this fix review.

## Fresh reviewer execution

Working directory: `D:\Noetome`.

Metadata/code commands actually used included:

```powershell
git status --short
git rev-parse HEAD
git diff --name-status 6e61705ce577da692d5dccd62b5a20de4baaabbe cd01771459d0ae0f55ad8758fa8bd80f95f54512
git diff 6e61705ce577da692d5dccd62b5a20de4baaabbe cd01771459d0ae0f55ad8758fa8bd80f95f54512 -- scripts/invoke_api_credential.ps1 tests/test_api_credential_scripts.py
git diff --exit-code cd01771459d0ae0f55ad8758fa8bd80f95f54512 -- scripts/invoke_api_credential.ps1 scripts/set_api_credential.ps1 tests/test_api_credential_scripts.py
git diff --check 6e61705ce577da692d5dccd62b5a20de4baaabbe cd01771459d0ae0f55ad8758fa8bd80f95f54512
git ls-tree cd01771459d0ae0f55ad8758fa8bd80f95f54512 scripts/invoke_api_credential.ps1 scripts/set_api_credential.ps1 tests/test_api_credential_scripts.py
Get-FileHash -Algorithm SHA256 -LiteralPath '.superpowers/sdd/B04-T02_GATEWAY_PLAN/task-1-brief.md','.superpowers/sdd/B04-T02_GATEWAY_PLAN/task-1-review.md','.superpowers/sdd/B04-T02_GATEWAY_PLAN/task-1-fix-report.md','.superpowers/sdd/B04-T02_GATEWAY_PLAN/review-6e61705..cd01771.diff','scripts/set_api_credential.ps1','scripts/invoke_api_credential.ps1','tests/test_api_credential_scripts.py' | Select-Object Path,Hash | ConvertTo-Json -Compress
rg -n 'function Open|IsSingleOrdinaryFile|File::Open|IO.File.*Open|credentialGuard|credentialReader|finally|Links == 1' scripts/invoke_api_credential.ps1
powershell -NoProfile -NonInteractive -Command '$PSVersionTable.PSVersion.ToString()'
pwsh -NoProfile -NonInteractive -Command '$PSVersionTable.PSVersion.ToString()'
```

Actual engines: Windows PowerShell `5.1.26100.7920` and PowerShell `7.6.5`.

The exact focused fresh test command selected five tests per installed engine:

```powershell
@'
import importlib.util, unittest
from pathlib import Path
spec = importlib.util.spec_from_file_location('credential_fix_review', Path('tests/test_api_credential_scripts.py'))
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)
names = ['test_hardlinked_credential_rejected_before_child', 'test_child_gets_explicit_environment_arguments_redaction_and_exit_code', 'test_native_arguments_preserve_embedded_quotes_and_backslashes', 'test_parent_environment_preserved_after_launch_failure', 'test_invalid_environment_and_corrupt_credential_stop_before_child']
suite = unittest.TestSuite()
for engine, _ in m.SHELLS:
    cls = getattr(m, 'TestCredentials_' + engine)
    for name in names:
        suite.addTest(cls(name))
result = unittest.TextTestRunner(verbosity=2).run(suite)
raise SystemExit(not result.wasSuccessful())
'@ | & 'C:\Users\Lyinn\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe' -X utf8 -
```

Actual result: **10 tests in 18.529 seconds, OK, exit 0, no skips.** Both engines passed actual hard-link rejection for both original and alias names, ordinary reuse/environment/redaction/exit behavior, native quoting, launch failure with unchanged parent environment, and invalid config/corrupt ciphertext rejection. No test result was simulated.

## RED archive and worker evidence verification

The archived worker RED JSON contains the native stdout/stderr string with original CRLFs and progress-line trailing spaces. Its `native_output`, encoded as recorded UTF-8, was compared with the preserved raw plan file. Both are **exactly 3952 bytes**, byte-for-byte equal, with SHA256 `9894DFBD8249502A418E9DAF33D1F35AE57ADF63F9BB560265907BE5CBEC632A`. JSON encoding therefore preserves the RED evidence while avoiding a repository whitespace-check failure; the native output was not trimmed or reconstructed.

Exact verification command used:

```powershell
@'
import hashlib, json
from pathlib import Path
p = Path('doc/validation/assets/B04-T02-credential-fix-red.json')
a = json.loads(p.read_text(encoding='utf-8'))
native = a['native_output'].encode(a['encoding'])
raw = Path('.superpowers/sdd/B04-T02_GATEWAY_PLAN/task-1-fix-red.txt').read_bytes()
assert len(native) == a['raw_byte_length'] == len(raw)
assert hashlib.sha256(native).hexdigest() == a['raw_sha256'] == hashlib.sha256(raw).hexdigest()
assert native == raw
print('RED archive verified: exact 3952 bytes and SHA256 ' + a['raw_sha256'])
for p in [Path('doc/validation/assets/B04-T02-credential-fix-red.json'), Path('doc/validation/assets/B04-T02-credential-fix-green-full.txt'), Path('doc/validation/assets/B04-T02-credential-fix-green-full-result-tool.json')]:
    print(str(p) + ' SHA256=' + hashlib.sha256(p.read_bytes()).hexdigest())
'@ | & 'C:\Users\Lyinn\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe' -X utf8 -
```

Actual archive verification result: all assertions passed; exit 0. The RED text records two tests/four selected-name failures in 2.964 seconds. The worker correctly notes that the initial RED did not clear the marker before the second selection, so those second-name failures alone are not independent launch evidence. The first selection in each engine is independent evidence, and the original reviewer had separately demonstrated alias-name launch in both engines. The final test clears its marker per selection and does not retain that ambiguity.

The archived full GREEN text and tool completion record were inspected and report **32 tests in 42.232 seconds, OK, exit 0**. That is the worker's execution, not a full fresh reviewer rerun. The reviewer freshly executed only the ten focused tests above. The original review retains its earlier 28-pass run and two failing additional hard-link checks; those historical failures are preserved, not overwritten by this closure report.

## Limits and handoff

Only public synthetic fixtures in new external test temporary directories were used. The synthetic Python child performs no HTTP. No actual credential/profile/Codex authentication value, private source directory or alias list was read. No real or modal operator window, real provider/model request, code edit, commit, push, task-state update or unrelated directory cleanup was performed. Test-owned fixture cleanup remains local to the existing tests. This report is the only persistent artifact written by this reviewer in this round.

The guard closes the stated static selected-file hard-link issue. This review does not claim protection against adversarial concurrent ancestor replacement or creation of later aliases, generic transformed-secret output from arbitrary untrusted executables, immediate wiping of immutable managed strings, other Windows accounts/machines, or a broader cross-platform matrix. The selected-handle metadata/read ownership is confirmed by code inspection and actual execution on the two installed engines.

WPF modal valid save/cancel and actual operator entry remain controller acceptance work; this fix round did not rerun unshown control tests. Full workspace/Rust/Python acceptance, production credential reuse, provider capabilities, returned token usage and scientific knowledge quality remain outside this focused fix review. The binding parent-environment ruling remains satisfied by child-only ProcessStartInfo environment injection.

The original P2 requires no further fix. Controller can proceed with the remaining authorized Task 1/B04-T02 verification while retaining the real operator and real-call acceptance limits.
