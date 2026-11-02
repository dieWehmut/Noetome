# B03-T03 PDF 来源登记实施方案

> 执行技能：superpowers:subagent-driven-development用于明确文件分工、任务审查与最终审查；耦合的Writer/CLI由主执行者协调。全部步骤属于唯一活动任务B03-T03，不各自推进后续阶段。用户已授权按既定任务卡持续开发、普通提交和推送。

**目标**：来源项目可安全登记一份PDF原始Version和独立解析尝试，真实重复复用、失败保留、只读重查及Schema升级通过。

**架构**：显式Schema3保留统一Source/Version及旧文本报告，新增PDF尝试/页/区域/资源/导入五表；复用现有Writer的候选数据库、配对配置、原始备份和严格清理。B03-T02 parser使用本轮owned job，在守卫存活时捕获字节并交给Writer，不能收养未知路径。

**技术**：现有Rust1.98.1/rusqlite/serde、唯一锁定Python3.12.14/pdfplumber0.11.9，不增加生产依赖或parser后端。

**契约**：[PDF_SOURCE_CONTRACT](../technical/PDF_SOURCE_CONTRACT.md)、[既定任务卡](NEXT_STEPS.md)、[旧数据契约](../technical/DATA_CONTRACTS.md)、[PDF沙盒](../technical/PDF_PARSE_CONTRACT.md)。每项验收据此，不以勾选方案或生成日志充当完成。

## 全局约束和接口分工

- 同时仅B03-T03 in_progress；仅新测试项目/自造已授权PDF，保留原有用户删除，不触碰其他worktree。
- 项目init继续Schema1；文本1→2；PDF1/2→3；3文本保持3；inspect永不迁移。独立compile.sqlite还是Schema1。
- PDFVersion仅acquired/raw.pdf，无旧normalized/Span；PDF导入schema2、旧文本导入schema1。失败/needs_review不报ready。
- CacheKey完整绑定raw/协议/parser/可信源码/import选项；failed重新尝试，--reparse新parse不改Version/旧定位。
- Git分支codex/b03-t03-pdf-registry；任务证据先红后绿、每组件审查后有意义提交，最后全回归/日志/标准提交推送。

| 文件 | 责任与生产接口 |
| --- | --- |
| source_schema.rs、source_schema3.sql | 保留SCHEMA2常量；validate路由2/3；upgrade保持原文本行为；`upgrade_pdf(&mut Connection,&ProjectInfo)->ProjectInfo`事务重建恢复旧行 |
| source_model.rs | 统一Sources/Versions与旧文本报告；PDF版本/导入初始身份联合验证；现有`Store::load`/`validate_snapshots`保留，新增支持staged parse的检查入口 |
| pdf_source_model.rs | 严格PDF报告/尝试/资源索引；`PdfStore::load(&Connection,&project_id)`，record/snapshot核对；版本化`cache_key(Fingerprint,ParserIdentity,adapter_sha,import_options)->String` |
| project.rs | schema1/2/3只读路由，未知/失配拒绝，不隐式迁移 |
| source_write.rs | `stage_pdf_version(id,raw)`、`stage_pdf_parse(id,files)`；受保护parser job移交，独占复制/同一提交保护，不改旧文本stage行为 |
| pdf_parse.rs/pdf_workspace.rs | crate内owned观察/捕获接口在Workspace/资源守卫释放前移交；普通parse/inspect和fake边界保持原接口 |
| sidecar_process.rs/sidecar_workspace.rs | 内部RunSession保留Prepared直至失败/成功捕获；默认public run仍返回原ValidatedResponse，捕获receipt移交live目录/文件守卫 |
| pdf_source.rs | `import(project,file,source_id,reparse)->CommandOutput`及PDF持久报告；实际读取、解析/缓存、候选登记及重读 |
| source.rs/lib.rs/main.rs | 扩展现有入口，Schema3文本读写/报告保持，PDFinspect按ID统一查找、帮助及--reparse校验 |
| tests/pdf_source.rs、tests/support/pdf_source_* | 真实普通CLI及进程故障；独立SQL/字节断言，不调用被测校验器生成预期 |
| scripts/verify_pdf_source.py | 独立普通CLI+SQLite+文件计数/SHA/只读/旧文本历史核对，保存实际证据 |

## 1 Schema升级和旧语义保护

- [ ] 写真实迁移/来源CLI红灯：初始化新项目，导入旧MD保留report，导入真实PDF要求Schema3和PDF报告；原CLI当前应SOURCE_UNSUPPORTED_TYPE，旧报告/快照不变。

```rust
let first = cli(&["source", "import", project, old_md]);
let old = json(&first.stdout);
let result = cli(&["source", "import", project, real_pdf]);
assert_eq!(result.status.code(), Some(0));
assert_eq!(read_project(project)["schema_version"], 3);
assert_eq!(old, json(&cli(&["source", "inspect", project, old["import_id"]]).stdout));
```

- [ ] Schema组件测试先红：升级候选旧5表行完整保留，未知对象/字段或坏引用拒绝；未知schema、失配和候选升级失败不改原始。
- [ ] 实现精确Schema3与严格数据/快照路由，记录PDF原始acquired/空Span和旧UTF8规则的分支，禁止Schema2接受新对象。
- [ ] 定向真实Schema迁移/旧MD回归绿后独立任务审查，提交Schema/模型与契约。

## 2 owned解析登记、复用与失败

- [ ] 普通CLI先红：单/多页PDF索引/资源、重复/重命名同Version同parse且newImport；内容改变revision+1、回旧SHA复用、显式Source跨同SHA不合并、--reparse新parse原快照不改。

```rust
assert_eq!(again["version_id"], first["version_id"]);
assert_eq!(again["pdf"]["parse_id"], first["pdf"]["parse_id"]);
assert_ne!(again["import_id"], first["import_id"]);
assert_eq!(again["pdf"]["reused"], true);
assert_eq!(query_counts(project), before_counts.with_one_more_import());
```

- [ ] 未读全失败与完整raw但parse failed先红；missing/occupied/too_large不造Version；damaged/encrypted/partial/无runtime保留真实raw和failed尝试，重复失败newparse，随后有效输入成功。
- [ ] 实现owned job回调/守卫移交、Writer PDF树独占复制及严格资源捕获；异常未知文件/替换不收养，保留安全待恢复现场。
- [ ] 实现PDF候选登记和缓存、报告schema2；旧source::inspect加载全部对象，PDFinspect在原文件搬移/环境无Python时仍只读验证持久资源。
- [ ] cache key逐字段变更契约测试；实际--reparse选项验证新attempt，明确不宣称合成旧parser是已测真实兼容版本。
- [ ] Pre-flight Important修复红灯：实际宿主失败到移交之间尝试rename parser目录，必须被仍存活的Prepared/receipt目录能力拒绝；不能只观察Err后事后读路径。Source/Version联合验图先加载两类导入边，两个表rowid共用全局正序，跨表重复ID/序号拒绝，同路径最近关联不按时间戳猜测。
- [ ] 定向真实CLI与模型/Writer绿色、独立任务审查后提交本组件。

## 3 故障、独立验收与交付

- [ ] SQL/资源/页/区域语义篡改先红：同步外层SHA时仍失败；缺页/乱序/越界/unknown缓存、Windows锁/硬链接/junction及输入/候选替换不误报成功、原始/哨兵不变。
- [ ] 用test-only callback和真实子进程在迁移Prepared/VersionPublished/DatabasePublished/ConfigPublished实际中断；握手完整发布、确认PID存活再kill/wait；inspect拒绝待恢复，不删除未知内容，不暴露生产故障开关。
- [ ] 独立脚本在新root实际核对10表、旧报告/全部旧快照、PDFraw及索引、cache/失败数量、每次parse/inspect退出码和只读bytes/mtime；保存实际环境和未运行项。
- [ ] Broad完整分支审查，重要问题定向先红后绿和复审；不复用旧T02全绿作为新证据。
- [ ] 实际fmt/check/test workspace、Python unittest、devlog check、git diff/index检查，完成本项账本和标准后提交推送并核对remoteSHA。
- [ ] B03三项验收后只按真实结果滚动最多3项下一批任务，切批不start API/OCR；持续目标仍active。
