# 章节粗对齐与全局概念清单契约（B10-T01）

本契约定义 Noetome 第一版「章节粗对齐 + 全局概念清单」：作者包长什么样、冻结根怎么建、
章节身份与概念身份由什么决定、七类对齐警告的判据、输出根为什么必须字节确定，以及本轮明确不做的事。

对应需求：FR-07（结构化知识组织）、FR-11（资料驱动的覆盖边界）、NFR-01（来源处置透明）、
AC-02（章节与来源对账）。

## 1. 边界

- 本层**只做粗对齐**：识别标题、拆编号、建章节树、统计对齐与警告。它**不合并章节**、
  **不改写正文**、**不生成知识**、**不做语义相似度或实体识别**。
- 本层**不联网、不调用模型**。所有输出固定写 `model_calls: 0`、`network_requests: 0`。
- 本层**不写知识库、不写托管 Vault**，也不接管已有项目目录。`adopt` 只创建全新的对齐根。
- 概念身份只来自**作者声明的词表与别名**。没有出现在词表里的词不进全局清单，
  即使它看起来是明显术语——漏报比臆造更安全。
- 章节身份只来自**标题编号与标题文本**。编号缺失或重复只产生警告，不会阻止建树。

## 2. 作者包（输入侧）

`author.json` 字段：

| 字段 | 说明 |
| --- | --- |
| `policy` | 固定 `outline-alignment-v1` |
| `schema_version` | 固定 `1` |
| `producer` | 作者包固定 `fixture_author` |
| `case_id` | 标识符（ASCII 字母数字加 `-_.`），根内所有摘要绑定它 |
| `description` | 不超过 600 字符，说明本包只验证工程契约 |
| `concepts` | 相对路径，指向概念词表 `.json` |
| `documents[]` | `source_id`、`path`（必须 `.md`）、`title` |
| `limits` | `max_documents`（1..=8）、`max_concepts`（1..=128）、`max_occurrences_per_concept`（1..=4096） |

校验要点：`source_id` 与 `path` 在包内唯一；文档数在 `1..=max_documents`；
概念词表非空且 `concept_id` 唯一，别名最多 8 个且不得与本概念正名重复。

## 3. 冻结根布局

```
<对齐根>/
  control/
    manifest.json        所有权与摘要（canonical JSON，无尾换行以外的空白）
    inputs.json          每份文档的 Span 切片清单
    text/
      <source_id>.md     冻结的规范化正文（逐字节不可变）
      <case_id>.lexicon.json  冻结的概念词表
```

顶层只允许 `control/` 一个条目；`control/` 只允许 `manifest.json`、`inputs.json`、`text/`；
`text/` 恰好是「文档数 + 1」个文件。任何多出来的条目按 `OUTLINE_INTEGRITY` 拒绝，
且**不修改任何字节**。

`manifest.json` 记录的 `root_identity`、`parent_identity`、`control_identity`、
`manifest_identity`、`inputs_identity` 用于检测目录被替换（例如把 `control/` 整个换掉）。
所有摘要都在打开时逐字节复核：`author_manifest_sha256` 绑作者包，`lexicon_sha256` 绑词表，
`inputs_sha256` 绑 `inputs.json`，每份文档的 `normalized_sha256` 绑 `text/<source_id>.md`。

## 4. 章节树与章节身份

对每份文档逐行扫描，`heading_level` 规则：行首最多 3 个空格缩进，其后 1 到 6 个 `#`，
再跟空白或行尾。`heading_title` 取 `#` 之后去掉前导空白的剩余文本。

- `split_number(title)`：取前导编号（数字与点），返回 `(编号, 标题)`。
  `1.2 糖酵解净能量收支` → `("1.2", "糖酵解净能量收支")`；`2、糖异生` → `("2", "糖异生")`；
  `附录A 术语` → `(None, "附录A 术语")`。编号后允许 `、`、`）`、`)`、`.` 等分隔符。
- `number_depth("1.2.3")` = 3。
- `section_id` 形如 `<source_id>-0001`，按文档内出现顺序编号，**不随标题文本改变**。
- `path` = 「编号 + 空格 + 标题」，无编号时就是标题。
- `parent` = 相邻且层级更小的最近祖先；同层或更高层先出栈。
- `start_line` = 标题所在行；`end_line` = **下一标题的前一行**，末节到文档最后一行。
  这条是硬要求：如果 `end_line` 只覆盖标题行，那么"标题提到了某概念但正文没讲"就永远无法证伪。

## 5. 概念索引与上下文判定

词表每个概念有 `concept_id`、`canonical`、`aliases`。扫描时把正名与每个别名都在正文里找出来，
匹配**ASCII 大小写不敏感**、保留字节偏移、只接受字符边界。同一概念在同一位置的重叠命中只保留一个，
排序键是 `(字节偏移, -词长, 词序)`，因此长词优先、词表顺序优先。

上下文判定顺序（先命中者胜）：

| 上下文 | 判据 |
| --- | --- |
| `heading` | 命中所在行是标题行 |
| `caption` | 命中所在行以 `图`、`表`、`附录`、`Figure`、`Fig.`、`Table`、`Appendix` 开头 |
| `appendix` | 命中落在 `path` 含"附录"或标题含 `appendix` 的章节内 |
| `body` | 其余全部 |

`first` 记录概念在全部文档里的首次出现（`document` + `line` + `byte`）。

## 6. 七类对齐警告

| 警告 | 判据 |
| --- | --- |
| `heading_without_number` | 层级 ≥ 2 的标题没有前导编号 |
| `level_jump` | 标题层级比父节层级大 2 或以上 |
| `duplicate_number` | 同一文档内同一编号出现多次（每个出现各报一条） |
| `number_depth_mismatch` | 编号段数 ≠ **父节编号段数 + 1** |
| `concept_only_in_caption_or_appendix` | 某概念的全部出现都在 `caption` 或 `appendix` 上下文 |
| `heading_concept_missing_from_body` | 标题里点到了某概念，但该节正文区间内该概念一次非标题出现都没有 |
| `no_headings` | 整份文档一个标题都没有 |

两条容易写错的规则，这里明确固定：

- `number_depth_mismatch` **必须相对父节判断**。用绝对层级（`level - 1`）会把常规的
  `## 1 节` / `### 1.1 子节` 判成不匹配，因为不同书的约定不同。
- `concept_only_in_caption_or_appendix` **必须要求所有出现都在图注或附录**。
  只判"正文计数为 0"会把只出现在**标题**里的概念也算进来，而那只该报 `heading_concept_missing_from_body`。

警告排序键 `(document, line, kind, detail)`，`warning_counts` 只统计这七类。

## 7. 输出根与字节确定性

```
<输出根>/
  tree.json        章节树（policy、schema_version、case_id、documents[].sections[]）
  concepts.json    全局概念清单（total、by_document、contexts、first、occurrences）
  report.json      计数、警告全集、model_calls=0、network_requests=0
  report.md        人读版：章节对齐 / 全局概念清单 / 对齐警告 / 边界
  manifest.json    输出清单（四个文件的 SHA256 + counts）
```

**同一冻结输入的两次构建必须逐字节相同**，包括 `manifest.json`。因此输出清单里
**不含任何墙钟时间戳**——这条不是风格选择，而是可验证承诺：两份构建若只有时间不同，
"可复现"就变成空话。判定顺序也必须固定：先派生全部四个文件，回读 `report.json` 拿计数，
最后才写 `manifest.json`。

输出根必须尚不存在，也不能位于冻结根内部或包含冻结根。构建失败会回滚整个输出根。

## 8. 所有权与身份校验

`adopt` 创建根前先 `canonicalize` 目标父目录并取身份；创建后立刻取新根身份并写进清单。
若 `adopt_into` 失败且根身份仍是刚创建的那个，则删除整个根，不留半成品。
`open` 时逐项 `check_identity`，任何一项不符即 `OUTLINE_INTEGRITY`，并保持只读。

对**陌生目录**（不存在 `control/`）报 `OUTLINE_UNKNOWN_ROOT`，不是 IO 错误：
"这不是对齐根"是正常结果，不是故障，错误码要能让调用方区分二者（同 `research`、
`retrieve`、`revision` 三类根的行为）。

## 9. CLI 动词与错误码

| 动词 | 作用 |
| --- | --- |
| `outline adopt <author.json> <根>` | 冻结作者包，建对齐根；根必须尚不存在 |
| `outline build <根> <输出根>` | 从冻结输入派生四个文件 + 输出清单 |
| `outline list <根>` | 机器可读摘要（case_id、摘要、每份文档、计数） |
| `outline status <根>` | 人读中文摘要（所有权、冻结来源、对齐数、警告分类、零调用） |

| 错误码 | exit | 含义 |
| --- | --- | --- |
| `OUTLINE_ARGUMENT` | 2 | 参数无效、根已存在、作者包字段越界、概念数超限 |
| `OUTLINE_INTEGRITY` | 3 | 摘要不符、布局多余条目、身份被替换、非规范 JSON |
| `OUTLINE_UNKNOWN_ROOT` | 3 | 目标不是已建立的对齐根 |
| `PROJECT_IO_ERROR` | 3 | 文件系统层失败（父目录不存在等） |

## 10. 报告与四维状态

`report.md` 四节固定：章节对齐表（文档｜标题｜章节｜已对齐｜未对齐）、全局概念清单表
（概念｜正名｜出现次数｜文档数｜首次出现｜图注或附录）、对齐警告（按 kind 计数 + 明细表）、
边界声明（确定性推导零模型零网络 / 章节身份来自编号、概念身份来自词表不做语义相似 /
未在词表中的词不进清单）。

`status` 输出固定 8 到 10 行，其中必须出现「模型调用与网络请求：0 次」。
无标题文档在表里显示 0 章节，同时仍报 `no_headings`——两者同时出现是刻意的，
不能因为"0 章节没什么可看"就把这类文档从清单里隐去。

**对齐成功不等于知识正确。** 本层只回答"章节结构有没有明显缺陷、词表里的概念都出现在哪里"，
不回答概念定义是否准确、来源是否权威。

## 11. 本轮明确不做

- 不做语义相似度、同义词扩展、实体识别或自动合并同义章节。
- 不判断章节内容质量，不检查正文事实，不生成或改写任何知识。
- 不合并章节、不重排章节、不改写来源正文。
- 不支持 PDF/DOCX 直接输入（先把正文规范化成 Markdown 属于上游职责）。
- 不写项目库与托管 Vault，不接管生产目录；不做增量对齐（每次都是全量冻结输入重算）。
- 不做多语言词表合并，别名匹配只做 ASCII 大小写折叠。
