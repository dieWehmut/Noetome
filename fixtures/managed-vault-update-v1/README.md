# managed-vault-update-v1

B07-T03 的冻结作者包。它固定两篇笔记、一个 PNG、一条链接和八类候选操作；Core 只读取，
不从 `cases.json` 或 `expected` 生成任何产物。

- `author.json`：托管集合声明。`notes` 两篇、`resources` 一个 PNG、`links` 一条，
  每条 baseline 都带独立 `sha256` 与 `byte_length`；`operations` 是允许的五种动词。
- `baseline/`：adopt 时逐字节复制的初始基准。
- `candidates/<case>/`：候选字节加严格 `operations.json`。
- `cases.json`：独立检查器使用的八类预期（批准/拒绝及拒绝码）。
- `foreign/`：不属于托管集合的陌生 Vault（含 `.obsidian` 配置），只能只读。
- `originals/`：字面内容与生成器 `generate.py`、`generated-manifest.json`。

`expected` 和 `cases.json` 只服务独立核验；Core 的判定不读它们。
