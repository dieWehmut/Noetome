# B06V-T02 budget and credential boundary subreview

Read-only review of the selected scan helper and independent original-ledger binding. No product/script/test source was changed, no user credential or current ledger was read, and no CLI, HTTP or full suite was started. Real state is still missing; live budget and Vision acceptance remain not_run.

## Important finding: nested JSON Unicode credential echo is retained

At reviewed helper SHA `0e7f6126a6c9773ac906eb571eab35e54ece5f2a6a66394335a58c6157ae9c8e`, `Hide-Credential` (lines 194–201) matches literal or single Unicode escapes only. A key encoded as Unicode and then serialized into another JSON string uses two backslashes before each Unicode escape. The current function returns all 266 recoverable bytes for the public 38-byte fixture key. `Save-Phase` lines 156–161 consequently writes such missed output with `omission_reason=null`; line 300 also forwards it to standard output.

Both actual Windows PowerShell 5.1 and PowerShell 7 read-only function probes reproduce the same result: plain and single Unicode cases redact, nested Unicode does not; each probe exits 1. Source bytes did not change. Exact helper source, extracted controlled probe, stdout/stderr and metadata are preserved in `budget-subreview.helper-original.ps1`, `budget-subreview.echo-probe.ps1`, `budget-subreview.powershell5.*`, `budget-subreview.powershell7.*` and `budget-subreview.echo-red.json`. Only a public synthetic key was used.

This is the helper's independent output/persistence boundary. Core already has a recursive credential-echo detector for model responses; the finding does not show that such a model response bypasses that Core detector. A repair should perform bounded recursive escape decoding/detection, or safely omit the whole output when an echo is found at any depth. Add actual nested and mixed-escape Windows regressions before using the repaired helper for live credentials. No repair or green retest was performed by this subreview.

## Other reviewed behavior

The helper opens a selected existing ledger with a held read/write handle, denies write/delete sharing, checks ordinary files/hard links and guarded ancestors, rejects duplicate JSON names including Unicode and case duplicates, and recomputes confirmed/pending/remaining from records. It preserves previous history, reserves the selected total cap before launch, settles valid usage even after semantic exit 3, and keeps unknown usage pending. Missing key/ledger stops without creating a new current ledger. The guarded write is non-atomic; before backups and unknown reservations remain the documented recovery boundary.

The independent binding requires an independently supplied manifest SHA, explicitly copied originals, all four helper phases, previous-record equality, recomputed quota arithmetic, actual config/binary pins, same paths and invocation, saved request and complete stdout, response fingerprint, usage and semantic exit. It distinguishes helper total-cap reserve from Core engineering reserve. The SHA anchor must originate in an independent capture; caller-generated files cannot by themselves prove current-account provenance. Byte-based Core reservation is an engineering estimate, not verification of a Vision tokenizer. Real usage/reservation acceptance has not been run.

A fresh read-only audit of the final g-run public mirrors passed for 20 existing synthetic helper invocations: 80 phase receipts, 80 ledger snapshots and 200 selected file pins. Earlier records, reserve increment, exact launch/reserved bytes, settlement arithmetic, original UTF-8 stream pins and omission reasons matched. Details are in `budget-subreview.originals-readback.json`. These were artifact reads only, not 20 new invocations or live-budget proof.

## Limits and required follow-up

The nested-echo finding is outstanding at this reviewed version. Parent/root has been notified and owns the fix and new validation. Full Linux/Windows regression and native Obsidian/model work were not run here. Historical 30,097 use and 9,969,903 remaining cannot be asserted as current while selected files are absent. No current-state recovery, fresh quota initialization, task completion, Git action or next product task occurred.
