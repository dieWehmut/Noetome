# Task 2 implementer report

Scope: configuration, Core orchestration/shared domain validation, API owner and receipt/report persistence, ordinary `extract api` parsing/dispatch. No Task1 transport/wire/schema or dependency edits; no ledger transitions, commits/pushes, independent auditor changes, real API requests, secret discovery, or T03 work.

## Actual RED / GREEN

- Before production implementation, added native tests with failing placeholders for strict numeric/official endpoint authority and credential scanning across nested serialized JSON plus malformed JSON. `cargo_local test -p noetome-cli --lib extraction_api --locked --offline -- --test-threads=4` observed **2 expected failures, 18 existing Task1 passes**.
- Added a second actual RED for input/output/total limits, missing/null usage, and checked-add overflow: `... --lib extraction_api::tests::actual_usage ...` observed **1 expected failure** from deliberately unimplemented usage predicate. Implemented actual usage checking and metadata reconciliation; then **21/21 passed**.
- Added regression coverage for strict duplicate/extra/missing/nullable config, version/provider/mode/input/env validation, selected read guard, actual Windows deny-write behavior, early hardlink rejection and 64 KiB bounds, while an unrelated sibling is guarded. First guard test had a test-path error (`../../target` rejected by existing path policy), corrected test construction to use `parent()` twice. Final native API suite **23/23 passed**, 0 ignored; 18 inherited Task1 + 5 Task2 tests.
- Final executable rebuilt after all production changes (including config fingerprint audit): `cargo_local test -p noetome-cli --test extraction_api --test extraction --test fixture --locked --offline -- --test-threads=4`: **API 3/3, fake 3/3, fixture 20/20 passed**, no warnings. `target/debug/noetome.exe` is current.
- Applied cargo format; `git diff --check` passed (only existing line-ending notices). No full workspace/Python/auditor/real-PDF tests run here; controller owns those.

## Implementation and interfaces

- Exact public `extraction_api::run(project, import_id, config, output, cancel)` returns existing `source::CommandOutput` and `ProjectError`.
- `SelectedConfig` owns only the explicitly selected file's read guard, identity, mtime, SHA and length. Link count is checked before reading any JSON. Config fields/limits/endpoints and the explicitly referenced env name/value are validated with stable errors; no parent directory content scan or default env lookup. The config fingerprint and selected reference are audited in request.json.
- `extraction::validate_candidates` takes units/concepts/evidence/atoms slices plus existing control/signal. Fake v1 wrapper and behavior remain; API candidate objects are never converted to fake payloads.
- `OutputRoot::create_api` writes a distinct api owner with transport mode; fake `create` still has its original shape. Root guards/file receipts/owned cleanup reuse existing protections.
- Actual serialized request bytes +1024 reserve, no request when count/reserve/price/capability/cancel stops, single native send only. Complete200 metadata is independently preserved before candidate/refusal/incomplete decoding; decoded metadata reconciles with that receipt. Actual limits and arithmetic are checked with no zero defaults. Local real-model/paid calls remain0; unstarted live calls0, attempted uncertain live calls null, valid completed decoded live model response1 with paid unknown.
- Repeated escape decoding catches plain/escaped/malformed/nested credential echoes before request or response persistence; unsafe response/header omits response.bin and candidates while preserving actual body SHA/length and omission reason. Diagnostic strings are also screened before report/terminal output. Work is bounded to 64 MiB cumulative scanning and fails closed on adversarial escape depth.
- Reports separate configured/returned model, actual framing/status/attempts, verified original usage, unknown price/tokenizer, pending/unpublished/not_run facts, source statuses, and artifact hashes/lengths. Markdown is readable and embeds the exact complete JSON snapshot. Candidate wrapper has api/mode provenance and ApiCandidates payload. Final source/config/output checks revoke only captured result artifacts on late change, leaving unknown human files intact.
- CLI/help supports `extract api ... [--cancel-file ...]` and native dash paths via `--` after api; distinguishes localfake and one configured live request without claiming actual provider smoke or scientific acceptance.

## Self-review and remaining work

Reviewed file scope, source/config/link boundaries, budget order, exact Task1 interface consumption, metadata independence, credential paths, owner/fake compatibility and failure cleanup. Removed the initial unused ParsedResponse fields warning by reconciling the independent decodes. Confirmed only assigned production files changed; user's old PRD deletions and controller's native-green asset edit remain untouched.

No real provider/model configuration or budget has been supplied; actual service smoke and scientific acceptance are not established. Fresh independent task review plus the controller's larger HTTP audit and broad checks remain required. Files are frozen for that review; no remaining known implementation issue from the targeted tests/self-review.

## Integration help repair and evidence clarification

Controller's actual workspace regression found `cli::help_succeeds_without_writing_files` failed because HELP no longer contained the required truthful `尚未实现` scope boundary; 362 native tests passed before the CLI failure. Preserved controller raw failure: `doc/validation/assets/B04-T02-workspace-help-failure.txt`.

Scoped repair changes only two help strings in lib.rs: main help now explicitly says complete knowledge compilation and incremental production Vault publication are not yet implemented; SOURCE_HELP directs users to extract fake/api and states actual model smoke/scientific quality remain unaccepted. API/pending/offline/real-service boundaries remain accurate. No test expectations or Core/config/transport behavior changed.

Actual post-fix targeted command: `cargo_local test -p noetome-cli --test cli --test source --test extraction_api --locked --offline -- --test-threads=4`. **CLI4/4 + source19/19 + API3/3 passed**, 0 ignored and no warnings; current executable rebuilt. `cargo fmt --all --check` and `git diff --check` passed. Files frozen for scoped re-review.

The earlier Task2 native RED/GREEN outputs were observed directly through tool responses; **no raw RED/GREEN file was saved by this worker**. The assertions/counts above are developer attestation, not independently preserved raw logs. No historical placeholder was recreated or rerun to manufacture retrospective evidence. The new help-fix GREEN also exists in the current tool output; controller's preserved full regression logs are the final raw GREEN evidence source.
