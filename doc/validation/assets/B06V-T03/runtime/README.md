# B06V-T03 official versions and isolated Windows runtime preparation

The official stable desktop version observed on **2026-11-06** is **Obsidian
1.15.0**, published at **2026-11-02T15:39:05Z**. This is supported by the
[official latest release API](https://api.github.com/repos/obsidianmd/obsidian-releases/releases/latest),
the [official stable desktop channel](https://raw.githubusercontent.com/obsidianmd/obsidian-releases/master/desktop-releases.json),
the [official download page](https://obsidian.md/download), and the
[desktop 1.15.0 changelog](https://obsidian.md/changelog/2026-11-02-desktop-v1.15.0/).
The channel lists 1.15.1 separately as beta. `web.run` returned a backend 404;
the exact primary official URLs were subsequently fetched with `urllib`, with
original bytes, response headers, source URL, final URL, retrieval time and
SHA256 saved in this directory. This is a preparation/provenance record, not a
claim that the current-version display matrix has already passed.

[Official version/source lock](official-version-source-lock.json) binds the
source snapshots, download/extraction receipts and observed runtime files.
[Source receipts](official-source-receipts.json) describe the stable channel,
GitHub API, download page and changelog index; the selected release's own
[changelog receipt](official-desktop-1.15.0-changelog.receipt.json) is separate.
[Minimum official source receipts](official-minimum-source-receipts.json)
preserve the official 1.6.7 release API and changelog as well. All raw `.bin`
responses retain their original bytes.

The Noetome baseline selected for the lowest tested version is **1.6.7**. Its
existing accepted compiler gates and standards are preserved with exact source
hashes and selected lines in [minimum support basis](minimum-support-basis.json).
Formula and scan gates use 1.6.7 / MathJax 3.2.2, process gates use 1.6.7 /
Mermaid 10.6.1, and image SVG gates use 1.6.7. These are the existing bounded
accepted combinations; the new task still needs its own representative matrix
for both selected versions. The upstream stable-channel `minimumVersion=1.1.9`
is an updater requirement, not the minimum supported Noetome output version.

The explicitly selected existing minimum runtime was only read:

| File | Bytes | SHA256 |
| --- | --- | --- |
| `C:\Program Files\Obsidian\Obsidian.exe` | 176,761,056 | `2156f15d12da8cc292fe0cc1884f32410fa187fede67beb6102cfcdc6442fbe2` |
| `resources/app.asar` | 1,172,522 | `4e6cab84d7d1ff5f63e6cb8fe742e99a8522cf4ee0d3e6c22495fb49928d2ad0` |
| `resources/obsidian.asar` | 22,737,197 | `ec2f038fc2f6ba59764bd737461afa21b5b32e5726b4922fdf1e2fd7c3b5cb8c` |

Both actual ASAR `package.json` members report **1.6.7**. Their original bytes
are saved as `minimum-*-package.json`. The selected three files were hashed
again after preparation and remained unchanged, as recorded in
[minimum post-preparation check](minimum-after-extraction-unchanged.json).
This work did not change the daily install, launch it, register protocols, or
read any daily Vault. New profiles are the root executor's later step.

The current stable installer and update package were downloaded to a newly
created owned directory:

```text
D:\Noetome\.artifacts\b06v-t03-runtime-20261106\
/mnt/d/Noetome/.artifacts/b06v-t03-runtime-20261106/
```

[Actual download receipts](official-download-receipts.json) show HTTP 200 and
successful exact comparison with both official GitHub asset size and digest:

| Official asset | Bytes | SHA256 |
| --- | --- | --- |
| `Obsidian-1.15.0.exe` | 339,587,200 | `b0f60c75a12f96d54cb16b9fc32919966f2ebf029d9af75d67a19ba6555e1da0` |
| `obsidian-1.15.0.asar.gz` | 8,528,283 | `011c70f176e44f0b215597cf0d420bae424be73583f2cb0205d3e63a26a0aa40` |

The updater gzip SHA also exactly equals the base64-decoded `hash` in the
official stable-channel JSON. No cryptographic verification of the separate
channel signature was performed or claimed.

7-Zip 26.03 was used as an archive reader. The installer executable was not
executed. The selected NSIS `$PLUGINSDIR\app-64.7z` payload was extracted to a
new directory, then unpacked to `win-x64`. Complete commands, exits, original
stdout/stderr bytes and fingerprints are in
[extraction receipts](extraction-commands.json). The extracted runtime has
**82 files, 402,876,476 bytes**, with a complete member inventory in
[stable runtime preparation](stable-extracted-runtime.json). The launcher is
actually PE x86_64 (`machine=0x8664`). Both actual ASAR `package.json` members
report **1.15.0**.

| Extracted file | Bytes | SHA256 |
| --- | --- | --- |
| `win-x64/Obsidian.exe` | 239,209,744 | `7cf93dfb355d834eb1868e91cc55927199dbf3673d28526e688e38232f93c4ea` |
| `win-x64/resources/app.asar` | 73,446 | `53cf8fd4a59718bb941db2e4b82eedc775f3e3ae38e978d9692cb0eab5ae65eb` |
| `win-x64/resources/obsidian.asar` | 24,553,884 | `0e6383301078a798f0f3bdb42bdb606d904ada77eab8c8fc89da533573979248` |

The installer's extracted `obsidian.asar` is byte-for-byte identical to the
decompressed official stable updater. Thus the prepared current-version app
and launcher come from the complete current installer; the preparation does
not pair a new app bundle with the old daily Electron launcher.

The root executor can launch this exact runtime later with a newly owned
profile and selected temporary Vault:

```text
D:\Noetome\.artifacts\b06v-t03-runtime-20261106\win-x64\Obsidian.exe
    --user-data-dir=<new owned task profile>
    --remote-debugging-port=<explicit owned loopback port>
    --remote-debugging-address=127.0.0.1
```

This preparation has launched no app. Actual `app.getVersion()`, Electron and
Chromium identity, selected user-data/Vault paths, loaded diagram/math resources,
semantic rendering and cleanup must be observed from those later live captures.

The collector-compatible normalized records are
[minimum prepared runtime](minimum-runtime-prepared.json) and
[current prepared runtime](current-runtime-prepared.json), with their independent
SHA anchors in [prepared runtime anchors](prepared-runtime-anchors.json). Their
`status=prepared_not_launched` and null actual engine-version fields deliberately
retain that boundary. The root executor should save later actual observed
runtime records and their new anchors separately, preserving these preparation
originals. Executable and ASAR references carry both explicit Windows and WSL
paths, while library-member hashes identify the actual selected ASAR contents.

The current version also has a material diagram-engine change. The official
changelog states that Obsidian now renders Mermaid syntax with **Pele**, and
does not bundle the old Mermaid implementation. Static inspection of the
actual `app.js` member confirms `/lib/pele.min.js` loading and the
`loadMermaid()` / `window.mermaid` compatibility interface. The saved member
pins are in [selected stable bundle members](stable-selected-bundle-members.json).
`lib/pele.min.js` has a **Pele 1.8.0** banner; this identifies its saved bytes,
not an actual Mermaid runtime version. The official stated Mermaid 12.1.0
syntax compatibility is likewise not a version observation of the old Mermaid
runtime. The current compatibility record must preserve the engine identity,
actual loaded member SHA and application-level semantic checks rather than
inventing `window.mermaid.version`.

The old selected math and diagram members are
`lib/mathjax/tex-chtml-full.js` and `lib/mermaid.min.js`. The new selected math
member is `lib/mathjax/obsidian-mathjax.js`. Its statically observed core
`VERSION=5.0.0` differs from the auxiliary `mathjaxVersion=4.1.2`; the auxiliary
string is not a valid substitute for the actual runtime probe. Member inventories
and hashes are preserved separately for both versions. No engine is considered
native-compatible by these strings alone: actual formula and diagram semantics
remain the next matrix gate.
