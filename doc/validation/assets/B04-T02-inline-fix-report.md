# B04-T02 Task 2 actual-gateway fix report

DONE for scoped implementation and native/ordinary offline behavior. The controller retains final fresh review, complete regression matrices and the separately authorized second bounded actual request. This worker made zero network/model calls and read zero actual credentials, DPAPI files, Codex auth, operator config or private materials. The only actual response read was the explicitly authorized, credential-echo-checked author-source archive.

## Scope and behavior

Only four native modules were edited: extraction_api_config.rs, extraction_api_schema.rs, extraction_api_wire.rs, extraction_api.rs and their internal tests. No controller CLI test, credential helper, Cargo dependency, document/task/log state or other source module was edited. User docs/Noetome-PRD-v1.0.* deletions remain. No directories were cleaned; no commits, pushes or subagents were used.

The selected strategy is now verbatim `tangmao-deepseek-responses-inline-json-schema-v2`. The input remains one JSON string with exactly task, response_schema and request. Task starts with the unchanged extraction instructions and explicitly demands one plain JSON object, no Markdown, fences, explanations or extra text. response_schema is the complete existing candidate_schema. request is the original request_id/binding/units/evidence_anchors object, retaining nonzero resource-absolute UTF8 offsets, quote and SHA. Top-level instructions and text.format schema/name remain, along with store=false, stream=false, reasoning.effort=none, empty tools and tool_choice=none. Shared constraints and actual serialized body size/pre-reservation are unchanged. Official/local retain their original input format/body and strict strategy version.

Core now calls decode_with_strategy with the RequestStrategy derived from validated endpoint/mode configuration. Neither returned model aliases nor raw server metadata choose a tolerant decoder. Only selected strategy plus explicit status=completed permits omission of error/incomplete_details. If present, those fields retain the old strict null/object/type and failure semantics. Invalid/missing status, nonnull error or incomplete details, incomplete output, bad metadata/usage, duplicates at all nested levels, tools, extra text, refusal and malformed/nonJSON candidates still fail. No raw bytes or candidate fields are repaired; no code fences are removed.

response_metadata remains a separate strict duplicate/type/usage validator so independently valid model/usage survives candidate failure, including the first actual response. It does not confer candidate success on either strategy. A cfg(test)-only one-argument decode wrapper retains strict mode for inherited native transport tests; production Core always passes explicit validated strategy to decode_with_strategy.

## Actual RED, failures and GREEN

Commands ran from D:/Noetome and were saved immediately under this plan's inline-fix prefix. Locked/offline Cargo used the literal PowerShell '--' with --test-threads=1.

| Command | Actual result | Evidence |
| --- | --- | --- |
| `.\scripts\cargo_local.ps1 test --locked --offline -p noetome-cli --lib inline_fix '--' --test-threads=1` | Genuine behavioral RED: 0 passed / 3 failed. Flat input lacked task/schema/request; valid selected sparse candidate was refused; first plaintext response failed envelope instead of candidates | inline-fix-native-red.json |
| Controller ordinary selected CLI RED | Preserved 1 passed / 1 failed, v1 versus expected v2 | inline-cli-red.json |
| Initial `.\scripts\cargo_local.ps1 test --locked --offline -p noetome-cli --lib extraction_api '--' --test-threads=1` after signature change | Compile failure: inherited transport unit test still called the one-argument decoder. Preserved; repaired by strict cfg(test) compatibility wrapper, without editing transport | inline-fix-native-green.json |
| Same native API command after repair | Exit 0; 29 passed / 0 failed / 0 ignored | inline-fix-native-green2.json |
| `.\scripts\cargo_local.ps1 test --locked --offline -p noetome-cli --test extraction_api '--' --test-threads=1` | Exit 0; 5 passed, including controller's actual ordinary selected inline schema/request oracle | inline-fix-cli-green.json |
| `.\scripts\cargo_local.ps1 test --locked --offline -p noetome-cli --test extraction '--' --test-threads=1` | Exit 0; old fake 3 passed | inline-fix-fake-green.json |
| `.\scripts\cargo_local.ps1 check --workspace --locked --offline` | Exit 0 | inline-fix-check.json |
| `rustfmt --check --edition 2024` with exactly the four owned source paths | Exit 0 | inline-fix-owned-fmt-check.json |
| `.\scripts\cargo_local.ps1 fmt --all --check` | Exit 1 solely for controller-owned crates/noetome-cli/tests/extraction_api.rs assertion formatting; reported to controller, not edited by worker | inline-fix-fmt-check.json |
| `git diff --check` | Exit 0; only Git LF-to-CRLF notices | inline-fix-diff-check.json |

Native tests now cover selected sparse/full completed acceptance versus strict-mode rejection of omitted fields, each separately omitted field, model-alias inability to select tolerance, missing/wrong/noncompleted status, present errors/details and wrong types, invalid usage/metadata, envelope/usage/status/candidate nested duplicate keys, trailing JSON, tools, duplicate messages, extra text, refusal and fenced/plaintext rejection. Existing full native response/domain/config/secret/budget/HTTP protection tests also ran in the 29-test suite.

The first actual response regression reads only `doc/validation/assets/B04-T02-live-first-response.bin`. It verifies independent returned alias deepseek-flash and exact input551/output385/total936, then requires selected decoding to fail EXTRACT_API_CANDIDATES while strict/default decoding fails EXTRACT_API_RESPONSE. The original response is neither changed nor accepted as valid candidates. Request strategy reasoning.none remains client intent; this test does not claim gateway execution of the reasoning instruction.

## Exact patch and fingerprints

`inline-fix-source.patch` contains the complete scoped native diff, SHA256 `57e58104049d19da289c8bc440522e7aee585b7cf49332b5852fc275107f02a9`. Full source/evidence/binary/archive hashes and lengths are in `inline-fix-artifacts.json`.

| File | SHA256 |
| --- | --- |
| crates/noetome-cli/src/extraction_api_config.rs | 8973f44079d6a73475f16622e253bc6faf8be49c4b73748767bff2eda9ba3f19 |
| crates/noetome-cli/src/extraction_api_schema.rs | 152818d66f6d8ae0018931676b43f34b3c114b5b3c990fbbc5154c390ef2cb69 |
| crates/noetome-cli/src/extraction_api_wire.rs | beb234e202f8d7fade0f0ccc49a0d89b24992b8e92f4de4585a04bc4c408351d |
| crates/noetome-cli/src/extraction_api.rs | 9627c8e5ceb7de7d28264f22a2154c83b336ad9213278204bd01c0d36555e64e |
| Authorized raw first response | 73de263b3d469facd25f3a80244c430165d0383b309efb37630fd280dab81d9b |

Artifact generation initially printed a Windows-path-separator KeyError after successfully writing hashes/patch; the raw command failure is preserved in inline-fix-artifact-command.json. Corrected normalized paths then produced exit0 with identical source patch/archive hashes, preserved in inline-fix-artifact-command2.json. Evidence paths are machine-local work artifacts; no runtime path was hardcoded in repository implementation.

## Remaining controller work

Controller must format its owned ordinary CLI assertion, run final fresh fullworkspace/matrices/review and evaluate one second bounded real request within the existing cumulative budget. This worker intentionally did not rerun broad matrices or fullworkspace tests. The first failed real response remains failed; no actual model compatibility, scientific quality, billing correctness, second-call result or B04-T02 completion is claimed here. No worker source concern remains from the executed scoped checks; fullworkspace fmt has the specific controller-owned pending correction above.
