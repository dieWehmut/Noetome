# B06V-T02 independent author and API audit evidence

One self-authored CC0 formula is represented by three scan states. The original
33-file author freeze remains `99789373318a9e2f93c5b8449e471567e58f448754e59f757164b7ad18ab4b54`.
The selected page/crop/PDF/handwritten context and expected data were frozen
before any recognition. See `author-freeze-audit.json`, the fixture README,
and the separately preserved old-CLI RED receipts. Expected files are outside
case input directories and are never included in model requests.

The active independent checker is `scripts/verify_scan_formula.py`. It imports
independent lexical/PNG/guard helpers; it does not import the product parser or
renderer as an oracle. It independently checks actual page/crop pixels, frozen
manual source semantics, the complete Chat request, original HTTP response,
strict normalized recognition, raw/normalized token usage, complete report
Markdown/JSON, output inventory, successful plan and literal note adjacency and
UTF-8 maps. Single-operand script grouping and dfrac compatibility are finite
rules; signs, variables, fraction order, bars and unknown macros are preserved
or rejected.

| Evidence | Actual result | Scope |
| --- | --- | --- |
| `local-policy-final/` | 28 unit checks passed | Sealed superseded local policy, not the active provider |
| `api-checker-first-red/` | 34 checks, one error and one failure | Preserved checker implementation errors; no CLI/model calls |
| `api-cli-preparation-red/` | Rejected before starting CLI | Cargo multiple-link artifact; zero calls |
| `api-cli-first-run/` | 31 ordinary CLI commands, 21 artificial HTTP POSTs, 20 mutations passed | First bounded API audit, same final binary SHA |
| `api-cli-final-01/` | 31 ordinary CLI commands, 21 artificial HTTP POSTs, 20 mutations passed | Final explicitly selected immutable binary |
| `api-cli-final-01-launch/` | exit 0 and source pins | Outer final audit command/stdio receipt |
| `api-transport-review-01/` | Two ordinary CLI commands and two client network attempts passed | Actual timeout and refused connection; one POST reached the local server |
| `api-checker-final-01/` | 41 current unit checks, zero skips | Synthetic responses and original-ledger attack fixtures; no model/network calls |

The final large matrix contains five author target presentations, twenty
supplemental artificial HTTP cases and six separate input faults. It includes
clear native reconstruction, original-image fallback, unavailable targets,
unknown sign/subscript blocking, semantic errors, strict wire/usage failures,
HTTP and resource limits, credential-echo omission and zero-request budget
stops. Its eighteen synchronized mutations and two unknown false-success
mutations refresh ordinary source/response/plan/report/mapping fingerprints;
all are rejected by independent meaning, pixels or literal association checks.

There were **64 actual ordinary CLI commands across both API matrices and the
two transport supplements**, **44 client network attempts**, **43 POSTs seen
by explicitly artificial numeric-loopback servers**, and **zero real or paid
model calls** in this independent API evidence. Historical local-model failures
and old-CLI RED calls are separate records. These counts do not prove real
visual recognition, a model success rate, scientific correctness or production
Vault behavior.

The immutable selected binary has SHA
`3fd4d9ad4995abea395f22443becd9334930763b2c1f3647d9d1c74fb54eee96`,
150787824 bytes. It is preserved in `.artifacts/b06v-t02-scan-api-final-01/`;
build products are not embedded in source evidence. The first audit selected
a same-byte single-link copy; its receipt and the later byte-preservation
receipt retain its identity. Existing source images, failed outputs and
receipts were preserved.

Read-only validation of captured outputs uses five explicit selected paths:

```text
python scripts/verify_scan_formula.py --case <case> --api-config <config> --profile <profile> --vault <capturedVault> --expected <frozenExpected>
```

`--cli <binary> --output <brand-new-root>` performs only the explicitly
artificial numeric-loopback audit. It never runs live calls. Actual live
commands must be separately authorized, prepared, invoked once and captured;
this checker reads their preserved outputs without repeating paid inference.

For cumulative budget evidence, `--budget-binding <manifest>` additionally
requires `--budget-binding-sha256 <independently-captured-SHA>`. The manifest
uses schema 1, producer `independent_scan_budget_originals`, one invocation ID,
four `receipts` references and explicit `copies` indexed by original recorded
absolute paths. Each local reference has `{file,sha256,byte_length}`. Windows
source references are compared lexically; only the explicitly copied local
files are read. The checker verifies the original before/reserved/launch/
settled ledger arithmetic and history, actual selected binary/config/profile,
ordinary child output/exit, response fingerprint and actual usage. Helper
reservation is the selected total cap; Core byte-based reservation may be
smaller. Unknown actual usage retains the helper cap. Normalized manual balances
and rewritten pin chains cannot substitute for independently anchored originals.

Without those actual live receipts, cumulative live budget and real recognition
remain `not_run`. The separately frozen public `authorized-stop.json` contains
no credential and is not a restored balance or current private settings file.
