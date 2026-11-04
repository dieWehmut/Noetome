# B05-T02 single final-fix report

DONE for the assigned R1/R2 Rust wave. Product B05-T02 remains in_progress; root owns scoped re-review, complete Python and v1/v2 audits, closure, and push.

Base: `b3a31eaf87be8bbd4c119f4af19c92aefd958ca8`. R1 commit: `3b6a59cb8cd06425464bc1dd40cf081bd4a9c20a`. R2 commit: `01742df21296b8b744b90155dbedb67f938fd312`.

## Root cause and behavior

`rebuild` finds byte indices in `RequestUnit.text`; candidate excerpt offsets are absolute source coordinates. Extraction validation bounds them against unit.start/end and subtracts unit.start before slicing (extraction.rs:240-253). The old coverage gate compared an absolute excerpt start directly with a local context start. R1 computes absolute_start/absolute_end once and uses them for coverage and emitted context. The corrected comment states the shared coordinate space. UTF-8 slicing remains local; exact complete support-set and background/fact requirements remain.

The new source-backed Rust regression reuses the frozen history author, unchanged gold/expected/context semantics and all original supports. Only a heading/blank separator is prepended to the selected test sources. Ordinary extract verify selects ordinal2 and succeeds for both inputs, with unit.start_byte=25 and joint support/context.start_byte=57. The positive checks the complete existing author-expectation helper (facts, occurrences, history direction, totals, all original supports/order, source slices and fingerprints) plus explicit selected ordinal and absolute coordinates. The negative shortens the support to only the fact using absolute offsets; upstream reviews still succeed and coordination refuses COORD_CONTEXT/3.

## RED, preparation, GREEN

- Genuine RED: `final-fix-red-verification.json`, `final-fix-red-focused.command.json`, original raw `.stdout.txt` / `.stderr.txt`. Cargo exit1; 2 tests, 1 failed positive (actual3 / expected0, COORD_CONTEXT), 1 passed negative. Both positive upstream reviews returned0. Prepared/protected roots and stream pins are in the verification JSON.
- Focused GREEN: `final-fix-green-verification.json` and `final-fix-green-focused.*`; exit0, 2 passed/0 failed. Negative fact-only protection retained.
- Earlier test preparation accidentally used a broader support from byte25, which masked the old filter because25<=local32. That attempt passed and is not RED evidence. All raw streams/receipt/snapshots were retained under `final-fix-red-preparation-*`. `final-fix-preparation-relocations.json` maps their original receipt paths to preserved files; original receipt contents were not rewritten. The test was corrected to use exactly the frozen author joint context (absolute57), matching the original review counterexample, before production was edited.
- Initial final fmt refused a multiline helper signature. Its actual failure/diff is retained in `final-fix-stable-fmt.*` and `final-fix-stable-verification.json`. Only the suggested signature formatting was corrected; subsequent stable-final validation is below.

## Final stable Rust verification

Runner: `.superpowers/sdd/B05-T02_VARIANTS_PLAN/final-fix-verify.py stable-final`, run from `D:/Noetome` with selected absolute runtime `C:/Users/Lyinn/.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe -X utf8`. Environment supplies NOETOME_TEST_PYTHON and NOETOME_PDF_PYTHON through the runner; runtime path is only in ignored tooling/evidence, never tracked source. Full argv/cwd/environment, raw stream pins, start/elapsed/exit, before/after source/build/test maps and actual CLI/test executable fingerprints are in `final-fix-stable-final-verification.json`.

- `& './scripts/cargo_local.ps1' fmt --all --check`: exit0, 0.916481 seconds; raw `final-fix-stable-final-fmt.stdout.txt` / `.stderr.txt`, per-command `.command.json`.
- `& './scripts/cargo_local.ps1' check --workspace --locked --offline`: exit0, 3.562347 seconds; raw `final-fix-stable-final-check.stdout.txt` / `.stderr.txt`, per-command `.command.json`.
- `& './scripts/cargo_local.ps1' test --workspace --locked --offline '--' --test-threads=4 --nocapture`: exit0, 124.176575 seconds; raw `final-fix-stable-final-workspace.stdout.txt` / `.stderr.txt`, per-command `.command.json`.

Workspace raw output: 17 result suites, 591 passed / 0 failed / 0 ignored; knowledge_variants has27 tests. The workspace invocation ran once after final code. 139 source/build/test/fixture input pins equal before/after and were rechecked against current files after commits. 121 actual CLI/test executable pins were saved and rechecked after commits. All 68 source-backed variants roots have equal before/after protection maps, with raw CLI argv/stdout/stderr/exit fingerprints retained.

Final CLI: `{"sha256": "74b5c8f6af000b70fd21a8bc8c556d9c90c3d40dc5ec641dfbd18ab85a3649e1", "byte_length": 18631680}`. The build changes the previous CLI as expected; postbuild binaries remain stable. No claim that prebuild and final binary hashes are equal.

## Independent nonzero-offset GREEN

Adapted script: `final-fix-offset-green.py`, generated by `final-fix-adapt-probe.py`; separate root `final-fix-offset-green/`. The original `final-review-offset-probe.py` and entire old failing root were included in protection pins and were unchanged.

Seven ordinary calls: six CLI calls (init, two imports, two upstream reviews, coordinate) plus `scripts/verify_knowledge_coordination.py check <project> <case> <output> <expected.json>` under the selected Python. Every call has original stdout/stderr and command JSON including argv/cwd, start/elapsed/exit and CLI before/after. `final-fix-offset-green/evidence.json` records all receipts, the source-backed Python context reconstruction, and the independent full-author successful result. Both unit starts25 and context/support starts57; coordinate exit0. The independent check reconstructs and compares all author occurrence/fact/pair direction expectations and all representations, and its full returned totals/status/input set are explicitly asserted against the frozen history author. Project/case inventories are unchanged by coordinate; project/case/output inventories unchanged by check. Original authors, source, gold, expected, old counterexample and source/protection pins remain equal before/after and current.

## Self-review and scope

R2 only expands DTO macro invocations one field per line. All non-whitespace tokens, field order and serde wire/deserializer rules equal the original blob after ignoring optional trailing field commas (verified again after commit). DTO macro itself is unchanged; no whitespace-only tests were added.

Exact commits cover only knowledge_coordinate_context.rs, knowledge_variants.rs, support/coordination_variants_case.rs (R1) and knowledge_coordination_v2_model.rs (R2). Index ownership was checked empty before work and against exact file sets before each commit. Git diff --check ran before and after both commits with exit0; raw git receipts are `final-fix-git-receipts.json` and `final-fix-*.stdout.txt/.stderr.txt`. No task transition, generated logs, Python production, fixtures, contract changes, user docs deletion index entries, push, subagents or later-task work occurred.

Remaining concerns: no known R1/R2 concern. Root must still perform the scoped fix-range review, fresh full Python157 and v2/v1 matrices, ledger/check/closure and normal remote push. The engineering source-backed case does not prove arbitrary semantic conditions, scientific validity, unit conversion, PDF coordination, cross-platform behavior or product-scale performance.
