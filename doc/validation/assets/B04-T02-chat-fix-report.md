# B04-T02 Task 2 Chat non-thinking strategy fix

DONE for the scoped offline implementation. Native API 35 tests, ordinary API CLI 7 tests and prior fake CLI 3 tests passed; workspace check, owned rustfmt, fullworkspace fmt and diff check passed. Controller retains fresh review/full matrices and the one separately authorized bounded third actual verification. No worker actual credential, DPAPI, Codex auth, profile/private environment read or live network/model call occurred. No directory cleanup, subagents, commit, push or task/log mutation occurred.

## Implementation and scope

Only existing extraction_api_config.rs, extraction_api_schema.rs, extraction_api_wire.rs, extraction_api.rs and the new focused child extraction_api_wire/chat.rs were changed, with scoped native tests. Child registration is `mod chat` inside the existing wire module; lib.rs and all dependencies remain unchanged. Controller CLI tests, scripts/docs/task state and both user docs/Noetome-PRD-v1.0.* deletions were preserved.

Exact provider `openai_chat_completions` maps to closed strategy `tangmao-deepseek-chat-json-object-no-thinking-v3`. Live accepts only https://ai.1343263.xyz/v1/chat/completions with exact deepseek-flash. Local accepts only HTTP numeric loopback with explicit valid port and /v1/chat/completions; existing public synthetic model/credential prefix rules remain. Provider/path mismatches, other authorities, explicit live ports, userinfo/query/fragment, other selected models and invalid config are rejected before output/network. All original Responses endpoint/model/strict configuration behavior remains.

Chat body contains exactly model, messages, thinking, response_format, max_tokens and stream. system content is the original complete extraction instructions; user content is the unchanged self-contained task/response_schema/request JSON string from v2, including complete strict candidate schema and original binding/units/evidence_anchors. thinking={type:disabled}, response_format={type:json_object}, max_tokens=existing output cap, stream=false. JSON-object mode is not claimed as server-enforced schema. Core keeps strict candidate/schema/domain/source/version/evidence validation and never strips fences or fills missing provider fields. Native body assertions include exact allowed fields, unchanged system/user prompts, full schema and nonzero Chinese UTF8 resource-absolute evidence offsets/quote.

Production Core dispatches metadata and candidate wire parsing only from validated RequestStrategy. Selected Chat parses one choice with index 0, finish_reason=stop, assistant role and nonempty string content. length yields EXTRACT_API_INCOMPLETE, filter/refusal yields refusal, tools/functions/additional or zero choices/wrong role/invalid types fail. Presence of tool_calls or function_call in the message is conservatively rejected. reasoning_content may be null/string only; it is never used as candidate content and raw response alone preserves its actual value. No switch-honored conclusion is inferred from the request.

Duplicate-key checking is the shared recursive strict_json reader at envelope and candidate levels. Fenced, trailing, malformed and unknown-field candidate JSON fails. Existing request byte cap 256 KiB, actual full-body UTF8 input reservation plus 1024, output 8192 / time 60000 / response 1 MiB bounds, once-only POST, no retry, unknown pricing, source/config/output checks and secret echo omission remain unchanged.

Complete HTTP200 Chat metadata independently maps prompt_tokens/completion_tokens/total_tokens to input_tokens/output_tokens/total_tokens and validates nonnegative integral counts, sum/overflow and known cached/reasoning details against their parent counts. Original Chat usage is preserved as usage.raw_model_usage, with the actual original serialized bytes remaining in unchanged response.bin. Normalized model_usage is separately recorded and reconciled with decoded candidate metadata. Valid usage survives length/nonJSON candidate failure; missing/invalid usage is not defaulted to 0. Responses report/body/metadata paths do not acquire Chat raw usage fields and retain their prior strategy/default strict behavior.

## Actual RED and verification evidence

Commands ran from D:/Noetome, were saved immediately as chat-fix-* JSON, and used locked/offline Cargo with literal PowerShell '--' --test-threads=1.

| Command | Actual result | Evidence |
| --- | --- | --- |
| `.\scripts\cargo_local.ps1 test --locked --offline -p noetome-cli --lib chat_fix '--' --test-threads=1` | Genuine compiled RED: 0 passed / 2 failed, Chat provider rejected and Chat candidate rejected by Responses decoder | chat-fix-native-red.json |
| Controller's two ordinary Chat tests | Preserved genuine compiled RED: 0 passed / 2 failed before implementation | chat-cli-red.json |
| `.\scripts\cargo_local.ps1 test --locked --offline -p noetome-cli --lib extraction_api '--' --test-threads=1` | Initial GREEN 34; final GREEN 35 after exact Chat body/config assertions; final routing assertion GREEN 35, 0 failed/ignored | chat-fix-native-green.json, chat-fix-native-final.json, chat-fix-native-route-final.json |
| `.\scripts\cargo_local.ps1 test --locked --offline -p noetome-cli --test extraction_api '--' --test-threads=1` | GREEN 7, including actual local Chat POST and selected zero-budget/no-request oracle; existing Responses tests passed | chat-fix-cli-green.json, chat-fix-cli-final.json |
| `.\scripts\cargo_local.ps1 test --locked --offline -p noetome-cli --test extraction '--' --test-threads=1` | GREEN 3 prior fake/source/UTF8/cancel tests | chat-fix-fake-green.json |
| `.\scripts\cargo_local.ps1 check --workspace --locked --offline` | Exit 0 | chat-fix-check.json |
| `rustfmt --check --edition 2024` with exactly the 4 existing owned paths plus child chat.rs | Exit 0 | chat-fix-owned-fmt.json |
| `.\scripts\cargo_local.ps1 fmt --all --check` | Exit 0 | chat-fix-fmt-all.json |
| `git diff --check` | Exit 0; only Windows Git LF/CRLF notices | chat-fix-diff-check.json |

The child native suite independently tests raw/normalized usage equality and preservation despite length/invalid candidate text, choice count/index and finish reasons, typed role/content/reasoning metadata, tool/function/refusal outputs, envelope/choice nested duplicates, trailing JSON, fenced and unknown candidate fields, fractional/wrong-sum/missing/invalid-detail usage. All inherited Responses v2/strict wire, Core budget/secret, config/source and local HTTP transport tests were included in the 35 API tests. Explicit cross-protocol assertions require Chat response success only under Chat strategy and failure under both Responses strategies.

Source reference captures read: target/gateway-primary-docs-20261103/deepseek-chat-current.md (Chat request/response and usage), deepseek-thinking-current.md (thinking.type disabled/JSON prompting) and gateway-routes-chat-current.go (published /chat/completions route). These are protocol references; this worker did not validate deployed live routing or switch behavior.

## Exact source patch and hashes

Complete scoped diff, including the new untracked child module, is saved as chat-fix-source.patch; SHA256 `99edc999813c0c8eeea8622930a254ffe957ce2da430fe609b5b1c2848384cc3`. A read-only reverse git apply --check passed against the current working tree. Full source/evidence/binary hashes and lengths are in chat-fix-artifacts.json; artifact generation exact command/output is chat-fix-artifact-command.json.

| File | SHA256 |
| --- | --- |
| crates/noetome-cli/src/extraction_api_config.rs | 8d011441ebb916e13930d2709f062aad84870ec4d719efe93e152813b46ea2c5 |
| crates/noetome-cli/src/extraction_api_schema.rs | e3ed5c8a3eda5607ca6e854e112c96e9b910062659e758dc6a7717a26b82157b |
| crates/noetome-cli/src/extraction_api_wire.rs | 8a12efbefadc606fd37e862b5ad531cec05144e76deda409eaf194dea0c488f7 |
| crates/noetome-cli/src/extraction_api.rs | f853d0aaa9db22ae3474ad61e0294d63f9354eb9116c0adc416b61779434f92e |
| crates/noetome-cli/src/extraction_api_wire/chat.rs | 67a2da2b56ba2b928ce6eaddf3db1882b10b9407715dcebaf9bf1100575ed9a9 |
| First original live response | 73de263b3d469facd25f3a80244c430165d0383b309efb37630fd280dab81d9b |
| Second original live response | f1afb7be3978a2fc7a1b3cf8dd86f63ba508b7a483860eaec881a4cce141435a |

## Limits and handoff

No actual third request, fullworkspace test run or independent broad matrices were run by this worker. All actual HTTP exercised in the worker's test commands was synthetic numeric-loopback fake HTTP; selected live CLI tests had explicit zero request budget. Existing actual Responses failures and their source/gold were not changed or accepted as candidates; response SHA matches the authorized archives. Neither deployed thinking toggle compliance, underlying model identity, live knowledge quality, pricing/billing nor B04-T02 completion is claimed. Controller will fresh-review this exact patch and finish its full regression matrices before any separately authorized third bounded real verification. No worker concern remained in the executed scoped checks.
