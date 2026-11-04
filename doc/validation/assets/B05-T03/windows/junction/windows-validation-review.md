# B05-T03 Windows validation evidence review

This review covers only the actual native checks and the supplementary NTFS ancestor-junction probe. It makes no repository or production settings changes.

## Native required gates

The native snapshot identifies commit `0dfa424c73522fb75b7803385c0e7897e3437289`, with 1323 tracked files checked before/after by the parent controller. The final formatter, workspace check, workspace tests after the pipe-runtime diagnostic, full Python tests and ordinary audit-v3 each have actual exit 0. The earlier workspace test exit 101 is retained as history; it is not counted as a pass.

The reviewer read the native command receipts and raw stdout/stderr and verified every recorded stream SHA/length. The workspace Rust stdout contains 18 suite summaries totaling **607 passed, 0 failed, 0 ignored**. Native Python reports **157 run, 1 skipped**, so 156 executed methods passed. The skipped new ancestor-symlink test is supplemented below by actual NTFS reparse checks. Native audit-v3 reports **55 scenarios, 172 ordinary CLI commands**; its four Linux-only alias scenarios are not claimed executed on Windows.

All 100 native audit source fingerprints match both the native files and the current WSL product source bytes. Audit source_before/source_after and protected_before/protected_after match. Detailed review values are in `acceptance-evidence-review.json`; parent command receipts remain in the native root.

## Actual NTFS ancestor-junction probe

The probe selected only a completed source-engineered `counter_to_claim` Rust root under the native snapshot's target/compiler-temp. Its saved Schema3 report preserves both 10/12, the refutes quotation explicitly concerning 10, needs_review and no successful coordination IR. The 44 frozen author-file signatures were checked before any probe check.

A new self-owned directory junction was created using a PowerShell file with structured parameters and native `cmd /d /c mklink /J`. Native lstat confirms FILE_ATTRIBUTE_REPARSE_POINT (0x400). The junction is left in this self-owned probe directory; no recursive deletion or traversal through it was performed.

There were **15 actual subprocess calls**:

- Generic and dedicated Python check entries each accept the ordinary direct case (exit 0, independently_verified=true).
- Each entry rejects project, case, output and gold through the junction ancestor: 8 checks return 1 with ancestor link/reparse diagnostics.
- Native Rust rejects ancestor project with exit 3 (source-review wrapper), ancestor case/output with exit 2, each with PROJECT_UNSAFE_PATH; no requested output is created.
- Generic Python also rejects a lexical junction/../ path before resolve can hide the junction.
- One call creates the actual junction itself.

The complete argv, raw streams, exit values, junction metadata and selected original project/case/output/gold/fixture inventories are in this directory. Original inventories and byte fingerprints match before/after. The CLI remains SHA256 `c8c03af4718bc668a89491033409142ff7fc00ccf462eae119488a2ed1426628`; selected verifier scripts remain unchanged.

`junction-probe-originals.zip` contains 152 members: command originals, drivers, selected source/project/old review/expected/result evidence and frozen authors. The archiver checks all CRCs and reads back every member byte. Its member list and exact archive SHA/length are in `junction-probe-archive.json`. It never traverses the junction.

## Acceptance disposition

No new Important defect was observed in these native checks. The Windows platform regression gate and the supplementary ancestor-reparse guard gate are now supported by actual evidence. The parent can close B05-T03 once it records these results, preserves the previous RED/failure/review history, runs current documentation/devlog/Git checks, and performs the authorized normal push with remote verification.

This review does not prove scientific accuracy, arbitrary natural-language refutation, GUI use of Schema3, production Vault maintenance, live model quality or all-platform compatibility. No model call, credentials disclosure or production file write was needed.
