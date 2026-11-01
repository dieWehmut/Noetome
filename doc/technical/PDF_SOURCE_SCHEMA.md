# PDF 来源登记与项目 Schema3

B03-T03在[PDF v2](PDF_PARSE_PROTOCOL.md)已验收基础上扩展来源持久化；不改Schema2含义，不写编译Schema1，不引入OCR、模型、IR或生产Vault。本方案按已采纳需求和自动推进授权冻结，状态以账本为准。

## 方案与命令

选择显式Schema3，统一原始Source/Version身份，解析结果另表。仅新增PDF版本表会使旧Source的“至少一版本/连续修订/首次导入”校验失配；直接放宽Schema2则无法让旧二进制拒绝新含义。Schema3保留原始版本15列，新增明确PDF acquired分支，旧文本ready/failed条件保留。

```text
source import <project> <PDF> --python <absolute Python> [--source-id <existing UUID>] [--max-pages <0..63>] [--timeout-ms <0..30000>]
source inspect <project> <import-id>
```

MD/TXT旧命令和报告Schema1保持；PDF导入/inspect报告Schema2。Python只由可信调用者显式选择，不进入项目配置；inspect不启动Python。没有Python配置在读取前返回参数错误，不造获取成功记录。项目配置仍只有project_id/schema_version/created_at/state/database五字段，Schema3只改变版本整数。

同family按明确路径、原始SHA复用来源和版本；显式source-id在该来源内复用。不同字节建立下一连续revision，回旧复用旧version。同Source可含不同family的不同raw版本；同SHA的text/PDF family冲突拒绝SOURCE_MEDIA_CONFLICT，不能把text-ready冒充PDF-acquired或重写旧版本。每次导入有新import_id，ID在两类imports中不得碰撞。

## 精确存储与迁移

Schema3应用标识仍0x4e4f4554，project_metadata五列与配置对应，CHECK及user_version=3。sources、source_spans、source_imports的Schema2 DDL、列和旧记录逐字保留。source_versions仅扩展media_type为application/pdf、ingest_status为acquired，并限定PDF normalized_sha256/normalization_version/normalized_snapshot/error均NULL；旧MD/TXT规则不变。PDF版本没有旧source_spans，raw acquisition与parse结果分开。

新增五张表及一个partial unique index。所有ID为标准小写UUIDv4，时间为UTC RFC3339，SHA为小写64位十六进制，数字非负且不超过该次政策。未知对象、字段、约束、记录或JSON拒绝；固定子DTO JSON只能表示本文命名的v2类型，不能作为万能payload。

| 表 | 精确字段 |
| --- | --- |
| pdf_parses | parse_id PK；version_id FK；ordinal；created_at；cache_key；config_json（PdfCacheConfig）；status ready/needs_review/failed；page_count nullable；report_path；report_sha256；report_byte_length；error_code/message nullable；UNIQUE(version_id,ordinal)、UNIQUE(parse_id,version_id) |
| pdf_artifacts | parse_id FK；artifact_id；kind binary/utf8_text；relative_path；byte_length；sha256；PK(parse_id,artifact_id) |
| pdf_pages | parse_id FK；physical_page_index；printed_page_label nullable；status parsed/needs_review/failed；geometry_json nullable（PdfPageGeometry）；glyph_count nullable；excluded_glyph_ordinals_json（Vec<u32>）；text_artifact_id/text_artifact_sha256 nullable；PK(parse_id,physical_page_index)，文本引用本parse的artifact |
| pdf_regions | parse_id；physical_page_index FK到pdf_pages；region_id；ordinal；bbox_json（[f64;4] pt）；artifact_id；sha256；start_byte；end_byte；glyph_ordinals_json（Vec<u32>）；PK(parse_id,region_id)，UNIQUE(parse_id,physical_page_index,ordinal)，资源引用本parse |
| pdf_imports | import_id PK；imported_at；selected_path；source_id nullable FK；version_id nullable；parse_id nullable；raw_status acquired/failed；parse_status ready/needs_review/failed/not_run；reused_version bool；reused_parse bool；knowledge_status pending；exit_code 0..3；error_code/message nullable；版本/source和parse/version为配对FK |

`pdf_ready_cache`是唯一显式index：UNIQUE(version_id,cache_key) WHERE status='ready'。失败/需复核允许同key追加parse_id，ordinal按版本从0连续，不覆盖旧页/区域。每个PDF版本必须有首次PDF import；每个parse必须被至少一个import引用，复用只能引用更早ready结果；旧text import验证和旧Version首次事件保持。

迁移先在只读原项目验证精确Schema、所有记录及旧快照。Writer保存原config/DB字节与身份并复制candidate。Schema1可先安全升2；Schema2在candidate保存全部有序旧行，BEGIN IMMEDIATE后按子表到父表重建完整Schema3并逐字段恢复相同ID/时间/SHA/revision及imports顺序，避免ALTER RENAME改变FK。候选shape/外键/全部记录和快照验证后才COMMIT，之后Writer配对发布config与DB。错误只回退确认属本次且字节/身份未变的对象；未知替换或kill保留marker、candidate和两份备份。inspect不迁移或自动恢复。Schema3文本写入保持3，禁止降级。

## 解析身份及快照

`PdfCacheConfig`按固定字段序列化：protocol_schema=2、protocol、adapter_id、adapter_version、options_version、runtime（9个PdfRuntime字段）、pdf_policy（PdfPolicy）、artifact_policy（ArtifactPolicy）。cache_key=SHA256(UTF8 JSON of {version_id,raw_sha256,config})，不以修改时间、路径或单个恒定options_version替代。timeout只影响本次执行，不影响成功内容缓存；CLI max-pages使用真实已支持的PdfPolicy收紧，默认63。

只缓存ready；相同版本/config复用成功parse的全部ID和快照，不重跑Python，不改mtime。失败/需复核每次实际运行并追加parse。实际max_pages63与1对N01都成功但形成不同结果，回63复用旧结果。当前仅一个已实现的锁定parser版本；未知parser/protocol/options仍拒绝，未来升库须保留旧reader支持，不能据key单元测试宣称另一个parser已验收。

原始版本目录保持`.noetome-sources/{version_id}/raw.bin`；PDF没有normalized.txt。解析根为`.noetome-pdf-parses/.owner.json`（Schema1/project_id）及数据库所有parse UUID目录，每目录仅`report.json`和`artifacts/`的完整声明资源。report_path严格为`.noetome-pdf-parses/{parse_id}/report.json`。资源仍是T02固定artifact ID/相对路径，不从数据库任意路径读取；PDF raw/报告/资源必须单链接。

`PdfEvidence`严格JSON所有字段必填：schema_version=1、input（InputFingerprint）、config、status（PdfParseStatus）、exit_code、host_report（PdfParseReport或null）、failure（SourceFailure或null）。有完整原始字节才创建该证据及parse_id。host_report可保存T02已验证响应或真实失败；无子进程输出的前置失败允许null并保存真实code/message，不能造页/资源。证据报告最多16MiB（T02 Host仍8MiB）；资源限制沿用其请求。

解析只消费本次确认的owned raw副本，不再次打开原selected_path：Writer创建固定selected.pdf，再把该份raw交给真实T02 parser。T02临时workspace在本次marker内，由专用方法只接收其UUID拥有标记、严格request/input、正式Host报告和已验证资源固定清单，保持身份/有界读取后登记为自有暂存树；未知或不完整树保留，不放宽为任意目录。Root再次绑定Host input到Version SHA/长度。最终parse树只复制report.json/资源，raw由Version保存一次。

Writer同时支持一个新raw Version与一个新Parse，PDF发布使用独占复制后逐字/身份重读，staged与published为不同文件且各nlink1；旧文本硬链接过渡规则不变。候选数据库重读使用唯一typed staged raw/parse定位，正式校验一律只读正式cache。所有关键阶段核对精确目录及外部哨兵。

## 只读验证与报告

Schema3 Store统一核对所有Source修订、raw版本和两类import首次/复用事件。PdfStore从关系表重建页/区域/资源，与saved evidence逐项相同；按保存的config创建PdfLimits，重跑validate_pdf并核对实际原始SHA/资源/全文/UTF8/坐标/状态，不能只比报告SHA或页内bbox。失败证据不造完整库存；非法record/report/resource拒绝，保留现场。原selected文件移走不阻止检查。

PDF报告Schema2字段为schema_version、project_id、import_id/imported_at/selected_path、source_id/version_id、status、exit_code、raw_acquisition（status/reused/version/error）、pdf_parse（status/reused/parse_id/config/report_path/report_sha256/evidence）、knowledge_status=pending、error。readonly inspect成功核对任何真实ready/review/failed记录均退出0；import按本次记录退出0/1/2/3，runtime native2不能折叠成3。

验收采用12组实际CLI/SQLite/快照矩阵：旧历史迁移、持久独立定位、复用、换版回旧/显式身份、真实选项变化、连续失败、实际runtime失败后恢复、未读全、需复核页、同步篡改、严格Schema/Windows边界、已知故障及真实kill。不修改冻结作者golden，成功/失败均核对原字节及外部哨兵。
