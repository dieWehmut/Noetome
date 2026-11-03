Spec Compliance: PASS for immutable BASE332fcd97c264ce8c9e88099ae3d0b1179e4f80d6..HEAD1b699d2bc9e30342953d7a687a4385a0bcb74eee.

Task quality: PASS / Approved with one nonblocking Minor finding. Critical: 0. Important: 0. Minor: 1.

The governing specification is doc/execution/B04-T02_GATEWAY_PLAN.md:47, “Task 2 第二次实际失败后的 Chat 关闭思考修复轮”. Older v1/v2 plan sections are historical. This review does not grade later working-tree closure changes.

## Strengths and verified scope

- crates/noetome-cli/src/extraction_api_config.rs:239 and :331 select a closed Chat strategy only after configuration validation. Live authority/model/path are exact; numeric loopback requires explicit valid port and matching Chat path. Responses strategies stay separately routed. No new SDK, dependency, Cargo lock or lib.rs change appears in the immutable package.
- crates/noetome-cli/src/extraction_api_schema.rs:28 and :50 preserve complete extraction instructions plus self-contained task/response_schema/request; Chat sends exactly model/messages/thinking/response_format/max_tokens/stream. The original full-body 256 KiB cap remains at :56. Schema enforcement, switching behavior, deployed route and bottom model identity are correctly left for actual validation, not inferred from client parameters.
- crates/noetome-cli/src/extraction_api_wire/chat.rs:40 in the reviewed HEAD requires chat.completion, one choice, index 0, stop, assistant and nonempty string content. It refuses truncation/filter/refusal/tools/functions/wrong roles/types, while recursive strict_json and deny_unknown_fields candidate DTOs reject duplicate keys, fenced/trailing JSON and invalid candidates. Reasoning content is not used as candidate text.
- crates/noetome-cli/src/extraction_api_wire/chat.rs:5 and :9 independently preserve valid prompt/completion/total usage plus raw original usage. The focused shared metadata check at extraction_api_wire.rs:306 validates integer counts, checked sum and known cache/reasoning detail bounds. Core at extraction_api.rs:347 records complete-200 metadata before candidate parsing and preserves it on candidate/length failure; :354 records raw_model_usage. Candidate reconciliation and domain/source evidence validation remain at extraction_api.rs:375 and :399.
- scripts/validate_extraction_api.py:294 adds a small local server factory hook. scripts/validate_extraction_chat.py:113 supplies the Chat server, :122 independently checks observed request bytes/schema/absolute anchors and author gold, and :182 covers 34 controlled scenes. The live-host rejection fixture at :212 explicitly sets max_requests=0. The focused inherited case check at validate_extraction_api.py:335 confirms source SQL/bytes/mtime/inventory, config immutability, response receipts, failure candidate absence and secret omission remain active.
- crates/noetome-cli/tests/extraction_api.rs:446 checks selected live configuration with zero requests; :493 makes an actual synthetic numeric-loopback Chat POST. The native wire cases at the reviewed chat.rs:113, :138 and :179 cover usage retention, choice/content boundaries and duplicate/fenced/trailing/type failures. Existing Responses tests remain in the scoped GREEN run.

## Issues

### Minor: reconcile optional DeepSeek cache hit/miss counters when both are present

crates/noetome-cli/src/extraction_api_wire/chat.rs:28-35 in HEAD1b699d2 checks each prompt_cache_hit_tokens and prompt_cache_miss_tokens independently as an unsigned count no greater than prompt_tokens. The captured primary Chat reference at target/gateway-primary-docs-20261103/deepseek-chat-current.md:956-958 states prompt_tokens equals their sum. Thus prompt_tokens=12, completion_tokens=8, total_tokens=20, hit=8 and miss=8 passes the current checks while preserving contradictory cache metadata. Primary prompt/completion/total budget accounting remains correct, so this is nonblocking. When both optional counters exist, check their addition with overflow protection against prompt_tokens; keep omission behavior unchanged, and add focused native rejection/preservation cases. The controller elected to close this small finding separately before final workspace validation; that closure is outside this immutable review.

The retained diff check contains Windows Git LF/CRLF notices. All inspected GREEN Rust test/compiler outputs have no Rust warnings. Those line-ending notices do not indicate a test failure or source behavior defect.

## Actual evidence and review limits

chat-review-command.json records the reviewer’s read-only audit exit 0: 56 checks passed and 46 fingerprints captured in chat-review-evidence.json. The optional executable audit source is chat-review-audit.py. No tests, builds, HTTP/model/paid requests, credential/profile/auth reads, production/test/docs/ledger edits or Git mutations were performed by this reviewer. Only the supplied source project, author fixture and non-secret retained archives were inspected.

Retained evidence, not rerun tests:

- chat-fix-native-red.json: exit 1, 0 passed / 2 failed. chat-cli-red.json: exit 101, 0 passed / 2 failed. The genuine preimplementation failures reject the new provider and Chat response.
- chat-fix-native-route-final.json: 35 passed / 0 failed; chat-fix-cli-final.json: 7 passed / 0 failed; chat-fix-fake-green.json: 3 passed / 0 failed. Workspace check, owned/all formatting and diff check exited 0.
- doc/validation/assets/B04-T02-root-chat-prototype-audit.json and command.json: 34 scenes / 36 CLI / 28 local POST, all passed, 0 real/paid calls. This is explicitly the historical component binary 6e5be2dbed79e696043611f9f46de7801c713487821f4bcdebb5927b2dd7b9dd, not final whole-workspace acceptance.
- B04-T02-root-chat-python-tests.json: exit 0, 76 tests. Both independent scripts match recorded SHA256. Three captured primary documents match recorded byte length/SHA256 and HTTP200 provenance, with credential_used=false.
- Both actual failed response archives match original SHA256 (first 73de263b3d469facd25f3a80244c430165d0383b309efb37630fd280dab81d9b; second f1afb7be3978a2fc7a1b3cf8dd86f63ba508b7a483860eaec881a4cce141435a), receipts and total usage 936/9683. The second stays incomplete/max_output_tokens. All five selected source files retain recorded bytes/SHA/mtime; author source/gold and gold-before remain fixed. The retained settlement totals 10619 accounted / 9989381 remaining / pending0. B04-T02 remains in_progress and B04-T03 todo.

The first reviewer audit attempt used the wrong expected controller RED exit code (1 instead of actual 101); the second observed the authorized later closure modifying current chat.rs; the third used the wrong fixture gold filename. These reviewer harness failures are preserved in chat-review-command-first/second/third.json and were corrected without running any suites or changing product evidence. The successful audit reconstructs the original new chat.rs from the immutable package and matches its original SHA256, keeping the later closure outside the verdict.

Final whole-workspace build/matrices and their new program fingerprint remain the controller’s next gate. The third bounded actual request, deployed thinking toggle behavior, actual valid candidates, scientific quality, prices and bottom model identity remain unverified. This task-scoped review does not approve B04-T02 completion or advance its phase.
