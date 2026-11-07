# fixtures/source-index-v1

自造的可移植来源索引（portable source index）作者包，用于验证「把输出区复制到别的机器之后，来源描述与定位还在，指纹还能不能核回来」。

作者包、原始资料与期望表都是为本项目手写的内容；这里没有任何真实教材、真实笔记、真实 Obsidian 库或真实文献。

## 成员

| 成员 | 字节 | 作用 |
| --- | --- | --- |
| `project.json` | 1621 | 工程声明：输出区 `vault`、运行库 `runtime`、临时区 `temp`、重定位目录 `moved`、2 份笔记、1 份资源、3 条来源 |
| `project-citation.json` | 916 | 变体：输出区换成 `variants/`，笔记正文里带 `[1]` 编号 |
| `project-leak.json` | 1623 | 变体：把 `note-gluconeogenesis` 的路径挪到 `runtime/笔记/糖异生.md` |
| `project-bad-heading.json` | 1627 | 变体：`src-textbook-a` 声明的章节写成一个不存在的标题 |
| `vault/笔记/糖酵解.md` | 386 | 输出区笔记：H1 + 3 个 H2，无编号、无参考文献小节 |
| `vault/笔记/糖异生.md` | 269 | 输出区笔记：H1 + 2 个 H2 |
| `vault/资源/术语表.md` | 157 | 输出区资源：术语表，不建立来源映射 |
| `variants/糖酵解-带编号.md` | 216 | 只被 `project-citation.json` 引用的带编号笔记 |
| `runtime/cache/manifest.json` | 31 | 运行库缓存：不进索引，只记摘要 |
| `runtime/session.sqlite` | 44 | 运行库状态文件：不进索引，只记摘要 |
| `runtime/笔记/糖异生.md` | 73 | 运行库里同名笔记：用来证明区域混用会整包拒绝 |
| `temp/upload.part` | 21 | 临时文件：不进索引，只记摘要 |
| `资料/教材A-糖代谢.md` | 100 | 原始资料 A：输出区之外的来源 |
| `资料/教材B-酶学.md` | 89 | 原始资料 B |
| `资料/讲义-糖代谢.md` | 113 | 原始资料 C |
| `moved/a/教材A-糖代谢.md` | 100 | 重定位目录：教材A 的逐字节副本（内容摘要一致） |
| `moved/b/教材B-酶学.md` | 89 | 重定位目录：教材B 的逐字节副本 |
| `moved/c/讲义-糖代谢.md` | 113 | 重定位目录：讲义 的逐字节副本 |
| `moved-partial/教材A-糖代谢.md` | 100 | 现场核对用：教材A 副本仍在 |
| `moved-partial/教材B-酶学.md` | 64 | 现场核对用：教材B 被改写 → 同名不同摘要 |
| `moved-partial/讲义-糖代谢.md` | 113 | 现场核对用：讲义 副本仍在 |
| `moved-foreign/无关材料.txt` | 28 | 现场核对用：只有无关文件，三份来源都找不到 |
| `expected.json` | 6043 | **实现之前手写**的索引期望表（含 13 项计数、3 份文件摘要、2 份笔记标题、3 条来源映射、3 条拒绝） |
| `cases.json` | 2333 | 端到端复核序列：adopt → build → list → status |

## 先验推导

`expected.json` 写在实现之前，按下面的规则一条条算出来，不是产品输出的转录。

1. 输出区是 `vault/`，运行库是 `runtime/`，临时区是 `temp/`，重定位目录是 `moved/`；四者互不包含（`vault2/x` 不会因为前缀像 `vault` 就被算进输出区，判据按路径组件比较）。
2. `output_files = 3`：输出区里被声明的文件只有 2 份笔记与 1 份资源；`variants/` 里的带编号笔记不属于这份工程声明。
3. `headings = 7`：糖酵解 4 个标题（H1 + 3 个 H2）、糖异生 3 个（H1 + 2 个 H2），合计 7；资源不计入标题数。
4. `excluded_runtime_entries = 3`（`runtime/cache/manifest.json`、`runtime/session.sqlite`、`runtime/笔记/糖异生.md`）、`excluded_temp_entries = 1`（`temp/upload.part`）：运行库与临时文件**留在原地**，索引只记摘要，不搬运、不复制。
5. `citation_marks = 0`：两份输出区笔记都没有 `[n]` 形式的编号，也没有「参考文献」「引用文献」小节；带编号的变体一旦把正文放进输出区就整包拒绝。
6. `absolute_paths = 0`、`secret_markers = 0`：所有路径都是相对路径、没有盘符；正文、来源描述与定位里没有 `api_key`/`password`/`secret`/`token=`/`authorization:` 之类字样。
7. 重定位按内容摘要判定：`moved/` 三份是逐字节副本，全部 `relocated`；`moved-partial/` 里教材B 被改写，同名但摘要不同 → `digest_mismatch`（"同名不能证明是同一份来源，需要重建映射"），另两份 `relocated`；`moved-foreign/` 只有无关文件 → 三份都 `missing`。
8. `missing` 与 `digest_mismatch` 都归为「需重建映射」，不指向任何页面：索引只保存来源描述与定位，不猜测新位置。`ad_hoc_relocations` 里 `mismatched` 只列 `digest_mismatch` 的来源，`missing` 已经由 `missing` 计数表达。
9. `relocation` 只对工程声明的 `relocate_area`（`moved/`）给结论；`moved-partial/` 与 `moved-foreign/` 由 `index relocate` 现场核对，结果单列在 `ad_hoc_relocations`，不参与 `counts`。
10. 三条变体各只改动一处，用来证明对应规则会整包拒绝：正文带编号 → `INDEX_CITATION_BURDEN`；笔记落在运行库 → `INDEX_AREA_LEAK`；声明的章节不存在 → `INDEX_LOCATION_MISSING`。三条都 `exit=3`，且拒绝之后不留下任何索引根。

## 冻结规则

`frozen-sha256.json` 由 `scripts/verify_source_index.py --freeze --package fixtures/source-index-v1` 生成。
它是审计器的自检基线：重跑 `--freeze` 必须写出完全相同的字节，任何成员被改动都必须被 `check_frozen` 发现。
包内每个成员都由本文件的手工内容构成，不存在生成脚本。

## 边界

- `scientific_status = not_run`：包里没有真实教材、没有真实 Obsidian 库、没有真实文献；只证明索引导出、区域分离与重定位语义，不证明真实教材追溯正确率。
- `uses_network = false`、`uses_model = false`：冻结包与产品流程都不联网、不调用模型，`model_calls` 与 `network_requests` 恒为 0。
- 只覆盖「导出可移植索引 + 按摘要重定位」这一条路径，不覆盖 PDF/DOCX 直入、增量重建索引、跨机器实际复制、多来源冲突合并或正文渲染。
- 索引不判断内容质量与科学正确性：它只记录来源身份（内容摘要）、描述与定位，说明哪一段正文来自哪份资料。
