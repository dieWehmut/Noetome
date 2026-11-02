# B03-T03 immutable model v3 independent re-review

Result: both previously confirmed P2 findings are closed on the exact v3 fingerprint. No new Important/P1/P2 finding was identified within this model re-review.

Reviewed source_model SHA256: 77f7841e1b141af76719bfff29eb649c8d02eac3c4f80d9e1f94b474f653e31c
Unchanged PDF source model SHA256: da7020d5c70c3cc134ba5511e3454b8482ae7332d44d0393891b0a3a3a98df45
Unchanged PDF model tests SHA256: c72cbddd210fc0db99310bf4ca44fbd14a692f024b1d5f889500264fd9c6a9d5
Unchanged source cache race tests SHA256: 45dc7f4309f445567ef72ffa75cfb1d1bd56c2f639bd36048513610fd995c3e0

## Boundary findings and evidence

1. Source cache receipt lifetime/inventory/absence P2:
The v1 reader checked the source cache root/owner once before long snapshot reads. Four probes synchronized by a real Windows raw-file sharing violation demonstrated that an added unknown entry, newly created formerly absent cache root, modified legacy-only owner bytes, and replaced legacy-only owner identity escaped the current read. The subsequent read rejected the persisted inventory/absence change. These four genuine assertion failures were retained as exit 101 in prior-evidence/v1-races/source-root-race-red.stdout.txt and .stderr.txt.

v3 source_model.rs:310-338 retains Directory and GuardedFile through all snapshot reads regardless of has_pdf, including Schema3 with only unacquired PDF failure. Lines 374-394 verify owner bytes/identity, directory identity, exact inventory, and initially absent root state at the end. The frozen four persistent race cases and independent paired Version+Parse staging case passed again on v3. The two owner mutation attempts were genuinely blocked by sharing violations; the inventory/absence mutations were successfully applied and rejected by both current and later reads. A blocked mutation alone is not represented as a successful post-mutation read. The independent paired probe preserves the successfully injected unknown sentinel.

2. Legacy text whole-history deadline P2:
v2 used the original 30-second Control for final retained-owner verification even when no acquired PDF existed. A legal Schema3 history with one source, twelve distinct 64 MiB UTF-8 versions, correct complete spans, separate raw/normalized regular files, and one unacquired PDF acquisition failure loaded successfully on all versions. There was no sleep, injected slowdown, production hook, or fault flag.

Actual immutable production runs of the same authored raw/normalized content:
- v1 source SHA f445fb4ba6dd62527428d1403d2827cccef5a8e1a0a6d2516c97a0d5ecbcd1b2: snapshot validation Ok after 42.080302 seconds; test exit 0.
- v2 source SHA de0983ed9064a5a6a58b5d3c0af7f11de291d0403777fc08deb03d0af21cca25: snapshot validation SOURCE_INVALID_SNAPSHOT / SIDECAR_TIMEOUT after 42.4671619 seconds; test exit 101.
- v3 source SHA 77f7841e1b141af76719bfff29eb649c8d02eac3c4f80d9e1f94b474f653e31c: snapshot validation Ok after 42.8708129 seconds; test exit 0.

v3 source_model.rs:365-384 only creates a fresh bounded Control for the two final owner verifications when has_pdf=false. has_pdf=true still uses the original shared Control, and PDF raw/directory checks at lines 356-364 are unchanged. Receipt handles still remain alive during the long history read. The v2-to-v3 production diff contains only this Control selection and explanatory comments.

All three history runs generated the same twelve authored byte payloads. Per-run SourceVersion/Span/Import UUIDs varied and Store::load validated them. Thus this is the same authored content and graph shape, not a claim of identical per-run SQL UUID values. Payload length and SHA256 for each revision are retained in legacy-budget-authored-input-fingerprints-v1-v2-v3.json.

## Fresh v3 execution

audit-v3.py rebuilt an independent direct-rustc test executable from the tracked base 058a8b353dd41cff7840a2995dedde032a41db9a, exact frozen model files, baseline native T02 snapshot validation, the minimal existing baseline harness coupling, and two explicitly separate extra probes.
- direct-rustc-v3: exit 0, 8.250 seconds.
- frozen-v3-model-33: exit 0, 33 passed / 0 failed, harness 18.01 seconds, process 18.078 seconds.
- legacy-budget-v3: exit 0, 1 passed / 0 failed, harness 55.50 seconds including authored fixture creation and cleanup; actual snapshot validation 42.8708129 seconds.

Every executed command, argv, exit, elapsed time, and complete raw stdout/stderr is retained. input-hashes.json records all four frozen production/test fingerprints, harness coupling files, the extra probe SHA256 values, and selected cached dependency rlib SHA256 values. inputs/v1, inputs/v2, inputs/v3 preserve exact frozen source files and available manifests. prior-evidence retains previous commands, compiler output, genuine RED and GREEN logs, probes, and authored-input fingerprints. launcher-tool-output.txt concatenates the complete launcher tool output chunks; launcher-exit.txt records actual launcher exit 0.

## Review scope and limits

Read-only repository review. No Cargo invocation, repository modification, commit, user data scan, or product stage execution occurred. Temporary code/harness/output writes remained under this independent audit directory.

The 33-case matrix comprises the existing seven UTF-8 model cases, twenty-one PDF model cases, four persistent cache-boundary race cases, and one independent paired-staging case. The separate legal history test makes 34 selected behavioral cases in this v3 review.

Earlier scoped review of the immutable model found no additional P1/P2 in required nullable DTO serialization, canonical parser/options cache key, the five PDF tables and unified import history, SQL-bound report hydration, or existing strict raw/native snapshot semantics. v3 changed only the owner Control branch; the other three frozen files exactly match v2.

This standalone debug rustc harness uses cached dependency rlibs and --cap-lints allow. It does not replace the root agent's final Cargo workspace/CLI integration, formatting, or warning checks. Windows deny-write behavior was directly exercised; cross-platform handle semantics were not executed. The authored large text history proves this particular compatibility regression and recovery, not a general performance benchmark. Synthetic model fixtures and parser-key variants do not prove alternate parser/backend compatibility, OCR/scientific extraction quality, production PDF accuracy, or later product phases.

