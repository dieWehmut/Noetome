# B03-T03 独立 PDF 来源验收

使用普通 Noetome CLI、Python 标准库 SQLite 只读查询及真实文件字节。正文、页和坐标预期来自事先冻结的 authoring fixture；不调用产品校验器生成预期。

| 实际验收 | 结果 |
| --- | --- |
| Schema2 history, all old identities/SHA/rows/bytes and Schema3 text semantics | passed |
| Schema1 real import, repeat/rename/revision/back and two forced fresh parses | passed |
| Explicit Source isolation for identical raw SHA | passed |
| Honest parser boundary/retry: damaged.pdf | passed |
| Honest parser boundary/retry: encrypted.pdf | passed |
| Honest parser boundary/retry: partial-page-failure.pdf | passed |
| Honest parser boundary/retry: scanned-page.pdf | passed |
| Honest parser boundary/retry: table-page.pdf | passed |
| Unread acquisition creates no fake Version: missing | passed |
| Unread acquisition creates no fake Version: too-large | passed |
| Unread acquisition creates no fake Version: windows-locked | passed |
| Complete acquisition survives missing runtime, valid recovery and moved input | passed |
| Integrity rejection preserves scene: raw | passed |
| Integrity rejection preserves scene: resource-index | passed |
| Integrity rejection preserves scene: page-json | passed |
| Integrity rejection preserves scene: region-index | passed |
| Integrity rejection preserves scene: wrong-bbox | passed |
| Integrity rejection preserves scene: wrong-geometry | passed |
| Integrity rejection preserves scene: cut-utf8 | passed |
| Integrity rejection preserves scene: changed-body | passed |
| Integrity rejection preserves scene: unknown-file | passed |
| Integrity rejection preserves scene: unknown-parse | passed |
| Integrity rejection preserves scene: unknown-source-version | passed |
| Integrity rejection preserves scene: unknown-table | passed |
| Integrity rejection preserves scene: duplicate-rowid | passed |
| Integrity rejection preserves scene: duplicate-import-id | passed |
| Integrity rejection preserves scene: schema-mismatch | passed |
| Integrity rejection preserves scene: unknown-schema | passed |
| Actual unchanged-byte Windows hardlink rejection | passed |
| Actual Windows cache junction rejection preserves target | passed |
| Actual Windows database publication lock preserves old bytes | passed |

实际 CLI 调用 159 次，检查 31 项。JSON 保存每次参数、退出码、stdout/stderr、实际耗时和独立 SQL/字节证据。

未运行：arbitrary power-loss recovery、1000-page throughput、24-hour stability、cross-platform execution、real model extraction、OCR、knowledge/scientific correctness、production Vault write。
