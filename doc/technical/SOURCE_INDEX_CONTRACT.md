# 可移植来源索引与原始资料重定位契约（B11-T02）

本契约定义 Noetome 第一版「可移植来源索引」：工程声明里四个区域怎么划、冻结根里存什么、
正文为什么必须保持干净、来源为什么按字节摘要识别而不是按文件名、重定位的三种结论各自
意味着什么，以及什么情况下整包拒绝而不写出任何报告。

对应需求：FR-27（Vault 输出与可移植索引）、FR-26（无正文来源负担）、
AC-36（移动原始资料后索引仍可读）。

## 1. 边界

- 本层只做**索引核对与报告**：把声明里的笔记、资源与来源各扫一遍，把每条来源现在落在哪里、
  内容摘要是否还对得上算清楚。它**不改写笔记**、**不搬动文件**、**不改名**、**不生成知识内容**。
- 本层**不联网、不调用模型**。输出固定写 `model_calls: 0`、`network_requests: 0`。
- 本层**不写知识库、不写托管 Vault**，也不接管已有目录。`adopt` 只创建全新的索引根。
- **正文必须保持干净**：正文里出现文献编号或"参考文献/引用文献"小节，`adopt` 直接拒绝。
  这条判据是硬条件——索引一旦允许正文编号，后续就没法区分"索引维护的追溯"与"作者手写的引用"。
- **运行库、缓存、密钥与临时文件不得进入输出区**：它们只被登记为排除项，字节留在原地。
- **不判断内容质量**：本层只核对来源落在哪、摘要是否一致，不评价笔记写得对不对、
  也不判断整本教材的科学正确性。

## 2. 工程声明（输入侧）

`project.json` 字段：

| 字段 | 说明 |
| --- | --- |
| `policy` | 固定 `source-index-v1` |
| `schema_version` | 固定 `1` |
| `producer` | 作者包固定 `fixture_author` |
| `case_id` | 标识符（ASCII 字母数字加 `-_.`），不超过 48 字符 |
| `description` | 不超过 600 字符，说明本包只验证工程契约 |
| `output_area` | 相对路径，输出区（如 `vault`） |
| `runtime_area` | 相对路径，运行库（如 `runtime`） |
| `temp_area` | 相对路径，临时区（如 `temp`） |
| `relocate_area` | 相对路径，重定位核对目录（如 `moved`） |
| `notes[]` | `note_id`、`path`（必须 `.md`，允许中文路径段）、`title` |
| `resources[]` | `resource_id`、`path`、`description` |
| `sources[]` | `source_id`、`note_id`、`heading`、`description`、`locator`、`path` |
| `limits` | `max_notes`（1..=8）、`max_resources`（0..=8）、`max_sources`（0..=8） |

四条校验顺序固定：常量与标识符 → 四区相对路径与互斥 → 声明数量上限 → 逐条声明本身。
**区域互斥先于逐条声明**：区域本身重叠时，后面每一条路径判断都没有意义。

## 3. 冻结根布局

`adopt` 成功后，索引根下：

```text
<root>/
  control/
    project.json
    inputs.json
    manifest.json
    files/                 # 输出区文件的副本，保留原相对路径
      <note 与 resource 的同路径副本>
```

- 顶层**只允许** `control/`；`control/` 下**恰好**四项 `files/`、`inputs.json`、
  `manifest.json`、`project.json`。多一项即 `INDEX_INTEGRITY`。
- `control/files/` 里只放输出区声明的笔记与资源（按原相对路径），
  **运行库与临时区的字节不复制**——它们的摘要只记在 `inputs.json` 的排除项里。
- 尾换行约定：`control/manifest.json` 与 `control/inputs.json` **不带**尾换行
  （`strict` 会重新规范化后逐字节比对，多一个换行就是"非规范编码"）；
  `control/project.json` 与输出的四个 JSON 各带一个尾换行。
- `adopt` **先按规则预演一次完整判定**，全部通过才建根；语义拒绝（区域重叠、正文有编号、
  章节不存在、疑似凭据）一律不留半个根。目标已存在、父目录不可访问同样在写盘前失败。

## 4. 四区互斥与区域判定

- 四个区域两两**互不包含**：任一对满足包含关系即 `INDEX_ARGUMENT`。
- 区域判定按**路径组件**比较，不是字符串前缀：`vault2/笔记.md` 不算在 `vault` 里。
  实现为 `under(path, area)`：`path == area`，或 `path` 以 `area + "/"` 开头。
- 笔记与资源必须在输出区内；来源必须在输出区**之外**（落在运行库、临时区或重定位目录里即
  `INDEX_AREA_LEAK`）。**来源一旦进了输出区，复制输出区时原始资料会跟着泄漏或消失**——
  这条不是风格问题，是可移植性的结构不变量。
- 区域混用报 `INDEX_AREA_LEAK`（exit 3）而不是 `INDEX_ARGUMENT`（exit 2）：
  它是"声明的结构和要求不符"，不是"参数写错了"。

## 5. 正文来源负担

正文里出现下列任一形态即 `INDEX_CITATION_BURDEN`（exit 3），整包拒绝：

| 形态 | 判定 |
| --- | --- |
| 方括号数字 | 字节扫描 `[` + 一个以上 ASCII 数字 + `]`，如 `[1]`、`[12]` |
| 参考文献小节 | 任一级标题（`#` 后必须行尾或空白）的标题文本含"参考文献"或"引用文献" |

拒绝信息逐条列出 `{note_id} 里出现 {mark}`，并附一句边界说明：默认不在正文插入编号、
逐句脚注或长出处列表，来源由索引维护。

另有两条同层硬条件：

- **只写相对路径**：索引里出现以 `/` 开头或以盘符（含 `:`）开头的路径即 `INDEX_ARGUMENT`。
  索引会跟着输出区一起被复制，绝对路径跨机器就是错的。
- **凭据不进索引**：正文、描述、定位里出现 `api_key`、`apikey`、`password`、`secret`、
  `token=`、`authorization:` 任一即 `INDEX_SECRET_MARKER`（exit 3）。

## 6. 来源章节与声明校验

逐条来源核对四项，全部必须在写盘前通过：

1. `note_id` 必须在本次声明里存在，否则 `INDEX_ARGUMENT`。
2. 来源指向的原始资料文件必须存在且可读，否则 `INDEX_ARGUMENT`。
3. `heading` 必须真的出现在该笔记的标题列表里，否则 `INDEX_LOCATION_MISSING`（exit 3）。
   拒绝信息列出**该笔记实际可用的章节**——只说"不存在"而不给可用值，作者没法修。
4. 来源路径不得落在输出区的三个排他区（运行库、临时区、重定位目录）内，否则 `INDEX_AREA_LEAK`。

章节身份只由**标题文本**决定，不做编号归一化、不做模糊匹配。同一笔记里出现两个同名标题时
按首次出现定位，并在 `report.md` 的边界声明里保留"按首次出现"这一口径。

## 7. 索引内容与摘要身份

输出 `index.json` 七个字段：

| 字段 | 内容 |
| --- | --- |
| `policy` / `index_version` / `case_id` | 常量与用例标识 |
| `output_area` | 输出区相对路径 |
| `files[]` | 输出区文件 `path`、`role`（`note` / `resource`）、`sha256`、`byte_length` |
| `notes[]` | `note_id`、`path`、`title`、`headings[]`、`sha256` |
| `sources[]` | `source_id`、`note_id`、`heading`、`description`、`locator`、`path`、`sha256`、`byte_length`、`digest_short` |

- `digest_short` 是 `sha256` 的前 16 个十六进制字符，只用于人读；**判定一律用完整摘要**。
- **身份来自字节摘要，不来自文件名**：重名但内容不同不算同一份来源，
  同一份来源改了名也算同一份来源。这条是重定位三态的全部依据。
- 索引里**没有**笔记正文。索引回答"这份笔记来自哪份原始资料的哪一节、那份资料现在在哪"，
  不承担内容存储。

## 8. 重定位三态

`relocate` 只读扫描一个目录，对每条来源给出三种结论之一：

| 状态 | 判据 | 提示语要点 |
| --- | --- | --- |
| `relocated` | 目录里存在摘要完全一致的文件 | 给出找到的相对路径与来源描述 |
| `digest_mismatch` | 只有同名文件，但摘要不同 | 「同名不能证明是同一份来源，需要重建映射」 |
| `missing` | 既无同摘要文件也无同名文件 | 「需要重建映射」，并附来源描述与定位 |

- **`missing` 与 `digest_mismatch` 必须分开计数**：前者是"文件没搬到这里"，
  后者是"搬来了但内容变了"。合并成一个数字会掩盖两种完全不同的处置方式。
- 任何情况下都**不指向具体页面**：映射过期时只提示需要重建，不会给出一个可能指错的链接。
- `relocation.json` 八个字段：`policy`、`index_version`、`case_id`、`directory`、
  `relocated`、`missing`、`digest_mismatch`、`sources[]`（每条 `source_id`、`status`、
  `found`、`detail`）。

## 9. 输出根、字节确定性与所有权校验

`build` 在**全新**输出根里写出五个文件：`index.json`、`relocation.json`、`report.json`、
`report.md`、`manifest.json`。

- 输出根必须尚不存在，且不得与索引根互相包含。
- **输出的 `manifest.json` 不含 `created_at`**：同一冻结输入的两次构建必须字节相同。
  索引根自己的 `control/manifest.json` 带 `created_at`（记录"何时冻结"），
  输出清单不带（它描述的是"这次算出了什么"）。
- `report.json` 九个字段：`policy`、`schema_version`、`case_id`、`description`、
  `areas`（四个区）、`counts`（13 项）、`model_calls`、`network_requests`。
  13 项计数：`notes`、`resources`、`sources`、`headings`、`output_files`、
  `excluded_runtime_entries`、`excluded_temp_entries`、`citation_marks`、`absolute_paths`、
  `secret_markers`、`relocated`、`missing`、`digest_mismatch`。
  三个负担计数与 `missing`/`digest_mismatch` 恒为 0——不为 0 的包早已被拒绝，不会写出报告。
- 所有权校验：`control/manifest.json` 记 `root`、`root_identity`、`parent_identity`、
  `control_identity`、`manifest_identity`、`files_identity`、`inputs_identity` 七项（`root` + 六处
  身份），加上 `project_sha256` 与 `inputs_sha256`。`open` 时逐项核对，任何一项漂移即
  `INDEX_INTEGRITY`——**根被别人搬走或替换时不能当成同一个根继续用**。

## 10. CLI 动词与错误码

| 动词 | 用法 | 行为 |
| --- | --- | --- |
| `adopt` | `noetome index adopt <工程声明> <尚不存在的索引根>` | 预演全部判据后建根 |
| `build` | `noetome index build <索引根> <尚不存在的输出根>` | 写五个输出文件 |
| `relocate` | `noetome index relocate <索引根> <目录>` | 只读扫描，报三态 |
| `list` | `noetome index list <索引根>` | 只读输出机器可读摘要 |
| `status` | `noetome index status <索引根>` | 只读输出中文摘要 |

| 错误码 | 退出码 | 触发条件 |
| --- | --- | --- |
| `INDEX_ARGUMENT` | 2 | 参数缺失、标识符非法、区域重叠、绝对路径、笔记/资源不在输出区、来源声明指向未声明笔记或资料不存在 |
| `INDEX_INTEGRITY` | 3 | 冻结根布局不符、清单字段或摘要漂移、身份不符、父目录不可访问、输出根已存在 |
| `INDEX_UNKNOWN_ROOT` | 3 | 目标不是已建立的索引根（缺少 `control/`） |
| `INDEX_CITATION_BURDEN` | 3 | 正文出现文献编号或参考文献小节 |
| `INDEX_AREA_LEAK` | 3 | 笔记/资源落在输出区外，或来源落在运行库/临时区/重定位目录 |
| `INDEX_LOCATION_MISSING` | 3 | 声明的章节在笔记里不存在 |
| `INDEX_SECRET_MARKER` | 3 | 正文或描述里出现疑似凭据字样 |

## 11. 本轮明确不做

- 不解析 PDF/DOCX 抽取来源定位：`locator` 是作者声明的文本，本层只保存与展示。
- 不做页码范围校验、不做页内偏移：本层无法验证"第 112-118 页"是否真的对应那一节。
- 不做增量重定位：每次 `relocate` 都是对整目录的一次完整只读扫描。
- 不做多候选目录合并：一次只扫一个目录；跨目录需要分次调用后人工合并。
- 不生成阅读器或编辑器界面：索引是数据与报告，不是 UI。
- 不判断内容质量与科学正确性：`scientific_status` 固定 `not_run`。
- 不改写笔记正文，也不替作者补引用编号——**这一层存在的意义正是让正文不需要编号**。
