# B02-T03 执行方案：fixture 到全新临时 Vault

本任务依赖已完成的B02-T02，按已采纳的Rust/SQLite/新测试目录基线推进。它引入有限Markdown渲染、实际输出验证、Run/Coverage/Release及双报告，不引入模型、PDF、OCR、生产Vault接管或项目Schema2迁移。基线完整Rust回归185项通过；原有两个旧docs删除保留在工作区。

## 设计与选择

直接边渲染边宣称published无法证明完整输出；把状态只放在终端文字也无法重读中断现场。本任务选择独占新根、受保护暂存、独立解析实际文件、再发布到该新根的Vault，最后以SQLite单事务提交发布依据。不扩展为增量合并或任意断电保证。

CLI为`noetome case compile <case-root> <new-output-root>`，动词后可用`--`分隔以`-`开头的路径。帮助仍只输出。新根父目录必须存在，根不得已经存在，不允许输入输出祖先重叠、reparse/junction或不安全Windows路径；以路径组件而非字符串前缀判断重叠。合法相邻目录不能误拒绝。

```text
new-output-root/
  state/compile.sqlite         编译工作区Schema1，与project.sqlite Schema2分开
  state/recovery.json          报告或失败状态登记无法完成时保存已确认现场的诊断
  staging/vault/<note paths>   已生成候选，失败时保留
  vault/<note paths>           通过暂存检查后独占写入，再次重读
  reports/validation.json      本轮验证事实快照
  reports/validation.md        同事实的Markdown报告
  reports/failure.json         验证报告已存在后发生登记失败时另存
  reports/failure.md
```

创建后默认保留现场，无Drop递归清理。state/staging/vault/reports基础空目录在初始化时建立；Vault为空不表示已发布。全部目录和文件保存身份；普通文件使用create_new/write_all/sync_all。阶段结束和动作前后对根、父目录、全部子目录、文件身份/SHA及精确清单核对。未知内容、同字节替换、目录换号或链接不被删除、覆盖或接管。输入LoadedCase守卫保持到最终提交，不能拼接两次不同视图的verify/load。

## 渲染与实际输出接口

纯模块`fixture_render.rs`定义以下crate内类型，保持与协调者一致：

```rust
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct ScopeLocator {
    section_id: String,
    node_id: String,
    kind: String, // paragraph/list_item/table_cell
    item_id: Option<String>,
    row_id: Option<String>,
    column_id: Option<String>,
}
struct ContentMapping {
    scope: ScopeLocator,
    atom_ids: Vec<String>,
    start_byte: u64,
    end_byte: u64,
}
struct LinkMapping {
    scope: ScopeLocator,
    target_concept_id: String,
    target_path: String,
    target_heading: String,
    display_text: String,
    start_byte: u64,
    end_byte: u64,
}
struct RenderedFile {
    relative_path: String,
    note_id: String,
    plan_revision: u64,
    content_sha256: String,
    byte_length: u64,
    content_mappings: Vec<ContentMapping>,
    links: Vec<LinkMapping>,
}
struct RenderedNote { file: RenderedFile, bytes: Vec<u8> }
struct ReadBackFile { relative_path: String, bytes: Vec<u8> }
fn validate_renderable(case: &LoadedCase, targets: &[ResolvedTarget]) -> Vec<Issue>;
fn render(case: &LoadedCase, targets: &[ResolvedTarget]) -> Result<Vec<RenderedNote>, Vec<Issue>>;
fn validate_rendered(
    case: &LoadedCase,
    targets: &[ResolvedTarget],
    manifest: &[RenderedFile],
    files: &[ReadBackFile],
) -> Vec<Issue>;
```

上述字段全部pub(crate)，metadata类型均derive Serialize/Deserialize且deny_unknown_fields；bytes不进入报告。输出总字节上限64MiB，提前累计并失败，不写截断笔记。输出`# title`、`## heading`，节点按计划顺序，列表按1起始序号，表格完整列/行/单元格。正文不写运行ID、时间或绝对根路径，稳定身份进入报告/数据库，不污染笔记确定性。

所有明文ASCII标点反斜线转义，阻止强调、公式、代码或HTML成为未经声明的展示。正文/单元格的换行编码为固定`<br>`，Tab为`&#9;`，独立解码恢复原字；source中恰有这些字面字符时先转义，不能二次解码成新语义。标题及wiki别名不接受换行/Tab，canonical目标标题也拒绝反斜线以避免表内wiki分隔转义奇偶歧义，别名另拒绝`[]|`等此版不支持的分隔字符；返回RENDER_TEXT_SYNTAX，不静默删改。

SemanticLink没有插入偏移，故按声明顺序追加在所属正文/列表项/单元格末尾，正文与link mapping范围分开。目标由有效canonical的完整Vault相对路径去掉`.md`及精确heading组成：`[[笔记/乙容器#定义与条件|乙容器]]`。表格别名分隔竖线写为`\|`，不直接split竖线；解析识别反斜线奇偶。普通数学`x < 2`/`[1,2]`作为明文保留，不宣称MathJax兼容。

验证器不调用render重建期望，不只信任mapping或SHA。它独立解析实际有限Markdown：标题、小节、节点/列表顺序、表格列/行/单元格及全文件wiki token。解析得到的scope实际边界须与metadata一致，正文范围不含追加alias；逐scope核对计划文字及独立expected原子content/qualifiers，required原子实际完整显示；canonical.atom_ids在对应note/section完整显示；ExpectedTable逐格匹配。所有实际链接与全部计划SemanticLink双向匹配，再定位到实际文件和唯一标题，不仅检查required_links。

输出问题固定为OUTPUT_MANIFEST、OUTPUT_MAPPING、OUTPUT_CONTENT、OUTPUT_LINK；规范解释归OUTPUT_CONTENT，结构/文本布局归OUTPUT_CONTENT；非法UTF8与清单/SHA归OUTPUT_MANIFEST。避免某检查failed被另一个检查计数掩盖。

## 编译事实与双报告

`fixture_compile.rs`负责同一LoadedCase的预检、协调持久状态、暂存/最终重读、报告与末尾提交；`compile_store.rs`负责独占目录/文件、精确身份清单与SQLite。不向GUI开放任意参数。

报告Schema1为验证事实快照：schema_version、producer、case_id、run_id、release_id、started_at、finished_at、result(`validated`或`failed`)、dimensions、checks、counts、issues、files、input_hashes、versions、publication。维度分别为source_coverage/knowledge_compilation/quality_validation/optional_research，每项含state(`complete`/`incomplete`/`not_applicable`)、reason、counts。checks使用passed/failed/not_run；scientific_quality固定not_run，可选研究not_applicable，模型/API未运行，不编造用量与价格。

publication只保存`database=state/compile.sqlite`、`commit_table=publication_commits`和release_id，明确验证通过不是已提交发布；两报告不提前写无条件published。Markdown用相同typed事实生成，列出相同键、状态、原因、计数、问题、文件修订号、完整scope与文件SHA，最后说明发布需要该数据库的已提交记录。

stdout为CompileOutcome：schema_version、producer、operation=`case_compile`、case_id、run_id/release_id、release_status、output_created、paths、report。report逐字段等同于磁盘JSON；外层release_status来自实际提交或失败结果，不能由report.result推断。成功退出0且release_status=published；拥有根后的失败退出3、外层staged或recovery_required、稳定CASE_COMPILATION_FAILED。无法解析/不安全路径等预检错误stdout空、退出2；输出已存在、输入输出重叠或实际I/O阻断为3。fixture语义预检失败输出既有分项问题JSON、output_created=false，退出3；不存在的产物不作为失败双报告已生成。

## SQLite Schema1与最终权威事务

文件`state/compile.sqlite`为独立编译工作区，user_version=1，application_id=`0x4e434d50`。journal_mode=DELETE、synchronous=FULL、foreign_keys=ON，无WAL持久侧文件。表固定如下，JSON列均保存已验证结构，不开通外部任意SQL：

```sql
CREATE TABLE compile_metadata (
 singleton INTEGER PRIMARY KEY CHECK(singleton=1),
 schema_version INTEGER NOT NULL CHECK(schema_version=1),
 producer TEXT NOT NULL CHECK(producer='fixture'),
 case_id TEXT NOT NULL, fixture_scope_id TEXT NOT NULL,
 input_hashes_json TEXT NOT NULL, versions_json TEXT NOT NULL
);
CREATE TABLE runs (
 run_id TEXT PRIMARY KEY, started_at TEXT NOT NULL, finished_at TEXT,
 stage TEXT NOT NULL, state TEXT NOT NULL,
 report_json_path TEXT, report_md_path TEXT, checks_json TEXT NOT NULL,
 counts_json TEXT NOT NULL, issues_json TEXT NOT NULL
);
CREATE TABLE releases (
 release_id TEXT PRIMARY KEY, run_id TEXT NOT NULL REFERENCES runs(run_id),
 status TEXT NOT NULL, ir_revision INTEGER NOT NULL, plan_revision INTEGER NOT NULL,
 renderer_version TEXT NOT NULL, manifest_json TEXT NOT NULL
);
CREATE TABLE coverage_records (
 run_id TEXT NOT NULL REFERENCES runs(run_id), span_id TEXT NOT NULL,
 disposition TEXT NOT NULL, atom_ids_json TEXT NOT NULL,
 required_atom_ids_json TEXT NOT NULL, duplicate_of_span_id TEXT,
 exclusion_reason TEXT, decision_source TEXT NOT NULL,
 policy_version TEXT NOT NULL, stage TEXT NOT NULL,
 PRIMARY KEY(run_id,span_id)
);
CREATE TABLE rendered_files (
 release_id TEXT NOT NULL REFERENCES releases(release_id), relative_path TEXT NOT NULL,
 note_id TEXT NOT NULL, plan_revision INTEGER NOT NULL,
 content_sha256 TEXT NOT NULL, byte_length INTEGER NOT NULL,
 mappings_json TEXT NOT NULL, links_json TEXT NOT NULL,
 PRIMARY KEY(release_id,relative_path)
);
CREATE TABLE publication_commits (
 release_id TEXT PRIMARY KEY REFERENCES releases(release_id),
 committed_at TEXT NOT NULL, report_json_path TEXT NOT NULL,
 report_json_sha256 TEXT NOT NULL, report_md_path TEXT NOT NULL,
 report_md_sha256 TEXT NOT NULL, manifest_sha256 TEXT NOT NULL
);
```

state/status/stage在Rust按固定枚举和合法转换验证；外部重读也核对，不仅接受数据库文件存在。Run与Release使用真实UUID v4，时间Rfc3339 UTC；两次运行身份独立，时间落于各自调用范围，不以秒精度强求字符串不同。Coverage的run_id改成本轮真实Run，原fixture作用域R8s保存在metadata，完整12位置不丢失。

初始化running/staged；暂存后stage=staged、Coverage=rendered；暂存重读通过且Vault复制/重读通过时Release=verified。生成并重读两报告，核对当前input守卫/全树身份/SHA/清单及SQLite外键。最后同一事务插入publication_commits，更新Run=succeeded/stage=finished、Release=published及Coverage=validated；该事务提交才是发布依据，之后没有必须成功的文件写入或清理。

事务错误时先检查is_autocommit并显式rollback未结束事务，不能在失败路径把pending publication意外提交；COMMIT遇Busy可仍有活动事务。中断读取先拒绝-journal/-wal/-shm，不能用immutable绕过热日志恢复后从未提交页推断published。测试只在新的自有目录显式恢复时才允许SQLite执行恢复，并将该动作与只读验收区分。

渲染失败留Run=failed、Release=staged；实际输出或登记失败留recovery_required。若报告写入或数据库占用导致完整失败状态无法保存，在已确认仍属本轮的state目录独占写recovery.json，含run/release/阶段与稳定错误；若目录身份不再可确认则仅stderr并保留原现场，不向替换路径写。publication_commits必须保持空。验证报告已存在后的失败另写failure.*，不覆盖未知文件或伪装先前验证事实为发布成功。

## 故障注入与TDD分工

内部用`CompileHooks` callback在固定阶段观察/修改当前本轮数据，默认Noop，测试工具只在Rust测试utilities中实现；不通过fixture、普通环境变量或公开CLI参数开放故障入口。真实进程终止使用Rust测试构建中的专用测试入口启动同一compile函数，持久staged后在自有测试根写固定握手并等待stdin，父测试确认活进程后kill/wait。没有测试环境变量时该入口正常返回，不进入产品CLI或忽略必需验收。普通CLI正常运行仍覆盖端到端。

纯渲染测试先用可编译stub获得正文/链接/表格及实际篡改断言红灯，再实现；协调者先用旧CLI的有效compile期望退出0获得实际退出2的红灯，再接线。损坏候选时删单位或限定后重新计算metadata SHA，证明OUTPUT_CONTENT发现实际遗漏；另注入断链、错误表格、范围外假链接及字符边界。

分工：

1. 渲染代理仅`fixture_render.rs`，含纯preflight/render/independent validate与单元TDD。典型正例literal含两定义、5/8cm条件、3列比较表与两个canonical标题链接；scope/alias隔离及实际损坏为负例。
2. 存储代理仅`compile_store.rs`，先真实独占根/身份/未知内容/SQLite末尾提交和失败保护TDD，保留现场。root固定报告/上下文类型后接线。
3. 进程代理仅`tests/compile.rs`及测试utility/helper，先旧CLI红，再测试C01至C15矩阵。实际SQLite查询、字面期望、全文件链接、SHA/范围和哨兵独立检查。
4. root统一`compile_model.rs`、`fixture_compile.rs`、lib/main帮助分发、日志/标准/独立执行与复审。只有一个in_progress，不推进B03。

额外表内别名正例采用测试作者事先声明的variant：复制旧fixture，把T5x/W3g/D4c的links设置为`C6h/乙容器入口`，独立expected同一cell.links与required_links明确增加该条；不由renderer输出生成预期，不修改原fixture。作者提供固定字面期望`[[笔记/乙容器#定义与条件\|乙容器入口]]`。必要原子和原始证据保持旧独立expected，variant的审阅修改在测试说明中先记，再跑。

## 验收顺序

运行模块和进程红绿→独立源码复审→实际CLI两新目录并核对稳定身份/字节与不同Run→完整Rust fmt/check/test、Python日志测试、前端help构建、devlog/diff/文档链接检查→保存doc/validation/B02-T03.md及可复现JSON→complete日志、小步commit/push、核对远端。原异常和修复历史追加，不把fixture工程通过称为真实PDF/API质量。

已启用有限Obsidian语法另需实际干净环境检查；先检测本机可用应用与版本，生成自己的临时测试Vault后验证普通标题、列表、表格和链接。不可用时如实保留未验收项并继续独立工程工作，不用AST替代真实应用检查，不用未运行MathJax/Mermaid等能力扩大完成声明。
