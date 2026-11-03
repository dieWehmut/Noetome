# B04-T02 Task 1 independent review

Review date: 2026-11-03. Scope: the credential save/reuse scripts, their Windows process tests, and their evidence in the immutable range below. This is a review of internal Task 1, not acceptance of B04-T02's real model invocation.

**SPEC: FAIL — changes required. QUALITY: FAIL — one actionable P2 finding.** The existing 28 process tests pass, and extra argument quoting checks pass. Both Windows engines accept a selected credential file with two hard links, contrary to the binding link rejection requirement. No other actionable issue was found in this scope.

## Inputs and immutable identity

Binding brief read first: `.superpowers/sdd/B04-T02_GATEWAY_PLAN/task-1-brief.md`.

Immutable review package: `.superpowers/sdd/B04-T02_GATEWAY_PLAN/review-d1a46df..6e61705.diff`.

Base: `d1a46df397d58b2f4fedd5b3aecd8a67dcebad03`.

Head: `6e61705ce577da692d5dccd62b5a20de4baaabbe` (`feat(api): store and reuse user credentials with DPAPI`). `git rev-parse HEAD` matched this head when inspected. The three production/test files are absent from the base tree. `git diff --exit-code <head> -- <all five reviewed files>` returned 0 before report creation: the working files inspected and executed match the immutable head.

| Input/file | SHA256 of inspected bytes |
| --- | --- |
| Task 1 brief | `4FC3C59E0261D7AB1A0E558282EFF23766D81094EDEDE49BEBF6D3BFF7EBA510` |
| Immutable review package | `9ADA37AF6D5A447363F434323E28091D4D6F798A182D7DDD634773082ECD9FB9` |
| `scripts/set_api_credential.ps1` | `99001ED7A90266480440B753659965DFED875D85B055821EC76CCABBD0B3C0CB` |
| `scripts/invoke_api_credential.ps1` | `EEA8034DBF773AD488F0ED2846E0E5185BBD4EE12F0F3D36C09DFB9E6B2B0B9D` |
| `tests/test_api_credential_scripts.py` | `D3BD41A3638A815E39B8727010907287657E0D10223D263142239031FE5D8A83` |
| `doc/validation/assets/B04-T02-credential-implementation.md` | `A3EAF93AB70C575FD5ADECD12E0058BE49464A7C6FA2C2044512978CEE1EF272` |
| `doc/validation/assets/B04-T02-credential-python.txt` | `3DBA8CF5BBDBC99DD536693079286E2F1AEC8A83A797730D844CE2B111578678` |

The head's Git blob IDs, independently inspected using `git ls-tree`, are `a1dbbfb153e5f393c27904830eed04d96a83ab3d` (setter), `cb54b4e9750300745ed7d206400d2cdb01149bfc` (invoker), `a072e27b7cc1ebac08206889a9d748aa6af9ca77` (tests), `ab8229d314053cf210afc5445f81d29853f37e48` (implementation report), and `9574901feef0e0d1edd30b3a2f9a79bb20756801` (controller Python evidence).

Repository instructions and the current STATUS/TODO/tasks.json, NEXT_STEPS, doc/README, WORKFLOW, TESTING and EXTRACTION_API_CONTRACT were read for context. Uncommitted controller documentation, native author tests, generated logs, and the existing user deletions in `docs/` are outside the implementation diff and were not modified. Task-state, Git commit/push, and cleanup of unrelated directories remain outside this review.

## Actionable finding

### [P2] Reject hard-linked credential files before decryption

Location: `scripts/invoke_api_credential.ps1:30`, through the final ordinary-file check at line 36 and credential read at line 64.

`Assert-CredentialPath` checks the selected path and ancestors for `FileAttributes.ReparsePoint`, then accepts any existing ordinary file. Windows hard links are ordinary file entries and do not have that attribute. An external alias therefore passes, even when the selected credential file has multiple directory entries. The brief explicitly requires rejecting credential links. Multiple aliases also defeat the assurance that the credential is confined outside the repository: an external selected name alone says nothing about the locations of the other aliases.

Actual offline reproduction on Windows PowerShell 5.1 and PowerShell 7:

1. Use the production setter to create a new external DPAPI file from the existing public synthetic test key.
2. Create a second external name using `os.link(original, alias)` and confirm the selected file's `st_nlink == 2`.
3. Invoke the production wrapper with that alias and the existing synthetic Python child.
4. Expected: exit 2 with `credential-invoke: path-rejected`, before child launch. Actual on both engines: exit 0 and the child launched normally. The wrapper still redacted the public value, so this test did not demonstrate a plaintext leak; it demonstrated the missing link protection.

Required fix: open only the explicitly selected credential file with a read guard, inspect its link count using that selected handle, reject link count greater than one with the existing stable path diagnostic, and read the DPAPI bytes through the same guarded file. This needs no alias enumeration or private-directory scan. Add a public synthetic hard-link regression for both Windows engines proving the child does not run. Retain the existing ordinary-file success, static junction rejection, DPAPI secrecy and parent environment checks. A static link-count test is sufficient to reproduce this finding; this review does not demand a general adversarial ancestor-race defense.

The controller explicitly confirmed during review that credential-file links include hard links; this is a binding requirement, not an optional hardening suggestion or a waived concern.

## Specification assessment

| Requirement | Assessment and evidence |
| --- | --- |
| Explicit save/reuse interfaces; no plaintext key argument or implicit credential lookup | PASS by code inspection. Setter uses masked Read-Host, optional WPF, or the explicitly named process variable. Invoker reads only the explicitly selected config/credential for this purpose. |
| CurrentUser DPAPI; new external ordinary destination; no overwrite | PASS for ordinary selected paths. Default `ConvertFrom-SecureString` is used with `FileMode.CreateNew`. Actual roundtrip asserted equality without printing the public value. Existing sentinel bytes survive rejection. Static junction ancestors, repository/relative/ADS/missing-parent destinations are rejected. |
| Nonempty printable ASCII without whitespace, maximum 8192 bytes | PASS. BSTR character inspection rejects characters outside 33..126. Original tests reject empty/control/space/Unicode/8193-character values and accept 8192. Invoker independently validates decrypted SecureString before child launch. |
| Native masked dialog with service/model, save/cancel and no default Enter bypass | PASS by code inspection plus actual installed WPF loading/control/invalid-save-handler tests. Save is not a default button and all save clicks call validation. No modal dialog or operator input was performed by this reviewer. |
| Invoker rejects repository credential paths and links | FAIL for hard links as reproduced above. Direct repository, directory and static junction checks pass. |
| Config chooses a 2..64-character uppercase environment name | PASS. The exact name is validated with a case-sensitive regex, and corrupt/malformed selections stop before child launch with stable diagnostics. Full config semantics are delegated to the selected existing CLI. |
| Decrypted key only in process memory/selected child environment; no key arguments or plaintext files | PASS for exercised paths. Actual Python child compares its environment with the public fixture, reports only a Boolean, and receives the literal selected CLI arguments. Its deliberate stdout/stderr echoes are redacted. |
| Preserve CLI exit status and unchanged parent environment; finally cleanup | PASS. Controlled exit 23 is preserved. Present/empty-or-absent parent state and launch failure are checked. ProcessStartInfo changes only the child environment. Per the binding controller ruling, unchanged parent environment satisfies the restoration outcome; no needless parent mutation is requested. BSTR and SecureString cleanup are in finally. |
| Windows PowerShell 5.1/7 and stable errors | PASS for the actual scoped test matrix on versions 5.1.26100.7920 and 7.6.5. Fixed diagnostic labels are used instead of exception/input bodies. Test harnesses intentionally let each engine establish its own module search paths. |
| No HTTP/retries/batches/private scans | PASS by code inspection and bounded synthetic child tests. Wrapper launches exactly one existing `extract api` command; the reviewer selected only a public synthetic Python child. |

The implementation report records earlier RED and repair runs. This reviewer read that history and confirmed the base lacks the scripts, but did not independently reproduce those historical runs. They are reported evidence, not fresh reviewer executions. The controller's archived 72-test Python evidence was inspected; it is not claimed as independently rerun here.

## Actual commands and results

Working directory for all commands: `D:\Noetome`.

Metadata and byte verification commands actually used included:

```powershell
git status --short
git rev-parse HEAD
git show --format=fuller --no-patch 6e61705ce577da692d5dccd62b5a20de4baaabbe
git diff --name-status d1a46df397d58b2f4fedd5b3aecd8a67dcebad03 6e61705ce577da692d5dccd62b5a20de4baaabbe
git ls-tree 6e61705ce577da692d5dccd62b5a20de4baaabbe scripts/set_api_credential.ps1 scripts/invoke_api_credential.ps1 tests/test_api_credential_scripts.py
git ls-tree d1a46df397d58b2f4fedd5b3aecd8a67dcebad03 scripts/set_api_credential.ps1 scripts/invoke_api_credential.ps1 tests/test_api_credential_scripts.py
git ls-tree 6e61705ce577da692d5dccd62b5a20de4baaabbe doc/validation/assets/B04-T02-credential-implementation.md doc/validation/assets/B04-T02-credential-python.txt
git diff --check d1a46df397d58b2f4fedd5b3aecd8a67dcebad03 6e61705ce577da692d5dccd62b5a20de4baaabbe
git diff --exit-code 6e61705ce577da692d5dccd62b5a20de4baaabbe -- scripts/set_api_credential.ps1 scripts/invoke_api_credential.ps1 tests/test_api_credential_scripts.py doc/validation/assets/B04-T02-credential-implementation.md doc/validation/assets/B04-T02-credential-python.txt
Get-FileHash -Algorithm SHA256 -LiteralPath '.superpowers/sdd/B04-T02_GATEWAY_PLAN/task-1-brief.md','.superpowers/sdd/B04-T02_GATEWAY_PLAN/review-d1a46df..6e61705.diff','scripts/set_api_credential.ps1','scripts/invoke_api_credential.ps1','tests/test_api_credential_scripts.py' | Select-Object Path,Hash | ConvertTo-Json -Compress
Get-FileHash -Algorithm SHA256 -LiteralPath 'doc/validation/assets/B04-T02-credential-implementation.md','doc/validation/assets/B04-T02-credential-python.txt' | Select-Object Path,Hash | ConvertTo-Json -Compress
Get-Command powershell,pwsh | Select-Object Name,Source | ConvertTo-Json -Compress
powershell -NoProfile -NonInteractive -Command '$PSVersionTable.PSVersion.ToString()'
pwsh -NoProfile -NonInteractive -Command '$PSVersionTable.PSVersion.ToString()'
```

Diff formatting and exact-head file checks returned exit 0 with no differences. Engine paths observed were `C:\WINDOWS\System32\WindowsPowerShell\v1.0\powershell.exe` and `C:\Users\Lyinn\.cache\codex-runtimes\codex-primary-runtime\dependencies\native\powershell\pwsh.exe`.

Fresh original scoped regression:

```powershell
& 'C:\Users\Lyinn\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe' -X utf8 -m unittest discover -s tests -p test_api_credential_scripts.py -v
```

Actual result: **28 tests in 33.618 seconds, OK, exit 0, no skips**. Fourteen tests per engine.

Fresh additional checks used the following exact stdin command. The dynamically constructed classes also inherited the original tests, so this run intentionally records 32 executions rather than presenting it as only four tests:

```powershell
@'
import importlib.util, json, os, unittest
from pathlib import Path
spec = importlib.util.spec_from_file_location('credential_review_subject', Path('tests/test_api_credential_scripts.py'))
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)
class ExtraChecks(m.CredentialChecks, unittest.TestCase):
    def test_quote_roundtrip(self):
        self.assertEqual(self.save().returncode, 0)
        self.make_child()
        project = 'public project \\ with "quoted" segment\\'
        import_id = 'public\\\\"quoted-id\\'
        output = 'public-output\\'
        code = (f'& {m.ps_quote(m.INVOKER)} -Binary {m.ps_quote(__import__("sys").executable)} '
                f'-Project {m.ps_quote(project)} -ImportId {m.ps_quote(import_id)} '
                f'-Config {m.ps_quote(self.config)} -OutputRoot {m.ps_quote(output)} '
                f'-CredentialFile {m.ps_quote(self.credential)}; exit $LASTEXITCODE')
        result = self.run_ps(code)
        self.assertEqual(result.returncode, 0, 'extra quoting child failed')
        args = json.loads(result.stdout.splitlines()[0])['args']
        self.assertEqual(args, ['api', project, import_id, str(self.config), output])
    def test_hardlink_is_rejected(self):
        self.assertEqual(self.save().returncode, 0)
        self.make_child()
        alias = self.work / 'public-hardlink.dpapi'
        os.link(self.credential, alias)
        self.assertEqual(self.credential.stat().st_nlink, 2)
        result = self.run_ps(self.invoke_code(credential=alias) + '; exit $LASTEXITCODE')
        self.assertEqual(result.returncode, 2, 'hard-linked DPAPI credential launched child instead of path-rejected')
        self.assertIn('credential-invoke: path-rejected', result.stderr)
suite = unittest.TestSuite()
for name, shell in m.SHELLS:
    cls = type('Extra_'+name, (ExtraChecks,), {'shell':shell})
    suite.addTests(unittest.defaultTestLoader.loadTestsFromTestCase(cls))
result=unittest.TextTestRunner(verbosity=2).run(suite)
raise SystemExit(not result.wasSuccessful())
'@ | & 'C:\Users\Lyinn\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe' -X utf8 -
```

Actual result: **32 tests in 38.712 seconds, FAILED (failures=2), exit 1**. The two failures were precisely `test_hardlink_is_rejected` on each engine (`AssertionError: 0 != 2`). Both extra native argument checks passed, including embedded quotes, backslashes immediately before quotes and trailing backslashes. The 28 inherited regression executions again passed. No test failure is concealed or counted as protection success.

## Quality assessment and tested limits

The scripts have a narrow scope, compatible syntax, explicit selected inputs, stable diagnostics and cleanup in finally. `CreateNew` provides actual no-overwrite behavior. Child streams are drained asynchronously to avoid pipe deadlock and are buffered for literal credential redaction. Native quoting follows the Windows CRT rules and the additional real-child roundtrip passed. Parent environment preservation is stronger than temporary parent injection and does not need a redesign.

The hard-link omission is the only actionable quality finding. Static reparse checks are not the same as checking selected-file alias count, and the committed test matrix covers junctions but omits this ordinary-file alias case.

Immutable .NET plaintext/output strings are dereferenced, not guaranteed immediately wiped from managed memory. The BSTR is zero-freed and SecureString disposed. The script is an adapter around a selected trusted CLI, not a sandbox for arbitrary binaries that encode or transform a secret. Literal output redaction was actually tested; general transformed-secret output safety is not claimed.

Path checks were tested against static junctions and hard links. Hostile concurrent ancestor replacement, alternate Windows accounts/machines, POSIX behavior beyond declared skip/unsupported handling, and arbitrary custom PowerShell module environments were not tested. The original tests remove inherited PSMODULEPATH case-insensitively so each installed engine establishes its own module paths.

No actual credential file, Codex authentication value or saved-profile secret was read. No aliases were enumerated. All generated DPAPI material came from public synthetic input and existed only in newly created external test temporary directories. The tests performed their own owned temporary-fixture teardown; no unrelated directory cleanup occurred. No HTTP, real model request, modal operator window, user input, commit, push, or task-state change was performed. The only persistent reviewer-written artifact is this report.

WPF tests instantiated actual installed controls and executed the actual invalid-save handler without showing the window. Valid modal save/cancel and real operator entry remain controller acceptance work. Full Rust/workspace checks, full fresh Python regression, production credential reuse, provider behavior, real token usage and scientific knowledge quality are outside this scoped reviewer execution. The reported controller 72-test pass is historical controller evidence, not a substitute for the failing independent alias check.

Task 1 should receive one focused fix for the selected-file hard-link guard and regression, then a fresh scoped review of that delta. This report does not authorize completing B04-T02 or substituting synthetic execution for its real-call acceptance.
