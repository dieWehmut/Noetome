# Task 1 implementer report

Status: DONE. No commits or pushes; controller owns Git and product-task state.

Scope: Task 1 of doc/execution/B04-T02_PLAN.md on codex/b04-t02-text-api. Implemented only three modules, exact dependencies/lock, and three top-level module declarations in lib.rs. The original three public CLI tests are unchanged and intentionally remain RED until Task 2 routes the command. No real API call, secret lookup, project/Vault write, task-state transition or further agent dispatch occurred.

## Interfaces

The original send, TransportResult, decode, ApiCandidates, ParsedResponse and request_body signatures and fields match the plan exactly. ApiCandidates additionally derives Debug/Clone/Serialize/Deserialize, with deny_unknown_fields and a schema_version=1 deserializer.

The only interface addition is the controller-requested refinement:

```rust
pub(crate) struct ResponseMetadata {
    pub(crate) id: String,
    pub(crate) model: String,
    pub(crate) usage: serde_json::Value,
    pub(crate) service_tier: Option<String>,
}
pub(crate) fn response_metadata(body: &[u8]) -> Result<ResponseMetadata, ProjectError>;
```

It validates the complete envelope JSON (including all nested duplicate keys and known field types) and integer/summed usage independently of status/candidate approval. Core should call it only for a complete HTTP 200 receipt and retain usable metadata before decode rejects incomplete/refusal/bad candidates. It does not turn invalid or absent usage into zero. Decode records the returned model; the caller retains the configured model separately.

Request input is a JSON string containing request_id, binding, units, evidence_anchors. Each full-unit anchor contains unit_id, excerpt_start_byte=0, excerpt_end_byte=unit.text.len(), excerpt_sha256=text::sha256(unit.text.as_bytes()), quote=unit.text. Offsets are relative to unit text, rather than document offsets. There is no model-generated producer field. Request bytes are the exact bytes for caller reservation/persistence, capped at 256 KiB.

## Implementation

- Transport constructs one native TLS verified client, HTTP1 only, no automatic proxy, no redirects and retry::never, then one POST with application/json. The current-thread runtime polls the network future against a 5 ms callback timer; it also checks elapsed time before every network poll. Control errors preserve their original codes. Timeout/cancellation drop the pending network future before runtime shutdown. Attempts starts at zero and becomes one when the network operation is actually polled.
- Receipt status and x-request-id are captured, and body bytes never exceed max_response_bytes. Truncated Content-Length/chunked and empty bodies fail explicitly; oversized bodies retain only the bounded actual prefix and complete=false. HTTP 401/403, 429, 5xx and other statuses use static failure codes. Remote diagnostics and bearer/header content are never included in error messages.
- Tokio runtime shutdown uses shutdown_background after dropping the request future/client, so uncancellable system resolver work cannot extend the deadline by blocking Runtime::drop. Existing OS DNS work may finish independently; it cannot resume the dropped POST future.
- Wire JSON uses a recursive unique-key visitor before typed candidate deserialization. This protects unknown future metadata too, rejecting duplicate keys, trailing JSON, malformed JSON and known type mismatches. It accepts one completed assistant message with one output_text, plus optional reasoning metadata. Reasoning content/summary must be reasoning_text/summary_text with text; it cannot hide a refusal, extra output_text or incomplete status.
- Usage requires nonnegative integer input/output/total, overflow-safe exact sum and correctly typed known detailed counters if present. Details remain absent when absent and are not filled. Known cached/cache_write/reasoning details cannot exceed their parent totals. Unknown metadata remains extensible.
- Schema contains all current SourceBinding/Concept/CandidateEvidence/CandidateAtom/KnowledgeAtom/Measurement fields and enums. Every schema object requires all fields and additionalProperties=false. Measurement/value/unit and source nullable fields explicitly permit null. Atom required=false and verification=unverified are encoded in Schema. Body includes explicit store/stream/background=false, tools=[], tool_choice=none, service_tier=default, truncation=disabled and max_output_tokens.

## TDD evidence and actual commands

All test commands used the Windows-safe scripts/cargo_local.ps1 wrapper and isolated component filter. Existing workspace/CLI/PDF tests are for controller integration and were not run here.

1. Initial RED before implementation:
   `.\scripts\cargo_local.ps1 test -p noetome-cli --lib extraction_api --offline --locked '--' --test-threads=4`
   Output: task1-red.txt. Actual result: 10 failed, 1 passed; failures came from intentionally unimplemented send/decode/request_body, e.g. EXTRACT_API_TRANSPORT instead of EXTRACT_CANCELLED, and no structured request body.
2. Metadata refinement RED before implementing its function:
   `.\scripts\cargo_local.ps1 test -p noetome-cli --lib metadata_preserves --offline --locked '--' --test-threads=4`
   Output: task1-metadata-red.txt. Actual result: 1 failed, EXTRACT_API_RESPONSE/not implemented when usable metadata should be returned despite incomplete/bad candidate output.
3. First component GREEN: same extraction_api filter, task1-green-first.txt. Actual result: 12 passed, 0 failed, no warnings.
4. Self-review RED: same extraction_api filter, task1-review-red.txt. Actual result: 12 passed, 2 failed. Zero deadline falsely counted attempt=1; optional type checker incorrectly accepted null object/known metadata fields. Both were fixed. task1-green.txt preserves the intermediate result: 13 passed, 1 failed, exposing a Tokio sleep(0) initial pending race. Checking timeout before polling the network fixed it; task1-review-green.txt reports 14 passed.
5. Reasoning boundary RED:
   `.\scripts\cargo_local.ps1 test -p noetome-cli --lib reasoning_cannot --offline --locked '--' --test-threads=4`
   Output: task1-reasoning-red.txt. Actual result: 1 failed because reasoning content hid extra output_text. Fixed by validating reasoning parts/status. task1-final-green.txt retains a later Windows harness failure: listener nonblocking mode was inherited by accepted sockets; explicitly setting accepted sockets blocking fixed the test setup. task1-native-green.txt reports 15 passed.
6. Runtime shutdown RED:
   `.\scripts\cargo_local.ps1 test -p noetome-cli --lib runtime_shutdown --offline --locked '--' --test-threads=4`
   Output: task1-shutdown-red.txt. Actual result: 1 failed; request return waited 302.3282 ms for a controlled 300 ms blocking worker, analogous to uncancellable resolver work. Changed Runtime::drop to shutdown_background after dropping the request future. The test waits for its own worker to finish before returning, leaving no test worker outstanding.
7. Final verified GREEN:
   `.\scripts\cargo_local.ps1 test -p noetome-cli --lib extraction_api --offline --locked '--' --test-threads=4`
   Exact output: `D:\Noetome\.superpowers\sdd\B04-T02_PLAN\task1-verified-green.txt`.
   Actual result: **16 passed, 0 failed, 339 filtered out; 0.41 seconds; no warnings.**

Dependency acquisition: `.\scripts\cargo_local.ps1 fetch --offline` failed because hyper-tls was absent from the local cache. `.\scripts\cargo_local.ps1 fetch` then downloaded the native TLS dependencies and updated Cargo.lock. All following test builds were offline/locked. Direct deps are exactly reqwest=0.13.5 (default-features=false, native-tls-no-alpn), tokio=1.54.1 (default-features=false, rt/net/time), futures-util=0.3.34 (default-features=false, std).

Formatting: initial fmt --all --check showed expected unformatted new source. Ran `.\scripts\cargo_local.ps1 fmt --all`, then final `.\scripts\cargo_local.ps1 fmt --all --check` passed with exit 0. Final scoped `git diff --check -- crates/noetome-cli/Cargo.toml Cargo.lock crates/noetome-cli/src/lib.rs` passed (Git only printed existing LF/CRLF conversion notices). `git diff -- crates/noetome-cli/tests/extraction_api.rs` was empty. Earlier full `git diff --check` also passed. Scoped lib diff contains only the three module declarations.

## Files changed by this worker

- crates/noetome-cli/src/extraction_api_transport.rs
- crates/noetome-cli/src/extraction_api_wire.rs
- crates/noetome-cli/src/extraction_api_schema.rs
- crates/noetome-cli/src/lib.rs (only module declarations)
- crates/noetome-cli/Cargo.toml
- Cargo.lock
- This internal report and named RED/GREEN evidence logs in .superpowers/sdd/B04-T02_PLAN.

Controller changes to the contract/plan and CLI evidence cosmetic fix, and the user's two deleted docs/PRD files, were left intact. No other repository source or route was edited.

## Self-review and limits

Reviewed the complete new files, existing DTO fields/enums, and the Cargo/lib diff. Fixed the meaningful boundary findings with recorded RED/GREEN above. Read installed reqwest 0.13.5 client/retry/chunk/native TLS feature APIs and Tokio 1.54.1 runtime shutdown docs for the actual behavior; no unverified API guesses remain.

No scope/interface deviation beyond the explicitly requested ResponseMetadata refinement. No known outstanding Task 1 bug. Controller/fresh reviewer still owns integration acceptance. Native HTTP socket tests are local artificial services; they do not prove a live TLS certificate handshake, provider/model compatibility, true tokenizer/billing, scientific correctness, cross-platform behavior or any real API call. System DNS shutdown behavior is validated with controlled blocking work, without external DNS calls. Task 2 retains all endpoint/auth configuration, secret echo checks, budget/source binding/domain semantic validation, outputs and CLI routing. B04-T02 remains in progress pending its actual real-call gate.

## Fresh reviewer fixes — two P2 findings

Status: DONE after correcting both findings. This appended section supersedes the original report's incorrect claims that empty HTTP bodies are truncated and that metadata extraction validates all known envelope/output fields. The earlier 16/16 result is a real historical test run, but two of its expectations encoded incorrect behavior. All original logs remain preserved.

Reviewer finding 1: a normal EOF for Content-Length:0 (or properly terminated zero-byte chunked body) is a complete HTTP receipt. The old transport code returned EXTRACT_API_TRUNCATED and complete=false before classifying HTTP status. Removed that body-emptiness check. Transport now sets complete=true after every normal chunk EOF; 200 has no transport failure and wire::decode rejects empty JSON with EXTRACT_API_RESPONSE. Empty 401/429 retain EXTRACT_API_HTTP_AUTH / EXTRACT_API_HTTP_RATE_LIMIT. Actual interrupted Content-Length/chunked and oversized bodies still have complete=false and their original failure codes.

Reviewer finding 2: response_metadata used envelope(), allowing malformed output fields to discard otherwise valid id/model/usage. It now uses the recursive strict_json parser directly and independent metadata validation. Metadata checks id/model, nullable optional service_tier, actual usage integers/sum and optional known detailed counters. It still rejects duplicates at all JSON depths, trailing/malformed JSON, and absent/invalid usage. Status/envelope/output validation stays in decode, so invalid output id:null, summary:"bad", annotations:{} still cause extraction failure while the caller can preserve the original 12+8=20 token metadata. Unrelated invalid object/created_at fields likewise do not suppress valid independent metadata, while decode rejects their types. Public interfaces are unchanged.

Tests were changed before production fixes: replaced the incorrect empty-body truncation expectation with real socket cases for 200/401/429 plus valid zero-byte chunked EOF; preserved real Content-Length/chunked truncation and oversize tests. Replaced the metadata-output rejection test with independent metadata-success / decode-failure assertions, retained actual id/model/service_tier type rejection, and extended existing duplicate/usage tests to assert response_metadata failures too.

Actual RED command:

```powershell
.\scripts\cargo_local.ps1 test -p noetome-cli --lib extraction_api --offline --locked '--' --test-threads=4
```

Output: `D:\Noetome\.superpowers\sdd\B04-T02_PLAN\task1-fix-red.txt`. **16 passed, 2 failed**. The empty-body test failed because Content-Length:0 was incorrectly complete=false. The independent-metadata test failed because envelope validation returned EXTRACT_API_RESPONSE instead of preserving valid metadata. These failures were recorded before changing production behavior.

Applied only the two production fixes in extraction_api_transport.rs and extraction_api_wire.rs, formatted the changed code and ran the same focused command again. Final output: `D:\Noetome\.superpowers\sdd\B04-T02_PLAN\task1-fix-green.txt`. **18 passed, 0 failed, 339 filtered out; 0.42 seconds; no warnings.** All new socket/output mutations and retained truncation/usage/duplicate cases passed.

`.\scripts\cargo_local.ps1 fmt --all --check` passed (exit 0). Scoped `git diff --check -- crates/noetome-cli/Cargo.toml Cargo.lock crates/noetome-cli/src/lib.rs` passed (only existing LF/CRLF conversion notices). `git diff -- crates/noetome-cli/tests/extraction_api.rs` was empty. No broad workspace, public CLI, PDF, real API or TLS handshake tests were rerun. No root docs/scripts, dependencies, schema implementation, CLI route, state or Git history were changed for these fixes. No known outstanding issue from the two reviewer findings; the controller owns re-review and integration.

## Controller integration fix — resource-absolute evidence anchors

Status: DONE. This section supersedes the earlier report's claim that evidence offsets are relative to unit text. The established Core candidate validator uses absolute positions in the referenced source resource and subtracts unit.start_byte only when slicing the selected unit text. The Schema's old 0..text.len anchors and instruction would fail for a nonzero-start Span/PDF region.

Changed only extraction_api_schema.rs and this report for the fix. Before production changes, updated the existing request/schema test to use RequestUnit start_byte=123, end_byte=129, text="中文" (six UTF8 bytes), with a Text locator at ordinal=1 and a correctly shaped SHA256 of the artificial source resource. The emitted full-unit anchor must have start=123/end=129, exact quote="中文" and SHA256 of quote bytes. Core/fake v1 and all shared candidate/source semantics remain unchanged.

RED command: `.\scripts\cargo_local.ps1 test -p noetome-cli --lib extraction_api_schema --offline --locked '--' --test-threads=4`. Exact evidence: `D:\Noetome\.superpowers\sdd\B04-T02_PLAN\task1-anchor-red.txt`. Actual result: **0 passed, 1 failed, 356 filtered out**; the assertion observed Number(0) instead of 123 before changing production anchors/instructions.

Production anchors now copy unit.start_byte and unit.end_byte. Request instructions explicitly describe UTF8 offsets in the referenced source resource and tell the model to copy resource-absolute anchor positions while preserving quote/SHA. No extra consistency rule was added: the function consumes already validated Core RequestUnit values, and this fix does not redefine source semantics.

GREEN command: `.\scripts\cargo_local.ps1 test -p noetome-cli --lib extraction_api --offline --locked '--' --test-threads=4`. Exact current final evidence: `D:\Noetome\.superpowers\sdd\B04-T02_PLAN\task1-anchor-green.txt`. Actual result: **18 passed, 0 failed, 339 filtered out; 0.41 seconds; no warnings**. Previous transport/wire P2 fixes remain passing.

Ran `rustfmt --edition 2024 crates/noetome-cli/src/extraction_api_schema.rs`, then the same command with `--check`; both exited 0. Only this source file was formatted. No interfaces, dependencies, source validator, fake exchange, root docs/scripts, CLI tests/routes, product-task state, commits or pushes were changed. No real model call/TLS handshake or ordinary CLI nonzero-Span audit was run by this worker; controller owns that integration evidence. No known outstanding issue from this anchor fix.
