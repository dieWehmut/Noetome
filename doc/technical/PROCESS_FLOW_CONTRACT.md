# 有界过程来源核对与流程表达 v1

B06-T02 的实现契约。当前验收状态以任务账本及验证记录为准，本文不作为测试通过证据。旧单测量来源核对、协调 v1/v2/v3 和比较表契约保持原义。此入口不调用模型，也不解释任意自然语言。

## 实际入口与来源

```text
noetome knowledge process-review <project> <case-directory> <new-review-root>
noetome knowledge flow <project> <case-directory> <saved-review-root> <profile.json> <new-test-Vault>
```

动词后允许 `--` 分隔以连字符开头的路径。只向父目录已存在的全新测试根写入，不覆盖用户资料或生产 Vault。成功返回 0；已接纳输入的语义待复核返回 3 并保存失败双报告；非法契约或不可信路径返回 2/3，不能留下成功表达。

case 目录严格只有 `process-review.json` 和其指定的 gold 文件。manifest 全部字段必填：`schema_version=1`、`purpose=process_source_review`、`project_id`、`import_id`、`selection`、`gold_file`、`gold_sha256`。本版 selection 只允许一个明确 text ordinal；来源由实际项目数据库和 SourceSession 重建，不能以作者提供的快照替代。身份为规范 UUID，gold 文件为不同的普通直接子文件。

输入是一个最多 64 KiB 的选定 UTF-8 RequestUnit，2–12 个步骤，0–24 个关系。受控中文声明语法见[独立作者语法](../../fixtures/process-flow-v1/GRAMMAR.md)。JSON 字符串/数组词面由 JSON decoder 定位；不能按中文句号或含糊子串分割。过程行唯一且在首行，步骤在关系前，整个选定文字必须消费。未知后缀、否定谓词、不完整声明不能静默跳过。末尾允许一个 LF，空白处理以冻结语法为准。

每个 ID 为小写 ASCII 字母开头、后续小写字母/数字/下划线，总长最多32字节，全局唯一，`process` 保留。文本字段最多1024 UTF-8字节，采用已有有界纯文本限制；必填数组最多8项，项唯一。数组为空表示“选定来源未列出”，不表示不存在或已知没有。LF/Tab 作为文本数据保留，不能默默压平。

## 独立 gold 与过程 IR

gold 严格字段为 `schema_version=1, producer=process_source_gold, author, scientific_quality=not_run, process, steps, relations, evidence`；不接受额外字段或缺失的 nullable 字段。

- process：`title, entry, goals, conditions, exceptions`。
- step：`id, kind(step|decision), label, description, criterion(string|null), inputs, outputs, conditions, exceptions`。
- relation：`id, left, right, kind(sequence|branch|causes|promotes|inhibits|correlates|unknown), branch_outcome(bool|null), conditions, exceptions`。
- evidence：`object_id, start_byte, end_byte, quote`。每个过程/步骤/关系恰有一条完整声明行证据，不含末尾 LF；半开 UTF-8 字节范围相对于所选 RequestUnit。

解析器只接收实际原文，不接收 gold。节点/关系按 ID 规范排序，不能从声明顺序、同名输入输出或 gold 的数组顺序推断边。过程/节点/关系的全部语义和证据与独立 gold 比较；证据定位由解析器重新计算。不可解析的否定语句在失败报告中计入未处理声明，不能冒充肯定关系或成功 IR。

来源忠实性与图模型支持分别记录。相关性、方向未知、循环、缺分支、未知判据或缺关键连接可以是忠实的来源记录，因此来源核对通过不保证能画图。只有完整解析且精确匹配 gold 才产生 `process-ir.json`，命名空间为 `process_source_review / bounded_process_text_v1`。IR 包含实际 binding、RequestUnit、document 和各证据指纹；知识仍为 pending、科学质量 not_run、published=false。

review 根保存 `.owner.json`、`.case.json`、`.gold.json`、`.source.json`、成功时的 `process-ir.json` 和 `review.json/review.md`。报告含来源解析/gold核对分开结果、真实分母、问题码和实际文件指纹。读取保存结果时持有全部文件，重新打开实际来源并重新运行相同来源判据，重建同 request_id 的整个输出并逐字比较；不能只相信 IR 中的 passed 或同步修改后的报告指纹。

## 流程模型与兼容性

只有明确有向无环关系可生成流程表达。入口须存在，全部节点从入口可达，明确目标须与终点集合一致。decision 必须有明确判据和真/假两条 branch，额外前提/例外均保留。非 branch 不得携带真假值；不得把普通步骤连线解释为因果。`causes` 只表示“导致”，只有 `promotes`/`inhibits` 表示正/负作用；`correlates` 和 `unknown` 端点没有推定方向。

语义阻断码包括 `FLOW_CORRELATION_UNDIRECTED`、`FLOW_DIRECTION_UNKNOWN`、`FLOW_UNKNOWN_CRITERION`、`FLOW_DECISION_INCOMPLETE`、`FLOW_UNSUPPORTED_CYCLE`、`FLOW_DISCONNECTED`。语义不足时无成功 plan/笔记，保留失败报告和来源语义；不能用文字回退把语义问题改成通过。

目标 profile 严格必填 `schema_version=1, producer=flow_render_profile, profile_id, app, app_version, mermaid_version, flowchart_support(supported|unverified|unavailable), label_encoding=decimal_ascii_punctuation_v1, html_labels`。声明不等于实测；本版已测组合仅为 `obsidian_1_6_7_flowchart / Obsidian / 1.6.7 / 10.6.1 / supported / html_labels=true`。其余组合回退有据文字，原因 `FLOW_TARGET_UNVERIFIED`；这不声称其他版本缺少 Mermaid 能力。

程序生成 ASCII 节点身份、固定 `flowchart TD`、矩形/菱形和明确边，标签中的 ASCII 标点用十进制 `#N;` 编码。来源不能提供 Mermaid、布局、init、click、class 或 URL 指令。含首尾/连续 ASCII 空格或非 ASCII 空白（含 LF/Tab）的实际图标签/判据回退 `FLOW_LABEL_WHITESPACE_FALLBACK`，依据见[真实能力探针](../validation/assets/B06-T02/capability-probe/README.md)；不能以 DOM 存在字符代替视觉保真。[空白补充实测](../validation/assets/B06-T02/capability-probe/whitespace/README.md)发现首尾空格被裁剪、连续空格折叠、行/段分隔符失去分隔含义；其余未测空白保守回退，不声称所有空白均实测失败。单个内部 ASCII 空格保持图形表达，正文中的空白继续原样保留。

生成计划为独立 `knowledge_flow / active_flow_v1` 命名空间。图、必要正文和文字回退完整保留过程条件、目标、每个节点的说明/输入/输出/条件/例外，以及关系含义/真假判据/条件/例外；正文普通链接负责导航。文字编号用于定位，不额外主张总执行顺序。预格式文字保留 LF/Tab；图/正文逐块保存 UTF-8 范围及对应 object_id，全部可回到 IR 证据。保存后重新读文件验证映射、链接及计划一致性。

## 保护与验收

沿用已验证的文件/目录 capability、完整输入清单、有限资源和只创建新输出的保护。流程运行继续持有原项目来源、case、保存的 review、profile；输出不能覆盖或嵌入输入。收尾再次验证所有输入和输出。失败只清理仍为本次持有且字节未变的生成结果，保留原件和未知现场。

工程验收必须包含冻结无图作者和普通 CLI RED、真实来源到流程 CLI、独立语义/同步伪造矩阵、新根与 NTFS 保护、完整 Rust/Python 和旧比较表/协调回归、真实 Obsidian 图/正文链接/回退、独立审查及原件归档。自造来源、能力探针和渲染成功均不证明科学正确率、任意文字理解、其他 Obsidian 版本或生产发布通过。
