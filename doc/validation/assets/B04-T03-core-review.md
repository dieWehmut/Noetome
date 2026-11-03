# Task 1 fresh spec and quality review

**SPEC COMPLIANCE: ❌ Issues found. CODE QUALITY: Needs fixes.**

Exact review: base `f11100df14318c470d84eae9dc728f6ceb4ff7d7`, head `1d4793c4b1b6015c30735b65d8a1fcae5130ea09`. Bound scope: Task 1 and Global Constraints in `doc/execution/B04-T03_REVIEW_PLAN.md`, plus `doc/technical/EXTRACTION_REVIEW_CONTRACT.md`. The original Task 1 brief and official task-reviewer prompt were read before review. This is a component gate, not final B04-T03 acceptance.

The implementation supplies all 14 listed changed files, the ordinary offline CLI, strict selected-input DTOs, bounded source/quote/fact checks, independent unpublished IR, retained original bytes, and matched JSON/Markdown reports. The three issues below affect the required accuracy of quality and candidate-count reports. None of the focused reproductions emitted a successful IR; diagnostics were retained. No Critical issue was found.

## Strengths and verified requirements

- **Actual source and fact are both checked.** `extraction_review.rs:45-173` parses the explicitly bounded Chinese grammar for the candidate and the original gold quote, requires exact normalized value/unit binding and full condition/exception sets, and checks the literal source slice. The retained `task-1-sourcefact-red.log` and final integration result cover gold10/source110 rejection; `tests/extraction_verify.rs:228` tests correct quote with wrong value/negation. This addresses the controller's source-fact risk without claiming general scientific reasoning.
- **Evidence and linked concepts cannot rescue a wrong fact.** `extraction_review.rs:338-436` validates source unit, UTF8 slice, SHA and support of the gold range, and resolves linked compound or subject/property concepts. `extraction_review.rs:458-477` separately checks measurement value and parsed content. Final archive rows preserve `VERIFY_FACT`, `VERIFY_CONTENT`, `VERIFY_EVIDENCE` and the actual atom/evidence IDs. The exact-binary probes below additionally confirm that value11 against gold10 remains a failed fact diagnostic.
- **Input scope is narrow and guarded.** `extraction_verify.rs:42-133,167-202` opens exactly review/candidate/gold/decision, checks distinct ordinary single filenames, holds file and directory capabilities, limits bytes and rechecks mtime. The named external check of `sidecar_workspace.rs:19-61,92-183` confirms directory verification inspects identity, not sibling contents, and GuardedFile checks held identity/bytes/link count. The final guard log includes the invalid-UTF8 private-sibling fixture; no private sibling contents were read during this review.
- **Strict schema and output gate remain intact.** `extraction_review_model.rs:7-137` uses required nullable deserializers, version/fixed-value checks, separate strict gold versions and `deny_unknown_fields`; `extraction_verify.rs:248-272` invokes the unchanged shared candidate validator. The one named external check of `extraction.rs:197-363` confirms basic references, UTF8 evidence, known/unknown measurement, qualifiers and unverified/required=false rules. `extraction_verify.rs:353-435` writes IR only for a passed assessment, retains raw payload objects, rereads IR/reports and performs final guards.
- **Late hardlink cleanup is narrowly scoped.** `extraction_output.rs:78-135,244-287` relaxes only link-count rejection during review-result cleanup, still checks captured path, file identity, bytes, SHA and mtime, and removes only knowledge-ir/verification JSON/Markdown. Existing fake/API cleanup still calls the nonrelaxed branch at `extraction_output.rs:244-256`. The final six-test guard log contains the hardlink invalidation/own-result removal test; the test also checks preservation of the new alias and input copies. This review did not rerun concurrency or claim broader OS race guarantees.
- **Archived output evidence is internally consistent.** The independent archive audit executed 1,206 recorded assertions over 26 report-producing saved calls, including every manifest-listed archived file fingerprint, original-byte copies, stdout versus saved JSON, embedded Markdown JSON, expected/candidate counts for these readable inputs, quality partitions, claimed artifact fingerprints, no-IR on failed exits, and successful IR payload preservation. All 1,206 passed. This verifies the archived facts; it does not make the implementer's semantic classifications an oracle.

## Important issues (P2; fix before accepting Task 1)

### R1 — Workflow review/pending overrides erase confirmed failed quality checks

**Location:** `crates/noetome-cli/src/extraction_review.rs:629-638`, related status rewrite `:583-591`.

The category reducer derives every category's failed/passed/review/unprocessed counts from the overall status. If a decision is pending or needs_review, it sets all passed/failed counts to zero regardless of independently failed checks. This contradicts Task 1's requirement that `quality` be genuinely classified from checks. The bounded spec permits overall pending/needs_review, so that overall state alone is **not** a finding; the lost category failure is.

**Actual reproduction:** On the archived valid synthetic source, change only measurement.value from `10` to `11` and disposition to pending, then to needs_review. Both ordinary CLI calls exit3, retain candidate `VERIFY_FACT` and a failed fact check, and write no IR. Nevertheless `quality.fact` becomes `{expected:1,passed:0,failed:0,unprocessed:1}` or `{expected:1,passed:0,failed:0,needs_review:1}`. The fact has actually been processed and determined wrong. Already-passed source/schema/evidence checks are also all relabeled as unresolved. Raw diagnostics are not deleted, but consumers of the promised quality summary receive a contradictory result.

**Fix:** Keep workflow disposition separate from per-category outcomes. Derive each category from its actual check results; only genuinely unknown/unprocessed checks should occupy those buckets. Preserve definite failures independently of whether the overall object must remain pending/needs_review. Add combination cases for wrong fact or invalid binding with pending/review/unknown/source-review, without changing the required no-IR behavior.

**Evidence:** `focused_probe.calls` entries `pending_wrong_fact` and `needs_review_wrong_fact` in `task-1-review-evidence.json`; raw stdout, stderr, exit, four inputs and full outputs are retained under `task-1-review-probes-faa52020-e1e2-42cd-ba2d-607c642a5c3f/` in this SDK.

### R2 — Duplicate IDs can make an invalid primary borrow another candidate's quality passes

**Location:** `crates/noetome-cli/src/extraction_review.rs:618-636`.

The reducer selects primary checks by `check.object_id == atoms.first().atom.atom_id`. It is deliberately still run when strict candidate validation fails, including duplicate IDs. Consequently both occurrences of the same ID enter the primary bucket, and counting passed checks with `min(n)` can overwrite the primary's failed condition items.

**Actual reproduction:** Remove both measurement conditions and their qualifiers from the first atom; append an intact copy with the **same** atom_id. The first two condition checks fail and the next two pass. Shared strict validation correctly reports `EXTRACT_KNOWLEDGE`, both candidate_results remain failed, duplicate_atoms=1 and no IR is produced. However quality.condition reports `{expected:2,passed:2,failed:0,status:"passed"}`. Existing `failed_primary_cannot_borrow_duplicate_quality_passes` only uses a new ID and misses this case.

**Fix:** Associate checks with the specific candidate occurrence or retain the primary's exact check span before aggregating. Do not use a provider-supplied ID whose uniqueness is already known invalid as the sole ownership key. Retain raw IDs for diagnostics and separately distinguish occurrences. Extend the focused test to duplicate IDs as well as new-ID semantic duplicates.

**Evidence:** `focused_probe.calls` entry `duplicate_id_borrows_conditions`; its saved report contains the two failed primary checks and misleading 2/2 condition summary.

### R3 — Duplicate JSON keys can recover a fictitious zero candidate count

**Location:** `crates/noetome-cli/src/extraction_verify.rs:288-302`.

After strict deserialization rejects duplicate keys, the fallback reparses into `serde_json::Value`, which discards earlier keys. A saved wrapper with two `payload.atoms` keys, the first holding one real atom and the second `[]`, is reported as candidate_atoms=0, unexpected_atoms=0 and candidate_results=[] even though the candidate count is ambiguous and an earlier actual object was present. Keeping duplicate_atoms=null does not fix the lost candidate count. The contract specifically requires unsupported/unknown candidate counts to remain unknown, not zero.

**Actual reproduction:** A syntactically readable JSON wrapper containing two atoms keys returns exit3 with `VERIFY_SCHEMA`, no IR, and the incorrect zero counts. The reviewer explicitly validated readable JSON and the two-key fixture before the corrected call.

**Fix:** Recover counts only through a parser that retains/rejects duplicate keys on the wrapper/payload/atoms path. If the candidate array cannot be unambiguously identified, preserve null counts and record the schema diagnostic. Test duplicate payload and atoms keys separately from duplicate schema_version fields.

**Evidence:** `focused_probe.corrected_duplicate_key` and the raw `duplicate_json_atoms_count_corrected` case/output. The first reviewer attempt accidentally produced malformed JSON, for which null counts were correct. Its failed zero-count expectation is retained as a reviewer setup failure; only the corrected readable duplicate-key input supports R3.

## Evidence, scope and limitations

Retained final logs were read, not rerun: `task-1-final-integration-3.log` shows 20 passed/0 failed; `task-1-final-guards.log` shows 6 passed/0 failed; final fmt log is empty as expected; final check log shows completion and no warnings. Original RED, source-fact RED, evidence-SHA RED and hardlink RED files were fingerprinted and retained. These are component results, not a full-suite claim.

Executed during this review: independent Python archive audit, four focused ordinary-CLI probes plus one corrected probe. Five CLI invocations total, all exit3, all within fresh SDK case/output directories, using one explicitly selected archived synthetic source. No original live root, PDF, model request, credential, configuration, budget, network, build, Cargo, Git, full suite or subagent was used. The source project's file listing/bytes/mtime and the executable are checked before/after the probes. Repository source/fixtures are compared with the retained tested manifest. All assertion results, commands, source receipts, file hashes and limitations are saved in `task-1-review-evidence.json`; its totals distinguish the retained reviewer setup failure from successful verification checks.

The exact diff was reviewed in ordered chunks. One large tool response was truncated, so only missing initial diff chunks were retrieved again. Changed files were not independently reread, except `Artifact::read_with_links`/`remove_with_links` and their capture state at `extraction_output.rs:23-135`, because the diff cuts these functions before their identity/mtime/removal checks. Outside the diff there were only two concrete named code checks: GuardedFile/Directory privacy/identity behavior, and unchanged `validate_candidates` semantics. The archive audit also reconstructs changed-file line numbers from diff hunks for verified finding locations.

**⚠️ Bound Task 2/controller work remains:** independent Python semantic matrix, original sixth live-root candidate/gold verification, actual PDF scope tests, original six responses/source/model-budget preservation, complete Rust/Python/devlog/diff checks, whole-branch fresh review, task completion and remote push. These are not missing Task 1 deliverables and were not claimed as performed here.

**Assessment:** Task 1 is structurally in scope and guards successful IR correctly, but its report contract cannot be accepted until R1–R3 are fixed. Maintain the existing evidence and run focused behavioral regression for the corrected accounting before the controller's final acceptance pass.
