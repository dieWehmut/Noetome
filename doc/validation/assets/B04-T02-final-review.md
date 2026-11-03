# B04-T02 fresh whole-branch SPEC + QUALITY review

Reviewed on 2026-11-03. Base: `b4fae257cd1b53c0f48788773ec1fac1f0154e40`; head: `981c1f66ca6cc0234dd9159c7e731af473be64a5` (six commits). Reviewer: fresh Sol fallback. The earlier Astra review failed before producing a review; its failure is not approval or evidence of review.

## Verdict

No actionable P0/P1/P2 findings identified in the reviewed range. The offline implementation is acceptable against the binding `doc/technical/EXTRACTION_API_CONTRACT.md` and `doc/execution/B04-T02_PLAN.md`. This verdict does **not** complete B04-T02, approve a real paid call, or satisfy the dependency gate for B04-T03. Actual user provider/endpoint/model, external environment-variable reference and explicit budget remain missing. The controller must record that real gate and finish the authorized Git/log workflow.

## Independent method and scope

Read the supplied diff package first, then repository AGENTS, current status/TODO/ledger, NEXT_STEPS, document index, the binding contract/plan, D07/D08 and applicable PRD requirements. Independently inspected current code and branch diffs for native transport, strict wire parsing and schema generation; selected configuration guards and credential handling; Core request reservation, usage reconciliation, source binding, shared candidate validation, API output ownership and persistence; CLI parsing/dispatch/help; locked dependency changes; ordinary Rust tests and the independent Python HTTP auditor. Inspected standards and acceptance records, including preserved failure evidence and explicit live/scientific-quality limitations. Also inspected the reused source-session and output capability implementation to assess integration effects.

Reviewed the following substantive boundaries:

- Reservation uses the serialized HTTP body including instructions/schema/units/anchors, plus the explicit 1024 policy margin and configured output limit. Checked arithmetic, input/output/total usage reconciliation, overflow, absent usage/details and unknown-price stopping. Reports distinguish engineering reserve from actual tokenizer measurement and billing.
- Transport builds one client/request, disables automatic proxy inheritance, redirects and retry, retains actual response bytes/status, distinguishes complete empty framing from partial reads, and drops the network future on timeout/cancellation. Runtime background shutdown avoids waiting for already-running blocking DNS work. No implementation claim was treated as a live TLS/cancellation/billing test.
- Envelope and candidate JSON reject duplicate keys and trailing JSON. Required completed assistant output, refusal/incomplete/tools/extra text and known metadata types are checked separately from independently valid response id/model/usage. Candidate failures preserve verified usage; partial/non-200 responses do not validate fragment usage.
- Credential checks precede request/response persistence, repeatedly decode escape tokens in nested and malformed JSON, fail closed at their work bound, and protect the only persisted response header. Stable diagnostics do not echo remote error text or Authorization. Safe raw responses are preserved; detected echoes save only receipt fingerprints/length and omission reason.
- Resource anchors copy unit start/end within the original resource, preserve full unit quote and slice SHA, and keep resource SHA separate. The common validator checks actual UTF8 ranges/quotes/SHA, source binding, domain references, required=false/unverified and explicit unknown values. The fake protocol remains separate and unchanged.
- Config/source/output/cancel preflight, held identity/mtime/SHA guards, final checks, whole-source overlap rejection and capability-owned cleanup remain integrated. Unknown files are preserved. Report success remains candidates_validated/pending/unpublished/not_run, and failed results do not create knowledge or Vault records.
- Python audit expectations come from the frozen author fixture before requests. It checks ordinary CLI and real local POST receipts, SQL/file invariants, persisted required reports, both complete report views, nonzero source offsets and gold content. Its fake service is explicitly artificial; its outputs do not establish real model knowledge quality.

## Evidence checks actually executed by this reviewer

Read-only PowerShell parsing and hashing independently confirmed:

- API asset: 84 scenario rows, 90 ordinary CLI calls, 54 local POSTs; no expected/observed exit, config SHA or source SQL SHA mismatches. Native and Python audit implementations were read rather than treating row `passed=true` as an oracle.
- Existing fake asset: 54 scenario rows and 60 CLI calls.
- API and fake evidence both reference binary SHA `41a193b62cfc6fa246a78e6e7bca76359780cefcd8b3ae90c1658a986db3cff7`, which matches the retained `target/debug/noetome.exe` bytes.
- Author gold asset SHA `7e6decaf0a137f089be668d251ace83fb7ea0f65019f5544cca26d6b1f4c489d` matches the actual frozen `fixtures/extraction-gateway-v1/expected.json`.
- Retained outputs named by the API calls contain 59 actual reports and six successful candidate results. Independently verified all 231 reported artifact fingerprints against actual retained files; JSON report bytes match recorded stdout and the Markdown JSON snapshot, with zero mismatches. Checked reserve values and validated usage sums in the archived report facts.
- Raw final workspace log reports 362 library tests and 118 other workspace tests, total 480 passed, zero failed/ignored. Raw Python log reports 44 tests and OK. These are inspected controller execution records, not tests rerun by this reviewer.
- HEAD remains the reviewed commit; the only visible working-tree changes are the user's two unrelated old `docs/Noetome-PRD-v1.0.*` deletions, which were preserved.

## Execution limits and remaining gate

This was a fresh static and retained-artifact review. No duplicate full regression, new test run or local targeted reproduction was performed because no concrete concern required reproduction. No actual credentials were looked up, no real service or paid request was invoked, no private scan or production Vault operation occurred, and no production/test/Git/task-state files were mutated. The sole output mutation is this requested review report.

The implementation and validation documents correctly distinguish offline engineering checks from real provider/model/TLS/billing and scientific-quality acceptance. API independent POST scenarios use authored text Spans; existing PDF regression and shared resource handling do not constitute real PDF model extraction acceptance. Ctrl+C, cross-platform behavior, production Vault, long-task performance and formal knowledge benchmarks remain untested as documented. Ordinary checks and any required blocker logging/push are owned by the controller; real-call acceptance remains conditional on the missing user configuration/budget.
