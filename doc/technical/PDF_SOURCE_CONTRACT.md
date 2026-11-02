# PDF 来源登记与项目 Schema3 契约

这是 B03-T03 的编码前契约，实施范围来自已采纳任务卡和 [NEXT_STEPS](../execution/NEXT_STEPS.md)。本文件描述必须实现与验证的行为，不声明它已完成；实测另存验收。Schema2及旧UTF8报告语义保留，不能在旧枚举中隐式塞入PDF。编译库Schema1、parser protocol1、PDF manifest1独立演进。

## 入口、身份与状态

`source import <项目> <明确MD/TXT/PDF> [--source-id <既有ID>] [--reparse]` 延续现有入口。`--reparse`只适用于PDF，明确要求新的解析尝试；资料不能提供参数、解释器或扩大路径范围。`source inspect <项目> <import-id>` 只读、不启动parser、不隐式迁移。`project init`仍建空Schema1；文本导入从1升级2，PDF导入从1/2显式升级3；已是3的文本导入保留3。

Source/Version仍统一保存在`sources`/`source_versions`。Source、Version、Import和parse_id都是标准小写UUIDv4；revision在同一Source所有格式的版本中从1连续。未指定source_id时，先沿用同一选取路径最近的关联，再按项目内既有raw SHA复用来源，否则新建；显式ID只在该来源内复用。SHA相同不跨显式来源合并。重命名同字节仍复用原Source/Version，回到旧字节复用旧Version，每次调用均新import_id。

PDF完整获取的Version为`media_type=application/pdf`、`ingest_status=acquired`，原始SHA/长度真实，`raw_snapshot=.noetome-sources/{version_id}/raw.pdf`；normalized SHA/version/path及error全部null，旧SourceSpan清单为空。它不表示解析或知识完成。未完整读取只登记失败导入，没有Version、完整SHA、快照、空normalized或假页。跨PDF/UTF8格式同SHA命中同一来源既有Version时必须拒绝矛盾媒体解释，不偷偷改变该Version类型。旧MD/TXT之间同字节仍沿用原Version的媒体标签和规范文本语义。

PDF导入报告独立`schema_version=2, report_kind=source_pdf_import`；旧文本ImportReport仍schema1、字段和值保持旧语义。PDF报告字段固定为`schema_version, report_kind, project_id, import_id, imported_at, selected_path, source_id, version_id, version, status, reused, acquisition_status, pdf, pending_extraction_count, knowledge_status, error`。nullable字段必填，未知字段拒绝。

`acquisition_status=acquired|failed`；`pdf`为`parse_id, reused, status, requested_parser, parser, protocol_version, import_options_version, adapter_sha256, cache_key, page_count, confirmed_page_count, parsed_page_count, needs_review_page_count, failed_page_count, region_count, snapshot_path, outcome_kind, outcome_sha256, manifest_sha256, issues, error`。未尝试时status为`not_attempted`，身份、版本键、snapshot/总页数等可空字段为null、计数0；不能以它们捏造PDF结果。真正尝试状态为`parsed|needs_review|failed`。报告总status分别为`ready|needs_review|failed`；knowledge始终pending，pending_extraction_count只反映已登记区域，零个区域不表示知识完成。

## Schema3 的精确表与字段

元数据仍五列`singleton,project_id,schema_version,created_at,state`，版本与`PRAGMA user_version`都为3，application_id仍`0x4e4f4554`。项目配置五字段不变，版本/元数据/PRAGMA必须匹配；未知表、列、约束、索引、view或trigger拒绝。以下10张表构成完整Schema3，SQLite自身对象除外。

| 表 | 字段/键与约束 |
| --- | --- |
| project_metadata | 原五列；singleton=1、project_id唯一、schema_version=3、state=initialized |
| sources | 原`source_id`主键、`created_at`，不增加格式身份 |
| source_versions | 原15列不变；唯一(source_id,content_sha256)、(source_id,revision)、(version_id,source_id)。Schema3明确允许PDF/acquired/null规范字段/raw.pdf，ready/failed仍只允许MD/TXT并保持原UTF8约束 |
| source_spans | Schema2原11列与全部约束不变，PDF版本不允许旧Span |
| source_imports | Schema2原12列与全部约束不变，只存旧文本报告；PDF导入另表 |
| pdf_parses | `parse_id`主键；`source_version_id`引用Version；`created_at,cache_key,protocol_version,requested_parser_json,parser_json,import_options_version,adapter_sha256,status,outcome_kind,page_count,confirmed_page_count,parsed_page_count,needs_review_page_count,failed_page_count,region_count,snapshot_path,outcome_sha256,manifest_sha256,error_code,error_message`；唯一(parse_id,source_version_id)、snapshot_path。SHA小写64hex，计数非负，protocol=1；validated必须actual parser/manifest SHA，host_failed必须failed/未知总页数/零页区域/actual parser null |
| pdf_pages | `parse_id,physical_page_index,display_page_number,printed_page_label,status,record_json`；复合主键(parse_id,index)、FK parse；index非负，display=index+1，status为3种。record_json为PDF manifest1的完整严格Page，包含geometry/glyph/region/visual/text，不遗漏正文或视觉原始证据 |
| pdf_regions | `parse_id,region_id,physical_page_index,locator_kind,bbox_left,bbox_top,bbox_right,bbox_bottom,text_resource_id,text_resource_sha256,start_byte,end_byte,glyph_indices_json`；主键(parse_id,region_id)、FK到同parse页/资源，locator固定pdf_page_region；有限非负非空bbox，UTF8半开范围非空。逐条与manifest核对 |
| pdf_resources | `parse_id,resource_id,relative_path,sha256,byte_length,kind,media_type,confidence`；主键(parse_id,resource_id)、唯一(parse_id,relative_path)、FK parse；kind为binary/utf8，confidence=confirmed/unconfirmed/diagnostic，字节非负。普通资源加Core报告；host失败的残留只标unconfirmed，不变成正文/页证据 |
| pdf_imports | `import_id,imported_at,selected_path,source_id,version_id,parse_id,status,reused,parse_reused,acquisition_status,error_code,error_message`；ID主键、source FK、(version_id,source_id)FK、(parse_id,version_id)FK；有完整Version必有parse_id；获取失败没有Version/parse/reused，必须真实错误 |

所有JSON对象使用严格类型；SQL索引字段、JSON、报告、原始/资源实际字节必须双向相符。每个Version有第一次导入，每个parse有首次非复用导入；不能插入孤立版本、解析、页、区域或资源。Schema3定义source_imports/pdf_imports的隐式SQLite rowid共用全局正序：迁移保留旧rowid，新两类导入都显式分配两表max(rowid)+1。联合模型拒绝跨表重复rowid或import_id，按该序核对首次/复用与同路径最近关联；不依赖可能同值或回拨的imported_at，也不在旧文本JSON加排序字段。

## 精确升级和受控发布

先只读验证原Project、Schema、全部旧记录和快照，再独占`.noetome-writing`。保留原配置/数据库逐字节备份和operation receipt。在原库副本中升级：Schema1先走已验收1→2；对合法Schema2在事务中保存原5表的所有行，按子表到父表顺序重建为精确Schema3并逐列恢复原记录。保留project_id/created_at、Sources/Versions/Spans/Imports身份、SHA、文本规则、原行顺序及既有快照字节；不转换旧normalized内容。失败只影响候选。

发布前完整重读候选Schema3、旧/新记录、全部快照及PDF索引。新raw与parse目录独占创建、按已捕获字节写入并核对；PDF用独占复制，避免发布阶段的双hardlink违反PDF资源守卫。正式数据库/配置配对发布、再次只读核对后才成功，之后仅清理登记且身份/字节仍属于本轮的临时文件。占用、目录替换、未知内容或受控中断保留备份/写目录并拒绝inspect；不承诺任意断电多文件原子性，不自动接管残留。

## PDF尝试、缓存与快照

尝试固定源Version及完整raw SHA/长度。缓存键为`pdf-source-cache-v1`的规范JSON SHA：raw SHA/长度、sidecar协议、完整requested ParserIdentity、adapter.py SHA、import_options_version。当前parser为B03-T02唯一锁定版本；引擎/依赖/adapter/options/Python或可信源码改变，都改变键。默认`pdf-import-default-v1`可以复用同Version同键的最新已验证parsed/needs_review；failed绝不命中可用缓存。`--reparse`使用`pdf-import-fresh-v1`且跳过缓存，总是新parse_id，不覆盖旧定位。版本字段变更用契约矩阵验证，实际不同导入选项用普通CLI验证；不将合成旧parser身份宣称真实版本兼容已测。

快照根`.noetome-pdf-parses`仅`.owner.json`和已登记parse UUID；owner沿用独立schema1/project_id。每次结果在`{parse_id}`保存B03-T02原始布局`parser/raw.pdf,pdf-manifest.json,pages/*`及Core parse-result.json/md。资源索引也包含Core两报告的SHA；raw同时与SourceVersion核对。每个旧结果只读保持原字节/位置，版本变化仅新目录。

Host失败若已完整取得Version，保存失败parse而不造actual parser/完整页清单。`failure.json`保留真实分类和有界诊断；parser残留保持实际字节、SHA并标unconfirmed，不生成Page/Region。只接受可信adapter固定文件模式，额外未知文件或可疑目录不自动收为己有。sidecar内部RunSession将Prepared保留到成功/失败捕获交付之后；成功ValidatedResponse亦由外层持有至Core与capture结束，不能在transport/语义错误的问号路径先释放。外层/内层目录、资源及输入守卫在移交前核对，捕获的Dir/File守卫随receipt移入Writer；不能仅凭事后路径/文件存在收养。前置runtime失败没有作业树时只创建自己的失败记录；无法建立/证明内层所有权的初始化错误保留待恢复现场，不收养可疑残留。

Inspector完整校验Project Schema3、统一Source/Version图、旧UTF8快照、PDF每次尝试、原始、所有已登记资源、JSON/MD及索引。validated结果调用B03-T02核心只读审计；host失败只验证失败快照与有界未确认清单，仍返回failed。manifest不完整/资源SHA或路径篡改/未知缓存/占用或链接均拒绝，不退回内存成功状态。

## 验收矩阵和当前不承诺项

真实CLI先红：新Schema1及带完整旧文本历史的Schema2升级、PDFimport/inspect、重复/重命名/换版/回旧版/显式来源隔离、`--reparse`新尝试、failed自动重试和失败后有效导入。独立SQLite查询全部10表及实际文件SHA/数量，旧文本报告逐字段/字节语义不变。原PDF和外部哨兵不变，移动原文件后inspect仍只读。

故障先红：原配置/数据库失配、未知Schema/对象/快照、原始/页/区域/资源篡改、Windows占用/硬链接/junction、候选目录替换、升级和提交实际受控中断。失败读取不建Version，完整raw而parser失败仍有真实Version/新失败尝试。统一Rust/Python、日志和diff检查之外，另保存独立实际支持/复用/故障证据。只使用本轮新测试项目，不写生产Vault，不接API/OCR/GUI/IR/Release，不声明1000页、24小时、跨平台或科学质量通过。
