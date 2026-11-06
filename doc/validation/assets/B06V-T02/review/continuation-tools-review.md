# B06V-T02 continuation tools independent review

Reviewer: `/root/review_scan_tools`, 2026-11-06. Scope is the current task's two new evidence utilities, their tests, and the four-line change to the independent scan checker. No product, ledger, plan, credential, model configuration, or subsequent task was changed by this review.

## Result

No actionable implementation defect was found in the reviewed state. The existing Windows creation/change-time correction is appropriate for the selected runtime: `same_open_file` compares common device/inode/size/mtime/link fields across path and descriptor stat, and birth time when available; each domain's complete identity, including ctime, remains independently checked before/after reading. It does not generally disable change detection. Linux keeps full cross-domain equality. The native Windows regression still requires the parent's Windows run; the Linux skip below is not evidence of native Windows success.

`capture_scan_budget_originals.py` reads only the completed explicitly selected helper invocation and recorded referenced files. It does not discover credential directories, launch a CLI, invoke a model, or modify a ledger. Source mapping keeps the original recorded spelling in the manifest while resolving only explicitly selected mappings. Longest-prefix selection and equal-depth ambiguity are checked. The utility binds all four phases, their immutable earlier records, selected binary/config, complete child stdout, actual ledger snapshots, and settlement through the independent budget checker; the returned manifest SHA must be retained independently. The capture itself is not a live recognition success claim.

The selected original hash, read, and copy limits use a one-byte overrun sentinel. `verify_scan_formula.file_pin` and `copy_unchanged` stop at the original length plus one; an overrun is rejected before excess bytes are copied. `checked_read` also reads no more than the original length plus one. The tests schedule actual file appends during real file I/O and confirm rejection, including an unchanged-length partial destination rather than an oversized copy. Existing paths are refused and failed new captures remain reviewable. Large runtime hashes are streamed and independently limited to 512 MiB.

`prepare_scan_obsidian.py` loads and fingerprints the current checker, independently rechecks saved CLI/output/budget provenance, and requires the accepted CLI and Obsidian executable/package pins. It copies held original output bytes into a new root and rechecks source, case, inputs, budget originals, native scripts, and selected launcher runtime. Source directories and links are rejected without traversal. Allowing a preexisting hard link for selected system PowerShell metadata does not relax the single-link source or artifact rules.

The preparer does not launch any application or request recognition. Its status stays `native_capture_status=not_run`. Successful native math is compared with the handwritten frozen author's mathematical tree; an unverified output profile stays on the original-image representation. Source uncertainty cannot be upgraded to success by a guessed clear recognition. The scoped collector additions preserve the existing collector, require exactly one insertion anchor each, verify the selected actual body theme, and require real PNG attachment clicks into local native image views with the independently bound dimensions. Preparing that collector alone does not prove rendering, click success, or cross-version compatibility.

## Actual checks in this review

Interpreter was `/tmp/noetome-b05-t03-python/bin/python`, reported Python 3.12.14. The following commands were executed against the working tree, with stdout/stderr and terminal exit 0 observed through the tool; no model calls were made:

| Command | Actual result |
| --- | --- |
| `python -m unittest discover -s tests -p 'test_capture_scan_budget_originals.py' -v` | 11 tests, 11 passed, 0 skipped, 0 failed, 0.230 s |
| `python -m unittest discover -s tests -p 'test_prepare_scan_obsidian.py' -v` | 19 tests, 18 passed, 1 skipped, 0 failed, 0.997 s |
| `python -m unittest discover -s tests -p 'test_verify_scan_formula.py' -v` | 46 tests, 46 passed, 0 skipped, 0 failed, 1.634 s |

Total: 76 tests, 75 passed, 1 platform skip. The skipped test is `test_native_windows_created_then_delayed_write_hash_and_read` because this reviewer runs on Linux. Synthetic helper/runtime fixtures are expressly artificial; these results do not establish actual provider recognition or native Obsidian rendering. The derived collector also passed the test's Node module syntax check on the available Node runtime.

Important actual assertions include: a changed descriptor ctime remains rejected; a changed path ctime remains rejected; non-ctime Windows identity changes remain rejected; file growth remains bounded; full four-phase budget arithmetic is rederived; unanchored/synchronized manifest substitution is rejected; a synchronized removed minus sign is rejected before a new preparation root exists; unknown-source success is rejected; selected runtime substitution is rejected; source changes after independent verification are rejected; existing human sentinels remain unchanged; unknown nested directories and source symlinks are rejected.

## Reviewed source pins

| Path | Byte length | SHA256 |
| --- | ---: | --- |
| `scripts/capture_scan_budget_originals.py` | 12365 | `cd189a9c26763bb6c257e5dfcdef16b8c11f406dbc1bb18fc5bfc7e449e6a5ea` |
| `scripts/prepare_scan_obsidian.py` | 23035 | `7fc2f31b676ca6946e343256aa45975a525e935ac63fbd615953a5c711a6b22e` |
| `scripts/verify_scan_formula.py` | 89503 | `b2982784a5cc87b16a610395f34f974fa6b5e9939d751e2201669b11629caa2c` |
| `tests/test_capture_scan_budget_originals.py` | 12351 | `c3745be1997aee77d0cc0fe78785c7c72807bd48b4193bdcea0f380d5cd85fe6` |
| `tests/test_prepare_scan_obsidian.py` | 17951 | `35160e5b928ab1283dc6c1025d1bf516b02fae82c788f908e4536001ed159a1e` |
| `tests/test_verify_scan_formula.py` | 41387 | `7a3932b1b9ffb96ceaa938d59256deedb843c02a8120854a2274aa0e4d48fdda` |

## Remaining parent acceptance work

The parent must still complete or independently bind current-source complete Windows/Python results, the native Windows ctime regression, complete Linux results, existing Rust and visual regressions, actual live request/response/settlement originals, actual Obsidian presentation/click originals, log consistency and Git checks. This scoped review does not replace those gates and does not complete the ledger or start B06V-T03.
