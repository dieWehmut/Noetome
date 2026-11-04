# B05-T03 fresh implementation review

Reviewer: fixture/test author, reviewing Rust production, independent Python implementation and adopted contract after implementation. This is fresh relative to production implementation; it is not independent of fixture authorship.

## Scope and evidence

Reviewed new Schema3 model and coordinator, ordinary CLI version routing/save path, shared context extraction, independent v3 source/author/report reconstruction, coherent forgery matrix, v1 source-bundle admission, new contract, task acceptance and frozen examples. Focused ordinary Rust execution passed **15/15**, 0 ignored, over 25 retained new roots; each input/source/gold/sentinel protection inventory matched. Evidence is `rust-focused-green.txt`, `rust-focused-green-receipt.json`, and `rust-focused-green-roots.tar.gz` in this directory. Full workspace/Python regression is a separate parent-owned gate.

## Important finding I1 — ancestor link admission mismatch

`scripts/verify_knowledge_conflicts.py::_verify` begins with `inventory(project/case/output)` and then calls `resolve()`. The shared `inventory` in `scripts/verify_knowledge_coordination.py` checks root and descendants, but does not inspect ancestors of the given root. An ordinary directory reached through a parent symlink is accepted, and the source helper's `resolve()` hides that parent link.

Actual reproduction uses an unchanged valid saved conflict root via `/tmp/noetome-b05-t03-fresh-review/valid-aliased-root`. Python `check` returned **0** and `independently_verified=true`; ordinary Rust `knowledge coordinate` with the same aliased project/case returned **2**, `COORD_INPUT_PATH/PROJECT_UNSAFE_PATH`, identifying the parent link. All original bytes matched before/after; no new output was written. The concern is strict independent admission consistency, not an observed incorrect source value or destructive write.

Raw commands, stdout/stderr, protection hashes and limitations are saved in `fresh-symlink-ancestor.json` and `fresh-symlink-ancestor-{0,1}.{stdout,stderr}.txt`. One earlier diagnostic chose a root without a coordination output and failed file-type admission; it was not evidence for this finding. Resolve by checking each original path component before canonicalization and add a targeted real-parent-link regression. Review remains open until this behavior is rejected independently and ordinary direct paths still pass.

## Correctness observations

- Same object/property/full applicability/unit/equal known time uses exact finite decimal comparison. Unequal values retain both original claims with `true_conflict`; numeric equality preserves lexical groups. Large decimals and signed zero passed ordinary CLI checks without float rounding.
- The independent verifier reconstructs actual SQL/resource and original source-review inputs before comparing complete expected groups, occurrences, pair directions, processing records, counts, Markdown and successful IR. It does not accept report statuses/counts as evidence.
- `refutes` and `qualifies` remain in original atom links; unique evidence excerpts remain in original link order. `evidence_links` counts links, `counter_evidence_links` counts refutes, and context support is assessed separately. Nonzero UTF8 positions and shared evidence IDs with distinct relations passed.
- Unknown conditions independently affect the original occurrence. Unrelated complement remains passed; majority support does not remove opposition. Unresolved results have canonical_facts=null, exit3 and no IR. Missing/broken support/references fail input admission before a legitimate-viewpoint report.
- Shared ordinary CLI holds input/source guards through save/recheck and removes only the captured result on failure. Existing output/case overlap and human-file collision tests passed; v1/v2 classification semantics were not changed by the context-builder extraction.

## Optional minor caveat

The engineered `counter_support` quote disputes value12 while its `refutes` link attaches to atom10. It proves retention of an explicitly supplied source relation and location; it does not prove scientific correctness of that relation. Keep this caveat explicit rather than editing frozen materials after execution. All examples already identify fake data and science not_run.

## Disposition

No Critical findings. One Important admission mismatch is reproducible and requires closure. No other Important production correctness findings identified within this bounded review. This review does not claim arbitrary language understanding, source authority, scientific accuracy, full PDF/multi-textbook coordination, Windows/platform guard coverage, production Vault publication, or completion of unexecuted regression gates.

## Counter fixture evidence refinement

The parent requested a separate meaningful counter-to-claim sample rather than altering the earlier frozen material. The new twentieth `counter_to_claim` case explicitly quotes rejection of value10 and attaches `refutes` to the original value10 atom. All earlier nineteen author definitions and source bytes remain unchanged; the prior authors/freeze/README are archived in `frozen-authors-19-before-counter-case/`. The new freeze manifest was saved before its source/candidate calls.

The added sixteenth ordinary Rust test ran separately and passed **1/1**, 0 ignored, after 2.06s. Its actual source preparation, conflict exit3, literal counter quote/link preservation, science not_run, knowledge pending and before/after input/source/author/sentinel protection are preserved in `rust-counter-to-claim-green.txt`, `rust-counter-to-claim-green-receipt.json` and `rust-counter-to-claim-green-root.tar.gz`. This extends the earlier 15-test run; it is not a claim that one 16-test run was already executed. I1 remains open pending the path-admission fix and fresh recheck.
