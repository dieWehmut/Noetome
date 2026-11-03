# B04-T02 gateway continuation — final whole-branch review

Reviewed immutable range: `8004df0f70dddc38a3f37be192d6928f3970c10e` → `bee1603bde81d555d11c7b9646c1ff5cb4cd5378`, using the supplied `review-8004df0..bee1603.diff`. This review covers the v5 qualifiers and v6 positive revisions continuation plus its captured validation and actual-call evidence. The already accepted toolbar and the user's unstaged legacy PRD deletions are outside this range.

## Strengths

- **The request correction matches the existing contract.** `crates/noetome-cli/src/extraction_api_schema.rs:33` confines the two `minimum=1` additions to selected Chat user Schema. Line 39 preserves the v4 nesting/provenance and v5 qualifiers instructions, appending the generic positive-revision rule. `doc/technical/DATA_CONTRACTS.md:13` already requires positive revisions. The diff does not change the shared candidate Schema, DTO/domain validation, endpoint selection, HTTP transport, or old Responses behavior.
- **The regressions retain the original failure evidence.** `crates/noetome-cli/src/extraction_api_wire/chat.rs:112` verifies the exact fifth response and its valid 3479 usage while retaining four `KNOWLEDGE_REVISION` failures. Line 169 preserves the fourth response's missing unit. The third flattened response remains strictly rejected. `scripts/validate_extraction_chat.py:195` adds ordinary local-HTTP missing-unit and separate zero-concept/zero-atom cases; all three recorded exit 3 rather than accepting repaired candidates.
- **Current validation is bound to current sources and executable.** `doc/validation/assets/B04-T02-revision-final-workspace-summary.json:4` lists five source hashes, independently matching this working tree; its executable hash also matches the current binary. Captured Rust output independently totals 502 passed, 0 failed and 0 ignored; recorded Python output reports 76 passed. API 84/90 CLI/54 local POST, fake 54/60 CLI, and Chat 37/39 CLI/31 local POST records agree with their final binary and successful check counts. Historical v4/v5 records are distinguished from final v6 evidence.
- **Actual response and spending remain auditable.** `doc/validation/assets/B04-T02-live-sixth-validation.json:1` and its Markdown snapshot agree with the original response payload and saved candidates. The response has one stopped assistant result, one atom and one evidence record, positive revisions, exact UTF8 quote/SHA and unit/condition qualifiers. Captured reservation/settlement records preserve the first five entries and reconcile 1792 + 4512 = 6304 with cumulative 30097, remaining 9969903 and pending 0. The saved command records one successful invocation; controller source uses the selected DPAPI helper with a reference, rather than a credential value in arguments.
- **The claim boundary is explicit.** `doc/validation/B04-T02.md:152` and `doc/validation/assets/B04-T02-live-sixth-verification.json:42` leave independent knowledge assessment to T03. The saved API report stays `knowledge_status=pending`, `scientific_quality=not_run`, `published=false`. The 16715-byte reasoning content, unknown billing and alias-only model identity are disclosed, and the literal gold diagnostic's zero matches are not presented as a semantic quality conclusion.

## Issues

### Critical (Must Fix)

None found in the reviewed scope.

### Important (Should Fix)

None found in the reviewed scope.

### Minor (Nice to Have)

1. **Current-next-step paragraph still describes the pre-v5-call state.** `doc/validation/B04-T02.md:121`, under “真实门槛和下一步”, says to finish v5 review, reuse balance 9979686 and execute another bounded request. The later v6 result is correct, but this unlabeled current instruction contradicts it and could confuse a later executor. Replace the paragraph with the final v6 result and remaining review/log/push steps, or label it explicitly as historical. Do not interpret it as a reason to perform another call. This is nonblocking documentation drift; the controller was notified to fold it into completion synchronization.

## Actual verification methods and evidence

I reviewed the production/test/script diff in focused passes, read the task acceptance and relevant plan/contracts/current-state documents, and read the captured actual-call controller and independent-verification source. I did not regenerate the Git diff or mutate Git state. Large historical JSON archives were evaluated through the relevant manifests and current final records rather than dumped or treated as current validation.

The local review audit is `gateway-final-review-audit.py`; its executed result is `gateway-final-review-evidence.json`: **469 assertions passed, 0 failed; 55 file fingerprints**. The assertions actually performed include:

- Five changed source hashes, final executable hash, and every size/hash in both current archive manifests.
- Captured test stdout/stderr hashes and actual result-row parsing; native RED 1 pass/3 failures and controller CLI RED 0 pass/2 failures. The zero-test mistaken target and initial formatting failure remain separately recorded and are not counted as passing validation.
- Counts, successful checks, SQL before/after equality in the final API/fake/Chat audits; zero model/paid calls for the local HTTP matrices.
- ZIP CRC and 728 unique files without extraction; SHA and JSON/Markdown equality for all 130 recorded report pairs; hashes for all 92 recorded request/body pairs.
- Actual sixth raw-response parsing with duplicate-key rejection, exact payload equality, request/report identities, schema delta relative to fifth request, unchanged system/task prefix/source binding/units/anchors, positive revisions, qualifiers, source quote and SHA, report parity, known usage and reservation arithmetic.
- Exact original first through fifth response fingerprints, captured sixth reservation/settlement continuity, recorded executable identity, and archived controller/verifier source-text hashes.
- Task ledger still has only B04-T02 in progress and B04-T03 todo.

No build, Rust/Python suite, HTTP/model request, external live usage-ledger read, private profile/DPAPI read, UI operation, installation, cleanup, Git/worktree mutation, commit or push was performed by this reviewer. Source-project SQL/mtime and external live-ledger equality were evaluated as captured controller evidence and source, not independently re-executed. The evidence file distinguishes that limit; checksum equality alone does not prove tests ran or scientific correctness.

## Plan/spec quality

The Task 2 v5/v6 amendments are narrow and justified by preserved actual failures. Their allowed change surface, strict original-response behavior, retained usage, old-strategy compatibility and fresh bounded request gate are specific enough to review. Task 3 cumulative budget and small actual API acceptance are supported by the captured call and accounting evidence. Reserving the external cap of 65536 and the Core's smaller body-derived reservation of 16721 are compatible bounds, not conflicting usage measurements.

The separate B04-T03 scientific/knowledge assessment remains necessary. Neither the plan nor this review treats successful formatting, correct citation, a single candidate or the literal-gold comparison as that assessment. No migration, SDK expansion, production Vault use or new phase is required for this continuation. The minor stale paragraph above is the only identified plan/execution-document inconsistency.

## Recommendations and final assessment

**Ready for B04-T02 completion and ordinary push: Yes.** No implementation or acceptance blocker was found. The two prompt/Schema corrections retain strict validation and are supported by current source-bound offline evidence plus one original, successful bounded live response and reconciled cumulative spending.

The controller should synchronize the minor paragraph and final current-state wording, archive this review/evidence, complete B04-T02 through `scripts/devlog.py` with the actual validation and non-ledger artifacts, run the required completion/document/log/diff checks, make the authorized final ordinary commit and push, and verify the remote ref. No repeated model request or identical Rust build/test rerun is justified solely by this review. Do not start B04-T03 before T02 completion is recorded. This is a readiness assessment for completion/push; no merge was requested or performed.
