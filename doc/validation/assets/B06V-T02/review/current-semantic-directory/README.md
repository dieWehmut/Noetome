# Current independent scan inventory review

The independent checker previously ignored ordinary empty directories while
collecting files recursively. Adding an unexpected empty directory or a nested
empty directory to a selected case or a correct synthetic Vault still produced
`passed`. This contradicted the flat, exact case and Vault inventory contract;
the ordinary product CLI already rejects such extra directory entries.

`red-probe.py`, `red-stdout.json`, `red-stderr.bin` and `red-receipt.json` preserve
the four actual checker acceptances on temporary copies before the fix. The
probe deliberately exits 1 because the required rejections were missing.
`source-before.json` records the then-current checker and test SHA/length.
The source fingerprints remained unchanged during that probe.

Four focused tests were added before changing the checker. The preserved
`red-tests-*` run executes 46 tests and reports four failed directory subcases
and one error for the then-missing flat-inventory option. Previous correct
behaviors, including legal nested audit files, still passed.

The fix adds explicit flat mode to inventory reads for selected cases and
saved Vaults. It enumerates their direct entries, preserves the existing
ancestor/link guards, and rejects directories without traversing their
contents. General audit archives retain recursive inventory mode.

`green-receipt.json` binds these actual checks to unchanged final sources:

| Check | Observed result |
| --- | --- |
| Current `test_verify_scan_formula.py` | 46 tests passed, zero skips |
| Independent original fixture verification | One author, three scan states, all 33 frozen files passed |
| `git diff --check` | Exit 0 |

The directory regression covers ordinary and nested unknown empty directories
in both selected cases and saved Vaults, linked ancestors rejected before
listing, legal nested archive files, and the existing valid native/image
presentations and synchronized semantic mutation tests.

`actual-cli-after-fix-receipt.json`, the corresponding original stdout/stderr,
and `actual-cli-after-fix/summary.json` preserve a complete new ordinary CLI
audit after the fix: 31 commands, 21 explicitly artificial loopback POSTs,
five author presentations, twenty supplemental HTTP cases, six input faults
and twenty synchronized mutations, all passed. The nested audit archive was
read successfully. CLI SHA remained
`3fd4d9ad4995abea395f22443becd9334930763b2c1f3647d9d1c74fb54eee96`.
The checker/tests and binary fingerprints were unchanged during this run.

Final source fingerprints are in `source-final.json`:

- Checker SHA256: `943678780a8212a4569b972eb83d75ff5e542d1347344981293bc791ce039ab6`.
- Test SHA256: `b2b483a3b3a85010fd3cd2e928a1f06880fb45fd4fb7b4e0de60bb08a1cccc3a`.

Only the checker/tests and this review evidence were changed by this review.
Author images, source declarations, expected files and the original 33-file
freeze remain unchanged. These are engineering checks with zero live or paid
model calls; real visual recognition, cumulative live budget and actual
Obsidian display remain separate required acceptance evidence.
