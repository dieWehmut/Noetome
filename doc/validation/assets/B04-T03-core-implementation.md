# Task 1 implementation report

Status: DONE_WITH_CONCERNS (component work complete; whole-task/live/PDF acceptance remains controller Task 2).

Commit: `1d4793c4b1b6015c30735b65d8a1fcae5130ea09`, parent `f11100df14318c470d84eae9dc728f6ceb4ff7d7`, branch `codex/b04-t03-knowledge-verify`.

Owned commit files: `.gitattributes` (only controller-authorized new B04-T03 fixture/audit hunk); `crates/noetome-cli/src/{extraction_verify.rs,extraction_review_model.rs,extraction_review.rs,extraction_output.rs,lib.rs,main.rs}`; `crates/noetome-cli/tests/extraction_verify.rs`; all six new `fixtures/extraction-review-v1/{README.md,source.txt,gold.json,review.json,decision.json,candidates.json}`. Controller plan change and both user deletions remain unstaged. No ledger, controller docs/assets, model/config/credentials or network read was performed. No push: controller owns Task 2/full checks/final push.

## Binding external interface

`pub fn extraction_verify::run(project: &Path, import_id: &str, case: &Path, output: &Path) -> Result<VerificationOutput, ProjectError>`; public output fields `json: String`, `failed: bool`. Ordinary CLI: `extract verify <project> <import-id> <review-case> <new-output>`, optional `--` immediately before four positional paths; no cancel/fault/env switch. Success exit 0, semantic/coverage/review/unprocessed result exit 3 with dual report, preflight strict gold/review/decision/path errors exit 2 or existing protected-source code. No config/budget/network interface.

Exactly four selected held input files: `review.json` and its distinct single-level candidate/gold/decision names. Review Schema1 fields match bound contract exactly; strict saved API wrapper fields are `schema_version, producer, transport_mode, request_id, binding, payload`, with unchanged strict `ApiCandidates`. Gold1 and Gold2 are separate strict DTOs with required nullable fields for Gold2 value/unit; Schema1 cannot accept exceptions/value_status. Decision includes required nullable `exclusion_reason`. Unknown/duplicate/extra/missing/trailing fields reject. Gold source validity is preflight, not a repaired/graded candidate.

`verification.json` is a strict nested DTO with these exact fields:

```text
schema_version = 1
report_kind = "source_knowledge_verification"
producer = "source_review"
request_id = original saved request ID (null if strict candidate decode failed)
status = passed | failed | needs_review | unprocessed
knowledge_status = "pending"
published = false
scientific_quality = "not_run"
research_status = "not_requested"
network_calls = 0
paid_calls = 0
scope = {purpose, selection, binding, units, expected_atoms:1}
binding = actual SourceSession binding
origin = {producer, transport_mode, reviewer, interpretation_timing,
          input_fingerprints:{selected_filename:{sha256,byte_length}}, gold_sha256}
rules = {grammar:"controlled_measurement_zh_v1", subject, property,
         allowed_contexts, reviewer, interpretation_timing}
expected_results = [{object_id:"expected-0", unit_id, start_byte, end_byte,
                     status, issue_codes:[string]}]
candidate_results = [{object_id:actual_atom_id, unit_id, start_byte, end_byte,
                      status, issue_codes:[string]}]
checks = [{category, object_id, unit_id, start_byte, end_byte,
           status:"passed"|"failed", code:string|null, expected:JSON, actual:JSON}]
totals = {expected_atoms:1, passed, failed, needs_review, unprocessed,
          candidate_atoms, unexpected_atoms, duplicate_atoms,
          required_concept_groups:1, additional_source_concepts}
quality = {category:{expected, passed, failed, needs_review, unprocessed, status}}
artifacts = {filename:{sha256,byte_length}}
```

`origin.producer`/`transport_mode` are null when strict candidate decode fails. `totals.candidate_atoms`, `unexpected_atoms`, `duplicate_atoms` are nullable when syntax makes actual counts unknowable. Syntactically readable malformed candidate arrays retain real count and IDs in unprocessed candidate_results; semantic duplicate count remains null on failed strict decoding. Normal candidate counts are actual lengths; unexpected=count-1 (floor0), semantic duplicates are independently counted across all actual atom IDs. Expected results always use one author denominator; passed+failed+needs_review+unprocessed=1. Quality denominators come from author items and only the first actual candidate's checks; extras cannot borrow primary quality passes. Categories: source/schema/binding/concept/fact/content/evidence/condition/unit/exception/coverage/exclusion. Empty exception/unit denominator0 -> not_applicable. Additional diagnostic categories: strict_candidate, duplicate, evidence_identity, condition_set, exception_set, qualifiers. Evidence_identity localizes actual evidence ID/range; per atom checks also carry actual atom ID and selected unit/range with raw actual evidence/measurement/qualifiers.

Copies: `.owner.json` producer source_review/purpose unpublished_source_review; `review-original.json`, `candidates-original.json`, `gold-original.json`, `decision-original.json` are byte-for-byte original files. `source-snapshot.json = {schema_version:1,binding,units}` with full actual held units. Only complete success writes `knowledge-ir.json = {schema_version:1,producer:"source_review",revision:1,request_id,binding,source_match:"passed",concepts,atoms,evidence,checks}`. Every concept/atom/evidence remains original; atom verification unverified and required false. IR and report use strict DTO rereads; Markdown has one ` ```json\n ... \n``` ` fenced full fact snapshot exactly equivalent to verification.json. Artifact fingerprints describe all files captured before report writing (including IR if present); reports cannot embed their own cryptographic fingerprints without self-reference, so independent audit must hash JSON/Markdown externally.

## Source fact interpretation for independent Task 2

Whitespace normalization only; candidate known predicate is `subject的property为valueunit`, or an explicitly allowed exact `在本夹具的subject中，property为valueunit` context root. Candidate unknown predicate is `subject的property未知` only. The remainder is absent for zero conditions or a single separator from comma/Chinese comma/semicolon/Chinese semicolon/Chinese full stop followed by `条件为` or `条件是`; condition connectors only `且`, `、`, `，`, `，且`. Optional final Chinese full stop. Nonempty exceptions require exactly the `；例外为` segment with the same connectors and exact set. No duplicate/missing/extra assertion, negation or wrong value passes. `content` check requires complete author condition/exception membership; unit/value/qualifiers independently checked. No candidate raw text is changed.

Gold's contiguous **actual source quote is also parsed using the same controlled predicate**, preventing substring support such as gold value10 against source value110. Source quote accepts the source `。条件是` pattern as well as `，条件为`. Its condition and exception sets must exactly equal author sets. For unknown author fixtures only, actual source quote may use declared source-context root `在本夹具的subject中，property未知`; candidate unknown body still requires `subject的property未知` and stays needs_review. PDF fixtures must therefore register controlled Chinese source text matching these roots/segments, not merely a page with tokens scattered in unrelated sentences. Raw quote exact source slice/UTF8/SHA/location are still checked independently. This restriction is a bounded deterministic interpretation, not general scientific/NLP inference.

Linked gold compound concept or exact linked subject+property components accepted; all extra concept names/aliases must be supported by actual supports evidence. Context declaration normalized must exactly equal 在本夹具的{subject}中 and occur literally in original gold quote. Measurement conditions, atom qualifiers and body sets checked separately; removing both measurement/qualifier fields still fails. Missing candidate -> unprocessed, no auto-adoption. Excluded never passes this expected fact; pending/unresolved decisions/unknown/source needs_review never produce IR.

## Fresh actual verification and TDD evidence

Runtime: rustc1.98.1 (`48a229cea 2026-09-01`), cargo1.98.1 (`797e8a9bc 2026-08-05`). All cargo commands through project `./scripts/cargo_local.ps1`, locked/offline. Python only used absolute selected runtime from brief for fixture/evidence authoring; no runtime path added to product/scripts.

| Evidence file in current SDD workspace | Actual command/result |
| --- | --- |
| task-1-red.log | `test -p noetome-cli --locked --offline --test extraction_verify '--' --test-threads=4`: exit101, 1 passed/10 failed; genuine old ordinary CLI missing verify entry, not compile/setup error. Gold2 saved before synthetic fixture and production wiring. |
| task-1-red-sha.txt | Actual old binary/test/gold/lib SHA256 and parent HEAD receipt. |
| task-1-green-1.log | Initial production GREEN exit0, 11/11 integration tests. |
| task-1-quality-red.log | 13/17 passed, exit101. Exposed primary borrowing extra quality, actualsource binding quality; extra-body category/Span setup also failed. All preserved, subsequently fixed. |
| task-1-green-2.log | 16/17 passed, remaining nonzeroSpan test accidentally selected formatting ordinal1; fixed test only to actual text ordinal2. Not a source product RED. |
| task-1-green-3.log | 17/17 integration GREEN exit0. |
| task-1-guard-green-1.log | Guard setup failure: uncanonicalized `..` fixture path rejected; fixed test path only. Not product RED. |
| task-1-guard-green-2.log | 4/5 actual guard checks pass; file-symlink creation privilege1314. Replaced with actual Windows junction /J; no file-symlink validation claim. |
| task-1-guard-green-3.log | 5/5 guards pass, actual hardlink+junction paths and concurrent write/delete/output/source additions. |
| task-1-evidence-red.log | Focused real RED evidence SHA quality falsely passed despite overall rejected; exit101, 0/1 passed; added actual evidence identity/range/SHA category and source-slice check. |
| task-1-sourcefact-red.log | Focused actual gold10/source110 falsely passed, exit101; fixed actual source quote predicate binding. |
| task-1-final-integration.log | 18/19 pass; unknown source-context parser newly rejected, fixed quote-only unknown source root. Failure preserved. |
| task-1-hardlink-guard-red.log | Actual concurrent hardlink creation to saved IR succeeded on Windows; guard failed but residual knowledge-ir.json exposed cleanup bug, exit101, 0/1 passed. |
| task-1-final-integration-2.log | 20/20 ordinary CLI integration GREEN exit0. |
| task-1-final-guards.log | `test -p noetome-cli --locked --offline --lib extraction_verify::guard_tests '--' --test-threads=4`: exit0, 6/6 pass, 380 filtered. Includes hardlink IR failure cleanup with new human alias preserved. |
| task-1-final-fmt.log | `fmt --all --check`: exit0. |
| task-1-final-check.log | `check -p noetome-cli --locked --offline`: exit0, no warnings. |
| task-1-final-integration-3.log | Final fresh ordinary CLI `test -p noetome-cli --locked --offline --test extraction_verify '--' --test-threads=4`: exit0, 20/20 pass; actual stdout/stderr/exits persisted for report-producing test runs. |

`git diff --check` and explicit staged `git diff --cached --check` both exited0. No whole-workspace/Python full regression repeated: controller runs it on final Task2 integration. Final tested actual binary SHA256 `6fdcbca416252b3e8d0cdcd36bb5208c77ff0c3f97dd21bedad32ddf8c266a7d`; exact file SHA+sizes in `task-1-final-sha.json`.

`task-1-cli-evidence.zip` archives 26 actual report-producing CLI runs' raw stdout/stderr/exit, selected input bytes, saved reports/snapshot/IR when present, plus final tested owned source/fixture bytes. `task-1-cli-evidence-manifest.json` records exact real temporary roots, exit and per-file fingerprints and archive SHA. Strict preflight loops have whole test harness result but not individual saved raw stderr in this archive; Task2 independent matrix is responsible for all row raw invocation audits. RED command/log source/test fingerprints and all intervening failures remain in SDD. SDK report/zip/receipts intentionally untracked for controller evidence promotion.

## Guard behavior and remaining limits

SourceSession held actual binding/selected units before scoring and through final saved-report recheck. Four bounded GuardedFile inputs hold same handles and parent/directory identities; explicit modified-time receipts cover timestamp changes too. Private sibling content in review-case is not scanned (invalid UTF8/private folder test passes). Final actual source/output additions produce rejection and only captured IR/JSON/Markdown removal, retain original input copies/snapshot and unknown human files. Windows actual concurrent selected source/SQL/input/result writes/deletes denied. Actual result hardlink creation invalidates run; review-only cleanup permits releasing that captured original pathname after proving original bytes/mtime/identity even when link count changed, retaining unknown alias. Ordinary fake/API cleanup behavior preserved. Hooks exist exclusively under cfg(test), never ordinary CLI/env/config.

Not claimed here: independent Python matrix, original sixth live-root run, actual PDF meaning/quality cases, scientific correctness, broad platform compatibility/performance, full workspace/Python/devlog acceptance or remote push. These remain explicit controller Task2/full B04-T03 closure. No unresolved known component test failure; exact committed diff and source evidence are ready for fresh component review.


## Fix 1 — confirmed review findings R1/R2/R3

Fix scope status: DONE. Commit `3abed632ff7e8533c90ec5cd92f462149da872de`, parent `1d4793c4b1b6015c30735b65d8a1fcae5130ea09`. Only `extraction_review.rs`, `extraction_verify.rs`, `tests/extraction_verify.rs` changed. Original review/probes/logs/archives retained. Controller spec/plan edits and user deletions remain unstaged. No API DTO/source/output-guard/library/model/network/private/original sixth/PDF work or delegation.

Fresh findings were checked against exact source and `task-1-review.md` plus parsed `task-1-review-evidence.json`, including reviewer retained setup correction. Applied `superpowers:receiving-code-review` and test-first repair. Four new ordinary CLI regression tests failed against unchanged production before fixes: workflow known fact failure erased; known source-review failure erased; same-ID primary condition borrowing; readable duplicate outer payload recovered zero. RED had no setup/compile failure. Two duplicate-key fixtures at both outer payload and inner atoms paths, both different and identical repeated values, are prechecked as readable JSON and now remain unknown counts.

R1: `quality` partitions are now reduced from actual shared checks and primary occurrence check statuses, independently of overall workflow. Observed failure stays failed; observed passing source/schema/evidence stays passed. Missing checks fill unprocessed obligations. Genuine source-review/unknown/pending obligations are explicitly represented in the corresponding check status. Overall pending/review still returns unprocessed/needs_review and no IR. Candidate results with actual failure stay failed when overall workflow is pending/review; an otherwise passing candidate inherits workflow state. Correct unknown author fact is a needs_review check (`VERIFY_UNKNOWN`) and cannot erase its independently failed conditions. This corrects the earlier report's statement that all category buckets followed whole status.

R2: Primary aggregation uses the captured exact end of the first candidate's check segment, together with preceding shared checks, rather than any supplied ID. Candidate_results diagnostics use each occurrence's own check slice; same raw IDs/order are retained without borrowing first or later issues. A first atom with zero measurement conditions/qualifiers followed by an intact same-ID atom has quality.condition expected2/failed2/passed0. Second candidate still reports the genuine shared strict duplicate-ID error and semantic duplicate, without inheriting first candidate's condition failure. Expected denominator remains1, actual count2, duplicates1, no IR.

R3: Raw count recovery first visits all JSON objects recursively to reject duplicate keys, then permits Value decoding. It never uses a duplicate-key last-wins array as source of counts. Syntax-ambiguous inputs preserve candidate_atoms/unexpected_atoms/duplicate_atoms null, candidate_results [], VERIFY_SCHEMA and raw copied candidate fingerprints/source snapshot. The existing private wire visitor cannot be called within authorized file scope, so a small local nonallocating-value syntax visitor is used. It retains normal recovery of unambiguous schema-invalid arrays/IDs. Saved wrapper/ApiCandidates schemas are unchanged.

External JSON **field structure/names are unchanged**. Corrected behavior: `checks.status` may now be passed/failed/needs_review/unprocessed for observed results versus unresolved obligations, `VERIFY_UNKNOWN` identifies genuine unknown fact review; quality can contain observed pass/failure even when report.status is needs_review/unprocessed; actual candidate failure status is not overwritten by workflow. Same-ID candidate result order and raw IDs remain unchanged. Quality denominators/count fields and null-count JSON types remain the established contract. Markdown continues embedding the same complete facts.

Actual commands, using `./scripts/cargo_local.ps1` with Rust1.98.1:

| New evidence (current SDD workspace) | Command and actual result |
| --- | --- |
| task-1-fix-1-red.log | `test -p noetome-cli --locked --offline --test extraction_verify fix1_ '--' --test-threads=4`: exit101, 0/4 passed, 20 filtered; genuine behavior failures. |
| task-1-fix-1-red-sha.txt | Pre-fix binary/review/verify/test SHA receipts, binary equal to reviewed Task1 binary. |
| task-1-fix-1-green.log | Same focused command: exit0, 4/4 passed, 20 filtered. Four tests execute11 focused real CLI calls after all loops complete. |
| task-1-fix-1-integration.log | `test -p noetome-cli --locked --offline --test extraction_verify '--' --test-threads=4`: exit0, 24/24 passed; existing valid pending/review/unknown, originalgold1/context/Span and all negative integration rows preserved. |
| task-1-fix-1-fmt.log | `fmt --all --check`: exit0. |
| task-1-fix-1-check.log | `check -p noetome-cli --locked --offline`: exit0, no warnings. |

`git diff --check` and staged `git diff --cached --check` exit0. Existing guard production code was untouched; no redundant guard/full-workspace/Python repetition. Controller integrated full checks remain required. Fresh final actual binary SHA256 `5246e09d18fbff0b1404e3ba13a9fc52f9c61b02bc7fa7b6ad8066f83ec94326`; `task-1-fix-1-final-sha.json` includes owned source/test SHA/size.

`task-1-fix-1-red-calls.zip` plus `task-1-fix-1-red-calls-manifest.json` preserves4 actual RED CLI stdout/stderr/exits, four raw inputs and all saved output facts/IR absence. `task-1-fix-1-green-calls.zip` plus manifest preserves48 actual report-producing calls from focused GREEN and full current integration, exact command vectors, stdout/stderr/exits and per-file SHA/size. Green rows include11 focused calls plus37 report-producing integration calls. The harness independently checks original selected input byte preservation and original SQLite bytes; raw duplicate-key input copies are explicitly equal after processing. Every negative new row directly checks no knowledge-ir.json. All these evidence files are new; no original reviewer/implementer evidence was overwritten.

Await fresh exact `1d4793c..3abed63` rereview before component gate/Task2. No unresolved known focused test failure; no task ledger completion or push claim.
