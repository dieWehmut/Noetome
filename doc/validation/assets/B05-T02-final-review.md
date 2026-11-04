# B05-T02 final whole-branch review

Reviewed immutable range `e1c3e0867304f3cbfab75e3d7fd2d579e777646a..b3a31eaf87be8bbd4c119f4af19c92aefd958ca8` against the B05-T02 acceptance criteria, `KNOWLEDGE_VARIANTS_CONTRACT.md`, inherited coordination/source contracts and the implementation plan. This is the single final broad review. No product source, task state, index, branch or remote was changed; the two user-owned migration-document deletions were left alone.

**Spec Compliance: Needs fixes.** Most bounded v2 requirements are implemented, but R1 rejects a valid source-backed history case solely because its selected text unit begins at a nonzero source offset.

**Code / overall quality readiness: Not ready for acceptance until R1 is fixed and verified.** No Critical finding was found. There is one Important correctness issue and one previously identified optional Minor readability issue. The previous input-qualified original-ID P2 is addressed; it is not an outstanding finding.

## Strengths

- The ordinary command dispatches explicit v1/v2 versions into separate strict DTOs. Required nullable fields remain required, duplicate/unknown JSON fields are rejected, and the private v1-shaped identity projection does not rewrite v2 originals or change B04 exchange formats.
- Source admission still opens actual source sessions, validates original gold/candidates, recomputes the source review and compares full saved snapshot/IR/JSON/Markdown. Identity construction retains input qualification, original concepts/atoms and all supports in original link order. The complete stable v2 key contains condition status, full conditions/exceptions and source-backed time.
- Pair precedence agrees with the adopted contract: unknown applicability cannot merge; unresolved same-source versions and different units remain reviewable; complete condition differences precede history; history requires nonoverlapping Gregorian ranges. No newest-source or majority-wins policy is introduced. Actual totals preserve null canonical counts for unresolved results.
- Python reconstructs source, identities, context, fact IDs, pairs, totals and saved representations independently. Its absolute-offset context calculation correctly accepted the new counterexample's real upstream bundles, helping isolate R1 to Rust. Existing synchronized semantic forgeries do not rely on stale Markdown/IR/artifact fingerprints for rejection.
- Saved evidence distinguishes synthetic engineering fixtures from scientific correctness, retains historical preparation/RED failures, and keeps science `not_run`, research `not_requested` and publication false. The shared original `a-speed` regression is now source-backed and preserves both sources and supports.

## Findings

### Important — R1 / P2: Compare context coverage in absolute source coordinates

**Location:** `crates/noetome-cli/src/knowledge_coordinate_context.rs:153-154`; the explanatory comment at line 173 is also incorrect. Regression gap: `crates/noetome-cli/tests/knowledge_variants.rs:657` adds an in-unit prefix, while `tests/support/coordination_variants_case.rs:212` still selects ordinal 0.

`start` and `end` are byte indices into `u.text`, but candidate evidence offsets are absolute source offsets. The existing extraction validator explicitly requires `excerpt_start_byte >= unit.start_byte` and subtracts `unit.start_byte` before slicing (`crates/noetome-cli/src/extraction.rs:240-253`). The new coverage filter instead compares these absolute evidence positions directly with local `start/end`. A correct support beginning at `u.start_byte + start` is therefore discarded when the selected unit starts after byte zero, so the exact declared support set fails with `COORD_CONTEXT`.

**Actual reproduction:** The new isolated `final-review-offset-probe` uses the frozen `history` author and unchanged gold/context semantics, with a heading and blank line preceding the source text. Both selected ordinary text units are ordinal 2 and begin at absolute byte **25**. The context and original support begin at absolute byte **57** (unit-local byte 32). Both `extract verify` commands return **0**; independent Python `source_bundle` and `rebuild_context` accept their actual SQL/resources, full source-review bundles and context/support ranges. Ordinary `knowledge coordinate` returns **3 / COORD_CONTEXT** instead of the author's expected **0**, and no output root is created. This is a valid-input failure, not preparation, schema, identity or missing-support failure.

**Impact:** Valid conditional/history inputs from later text spans can be rejected. This is within the existing selected-Text contract; it does not require adding PDF coverage or broader language understanding.

**Fix:** Compare evidence positions against `u.start_byte + start` and `u.start_byte + end` (or explicitly convert both sides to unit-local coordinates) and correct the misleading comment. Add an ordinary source-backed regression selecting a unit with `unit.start_byte > 0`, preserving the complete joint background/fact support set and absolute emitted context range. Retain the existing rejection for a support that covers only the fact. Run the necessary affected regressions after the source change; do not change frozen author semantics to accommodate the bug.

**Evidence:** [Probe receipt and source-backed reconstruction](final-review-offset-probe/evidence.json), [ordinary failed command](final-review-offset-probe/04-coordinate.command.json), [raw error](final-review-offset-probe/04-coordinate.stderr.txt), [reproducible probe script](final-review-offset-probe.py). Six actual CLI calls were made in this new directory: one init, two imports, two successful upstream reviews and one coordinate. Original fixture/CLI/source fingerprints and new project/case inventories remain equal before/after the coordinate check.

### Minor — R2: Format the dense v2 DTO invocations

**Location:** `crates/noetome-cli/src/knowledge_coordination_v2_model.rs:67-73`.

The occurrence/fact/relation/totals/report/IR macro invocations pack many fields and serde attributes onto single long lines. This obscures future strict-schema changes and diagnostics. Formatting one field per line would improve reviewability without changing the DTO macro or wire behavior. This is the same optional Task 1 readability observation, not a new correctness blocker.

### Critical

None found in the reviewed scope.

## Evidence inspected and boundaries

- Read the controller brief first. Reviewed the complete changed production paths, external tests, Python verifier/audit and their source-backed fixture preparation, plus the relevant inherited identity/source admission code. Inspected the 14 author cases through a compact semantic projection and checked their gold/context/source/expected correspondence; no author expectation was derived from product output.
- Independently verified both supplied package hashes and all **82 immutable Git blob fingerprints**, including the evidence assets. Core package SHA256 is `952389b4ea99176fae5e9e5229038603802d889af88543d879b62b6e5ff56898`. All **31 frozen signed files** match the original freeze manifest. The manifest's earlier CRLF/LF diagnostic is preserved and its final byte handling does not change author semantics.
- Read and fingerprinted existing stable Rust command stdout/stderr. Actual fmt/check/workspace exits are 0. The raw workspace stdout contains **17 result suites, 589 passed / 0 failed / 0 ignored**. All **89 current dependency pins** match the stable before/after maps; CLI remains `ae987370f93969a27e9bff1c3b394c6f82fac93e12078b6fb6c9c4b73275cd8b`, 18,631,680 bytes. Actual recorded commands do not use locked/offline flags.
- Fingerprinted original fix1 RED/GREEN/full-Python streams. RED is **14 tests, 3 failures**, exit 1. GREEN is **14 tests / OK**, exit 0; full Python is **157 tests / OK**, exit 0. The raw unittest full duration is 103.282 seconds, while the outer receipt's process duration is 103.437 seconds; these measure different intervals. All **134 current final source/test/CLI/author pins** in successful receipts match.
- Inspected all **126 v2** and **56 v1** existing command stream fingerprints and recorded exits; v2 per-call CLI before/after pins agree. Final audit records contain **42/33 scenarios** and unchanged **75/29 protected groups**, respectively. These are saved test results, not newly rerun audits.
- Independently checked existing deleted-condition and invented-time forgeries plus the separately retained actual unit false-group forgery. Each inspected report has synchronized embedded Markdown JSON and all artifact fingerprints; successful-case forged IR matches the forged report fields. Saved semantic rejection evidence therefore is not explained by stale representations. The old shared-original-ID rejection has been corrected, with the genuine source-backed positive and deeper missing-second-support refusal retained.
- Verified the final Python archive's **16,763-member** and Rust resume archive's **6,135-member** inventories and complete archive hashes. Sampled eight actual member byte pins from each, including sources/SQL/originals/reports/raw failure or test output. I did not repeat the prior full CRC/member scan. Repetitive audit-progress snapshot omissions are disclosed and individually pinned.
- Compact independent inspection output is [final-review-evidence.json](final-review-evidence.json), produced by [final-review-inspect.py](final-review-inspect.py). The only new behavior execution was the six-command nonzero-unit probe prompted by the concrete source-coordinate gap. No passing full suite or matrix was rerun, no subagent was spawned, and no commit/status/push or next-task action was taken.

Both adopted rulings are consistent with the reviewed implementation: missing time blocks differences that need temporal comparison, and Python preserves the existing success-dict / AssertionError failure protocol. Formal conflict/science arbitration remains B05-T03 scope. Arbitrary natural-language conditions, unit conversion, full-PDF coordination, scientific benchmarks, cross-platform compatibility and product-scale performance were not tested or accepted by this review.

Root owns the one final fix wave, scoped re-review, task closure, final checks and remote verification. This report does not mark B05-T02 complete or pushed.
