# B04-T03 final whole-branch review

**SPEC: With fixes. CODE QUALITY: With fixes. Ready to merge: With fixes.**

Reviewed immutable range `bdae557a884bd784a7ba52252bb5828e6ffd5c17..cccd784f607d506d0b6c8b0a12aea7973fadeb57`. Findings: **0 Critical, 1 Important (P2), 1 Minor (P3)**. R1 is the sole merge blocker. R2 can remain deferred. This verdict does not invalidate the successful retained original-sixth result or native installation evidence.

## Strengths

- The Rust implementation separates guarded input/source ownership, strict saved candidate decoding, deterministic source/gold assessment and output serialization. It preserves the original candidates and gold, checks real unit/byte-range/SHA identity, rechecks source and input guards after saving, and only removes captured result files on a failed final guard. The new close UI is similarly isolated from existing desktop preference logic.
- Knowledge totals use one author obligation, independent candidate/unexpected/duplicate counts and per-category denominators. The earlier workflow-status and duplicate-ID issues have substantive fixes: observed failures survive pending/review, primary quality is occurrence-bound, and ambiguous duplicate-key JSON cannot establish a count. Failure does not leave a successful IR in the exercised guard cases.
- Retained evidence distinguishes initial RED, setup failures, product failures, fixes and successful reruns. The original sixth candidate and original gold are byte-identical across their saved copies and final call receipts. The final report preserves value `10`, unit `m/s`, both necessary conditions, denominator one, and exception denominator zero/not_applicable. It states automated post-response interpretation, pending knowledge, scientific not_run, research not_requested and unpublished status.
- The native correction removes the extra visible title row, retains the 36px menu with same-row controls and routes close through the existing CloseRequested/preference/tray code. The full visible-window image retains its top edge; native geometry, UI controls, installed executable fingerprint, real exit observation, persisted exit setting, restored hide preference and ordinary final launch corroborate the claimed scope. Physical tray restore and dragging are specifically user-confirmed, rather than inferred from CDP operations.

## Issues

### Critical

None found.

### Important — R1 / P2: Unknown raw candidate count is coerced to zero in the independent verifier

**File:** `scripts/verify_extraction_knowledge.py:510` (count calculation; assertion at line 513).

For syntactically valid JSON whose `payload.atoms` is missing or is not an array, `count = ... else (0 if syntax else None)` invents a count of zero. The Rust implementation correctly initializes `candidate_atoms`, `unexpected_atoms` and `duplicate_atoms` to null at `crates/noetome-cli/src/extraction_verify.rs:342` and only recovers the first two after finding an actual array. The source-review contract requires unknown quantities to remain unknown.

This causes both a false rejection and a false acceptance. A coherent negative report with candidate bytes `{}`, null candidate/unexpected counts and no recovered candidate results is rejected with `AssertionError: actual candidate count`. Changing only those two counts to zero, with the Markdown updated to match, makes `verify_report` accept the report and return `candidate_atoms: 0`. Thus the independent gate can approve fabricated known-zero counts and reject truthful Rust rejection reports for missing payload/atoms or a wrong array type. No false successful knowledge IR was observed; the affected gate is failure-accounting integrity, which is a stated acceptance requirement.

**Evidence/method:** One narrow in-memory artifact probe derived from the retained ordinary CLI `original06/out-candidate-extra` output in root evidence 01. The authentic baseline passed the current verifier before modification. Candidate bytes, copied candidate, both input/artifact fingerprints, count fields, recovered result list and Markdown were updated coherently; the virtual path adapter prevented filesystem writes. The truthful null-count variant failed, and the fabricated zero-count variant passed. This was not a new Rust execution. Exact candidate bytes, complete null-count file bytes, the zero-count report bytes, fingerprints and returned outcomes are embedded under `summaries.missing_array_probe` in `final-review-evidence.json`; the reproducible helper is `final-review-audit.py`.

**Fix:** Recover `len(raw_atoms)` only when `raw_atoms` is an actual list; otherwise use `None` regardless of whether JSON syntax parsed. Keep the genuine empty-array case as zero. Add focused coherent-artifact checks for missing payload, missing/non-array atoms, and real empty arrays so this distinction cannot regress. The fix should not change Rust behavior, source/gold/model inputs or the already-passed native implementation.

### Minor — R2 / P3: Successful state recovery leaves a stale state-read error

**File:** `apps/desktop/src/WindowControls.tsx:27`.

If a resize-triggered `isMaximized()` read first fails and a later resize read succeeds, the success branch updates `maximized` but leaves the previous “读取窗口状态失败” alert visible. Controls recover correctly, and the next explicit window operation or drag clears the alert, so this is non-blocking. The documented task-review deferral is reasonable.

If addressed, track the error category and clear only a prior state-read error on successful state recovery. Do not clear an unrelated close/minimize/drag failure whenever a resize happens. This finding is from static control-flow review; no new UI operation was run.

## SPEC and architecture assessment

Task 1’s four explicit selected inputs, strict versioned DTOs, source binding, restricted single-measurement interpretation, failure reporting and guarded new-output flow match the reviewed contract. It does not alter the shared API candidate wire or promote atoms to scientifically verified/published state. The special handling of original source context and linked subject/property concepts is narrow and declared after model response; it preserves the independent original gold instead of editing it to match the candidate.

Task 2 supplies independently computed source/SQL/byte/semantic accounting and corruption tests. It imports no Rust oracle and its recorded original-sixth and matrix results remain supported. R1 is a genuine gap in its strict-invalid-input count handling that should be fixed before accepting the whole gate. A separate static language difference exists for adding the redundant allowed-context phrase as a body condition: Rust permits it, while Python’s exact body condition set rejects it. The contract expressly permits this extra phrase in measurement/qualifiers and does not require it as an additional body condition, so I have not classified that difference as another confirmed blocking requirement violation. Future grammar changes should keep both implementations aligned.

User-added Task 3 is a justified scope correction within B04-T03, and its native verification is now present. Main-only permissions cover the specific window operations plus listener lifecycle. The new close control invokes `close()`, and outside-diff inspection of `crates/noetome-desktop/src/main.rs` show/close handlers confirms the existing reload, hide-to-tray and exit behavior remains the integration point. No new schema migration or durable knowledge-store change is needed by this branch.

## Evidence actually inspected

- Read the supplied 36-section source/doc/fixture/ledger diff in bounded chunks; fingerprinted both packages. Full package: 151 sections, SHA256 `2af652c0fd464eb90adf57c7e48a0b55dfd9e2cd5846d3240d06e7b8175ea429`. Source/doc package: SHA256 `0b3fdcc876dc98846d061489a88ad56ecd0f96896b57f8ec79865f2d6388e282`.
- Recomputed all five archive fingerprints and **7,562 archive member fingerprints**: core 49, Task 2 3,829, Task 2 fix 2,960, root 01 681, root 02 43. Verified **63 readable copies** against archive members. Historical bulk assets were parsed and hashed, not blindly treated as new passing behavior.
- Parsed root matrix audit: **53 scenarios, 61 CLI calls, 35 reports, 6 bounded knowledge passes**, no audit error. Reconciled actual root matrix report JSON/Markdown, partitions, artifact fingerprints and IR presence. The final one-call original-sixth result is bound to CLI SHA `7a1eb3cae1fef5e9485848304d59dfd5c7804299190465a0aa126acfd6b56241`; it is a repeat of that original sample, not an extra independent knowledge success.
- Parsed retained complete test logs: **Rust 532 passed / 0 failed / 0 ignored; Python 104 passed**. Relevant source hashes from command receipts still match this checkout. Retained browser **10/10** results are bound to the current five UI/config source fingerprints. Build/install command receipts both record exit 0.
- Compared whole-root before/after `facts` and the final-call before/after input facts: same source files, SQL dump, 54 immutable call-input records, three saved-service fingerprint receipts and budget values. Original source-file count is five; budget remains 30,097 used / 9,969,903 remaining / zero pending / six prior attempts. No actual encrypted credential or private folder was opened.
- Viewed `B04-T03-ui-native-full-window.png`, checked the crop top is zero and native title band is zero. Read native action and settings receipts: 11 successful first-stage checks plus 3 exit-stage and 2 restoration checks, with the failed maximize-as-show experiment retained. Checked user confirmation, bound process exit 0, persisted exit then restored hide-to-tray, and final normal PID 53980 with debug fields null.
- The final named audit ran **8,452 explicit assertions, 8,452 satisfied, zero unexpected audit failures**. These are byte/receipt/probe assertions, not 8,452 product tests; reproducing R1 is one of their expected outcomes. The first audit’s asset-relative path setup error and second pass’s overly broad CRLF/report selection are recorded as helper setup issues. No product suite was rerun to resolve them.

Actual source/evidence hashes, methods, raw probe bytes, counts and limitations are in `final-review-evidence.json`. Final audit-helper SHA256: `dfdfbe61a229d98a2e6fda078f34982e23545dc80e6364d286a4c591628f064f`.

## Independent rulings review

- **Existing checkout/task branch:** Accept. The reviewed state has only the two preserved user deletions outside the committed range; no extra worktree is necessary for this read-only review.
- **Context/compound-concept interpretation:** Accept for the declared controlled source grammar and current original-sixth evidence, with post-response labeling and scientific limitations retained. The source quote, numeric predicate and both gold conditions agree; this is not a fresh blinded benchmark.
- **No new model request:** Accept. Saved candidate/source/gold provide the material needed for this offline review; receipts preserve the original six attempts and cumulative budget.
- **Retain SDK/recovery archives:** Accept. Retention has no product correctness consequence and avoids deleting unreviewed or policy-rejected cleanup targets.
- **Preserve category failures under workflow review/pending:** Accept. Observed failures remain in actual checks and category totals while the overall workflow status remains honest.
- **Occurrence-bound duplicate-ID handling:** Accept. The explicit first-segment quality boundary and per-candidate slices prevent borrowing another occurrence’s successful check. Unambiguous JSON is necessary for count recovery; R1 concerns the independent Python implementation’s remaining missing-array case.
- **Exact source predicate rather than token substring:** Accept. Source 110 cannot justify gold 10 under the exact restricted predicate; the retained failing source-value test addresses that risk.
- **Byte-preserving attributes:** Accept. Fixtures and raw audit copies must survive checkout without changing signed bytes; the UI-source LF attributes likewise bind receipt hashes consistently.
- **User’s one-row correction within the active task:** Accept. The user correction supersedes earlier decorated-window design while preserving the single active task and other product-stage boundaries.
- **Visible-frame/client geometry rather than WS_CAPTION absence:** Accept based on the full top-preserving image and actual geometry. A retained caption style bit is not proof of a second visible row; the failed style-bit experiment remains recorded. I did not independently reread the Tao dependency implementation.
- **Real tray restore versus maximize-as-show:** Accept. Existing restore code calls show/unminimize/focus; the failed maximize-as-show test is not a tray failure. Actual user confirmation and selected-window observation support the manual steps with no invented position delta.
- **External TEMP for credential tests:** Accept. The first failure triggered the existing repository-write protection, and the rerun changed the test environment rather than the credential guard or keys. Tests remained synthetic; no real credential decryption is asserted.
- **Deferred stale state alert:** Accept as Minor R2, with precise future clearing recommended.

## Recommendations and readiness

Fix R1 and validate the missing/non-array-versus-empty distinction through a focused test and fresh scoped review. No new model request, UI install, entire matrix or unrelated full regression is justified solely by that Python count-expression correction unless its implementation changes reveal a broader concern. R2 may remain documented.

**Ready to merge: With fixes.** Core source-review and installed one-row behavior are supported by the reviewed code and retained evidence, but the independent gate presently accepts a known-zero fabrication where the contract requires unknown. After R1 is resolved, complete the controller-owned ledger/final documentation and normal commit/push checks; those operations were intentionally outside this read-only review.

HEAD remained `cccd784f607d506d0b6c8b0a12aea7973fadeb57` throughout. Git status remained the two user-deleted legacy PRD files. No Git/index/HEAD/product mutation, subagent, new suite/matrix/build/product CLI/model/network/CUA operation, private-folder access or cleanup was performed.
