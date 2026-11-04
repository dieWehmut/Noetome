# B05-T02 single final-fix scoped re-review

**R1: ADDRESSED. R2: ADDRESSED. Spec Compliance: PASS for this fix wave. Code / quality readiness: PASS for this fix wave. Round verdict: APPROVE the scoped final fix; no additional product fix wave is requested.**

Reviewed only `b3a31eaf87be8bbd4c119f4af19c92aefd958ca8..01742df21296b8b744b90155dbedb67f938fd312`, comprising R1 commit `3b6a59cb8cd06425464bc1dd40cf081bd4a9c20a` and R2 commit `01742df21296b8b744b90155dbedb67f938fd312`. The earlier whole-branch review remains the review of `e1c3e08..b3a31ea`; this report does not repeat that review or mark the product task complete. Root owns the independent full Python and v1/v2 audits, archival, acceptance, ledger and remote verification.

Read the scoped re-review brief, final-fix brief, original R1/R2 findings, fix report, exact supplied four-file diff, strict v2 contract and extraction coordinate definition. Checked the changes for introduced breakage and inspected covering saved raw evidence without re-running any already-passing suite or matrix. A read-only inspection script and its result are saved as [final-rereview-inspect.py](final-rereview-inspect.py) and [final-rereview-evidence.json](final-rereview-evidence.json). Only this report and inspection artifacts were written; no product source, index, HEAD, branch, task state, fixture or remote was changed by this reviewer, and no subagent was spawned.

## R1 / Important P2 — ADDRESSED

The root cause and fix agree with the inherited coordinate contract. `extraction.rs:240-253` validates candidate excerpt ranges against absolute `unit.start_byte/end_byte` and subtracts `unit.start_byte` to slice `unit.text`. The local context indices arise from searches in `u.text` (`knowledge_coordinate_context.rs:80-93,139`). The fixed code converts them to absolute source coordinates once (`knowledge_coordinate_context.rs:140-142`) and compares candidate evidence against those absolute bounds (`knowledge_coordinate_context.rs:153-157`). Emitted context reuses the same absolute bounds (`knowledge_coordinate_context.rs:181-182`). UTF-8 slicing remains local (`knowledge_coordinate_context.rs:175`), so the patch does not apply an absolute index to a unit-local string. The incorrect prior comment has been removed and the replacement accurately describes the shared evidence/output coordinate space.

The gate remains strict. It still filters only original supports for the selected actual unit, collects the full covering set, rejects empty/duplicate/invalid declarations, and requires exact equality with the declared evidence IDs (`knowledge_coordinate_context.rs:143-173`). No background, condition/time, source-admission or support requirement is weakened. The absolute-offset additions already existed in emitted output before this fix; the patch reuses the same bounded range computation for coverage rather than introducing a new range policy.

The positive regression is an ordinary source-backed frozen `history` case (`knowledge_variants.rs:672-690`). It calls the existing complete author expectation helper and explicitly asserts selected unit start25, context start57 and selection ordinal2 (`knowledge_variants.rs:683-689`). The helper verifies the complete original author totals/status/scientific fields, full occurrence values/conditions/exceptions/context, source byte slices and fingerprints, original concepts/atoms/all supports in link order, occurrence/fact IDs, fact groups, history direction and saved JSON/Markdown/IR correspondence (`knowledge_variants.rs:29-241`). This is more than an exit-code assertion.

Preparation preserves the frozen author and gold semantics, adding only a separate heading and blank separator to selected test source copies (`tests/support/coordination_variants_case.rs:61-68,144-154`). It selects ordinal2 and uses the author's exact joint context quote as the support (`coordination_variants_case.rs:149-150,206-222,238`), preventing the previously too-wide preparation support from masking the old comparison. Both ordinary upstream `extract verify` calls must succeed before coordinate (`coordination_variants_case.rs:239-255`). The empty-prefix path used by all existing callers retains ordinal0 and the previous whole-source trim behavior (`coordination_variants_case.rs:53-59,150,206-207`).

The negative regression shortens the actual support to the fact alone using an absolute start and recalculated quote/end/hash, then requires `COORD_CONTEXT/3` (`knowledge_variants.rs:693-707`). Both its upstream reviews succeeded in the saved raw roots. The exact support-set gate is unchanged; the genuine positive support starts57 while the negative support starts110 and omits the adjacent background. The negative stayed rejected in both RED and GREEN.

### R1 covering raw evidence

- Genuine RED: [final-fix-red-focused.stdout.txt](final-fix-red-focused.stdout.txt), [stderr](final-fix-red-focused.stderr.txt) and [receipt](final-fix-red-verification.json). The raw positive panics at `knowledge_variants.rs:676` with actual `Some(3)`, expected `Some(0)` and `COORD_CONTEXT`; the negative passes. Actual focused result is **1 passed / 1 failed / 0 ignored**, cargo exit1. The saved context source pin matches the original base Git blob, accounting only for Windows line endings. Both positive upstream reviews have original exit0 and status `passed`; their selected sources equal the original fixtures plus the new heading, original author/gold/expected are unchanged, and supports start57.
- Focused GREEN: [final-fix-green-focused.stdout.txt](final-fix-green-focused.stdout.txt), [stderr](final-fix-green-focused.stderr.txt) and [receipt](final-fix-green-verification.json). Actual focused result is **2 passed / 0 failed / 0 ignored**, cargo exit0. The saved positive outputs show unit start25 and absolute context start57; the negative remains `COORD_CONTEXT/3`.
- Independent ordinary GREEN: [final-fix-offset-green/evidence.json](final-fix-offset-green/evidence.json). Verified all **seven** raw calls: six actual CLI calls, including both successful upstream reviews and coordinate exit0, plus the ordinary full-author Python check exit0. The saved Python result is `independently_verified=true`, author status `passed`, exact complete author totals and input set `a,b`. The source-backed contexts for both inputs have unit start25 and support/context start57. The saved original author and expected equal the frozen `history` author; gold remains equal in the ordinary bundles. Inspected original concepts, atoms and every support/link order against candidates, actual source byte slices/hash, and equality of stdout/report/embedded Markdown/successful IR fields. The new test root remains separate from the old failing probe.
- All command/stream fingerprints were recomputed before trusting these results. The four focused roots' protection maps equal before/after and all mapped files/receipts still match their saved pins. All **94** independent GREEN protection pins, including the original failing probe/script, still match and before/after are equal. The full ordinary check's project/case/output inventories are unchanged.

The historical test-preparation pass and initial fmt failure remain preserved. Independently verified their named raw stream fingerprints using the saved relocation mapping: preparation exit0 is **not** counted as RED; initial stable fmt exit1 is **not** counted as a final passing check.

## R2 / Optional Minor — ADDRESSED

The v2 DTO invocations now place fields and serde attributes on separate lines, including the smaller case/identity/time/context DTOs (`knowledge_coordination_v2_model.rs:65-227,251-277`). The macro itself is unchanged at `knowledge_coordination_v2_model.rs:64`. Read every changed invocation in the supplied diff and independently compared the complete base/head file token sequences. After excluding whitespace and optional field trailing commas, all **1,956** significant tokens match across all **12** DTO invocations. This preserves every original field, order, type, nullable deserializer, enum/version rule, `deny_unknown_fields` rule and wire behavior. No whitespace-only test was added.

## Introduced breakage in the four-file fix

**Critical: none. Important: none. Minor: none.**

No introduced correctness or maintainability finding was identified in this bounded patch. The coverage change corrects the coordinate mismatch and retains the complete support gate; test helpers retain the old path for existing callers; DTO changes preserve significant tokens. The added regression demonstrates the actual old production failure, its corrected behavior and the retained refusal rather than changing frozen expected semantics to make the old behavior pass.

## Final stable verification evidence

Read and fingerprinted the raw saved command/stream files named in [final-fix-stable-final-verification.json](final-fix-stable-final-verification.json). These are the fix executor's actual saved runs, not tests executed again by this reviewer:

| Recorded final command through `scripts/cargo_local.ps1` | Exit | Actual outer duration |
| --- | ---: | ---: |
| `fmt --all --check` | 0 | 0.916481 s |
| `check --workspace --locked --offline` | 0 | 3.562347 s |
| `test --workspace --locked --offline -- --test-threads=4 --nocapture` | 0 | 124.176575 s |

Raw workspace stdout contains **17 result suites, 591 passed / 0 failed / 0 ignored**, including **27 knowledge_variants tests** and both new regression names passing. The ordinary CLI/B04/v1 and other workspace tests are part of that recorded run; no additional broad result is inferred from it. The receipt records the selected runtime via `NOETOME_TEST_PYTHON/NOETOME_PDF_PYTHON`, real argv/cwd/environment, streams, exits and durations. Explicit locked/offline flags and test threads4 are present.

Independently verified that all **139** final source/build/test/fixture pins match their equal before/after maps and current files, all **121** final executable pins match current files, and all four changed working files match immutable fix-head blobs after line-ending normalization. The **68** stable source-backed variants roots have equal before/after protection maps; verified their **4,964 protected-file instances** and **2,839 saved raw receipt/stream instances**. The main inspection checked **5,782 unique file pins** in total, plus the four preserved preparation/fmt raw streams checked afterward.

Current CLI equals the recorded final stable CLI and every independent GREEN per-call before/after pin: SHA256 `74b5c8f6af000b70fd21a8bc8c556d9c90c3d40dc5ec641dfbd18ab85a3649e1`, **18,631,680 bytes**. Earlier and final build hashes differ as expected; no unchanged prebuild/final binary claim is made.

## Out-of-scope evidence observation

The independent GREEN adaptation retains original probe labels `base=e1c3e08`, `head=b3a31ea` in `final-fix-offset-green/evidence.json:3-4` and its script. Those labels describe the inherited original probe baseline and do not identify the executed GREEN commit. The actual GREEN command receipts instead consistently pin the final CLI (`evidence.json:198-204` for coordinate), and their protected fixed source pins match the accepted fix head. The report is unambiguous about the accepted range `b3a31ea..01742df`. This is a nonblocking provenance-label observation outside the four-file patch, not introduced product breakage. Root has confirmed it will retain the original receipt and add explicit supplemental provenance in the final manifest/validation, without silently rewriting historical evidence or starting another product wave.

This scoped approval does not certify arbitrary natural-language applicability, scientific correctness, unit conversion, full-PDF coordination, cross-platform compatibility or product-scale performance. Root's independently running complete Python and ordinary v1/v2 matrices, final document/log checks, acceptance/archival and authorized normal push remain controller gates outside this review. No R1/R2 blocker or further product fix remains from this round.


