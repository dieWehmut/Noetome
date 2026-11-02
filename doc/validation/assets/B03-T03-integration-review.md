# B03-T03 fresh integration review

Result: no additional actionable P1, P2, or Important findings in the coupled registry implementation reviewed.

Scope is the immutable 20-file package at D:\Noetome\.artifacts\b03-t03-registry-review, against base 058a8b353dd41cff7840a2995dedde032a41db9a. The manifest SHA256 is 66eb4a26f47681098cb6ceffc03dec7a6cf26ef7892c2f42248a4d49f2a65277. Both beginning and final independent hash checks match every file SHA/length, with no extra or missing files. No production source, frozen package, ledger, or Git state was edited.

The review read AGENTS.md, the current B03-T03 task/acceptance and plan, frozen PDF_SOURCE_CONTRACT.md, relevant PRD FR-01/02/06/35/40, and testing/data/design standards. The source_model cache-root timing probe was deliberately left to the separate assigned model review. This result covers the integration and code-quality review; that separate probe's closure remains part of the parent's final acceptance.

## Spec compliance

- Source identity, original content Version, parse attempt, and import identity remain separate. Every invocation produces a new import UUID; global ordering uses both import tables' positive rowids. Source path association, raw SHA reuse, explicit Source isolation, contiguous mixed-format revisions, legacy MD/TXT media retention, and PDF/UTF8 contradictory interpretation rejection are reflected in source.rs and pdf_source.rs.
- Schema1 initialization remains separate; text import retains Schema2 semantics and PDF import explicitly upgrades a copied candidate to Schema3. source_schema.rs retains old rows/rowids and uses the known ten-table shape. Writer preserves originals and backups, publishes source/parse trees by exclusive copy, publishes the database/config pair, and performs a full persisted read before successful cleanup.
- The default cache accepts only validated parsed/needs_review attempts for the same Version and complete cache key. Failed attempts are retried. --reparse skips reuse and creates a fresh attempt with a distinct options version, preserving the old Version and snapshot location.
- Incomplete acquisition has no Version or parse identity. Complete raw acquisition with parser/runtime failure preserves a failed attempt and truthful unconfirmed/diagnostic resources. PDF failed/needs_review output maps to CLI exit 3; knowledge remains pending. Legacy text report schema1 and legacy inspect behavior are retained.
- Full original page JSON numbers are preserved with RawValue in the same pinned serde_json 1.0.154 dependency. Native page/region/resource persistence is independently rechecked from bytes and SQL, and the inspector routes through the existing Core semantic audit.

## Code quality and failure paths

- sidecar_process.rs:50-217 retains Prepared in RunSession on transport and validation failures. pdf_parse.rs:123-254 retains failed RunSession or successful ValidatedResponse through Core processing and capture; capture acquires replacement Dir/File capabilities while the original owners remain alive.
- pdf_workspace.rs:103-218 admits only bounded regular files with fixed native names, rejects unknown or unowned entries, links and hardlinks, and rechecks the original owners and captured inventory. Unsafe inner initialization preserves the scene instead of adopting a path after the owner has expired.
- source_write.rs:596-681 transfers the actual CapturedTree to Writer, verifies it, prunes only proven empty parser/pages or parser directories using guarded nonrecursive cleanup, and preserves the strict readonly inventory. PDF raw and parser publication creates exclusive copies; resources retain nlink=1.
- Writer integrity checks track original/staged file identity and bytes, exact directories, root inventory, both cache owners/inventories, and the live parse receipt. Known aborts restore old files and remove owned additions; changed/unknown/locked scenes retain marker/backups and return incomplete. Successful cleanup begins only after the published pair and all persisted records/snapshots are reread.
- The inspected tests have independent byte/SQL/fixture oracles, actual stage observations and process handshakes. Their negative control deliberately removes Writer::abort restoration/cleanup and produces a real failing assertion. No production fault/provider switches were added.

## Fresh verification performed in this review

Both scripts use bundled Python 3.12.14 at C:\Users\Lyinn\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe.

- manifest_check.py: exit 0; exact 20-file immutable package verified before and after review.
- snapshot_audit.py: exit 0; uses SQLite mode=ro&immutable=1, Python's standard library, actual file bytes/metadata, and independent JSON number lexemes. It checks twelve preserved ordinary verifier projects: 24 SourceVersions, 27 parses, 43 imports, 27 complete page JSONs, 127 resource SHA/length/nlink values, exact parse directory/file inventories, cache-key preimages, four complete old SQL tables, and honest acquisition/host failure evidence. All 202 project files retain bytes, mtime and inventory.
- The archived verifier receipt is checked structurally: 31 passed checks and 159 actual recorded CLI calls; 85 PDF report status/exit-code pairs match. Five archived process-kill facts have distinct expected stages and record alive-before-kill, reaped exit, old history and readonly preservation.

Two initial local audit-script errors are preserved: missing sqlite3 uri=True, then missing the old normalization rule's first-BOM removal. The existing data contract at DATA_CONTRACTS.md:130 and frozen old fixture distinguish that normalization rule. Both corrections affect only this audit script. Neither error is a product RED.

## Existing evidence read and limits

Read the archived 18 ordinary CLI gates plus the nineteenth 17 MiB boundary gate, the independent 31-check/159-call receipt, four Writer fault groups / sixteen scenes, five actual PID-kill facts, and the same-module positive / isolated abort-mutant RED records.

Those records are prior executions. This reviewer did not launch a fresh CLI/parser, Rust compilation, or process termination, and does not claim a fresh full-workspace test run. The parent owns the final current-source Cargo/Python/log checks, standards/ledger updates and Git delivery. Earlier Writer fault receipts record earlier Writer hashes; this review separately inspects the final frozen Writer and relies on the parent's final regressions to bind final source to runtime evidence.

No arbitrary power-loss, 1000-page throughput, 24-hour stability, cross-platform execution, model/OCR/scientific correctness, production Vault behavior, or live parser upgrade compatibility was tested here.

## Retained audit artifacts

All local audit artifacts are in this new owned Low-inherited directory:

D:\Noetome\target\compiler-temp\b03-t03-integration-review-2d295310-5b53-49d1-904d-f4d978c6441a

- REVIEW.md
- manifest_check.py and manifest-verification.json (exit 0)
- snapshot_audit.py and snapshot-audit.json (exit 0)
- snapshot_audit_initial.py and snapshot-audit-initial.json (exit 1)
- snapshot_audit_uri_fixed.py and snapshot-audit-uri-fixed.json (exit 1)
- integrity.log (initial icacls relabel attempt denied with exit 5; subsequent readonly ACL output confirmed inherited Low Mandatory Level and no ACL change was made)

