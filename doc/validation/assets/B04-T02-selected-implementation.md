# Task 2 report — selected Responses gateway

DONE for the bounded offline adapter implementation. B04-T02 itself remains controller-owned and requires its real-call acceptance. No live HTTP, real credential/DPAPI/Codex-auth reads, dialogs, commits, pushes, subagents, or directory cleanup were performed by this worker.

## Implemented contract

- `extraction_api_config.rs` accepts exactly `https://ai.1343263.xyz/v1/responses` in live mode and exactly `deepseek-flash` for that gateway. Existing official HTTPS endpoint and numeric-loopback synthetic local mode retain their original restrictions. Explicit ports, alternate schemes/hosts/paths, userinfo, query, fragment, casing aliases and different selected model spellings are rejected before credential/network/output processing. Existing strict config parsing, selected-file identity/mtime/SHA checks and secret prefix rules remain.
- Closed native `RequestStrategy` selects by validated endpoint/mode, independently of a model alias. Versions are verbatim `tangmao-deepseek-responses-json-schema-v1` and `openai-responses-strict-json-schema-v1`.
- `extraction_api_schema.rs` preserves the shared instructions/input/schema/name/evidence anchors, max output, store=false, stream=false, empty tools and tool_choice=none. The selected strategy sends reasoning={effort:none} and omits background/service_tier/truncation/text.format.strict. Official/local retains strict=true and its original fields. Native selected tests explicitly check the full top-level field set, nonzero UTF8 resource-absolute evidence offsets, independently hand-checked SHA and bounded schema. Selecting a model name alone does not change official behavior.
- `extraction_api.rs` records request_strategy in request.json and validation.json; validation also records endpoint. The unchanged Markdown serializer embeds the identical complete validation snapshot, so both report formats contain those fields. Configured/actual model remain separate. Input reservation is still actual serialized body UTF8 bytes plus 1024; no changes to null/unknown metering, output limit, once-only POST, no retry, strict wire/domain validation, secret-echo scanning or source/output protections.

Binding material read: `task-2-brief.md`, `doc/technical/EXTRACTION_API_CONTRACT.md`, and the previously fetched primary `target/gateway-primary-docs-20261103/deepseek-0.txt` and `deepseek-guide.md` (DeepSeek create-response reference and Responses compatibility table). Gateway support beyond the exact selected pair is not claimed. Core remains the strict candidate validator; omission of unsupported server strict flags is not treated as a server guarantee.

## Owned source fingerprints

| File | Final SHA256 | Bytes |
| --- | --- | ---: |
| crates/noetome-cli/src/extraction_api_config.rs | f1db3372efdbebe8f200203ec46062c7e24416cecae6c38fbd38453e3ae3bed3 | 15976 |
| crates/noetome-cli/src/extraction_api_schema.rs | 8b840d2b9f54f8b1241b9b286a6c2ed4f04f07beb1719b2f68bf089c09629737 | 11842 |
| crates/noetome-cli/src/extraction_api.rs | 1037f4cce5fd2775b2b7c0fd46b7b5296cdd332675a092ca905957eb0de9d957 | 19082 |

No worker edits to controller CLI tests, scripts, logs/docs, task state, dependencies, or other source modules. Existing user deletions of docs/Noetome-PRD-v1.0.md/html were preserved. Additional worker files are evidence under this ignored plan workspace; full evidence hashes and selected CLI output paths are in `task-2-final-artifacts.json`.

## Actual RED and GREEN commands

All paths below are relative to D:/Noetome unless absolute. Every result was saved immediately; running commands retain initial/final output chunks in their evidence JSON. Commands used locked/offline Cargo and a quoted PowerShell '--'.

| Command | Actual result | Raw evidence |
| --- | --- | --- |
| `.\scripts\cargo_local.ps1 test --locked --offline -p noetome-cli --lib selected_gateway_accepts_only_exact_live_endpoint_and_selected_model '--' --test-threads=1` | Native RED: exit 1, 0 passed / 1 failed; exact selected pair rejected EXTRACT_API_ENDPOINT before implementation | task-2-native-red.json |
| Controller's earlier `.\scripts\cargo_local.ps1 test --locked --offline -p noetome-cli --test extraction_api ordinary_selected_gateway '--' --test-threads=4` | Preserved original ordinary CLI RED:1 passed / 1 failed | gateway-cli-red.json |
| `rustfmt --edition 2024 crates/noetome-cli/src/extraction_api_config.rs crates/noetome-cli/src/extraction_api_schema.rs crates/noetome-cli/src/extraction_api.rs` | exit 0; only owned files formatted | task-2-format.json |
| `.\scripts\cargo_local.ps1 test --locked --offline -p noetome-cli --lib extraction_api_ '--' --test-threads=1` | exit 0;23 passed; this initial filter omits the 2 Core tests | task-2-native-green.json |
| `.\scripts\cargo_local.ps1 test --locked --offline -p noetome-cli --lib extraction_api '--' --test-threads=1` | exit 0;25 passed,0 failed / ignored, including Core secret/budget, config, schema, native transport and wire | task-2-native-all-green.json |
| `.\scripts\cargo_local.ps1 test --locked --offline -p noetome-cli --test extraction_api '--' --test-threads=1` | exit 0;5 passed, including selected zero-budget/body audit and invalid selected pair rejection | task-2-cli-green.json |
| `.\scripts\cargo_local.ps1 test --locked --offline -p noetome-cli --test extraction '--' --test-threads=1` | exit 0;3 passed, prior fake/source/UTF8/cancel behavior | task-2-fake-green.json |
| `.\scripts\cargo_local.ps1 check --workspace --locked --offline` | exit 0 | task-2-check.json |
| `.\scripts\cargo_local.ps1 fmt --all --check` | exit 0 | task-2-fmt-check.json |
| `git diff --check` | exit 0; Windows Git emitted only LF-to-CRLF notices | task-2-diff-check.json |

The actual interpreter returned by workspace_dependencies was `C:/Users/Lyinn/.cache/codex-runtimes/codex-primary-runtime/dependencies/python/python.exe`. This machine path was used only in commands and evidence, never repository runtime configuration/code.

1. Independent API command: `& 'C:\Users\Lyinn\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe' scripts/validate_extraction_api.py --binary target/debug/noetome.exe --root .superpowers/sdd/B04-T02_GATEWAY_PLAN/task-2-api-independent`. Exit0,84 independent scenes,90 ordinary CLI calls; raw command output `task-2-api-independent-command.json`, structured audit `task-2-api-independent/audit.json`.
2. First independent fake command: same interpreter, `scripts/validate_extraction_gateway.py --binary target/debug/noetome.exe --root .superpowers/sdd/B04-T02_GATEWAY_PLAN/task-2-fake-independent`. Genuine exit 1 because the worker process did not set NOETOME_PDF_PYTHON; PDF import failed PDF_RUNTIME. Partial scenes and failure were retained in `task-2-fake-independent-command.json` and `task-2-fake-independent/failure-evidence.json`; that test root was not deleted/reused.
3. Corrected fake command set `$env:NOETOME_PDF_PYTHON = 'C:\Users\Lyinn\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe'` in the same command process, then ran that interpreter with `-X utf8 scripts/validate_extraction_gateway.py --binary target/debug/noetome.exe --root .superpowers/sdd/B04-T02_GATEWAY_PLAN/task-2-fake-independent-corrected`. Exit0,54 independent scenes,60 ordinary CLI calls; raw output `task-2-fake-independent-corrected-command.json`, full source inventories/SQL/calls `task-2-fake-independent-corrected/evidence.json`. The reused interpreter's actual metadata was CPython 3.12.14 / pdfplumber 0.11.9 / pdfminer.six 20251230.
4. Additional actual snapshot audit invoked the same selected interpreter through a literal PowerShell here-string (`-X utf8 -`); full source/command/result are in `task-2-snapshot-check-command.json`. Exit0. It read the selected zero-budget CLI output and 59 old API validation outputs:60 reports verified, JSON equals complete Markdown snapshot, request/report strategies and endpoints agree, reservation equals actual body bytes+1024, body SHA matches saved request; selected report retains actual_model=null and network_attempts=0, selected unsupported fields absent; all old/local reports retain strict strategy/body and lack reasoning. Full result paths/fingerprints are `task-2-final-artifacts.json`.

## Limits and handoff

These are offline engineering contract results. Live gateway/model compatibility, model quality, billing, actual paid calls and production Vault behavior were not exercised. All HTTP exercised here was a local synthetic fake. Selected live CLI used explicit zero call budget and public synthetic test credential; no live HTTP request was sent. No real credential was read or saved. No broader providers/models, retry framework, real-call evidence, gold interpretation or scientific accuracy was added.

Controller retains full workspace/Python/log checks, independent final review, persisted cumulative budget and operator credential/live smoke coordination, commits/pushes and task acceptance. This Task 2 report does not complete B04-T02 or start B04-T03. The one missing-PDF-env test failure is repaired and preserved; no remaining worker implementation concern was observed in the executed checks.
