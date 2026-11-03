SPEC PASS / QUALITY PASS. Critical: 0. Important: 0. Minor: 0.

Reviewed immutable BASE94b0ebb40bb43cb5cdb72d924a40cf2d0893fa10..HEAD25bcff5b474611fdb2b1c1f96eb0a33caae42e4d against doc/execution/B04-T02_GATEWAY_PLAN.md:63, “Task 2 Chat 实际候选包装层修复”. Scope is this failure/prompt fix, its regressions and named archived provenance. Prior component/cache verdicts are historical; no future product task or broad component review was reopened.

## Spec and quality evidence

- extraction_api_config.rs:233 changes only the selected Chat strategy version to tangmao-deepseek-chat-nested-atoms-v4. Official/local Responses strict and selected Responses v2 strings remain unchanged.
- extraction_api_schema.rs:33-35 appends a generic Chat-only task hint specifying exactly atom/measurement outer keys, the ten fields inside atom and the sibling nullable measurement shape. The handwritten field list matches the original generated schema in the archived third request. Conditions/qualifiers remain attached to their claim, and registration/parsing/provenance is evidence context rather than subject knowledge. The hint includes no fixed gold facts or completed response template. The safe expect is applied to a task string constructed immediately above on this same branch.
- The immutable diff preserves candidate_schema, DTOs, enums, cache handling, wire production, complete base system instructions, six Chat body fields, exact three user keys, original request/schema/binding/units/absolute evidence anchors and all limits. The new native request assertions at extraction_api_schema.rs:124 check original task prefix, unchanged request/schema/system and nonzero Chinese UTF8 anchors in addition to the nesting instruction.
- extraction_api_wire/chat.rs:112 adds only a regression reading the authorized raw third-response archive. It requires valid input1583/output3329/total4912 metadata and continued EXTRACT_API_CANDIDATES rejection. No response wrapping, flattening repair, missing-field insertion or relaxed decoding is introduced. Independent archive inspection confirms complete stopped JSON with flattened atom fields and no atom wrapper; actual statement/not_applicable values remain legal in the unchanged original schema.
- The ordinary CLI author oracle at tests/extraction_api.rs:123 checks the existing nested schema and prompt before asserting v4 strategy. scripts/validate_extraction_chat.py:20 and :132 update the independent version/prompt assertions without changing its transport/domain oracle. Their preserved RED proves the old prompt failed for the missing nesting phrase, rather than merely a renamed strategy.
- The .gitattributes diff is scoped to command/call/shape archive families and original binary response/ZIP files. It preserves actual recorded bytes and does not expand provider or runtime behavior. No new SDK, dependencies, lockfile, lib.rs or CLI interface change appears in the package.

## Actual retained checks

shape-fix-native-red.json: genuinely compiled exit1, 0 passed /1 failed because the old task lacked “two top-level keys: atom and measurement”. root-shape-cli-red.json: genuinely compiled exit101, 0 passed /2 failed for that same phrase. The source-before and source-patch fingerprints match the immutable fix artifacts.

shape-fix-native-green.json records 37 passed /0 failed, including the unchanged older API guards and new raw-third regression; CLI GREEN is 7 passed /0 failed and fake GREEN 3 passed /0 failed. Workspace check and owned formatting/diff checks exit0. GREEN Rust/compiler outputs have no Rust warnings; diff output contains only Windows Git LF/CRLF notices.

The retained v4 independent Chat prototype is 34 scenes /36 CLI /28 synthetic loopback POST, all passed, zero real/paid calls, tied to component program ce27b6e5144c8aa638b844d409989e5a58365f51380294648040ae868f84ac8d. Its script fingerprint matches current validate_extraction_chat.py. Retained Python results are 76 passed. None of these runs were rerun by this reviewer.

## Independent archive and call audit

shape-review-command.json records the successful reviewer audit exit0: **1124 checks /797 fingerprints**. Reproducible source is shape-review-audit.py; full evidence is shape-review-evidence.json.

- Verified all 18 records in B04-T02-chat-final-archive.json against current original bytes/length/SHA256. The manifest explicitly labels v3 and program07536109b727bb199213cb9f05a53988e1791de4d54f0b9ba4c1e4a6c4a78506.
- Inspected the ZIP without extraction: exactly710 unique files, each byte-identical to its explicitly named synthetic original. Independently checked all127 JSON/Markdown report snapshots and89 request-body/request fingerprint and strategy records. The historical workspace497 and these reports belong to v3/program07536109..., not current final v4 acceptance.
- The retained first staging failure remains intact. The successful controller guard reports71 staged files /57 index byte equalities /18 manifest records and preserved unstaged user deletions. This reviewer independently verified all57 current archive fingerprints;55 text archives were also reconstructed from the immutable package and matched exact bytes. The two binaries match their manifest/receipt hashes. No current Git/index command was executed.
- Original third response is exactly14489 bytes, SHA6ed4b15a330aeb8d552d8bd7ef2d684345427d662cbb54b2aae4b1a5732fb04b. Its raw usage, normalized usage, persisted validation and actual command stdout reconcile at4912. Validation remains failed/unpublished, zero validated atoms. The10305 reasoning bytes remain visible, so switching off thinking is not inferred from the request.
- The explicitly authorized repository profile/config/reservation/settlement snapshots establish the same origin/model/credential reference/cumulative ceiling, historical program and three attempts. Actual totals936+9683+4912=15531 reconcile with archived remaining9984469 and pending0. Active external profiles/ledger/credentials/ciphertext were never followed or opened.
- All five explicitly selected source files retain their before-call bytes/SHA/mtime, matching third-after and historical snapshot records. Author gold matches its before-call hash; original first/second response hashes also remain intact.

The audit’s earlier parser/key assumptions, initial strict archive-read guard stop and missing-final-newline reconstruction handling are preserved in shape-review-command-first/second/third/fifth.json. shape-review-command-fourth.json preserves the successful restricted-scope audit before the controller clarified authorization for non-secret repo snapshots. Corrections changed only reviewer audit artifacts; no test/suite, product evidence or source was changed.

## Limits and verdict

No concrete finding remains. Reviewer Cargo/builds/tests/ordinary CLI/network/model/paid calls/active profile or ledger/credential reads/Git mutations:0. No source/docs/task state, production data or user deletions were changed.

Current final v4 whole-workspace validation, expected498 tests, final executable binding/matrices and actual04 have not run in this review. The v4 prototype and historical v3 final evidence do not prove those gates, deployed thinking-toggle behavior, bottom model identity, live valid candidates, scientific quality or prices. T02 is still in_progress and T03 todo according to the permitted status/plan evidence; the ledger was not read or modified. This narrow PASS is sufficient to proceed to the controller’s prepared final checks and then the single separately bounded request.
