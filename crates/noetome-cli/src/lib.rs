use std::ffi::OsString;
use std::path::PathBuf;

mod audit_model;
pub mod audit_store;
mod bench_model;
pub mod bench_store;
mod change_model;
pub mod change_store;
mod compile_identity;
mod compile_model;
mod compile_report;
mod compile_store;
pub mod corpus;
mod diagnostics_model;
pub mod diagnostics_store;
pub mod extraction;
pub mod extraction_api;
mod extraction_api_config;
mod extraction_api_schema;
mod extraction_api_transport;
mod extraction_api_wire;
mod extraction_fake;
mod extraction_model;
mod extraction_output;
pub mod extraction_preview;
mod extraction_review;
mod extraction_review_model;
mod extraction_source;
pub mod extraction_verify;
pub mod fixture;
pub mod fixture_compile;
mod fixture_input;
mod fixture_knowledge;
mod fixture_model;
mod fixture_plan;
mod fixture_render;
pub mod fixture_run;
pub mod formula;
mod formula_input;
mod formula_model;
mod formula_parser;
mod formula_render;
pub mod image_fidelity;
mod image_input;
mod image_model;
mod image_render;
mod index_model;
pub mod index_store;
mod journal_model;
pub mod journal_store;
pub mod knowledge_coordinate;
mod knowledge_coordinate_conflicts;
mod knowledge_coordinate_context;
mod knowledge_coordinate_facts;
mod knowledge_coordinate_input;
mod knowledge_coordinate_variants;
mod knowledge_coordination_model;
mod knowledge_coordination_v2_model;
mod knowledge_coordination_v3_model;
pub mod knowledge_presentation;
mod knowledge_presentation_input;
mod knowledge_presentation_model;
mod knowledge_presentation_plan;
mod knowledge_presentation_render;
mod migrate_model;
pub mod migrate_store;
#[cfg(windows)]
pub mod native_process;
mod outline_model;
pub mod outline_store;
mod pdf_model;
pub mod pdf_parse;
mod pdf_source;
mod pdf_source_model;
mod pdf_validate;
mod pdf_workspace;
mod plan_model;
pub mod plan_store;
pub mod process_flow;
mod process_flow_model;
mod process_flow_plan;
mod process_flow_render;
mod process_input;
mod process_model;
mod process_parser;
pub mod process_review;
pub mod project;
mod relevance_model;
pub mod relevance_store;
mod request_budget_model;
mod request_budget_store;
pub mod request_run;
mod research_model;
pub mod research_store;
mod retention_model;
pub mod retention_store;
mod retrieval_model;
pub mod retrieval_store;
mod revision_model;
pub mod revision_store;
mod run_model;
mod run_recovery;
mod run_store;
mod scale_model;
pub mod scale_store;
mod scan_api;
pub mod scan_formula;
mod scan_input;
mod scan_model;
mod scan_render;
pub mod sidecar_model;
pub mod sidecar_process;
mod sidecar_workspace;
mod soak_model;
pub mod soak_store;
pub mod source;
mod source_model;
mod source_schema;
mod source_write;
mod structure_model;
pub mod structure_store;
mod template_model;
pub mod template_store;
pub mod text;
mod vault_model;
pub mod vault_store;
pub mod vault_update;
mod visual_capabilities;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const INVALID_ARGUMENT: &str = "CLI_INVALID_ARGUMENT：参数无效或缺少项目路径。请运行 noetome --help 查看用法；包含空格的路径请作为一个参数传入。";

pub const HELP: &str = "Noetome — 个人学科学习知识编译器

用法：noetome [选项]
      noetome corpus --help  （真实整书PDF资料库）
      noetome project <init|inspect> <路径>
      noetome source import <项目路径> <MD/TXT/PDF文件> [--source-id <既有ID>] [--reparse]
      noetome source inspect <项目路径> <import-id>
      noetome source parse-pdf <PDF文件> <尚不存在的测试根>
      noetome source inspect-pdf <PDF测试根>
      noetome extract fake <项目路径> <import-id> <fake目录> <尚不存在的测试输出根>
      noetome extract api <项目路径> <import-id> <config.json> <尚不存在的测试输出根>
      noetome extract verify <项目路径> <import-id> <review目录> <尚不存在的测试输出根>
      noetome preview choices <项目路径> <import-id>
      noetome preview units <项目路径> <import-id> <选择.json>
      noetome preview export <项目路径> <import-id> <成功API审计根> <尚不存在的新输出根>
      noetome knowledge coordinate <项目路径> <coordination-case> <尚不存在的测试输出根>
      noetome knowledge present <项目路径> <coordination-case> <协调输出根> <尚不存在的测试Vault>
      noetome knowledge formula <formula-case> <profile.json> <尚不存在的测试Vault>
      noetome knowledge image <image-case> <profile.json> <尚不存在的测试Vault>
      noetome knowledge scan-formula <scan-case> <api-config.json> <profile.json> <尚不存在的测试Vault>
      noetome case verify <fixture目录>
      noetome case compile <fixture目录> <尚不存在的测试输出根>
      noetome run start <fixture目录> <尚不存在的运行根> [--paused]
      noetome run resume <原fixture目录> <运行根>
      noetome run <status|pause|cancel> <运行根>
      noetome run api init <authority-config.json> <尚不存在的预算根>
      noetome run api start <run-spec.json> <预算根> <尚不存在的运行根> [--paused]
      noetome run api resume <同一run-spec.json> <预算根> <运行根>
      noetome run api status <预算根> <运行根>
      noetome vault adopt <author.json> <尚不存在的托管根>
      noetome vault plan <托管根> <候选目录> <尚不存在的计划文件>
      noetome vault approve <托管根> <计划文件> <尚不存在的批准文件>
      noetome vault publish <托管根> <批准文件>
      noetome vault status <托管根>
      noetome vault rollback <托管根> <回滚计划文件>
      noetome research adopt <author.json> <尚不存在的队列根>
      noetome research observe <队列根> <观测文件>
      noetome research decide <队列根> <决定文件>
      noetome research list <队列根>
      noetome research status <队列根>
      noetome retrieve adopt <plan.json> <尚不存在的检索根>
      noetome retrieve step <检索根>
      noetome retrieve run <检索根>
      noetome retrieve pause <检索根>
      noetome retrieve resume <检索根>
      noetome retrieve finish <检索根> <终止理由文件>
      noetome retrieve list <检索根>
      noetome retrieve status <检索根>
      noetome revision adopt <author.json> <尚不存在的修订根>
      noetome revision gap <修订根> <缺口文件>
      noetome revision revise <修订根> <修订文件>
      noetome revision list <修订根>
      noetome revision status <修订根>
      noetome outline adopt <author.json> <尚不存在的对齐根>
      noetome outline build <对齐根> <尚不存在的输出根>
      noetome outline list <对齐根>
      noetome outline status <对齐根>
      noetome relevance adopt <author.json> <尚不存在的筛选根>
      noetome relevance build <筛选根> <尚不存在的输出根>
      noetome relevance list <筛选根>
      noetome relevance status <筛选根>
      noetome structure adopt <family.json> <尚不存在的结构根>
      noetome structure build <结构根> <尚不存在的输出根>
      noetome structure list <结构根>
      noetome structure status <结构根>
      noetome migrate adopt <plan.json> <尚不存在的迁移根>
      noetome migrate build <迁移根> <尚不存在的输出根>
      noetome migrate list <迁移根>
      noetome migrate status <迁移根>
      noetome index adopt <工程声明> <尚不存在的索引根>
      noetome index build <索引根> <尚不存在的输出根>
      noetome index relocate <索引根> <目录>
      noetome index list <索引根>
      noetome index status <索引根>
      noetome audit adopt <工程声明> <尚不存在的审计根>
      noetome audit build <审计根> <尚不存在的输出根>
      noetome audit list <审计根>
      noetome audit status <审计根>
      noetome plan adopt <规划声明> <尚不存在的规划根>
      noetome plan build <规划根> <尚不存在的输出根>
      noetome plan list <规划根>
      noetome plan status <规划根>
      noetome template adopt <结构声明> <尚不存在的结构根>
      noetome template build <结构根> <尚不存在的输出根>
      noetome template list <结构根>
      noetome template status <结构根>
      noetome scale measure --pages 1000 --headings 4 --paragraphs 3 --output <输出根>
      noetome bench measure --notes 3000 --edits 200 --samples 20 --warmup 2 --output <输出根>
      noetome soak run --rounds 20 --interval-ms 100 --pause-after-ms 1500 --max-rss-growth-bytes 1073741824 --output <输出根>
      noetome change adopt <变更声明> <变更根>
      noetome change build <变更根> <输出根>
      noetome change list <变更根>
      noetome change status <变更根>
      noetome journal adopt <事件声明> <尚不存在的审计根>
      noetome journal build <审计根> <尚不存在的输出根>
      noetome journal list <审计根>
      noetome journal status <审计根>
      noetome diagnostics adopt <导出声明> <尚不存在的诊断根>
      noetome diagnostics build <诊断根> <尚不存在的输出根>
      noetome diagnostics list <诊断根>
      noetome diagnostics status <诊断根>
      noetome retention adopt <保留声明> <尚不存在的保留根>
      noetome retention build <保留根> <尚不存在的输出根>
      noetome retention list <保留根>
      noetome retention status <保留根>

当前支持：
  -h, --help       显示帮助
  -V, --version    显示版本
  project init <路径>     在尚不存在的目录初始化空项目
  project inspect <路径>  只读检查已有项目
  source import           登记单份文本或PDF的原始版本、解析状态及可追溯位置
  source inspect          只读核对导入记录和真实快照
  source parse-pdf        单份PDF解析到全新测试根，完整物理页/区域及需复核原因
  source inspect-pdf      只读核对已保存PDF、文本、位置和双报告
  extract fake            离线回放并核对选定来源的候选、证据与单请求预算
  extract api             按明确配置与预算发送一次文本API请求并核对候选
  extract verify          离线复核保存候选与原gold的有界来源事实
  knowledge coordinate    按明确身份、来源背景及证据保留多来源事实和未决关系
  knowledge present       重验协调来源并主动选择有据比较表或正文，保存未发布预览
  knowledge formula       保留明确选取TeX/公式原区域并核对语义，回退或阻断未确认表达
  knowledge scan-formula  对明确扫描区域执行一次有预算视觉请求，保留原响应并核对语义
  case verify             只读校验明确fixture的来源、IR、计划与独立预期
  case compile            将已校验fixture编译到全新临时Vault并保存双报告
  run                     执行有界fixture或人工环回API流程，持久保存检查点和预算
  vault                   托管测试集合的人工审批与可恢复发布（adopt/plan/approve/publish/status/rollback）
  research                研究候选队列、授权边界与拒绝记忆（adopt/observe/decide/list/status）
  retrieve                批准范围内的有界检索、持久预算与终止理由（adopt/step/run/pause/resume/finish/list/status）
  revision                写作反馈缺口回到修订与依赖重编译（adopt/gap/revise/list/status）
  outline                 章节粗对齐与全局概念清单（adopt/build/list/status）
  relevance               用户水平与相关性筛选的可审计记录（adopt/build/list/status）
  structure               同类结构一致性与导航复习组织（adopt/build/list/status）
  migrate                 小节提升为独立笔记与链接批量更新（adopt/build/list/status）
  index                   可移植来源索引与原始资料重定位（adopt/build/relocate/list/status）
  audit                   全库链接可用性与唯一解释位置审计（adopt/build/list/status）
  plan                    教材式自适应规划：概念关系、教学顺序与规划位置（adopt/build/list/status）
  template                同一主题的多阅读目标与同类结构一致（adopt/build/list/status）
  scale                   合成语料的千页规模实测：真实跑文本分析与章节对齐，规划上限如实列为未达标（measure）
  bench                   大库规模实测：3000 笔记 / 200 处人工改动的四阶段 P95 与内存（measure）
  soak                    长时运行 soak：逐轮耗时/内存/错误码，用可观测计数证明暂停后不再调度（run）
  change                  锁定位置与规划变更的局部影响（adopt/build/list/status）
  journal                 可定位任务的审计事件日志：四层回溯与凭据硬拒绝（adopt/build/list/status）
  diagnostics             带范围与内容预览的诊断导出：勾选决定内容、默认排除原文（adopt/build/list/status）
  retention               保留与清理策略及已发布索引保护：清理前说明影响（adopt/build/list/status）

不传参数时显示帮助；project --help、source --help、extract --help 或 run --help 查看命令说明。
审计事件日志只追加、不重写：同一任务的先后由事件序号决定，重排声明不会悄悄改变日志；每条失败事件必须能定位到任务、来源、知识对象与受影响文件；声明里出现疑似凭据字样时整包拒绝，不做「过滤后继续」。
诊断导出只导出声明的类别与条目：没勾选的内容如实标为未导出，不静默补全；默认不含原文与来源正文，勾选敏感范围必须记录同意；声明里出现绝对路径或疑似凭据时整包拒绝，拒绝时不写出任何字节。
保留与清理只生成计划、不删除任何文件：已发布版本必需的索引与追溯条目一律受保护，命中保护即整条拒绝，而不是跳过它们继续清理；清理前必须说明会丧失哪些恢复能力、哪些追溯链会变短。
当前来源由用户明确选取；候选知识保持pending。API本地HTTP假服务仅验证工程契约，实际服务冒烟与科学质量验收仍待执行。
完整知识编译与生产Vault增量发布尚未实现。
研究候选只是待批准线索：队列从不联网、不调用模型，也不把候选当成已完成知识。
有界检索只在冻结语料内进行：关闭外部检索时零网络请求，命中只是证据线索，不写入知识库或托管Vault。
写作缺口必须回到修订流程：修订经核验生成新IR版本并重编译所有依赖表示，未受影响表示字节不变；无来源补句标为待核验。
章节对齐只做粗对齐与概念索引：不合并章节、不改写正文、不生成知识；同一冻结输入的任何两次构建必须字节相同。
相关性筛选只记录保留与排除：必要前提缺失或必要原子被排除时整包拒绝，极简与详细的必要原子及条件集合必须一致；不静默省略、不改写来源。
同类结构只做一致性与链接核对：偏离只报告不自动改写、不静默补齐；同一概念只能有一处完整解释，生成链接断链必须为 0。
链接迁移只按知识对象ID核对引用：概念与原子ID不因拆分改变，断链/歧义/目标缺失/表格未转义一律整包拒绝；不自动搬动作者包文件。
来源索引不写入正文：文献编号、脚注与出处列表一律整包拒绝；来源按内容摘要识别、缺失只提示重建映射，运行库与临时文件不进入输出区。
全库审计只核对结构与位置：链接须在笔记与标题都唯一时才算可用，同名标题与同名笔记记为歧义；「完整解释」只认显式标记，有缺口仍写出报告并逐条列名，不判断内容质量。
教材式规划只决定知识点放在哪个文件：知识图谱可以多重归属，但文件树只选一个主要位置，其余联系用链接表达；连续反应链被拆到多份笔记时必须给出可检查的拆分理由，否则整包拒绝。
结构模板是结构规范而不是填空：不适用的章节可以省略，但不得为模板齐全生成空章节，特殊结构必须写出理由；同类同层级主题的命名、章节顺序与导航深度必须一致，差异必须声明理由。
";

pub const PROJECT_HELP: &str = "Noetome 项目命令

用法：noetome project init <路径>
      noetome project inspect <路径>

init：父目录须已存在，目标目录须尚不存在；不覆盖已有内容。
inspect：核对配置和 SQLite 状态，只读查看，不执行迁移或修复。
路径可含中文和空格；以 - 开头的路径前加 -- 分隔符。
初始化中断时保留未确认内容；请检查旧目录并选择新目录重试。
init建立空项目；来源登记通过source import，知识生成尚未实现。
";

pub const SOURCE_HELP: &str = "Noetome 文本与PDF来源命令

用法：noetome source import <项目路径> <MD/TXT/PDF文件> [--source-id <既有ID>] [--reparse]
      noetome source inspect <项目路径> <import-id>
      noetome source parse-pdf <PDF文件> <尚不存在的测试根>
      noetome source inspect-pdf <PDF测试根>

import：只读选取的单文件（最多64 MiB）；MD/TXT保存原始/规范快照与Span，PDF保存原始版本及独立页/区域解析尝试。
文本初次写入升级为Schema2，PDF写入升级为Schema3；保持项目身份、创建时间与已有历史，检查命令不会升级。
相同内容提示复用；显式source-id只在该来源身份内换版，不按名称合并。
PDF同字节按解析器/协议/选项复用已确认结果；--reparse只用于PDF，建立新解析尝试并保留旧结果。
inspect：使用导入结果的import-id只读核对持久记录、快照与位置，不迁移或修复。
结果为JSON；分别显示原始获取、PDF解析及pending知识状态。未能完整读取不造版本，完整PDF解析失败仍保留原始版本和真实尝试；失败/需复核返回非零。
以 - 开头的路径可在动词后加 -- 分隔符；中文和空格路径须作为单独参数。
项目写入中断保留标记、候选与备份；请检查旧目录并用新的测试项目重试。
parse-pdf：用NOETOME_PDF_PYTHON指定的锁定Python解释器解析一份明确PDF，仅写全新测试根。
保存原始PDF、完整物理页/区域、逐页UTF8及双报告；扫描/空白/复杂版式需复核，返回非零。
inspect-pdf：只读重查测试根的实际SHA、位置、正文与报告，原文件移动后仍可核对；不修复残留。
PDF物理索引从0开始、显示页为索引+1，印刷页号未知；不以行号或文件名代替页码。
source导入不生成IR或Vault；已审阅fixture通过case compile输出临时Vault。
候选抽取通过extract fake或extract api；API本地HTTP假服务仅验证工程契约，实际模型冒烟与科学质量尚未验收。
";

pub const CASE_HELP: &str = "Noetome fixture契约校验

用法：noetome case verify <case目录>
      noetome case compile <case目录> <尚不存在的测试输出根>

只读取case.json明确声明的根内MD/TXT、IR、NotePlan和独立expected。
校验完整Span覆盖、支持证据、必要原子与条件/单位、规范位置和实际计划显示映射。
固定身份与审阅标记来自自造fixture，不代表模型抽取或科学质量已通过。
结果为JSON，语义失败返回3；非法输入返回2，读取受阻返回3。
verify保持只读，不写项目、Run、Vault或报告文件；rendering和scientific_quality为not_run。
compile先校验再暂存和重读实际Markdown、标题引用、比较表，建立本轮Run/Coverage/Release。
compile只使用父目录已存在的新测试根，已有目标、输入输出重叠或路径风险均拒绝，不接管生产Vault。
双报告保存验证事实；最后SQLite发布事务才允许published。失败保留staged/recovery_required现场。
不自动迁移、修复或修改expected；科学质量和模型仍未运行。
以 - 开头的路径可在动词后加 -- 分隔符，包含空格的路径作为单独参数。
";

pub const PREVIEW_HELP: &str = "Noetome 待核验模型笔记

用法：
  noetome preview choices <项目目录> <import-id>
  noetome preview units <项目目录> <import-id> <选择.json>
  noetome preview export <项目目录> <import-id> <成功API审计根> <新输出根>

选择.json 是严格数组：文本 {kind: text, ordinal: N}，PDF {kind: pdf, page_index: N, region_id: 真实区域ID}。
最多64个单位、64 KiB正文。export 复核已保存的成功 API 请求及原始响应，输出新根/vault 和 reports；知识保持 pending，科学质量 not_run，published false。
只向已有 Obsidian Vault 外的新测试目录导出。
";

pub const EXTRACTION_HELP: &str = "Noetome 有界知识候选验证

用法：noetome extract fake <项目路径> <import-id> <fake目录> <尚不存在的测试输出根> [--cancel-file <取消信号路径>]
      noetome extract api <项目路径> <import-id> <config.json> <尚不存在的测试输出根> [--cancel-file <取消信号路径>]
      noetome extract verify <项目路径> <import-id> <review目录> <尚不存在的测试输出根>

api读取明确配置、外部环境变量凭据引用、模型能力、选定范围与预算，单次请求且不自动重试。
live只支持官方HTTPS OpenAI Responses；local_http_test只向数字环回HTTP假服务发送人工测试凭据/模型。
API保存实际request-body.json、安全原响应、真实用量与未知价格；本地假服务不证明真实模型冒烟或科学质量通过。
配置在来源/输出之外且不保存凭据值；实际来源只读，输出须为全新测试根。

只读取指定导入的真实文本Span或PDF页/区域；fake.json明确选择范围、能力、单请求限制及预算。
fake回放原样候选JSON，Core核对严格契约、来源版本、UTF8证据、条件与未知值表达。
输出父目录须已存在，测试根须全新且位于项目与fake目录之外；不接管已有内容。
保存request.json、原响应response.bin、成功候选candidates.json与validation.json/validation.md。
成功只表示candidates_validated；候选仍unverified，知识pending、published=false、科学质量not_run。
fake以UTF8字节计量，价格及真实模型用量未知，网络和付费调用为0；预算不足或超时明确停止。
--cancel-file指定输入/输出根外的停止信号，运行期间创建该普通文件可取消请求。
语义、预算、超时与取消失败返回3；参数或配置错误返回2；失败不保存已验收候选。
以 - 开头的路径可在fake或api后加 -- 分隔符，中文和空格路径作为单独参数。
";

#[derive(Debug)]
pub enum CliCommand {
    Corpus(corpus::command::Command),
    Help,
    RunHelp,
    ApiRunInit {
        config: PathBuf,
        budget: PathBuf,
    },
    ApiRunStart {
        config: PathBuf,
        budget: PathBuf,
        output: PathBuf,
        paused: bool,
    },
    ApiRunResume {
        config: PathBuf,
        budget: PathBuf,
        output: PathBuf,
    },
    ApiRunStatus {
        budget: PathBuf,
        output: PathBuf,
    },
    RunStart {
        case: PathBuf,
        output: PathBuf,
        paused: bool,
    },
    RunResume {
        case: PathBuf,
        output: PathBuf,
    },
    RunStatus(PathBuf),
    RunPause(PathBuf),
    RunCancel(PathBuf),
    KnowledgeHelp,
    ScanFormula {
        case: PathBuf,
        api_config: PathBuf,
        profile: PathBuf,
        output: PathBuf,
    },
    Image {
        case: PathBuf,
        profile: PathBuf,
        output: PathBuf,
    },
    Formula {
        case: PathBuf,
        profile: PathBuf,
        output: PathBuf,
    },
    ProcessReview {
        project: PathBuf,
        case: PathBuf,
        output: PathBuf,
    },
    ProcessFlow {
        project: PathBuf,
        case: PathBuf,
        reviewed: PathBuf,
        profile: PathBuf,
        output: PathBuf,
    },
    KnowledgeCoordinate {
        project: PathBuf,
        case: PathBuf,
        output: PathBuf,
    },
    KnowledgePresent {
        project: PathBuf,
        case: PathBuf,
        coordinated: PathBuf,
        output: PathBuf,
    },
    ProjectHelp,
    Version,
    ProjectInit(PathBuf),
    ProjectInspect(PathBuf),
    SourceHelp,
    SourceImport {
        project: PathBuf,
        file: PathBuf,
        source_id: Option<String>,
    },
    SourceImportFresh {
        project: PathBuf,
        file: PathBuf,
        source_id: Option<String>,
    },
    SourceInspect {
        project: PathBuf,
        import_id: String,
    },
    SourceParsePdf {
        file: PathBuf,
        output: PathBuf,
    },
    SourceInspectPdf(PathBuf),
    PreviewHelp,
    PreviewChoices {
        project: PathBuf,
        import_id: String,
    },
    PreviewUnits {
        project: PathBuf,
        import_id: String,
        selection: PathBuf,
    },
    PreviewExport {
        project: PathBuf,
        import_id: String,
        api_root: PathBuf,
        output: PathBuf,
    },
    ExtractionHelp,
    ExtractFake {
        project: PathBuf,
        import_id: String,
        case: PathBuf,
        output: PathBuf,
        cancel_file: Option<PathBuf>,
    },
    ExtractApi {
        project: PathBuf,
        import_id: String,
        config: PathBuf,
        output: PathBuf,
        cancel_file: Option<PathBuf>,
    },
    ExtractVerify {
        project: PathBuf,
        import_id: String,
        case: PathBuf,
        output: PathBuf,
    },
    CaseHelp,
    CaseVerify(PathBuf),
    CaseCompile {
        case: PathBuf,
        output: PathBuf,
    },
    VaultHelp,
    VaultAdopt {
        author: PathBuf,
        root: PathBuf,
    },
    VaultPlan {
        root: PathBuf,
        candidate: PathBuf,
        plan: PathBuf,
    },
    VaultApprove {
        root: PathBuf,
        plan: PathBuf,
        approval: PathBuf,
    },
    VaultPublish {
        root: PathBuf,
        approval: PathBuf,
    },
    VaultStatus {
        root: PathBuf,
    },
    VaultRollback {
        root: PathBuf,
        plan: PathBuf,
    },
    ResearchHelp,
    ResearchAdopt {
        author: PathBuf,
        root: PathBuf,
    },
    ResearchObserve {
        root: PathBuf,
        file: PathBuf,
    },
    ResearchDecide {
        root: PathBuf,
        file: PathBuf,
    },
    ResearchList {
        root: PathBuf,
    },
    ResearchStatus {
        root: PathBuf,
    },
    RetrieveHelp,
    RetrieveAdopt {
        plan: PathBuf,
        root: PathBuf,
    },
    RetrieveStep {
        root: PathBuf,
    },
    RetrieveRun {
        root: PathBuf,
    },
    RetrievePause {
        root: PathBuf,
    },
    RetrieveResume {
        root: PathBuf,
    },
    RetrieveFinish {
        root: PathBuf,
        reason: PathBuf,
    },
    RetrieveList {
        root: PathBuf,
    },
    RetrieveStatus {
        root: PathBuf,
    },
    RevisionHelp,
    RevisionAdopt {
        author: PathBuf,
        root: PathBuf,
    },
    RevisionGap {
        root: PathBuf,
        file: PathBuf,
    },
    RevisionRevise {
        root: PathBuf,
        file: PathBuf,
    },
    RevisionList {
        root: PathBuf,
    },
    RevisionStatus {
        root: PathBuf,
    },
    OutlineHelp,
    OutlineAdopt {
        author: PathBuf,
        root: PathBuf,
    },
    OutlineBuild {
        root: PathBuf,
        output: PathBuf,
    },
    OutlineList {
        root: PathBuf,
    },
    OutlineStatus {
        root: PathBuf,
    },
    RelevanceHelp,
    RelevanceAdopt {
        author: PathBuf,
        root: PathBuf,
    },
    RelevanceBuild {
        root: PathBuf,
        output: PathBuf,
    },
    RelevanceList {
        root: PathBuf,
    },
    RelevanceStatus {
        root: PathBuf,
    },
    StructureHelp,
    StructureAdopt {
        family: PathBuf,
        root: PathBuf,
    },
    StructureBuild {
        root: PathBuf,
        output: PathBuf,
    },
    StructureList {
        root: PathBuf,
    },
    StructureStatus {
        root: PathBuf,
    },
    MigrateHelp,
    MigrateAdopt {
        plan: PathBuf,
        root: PathBuf,
    },
    MigrateBuild {
        root: PathBuf,
        output: PathBuf,
    },
    MigrateList {
        root: PathBuf,
    },
    MigrateStatus {
        root: PathBuf,
    },
    IndexHelp,
    IndexAdopt {
        project: PathBuf,
        root: PathBuf,
    },
    IndexBuild {
        root: PathBuf,
        output: PathBuf,
    },
    IndexRelocate {
        root: PathBuf,
        directory: PathBuf,
    },
    IndexList {
        root: PathBuf,
    },
    IndexStatus {
        root: PathBuf,
    },
    AuditHelp,
    AuditAdopt {
        project: PathBuf,
        root: PathBuf,
    },
    AuditBuild {
        root: PathBuf,
        output: PathBuf,
    },
    AuditList {
        root: PathBuf,
    },
    AuditStatus {
        root: PathBuf,
    },
    PlanHelp,
    PlanAdopt {
        project: PathBuf,
        root: PathBuf,
    },
    PlanBuild {
        root: PathBuf,
        output: PathBuf,
    },
    PlanList {
        root: PathBuf,
    },
    PlanStatus {
        root: PathBuf,
    },
    TemplateHelp,
    TemplateAdopt {
        project: PathBuf,
        root: PathBuf,
    },
    TemplateBuild {
        root: PathBuf,
        output: PathBuf,
    },
    TemplateList {
        root: PathBuf,
    },
    TemplateStatus {
        root: PathBuf,
    },
    ScaleHelp,
    ScaleMeasure {
        pages: String,
        headings: String,
        paragraphs: String,
        output: PathBuf,
    },
    BenchHelp,
    BenchMeasure {
        notes: String,
        edits: String,
        samples: String,
        warmup: String,
        output: PathBuf,
    },
    SoakHelp,
    SoakRun {
        rounds: String,
        interval_ms: String,
        pause_after_ms: Option<String>,
        max_rss_growth_bytes: String,
        output: PathBuf,
    },
    ChangeHelp,
    ChangeAdopt {
        project: PathBuf,
        root: PathBuf,
    },
    ChangeBuild {
        root: PathBuf,
        output: PathBuf,
    },
    ChangeList {
        root: PathBuf,
    },
    ChangeStatus {
        root: PathBuf,
    },
    JournalHelp,
    JournalAdopt {
        project: PathBuf,
        root: PathBuf,
    },
    JournalBuild {
        root: PathBuf,
        output: PathBuf,
    },
    JournalList {
        root: PathBuf,
    },
    JournalStatus {
        root: PathBuf,
    },
    DiagnosticsHelp,
    DiagnosticsAdopt {
        project: PathBuf,
        root: PathBuf,
    },
    DiagnosticsBuild {
        root: PathBuf,
        output: PathBuf,
    },
    DiagnosticsList {
        root: PathBuf,
    },
    DiagnosticsStatus {
        root: PathBuf,
    },
    RetentionHelp,
    RetentionAdopt {
        project: PathBuf,
        root: PathBuf,
    },
    RetentionBuild {
        root: PathBuf,
        output: PathBuf,
    },
    RetentionList {
        root: PathBuf,
    },
    RetentionStatus {
        root: PathBuf,
    },
}

/// Parse without touching the filesystem. Paths remain native OS arguments.
pub fn parse(args: &[OsString]) -> Result<CliCommand, &'static str> {
    if args.first().is_some_and(|arg| arg == "corpus") {
        return corpus::command::parse(&args[1..]).map(CliCommand::Corpus);
    }
    if args.first().is_some_and(|arg| arg == "run") {
        return run_command(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "knowledge") {
        return knowledge_command(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "case") {
        return case_command(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "source") {
        return source_command(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "preview") {
        return preview_command(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "extract") {
        return extraction_command(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "vault") {
        return vault_command(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "research") {
        return research_command(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "retrieve") {
        return retrieve_command(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "revision") {
        return revision_command(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "outline") {
        return outline_command(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "relevance") {
        return relevance_command(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "structure") {
        return structure_command(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "migrate") {
        return migrate_command(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "index") {
        return index_command(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "audit") {
        return audit_command(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "plan") {
        return plan_command(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "template") {
        return template_command(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "change") {
        return change_command(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "scale") {
        return scale_command(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "bench") {
        return bench_command(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "soak") {
        return soak_command(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "journal") {
        return journal_command(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "diagnostics") {
        return diagnostics_command(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "retention") {
        return retention_command(&args[1..]);
    }
    match args {
        [] => Ok(CliCommand::Help),
        [arg] if arg == "--help" || arg == "-h" => Ok(CliCommand::Help),
        [arg] if arg == "--version" || arg == "-V" => Ok(CliCommand::Version),
        [project] if project == "project" => Ok(CliCommand::ProjectHelp),
        [project, help] if project == "project" && (help == "--help" || help == "-h") => {
            Ok(CliCommand::ProjectHelp)
        }
        [project, verb, help]
            if project == "project"
                && (verb == "init" || verb == "inspect")
                && (help == "--help" || help == "-h") =>
        {
            Ok(CliCommand::ProjectHelp)
        }
        [project, verb, path] if project == "project" => project_command(verb, path, false),
        [project, verb, separator, path] if project == "project" && separator == "--" => {
            project_command(verb, path, true)
        }
        _ => Err(INVALID_ARGUMENT),
    }
}

pub const VAULT_HELP: &str = "Noetome 托管测试集合的人工审批与可恢复发布

用法：noetome vault adopt <author.json> <尚不存在的托管根>
      noetome vault plan <托管根> <候选目录> <尚不存在的计划文件>
      noetome vault approve <托管根> <计划文件> <尚不存在的批准文件>
      noetome vault publish <托管根> <批准文件>
      noetome vault status <托管根>
      noetome vault rollback <托管根> <回滚计划文件>

adopt：只接受明确作者包与全新目录；baseline逐字节冻结，vault复制为初始已发布态。
plan：把候选操作与当前磁盘、基准三方比对，只写计划文件；删除一律要求批准阶段显式携带。
approve：绑定计划、受影响路径当前字节、影响清单与owner指纹；输出批准文件。
publish：写前重采集指纹，任何变化即approval-stale；stage→verify→commit逐项落盘，中断后status报needs_recovery可续。
status：只读核对托管根布局、账本与待恢复状态。
rollback：生成反向计划文件；发布后人工改过的路径以rollback-conflict停止。
仅操作新的测试根；不接管生产Vault；不承诺任意断电原子性。
";

fn vault_command(args: &[OsString]) -> Result<CliCommand, &'static str> {
    const VERBS: [&str; 6] = ["adopt", "plan", "approve", "publish", "status", "rollback"];
    if args.is_empty() || matches!(args, [help] if help == "--help" || help == "-h") {
        return Ok(CliCommand::VaultHelp);
    }
    let (verb, tail) = args.split_first().ok_or(INVALID_ARGUMENT)?;
    if matches!(tail, [help] if help == "--help" || help == "-h")
        && VERBS.iter().any(|candidate| verb == candidate)
    {
        return Ok(CliCommand::VaultHelp);
    }
    let (tail, separated) = if tail.first().is_some_and(|arg| arg == "--") {
        (&tail[1..], true)
    } else {
        (tail, false)
    };
    let valid = |path: &OsString| {
        !path.is_empty() && (separated || path.as_encoded_bytes().first() != Some(&b'-'))
    };
    match tail {
        [author, root] if verb == "adopt" && valid(author) && valid(root) => {
            Ok(CliCommand::VaultAdopt {
                author: author.into(),
                root: root.into(),
            })
        }
        [root, candidate, plan]
            if verb == "plan" && valid(root) && valid(candidate) && valid(plan) =>
        {
            Ok(CliCommand::VaultPlan {
                root: root.into(),
                candidate: candidate.into(),
                plan: plan.into(),
            })
        }
        [root, plan, approval]
            if verb == "approve" && valid(root) && valid(plan) && valid(approval) =>
        {
            Ok(CliCommand::VaultApprove {
                root: root.into(),
                plan: plan.into(),
                approval: approval.into(),
            })
        }
        [root, approval] if verb == "publish" && valid(root) && valid(approval) => {
            Ok(CliCommand::VaultPublish {
                root: root.into(),
                approval: approval.into(),
            })
        }
        [root] if verb == "status" && valid(root) => {
            Ok(CliCommand::VaultStatus { root: root.into() })
        }
        [root, plan] if verb == "rollback" && valid(root) && valid(plan) => {
            Ok(CliCommand::VaultRollback {
                root: root.into(),
                plan: plan.into(),
            })
        }
        _ => Err(INVALID_ARGUMENT),
    }
}

pub const RESEARCH_HELP: &str = "Noetome 研究候选队列、授权边界与拒绝记忆

用法：noetome research adopt <author.json> <尚不存在的队列根>
      noetome research observe <队列根> <观测文件>
      noetome research decide <队列根> <决定文件>
      noetome research list <队列根>
      noetome research status <队列根>

adopt：只接受明确作者包与全新目录；冻结本轮目标、必做集合与上限，之后不接受增删。
observe：登记一条发现的候选主题（主题/发现原因/关系/收益/来源线索/代价/依赖/状态）。
decide：接受、拒绝、延后、合并或修改范围；每条决定留痕，拒绝结果会被记住。
list：只读输出候选与计数的完整JSON。
status：只读核对队列布局、拒绝记忆与必做集合。
越界主题只进候选队列，不改变本轮必做集合；相关性只用于排序，不代替授权。
队列从不联网、不调用模型：候选只是待批准线索，不是已完成知识。
仅操作新的测试根；不接管生产项目或生产Vault。
";

fn research_command(args: &[OsString]) -> Result<CliCommand, &'static str> {
    const VERBS: [&str; 5] = ["adopt", "observe", "decide", "list", "status"];
    if args.is_empty() || matches!(args, [help] if help == "--help" || help == "-h") {
        return Ok(CliCommand::ResearchHelp);
    }
    let (verb, tail) = args.split_first().ok_or(INVALID_ARGUMENT)?;
    if matches!(tail, [help] if help == "--help" || help == "-h")
        && VERBS.iter().any(|candidate| verb == candidate)
    {
        return Ok(CliCommand::ResearchHelp);
    }
    let (tail, separated) = if tail.first().is_some_and(|arg| arg == "--") {
        (&tail[1..], true)
    } else {
        (tail, false)
    };
    let valid = |path: &OsString| {
        !path.is_empty() && (separated || path.as_encoded_bytes().first() != Some(&b'-'))
    };
    match tail {
        [author, root] if verb == "adopt" && valid(author) && valid(root) => {
            Ok(CliCommand::ResearchAdopt {
                author: author.into(),
                root: root.into(),
            })
        }
        [root, file] if verb == "observe" && valid(root) && valid(file) => {
            Ok(CliCommand::ResearchObserve {
                root: root.into(),
                file: file.into(),
            })
        }
        [root, file] if verb == "decide" && valid(root) && valid(file) => {
            Ok(CliCommand::ResearchDecide {
                root: root.into(),
                file: file.into(),
            })
        }
        [root] if verb == "list" && valid(root) => {
            Ok(CliCommand::ResearchList { root: root.into() })
        }
        [root] if verb == "status" && valid(root) => {
            Ok(CliCommand::ResearchStatus { root: root.into() })
        }
        _ => Err(INVALID_ARGUMENT),
    }
}

pub const RETRIEVAL_HELP: &str = "Noetome 批准范围内的有界检索与终止理由

用法：noetome retrieve adopt <plan.json> <尚不存在的检索根>
      noetome retrieve step <检索根>
      noetome retrieve run <检索根>
      noetome retrieve pause <检索根>
      noetome retrieve resume <检索根>
      noetome retrieve finish <检索根> <终止理由文件>
      noetome retrieve list <检索根>
      noetome retrieve status <检索根>

adopt：只接受明确作者包与全新目录；冻结语料、预算上限与待执行查询，之后不接受增删。
step：执行一条待结算查询；先持久预留预算，再扫描，最后结算。
run：连续执行到没有待执行查询、预算到限、暂停或已记终止理由。
pause|resume：停止或恢复调度新查询；已预留的尝试只补结算一次，不重复扣减。
finish：记录终止理由；理由必须有账本证据支撑，否则拒绝且不写任何字节。
list|status：只读输出预算、查询状态与终止理由的完整记录。
检索只在已授权的目标与必需前提内进行；越界主题必须先进入候选队列并由人工批准。
关闭外部检索时零网络请求；命中只是证据线索，不写入知识库或托管Vault。
仅操作新的测试根；不接管生产项目或生产Vault。
";

fn retrieve_command(args: &[OsString]) -> Result<CliCommand, &'static str> {
    const VERBS: [&str; 8] = [
        "adopt", "step", "run", "pause", "resume", "finish", "list", "status",
    ];
    if args.is_empty() || matches!(args, [help] if help == "--help" || help == "-h") {
        return Ok(CliCommand::RetrieveHelp);
    }
    let (verb, tail) = args.split_first().ok_or(INVALID_ARGUMENT)?;
    if matches!(tail, [help] if help == "--help" || help == "-h")
        && VERBS.iter().any(|candidate| verb == candidate)
    {
        return Ok(CliCommand::RetrieveHelp);
    }
    let (tail, separated) = if tail.first().is_some_and(|arg| arg == "--") {
        (&tail[1..], true)
    } else {
        (tail, false)
    };
    let valid = |path: &OsString| {
        !path.is_empty() && (separated || path.as_encoded_bytes().first() != Some(&b'-'))
    };
    match tail {
        [plan, root] if verb == "adopt" && valid(plan) && valid(root) => {
            Ok(CliCommand::RetrieveAdopt {
                plan: plan.into(),
                root: root.into(),
            })
        }
        [root] if verb == "step" && valid(root) => {
            Ok(CliCommand::RetrieveStep { root: root.into() })
        }
        [root] if verb == "run" && valid(root) => Ok(CliCommand::RetrieveRun { root: root.into() }),
        [root] if verb == "pause" && valid(root) => {
            Ok(CliCommand::RetrievePause { root: root.into() })
        }
        [root] if verb == "resume" && valid(root) => {
            Ok(CliCommand::RetrieveResume { root: root.into() })
        }
        [root, reason] if verb == "finish" && valid(root) && valid(reason) => {
            Ok(CliCommand::RetrieveFinish {
                root: root.into(),
                reason: reason.into(),
            })
        }
        [root] if verb == "list" && valid(root) => {
            Ok(CliCommand::RetrieveList { root: root.into() })
        }
        [root] if verb == "status" && valid(root) => {
            Ok(CliCommand::RetrieveStatus { root: root.into() })
        }
        _ => Err(INVALID_ARGUMENT),
    }
}

pub const REVISION_HELP: &str = "Noetome 写作反馈缺口、修订与依赖重编译

用法：noetome revision adopt <author.json> <尚不存在的修订根>
      noetome revision gap <修订根> <缺口文件>
      noetome revision revise <修订根> <修订文件>
      noetome revision list <修订根>
      noetome revision status <修订根>

adopt：只接受明确作者包与全新目录；冻结原子、表示与语料，建立IR v1并渲染三种表示。
gap：登记写作/表格/绘图发现的缺口；影响范围必须恰好等于引用该原子的表示，缺口与研究候选分开存放。
revise：校验来源后生成新IR版本，只重编译声明依赖的表示，其他表示字节必须不变。
list|status：只读输出版本、缺口、修订与核验分布。
无来源补充标为待核验，不发布为结论；同一问题反复修订记录版本、原因与结果。
超过可配置上限即升级为人工判断；改回旧值而没有新来源会被拒绝。
仅操作新的测试根；不接管生产项目或生产Vault；不联网、不调用模型。
";

fn revision_command(args: &[OsString]) -> Result<CliCommand, &'static str> {
    const VERBS: [&str; 5] = ["adopt", "gap", "revise", "list", "status"];
    if args.is_empty() || matches!(args, [help] if help == "--help" || help == "-h") {
        return Ok(CliCommand::RevisionHelp);
    }
    let (verb, tail) = args.split_first().ok_or(INVALID_ARGUMENT)?;
    if matches!(tail, [help] if help == "--help" || help == "-h")
        && VERBS.iter().any(|candidate| verb == candidate)
    {
        return Ok(CliCommand::RevisionHelp);
    }
    let (tail, separated) = if tail.first().is_some_and(|arg| arg == "--") {
        (&tail[1..], true)
    } else {
        (tail, false)
    };
    let valid = |path: &OsString| {
        !path.is_empty() && (separated || path.as_encoded_bytes().first() != Some(&b'-'))
    };
    match tail {
        [author, root] if verb == "adopt" && valid(author) && valid(root) => {
            Ok(CliCommand::RevisionAdopt {
                author: author.into(),
                root: root.into(),
            })
        }
        [root, file] if verb == "gap" && valid(root) && valid(file) => {
            Ok(CliCommand::RevisionGap {
                root: root.into(),
                file: file.into(),
            })
        }
        [root, file] if verb == "revise" && valid(root) && valid(file) => {
            Ok(CliCommand::RevisionRevise {
                root: root.into(),
                file: file.into(),
            })
        }
        [root] if verb == "list" && valid(root) => {
            Ok(CliCommand::RevisionList { root: root.into() })
        }
        [root] if verb == "status" && valid(root) => {
            Ok(CliCommand::RevisionStatus { root: root.into() })
        }
        _ => Err(INVALID_ARGUMENT),
    }
}

pub const OUTLINE_HELP: &str = "Noetome 章节粗对齐与全局概念清单

用法：noetome outline adopt <author.json> <尚不存在的对齐根>
      noetome outline build <对齐根> <尚不存在的输出根>
      noetome outline list <对齐根>
      noetome outline status <对齐根>

adopt：只接受明确作者包与全新目录；逐份文档核对字节、规范化摘要与文本片段后冻结，全部通过才建根。
build：从冻结输入重新派生章节树、概念清单与对齐报告，写入全新输出根；同一冻结输入的两次构建必须字节相同（输出清单不含时间戳）。
list|status：只读重算并输出章节数、概念出现次数与七类对齐警告。
章节只做粗对齐：缺编号、重复编号、跳级、图注或附录独有概念都只报警告，不自动改标题或正文。
不联网、不调用模型、不写项目与Vault；本阶段不合并章节、不生成知识、不改写来源。
";

fn outline_command(args: &[OsString]) -> Result<CliCommand, &'static str> {
    const VERBS: [&str; 4] = ["adopt", "build", "list", "status"];
    if args.is_empty() || matches!(args, [help] if help == "--help" || help == "-h") {
        return Ok(CliCommand::OutlineHelp);
    }
    let (verb, tail) = args.split_first().ok_or(INVALID_ARGUMENT)?;
    if matches!(tail, [help] if help == "--help" || help == "-h")
        && VERBS.iter().any(|candidate| verb == candidate)
    {
        return Ok(CliCommand::OutlineHelp);
    }
    let (tail, separated) = if tail.first().is_some_and(|arg| arg == "--") {
        (&tail[1..], true)
    } else {
        (tail, false)
    };
    let valid = |path: &OsString| {
        !path.is_empty() && (separated || path.as_encoded_bytes().first() != Some(&b'-'))
    };
    match tail {
        [author, root] if verb == "adopt" && valid(author) && valid(root) => {
            Ok(CliCommand::OutlineAdopt {
                author: author.into(),
                root: root.into(),
            })
        }
        [root, output] if verb == "build" && valid(root) && valid(output) => {
            Ok(CliCommand::OutlineBuild {
                root: root.into(),
                output: output.into(),
            })
        }
        [root] if verb == "list" && valid(root) => {
            Ok(CliCommand::OutlineList { root: root.into() })
        }
        [root] if verb == "status" && valid(root) => {
            Ok(CliCommand::OutlineStatus { root: root.into() })
        }
        _ => Err(INVALID_ARGUMENT),
    }
}

pub const RELEVANCE_HELP: &str = "Noetome 用户水平与相关性筛选

用法：noetome relevance adopt <author.json> <尚不存在的筛选根>
      noetome relevance build <筛选根> <尚不存在的输出根>
      noetome relevance list <筛选根>
      noetome relevance status <筛选根>

adopt：只接受明确作者包与全新目录；核对用户水平、原子清单、排除登记与每份来源的字节、规范化摘要和文本片段后冻结，全部通过才建根。
build：从冻结输入重新做保留/排除决策并编译极简与详细两种密度，写入全新输出根；同一冻结输入的两次构建必须字节相同（输出清单不含时间戳）。
list|status：只读重算并输出保留数、排除数、排除理由分布、原子保留与暂缓数以及两种密度的一致性。
排除必须登记理由并落在真实正文片段上：理由未登记、落在标题或空行、必要原子被排除、必要前提缺失，都会拒绝整包而不是静默省略。
极简与详细只差展开程度：必要原子与适用条件集合必须完全一致，计数可复算。
不联网、不调用模型、不写项目与Vault；本阶段不判断科学正确性、不改写来源、不删除冻结片段。
";

fn relevance_command(args: &[OsString]) -> Result<CliCommand, &'static str> {
    const VERBS: [&str; 4] = ["adopt", "build", "list", "status"];
    if args.is_empty() || matches!(args, [help] if help == "--help" || help == "-h") {
        return Ok(CliCommand::RelevanceHelp);
    }
    let (verb, tail) = args.split_first().ok_or(INVALID_ARGUMENT)?;
    if matches!(tail, [help] if help == "--help" || help == "-h")
        && VERBS.iter().any(|candidate| verb == candidate)
    {
        return Ok(CliCommand::RelevanceHelp);
    }
    let (tail, separated) = if tail.first().is_some_and(|arg| arg == "--") {
        (&tail[1..], true)
    } else {
        (tail, false)
    };
    let valid = |path: &OsString| {
        !path.is_empty() && (separated || path.as_encoded_bytes().first() != Some(&b'-'))
    };
    match tail {
        [author, root] if verb == "adopt" && valid(author) && valid(root) => {
            Ok(CliCommand::RelevanceAdopt {
                author: author.into(),
                root: root.into(),
            })
        }
        [root, output] if verb == "build" && valid(root) && valid(output) => {
            Ok(CliCommand::RelevanceBuild {
                root: root.into(),
                output: output.into(),
            })
        }
        [root] if verb == "list" && valid(root) => {
            Ok(CliCommand::RelevanceList { root: root.into() })
        }
        [root] if verb == "status" && valid(root) => {
            Ok(CliCommand::RelevanceStatus { root: root.into() })
        }
        _ => Err(INVALID_ARGUMENT),
    }
}

pub const STRUCTURE_HELP: &str = "Noetome 同类结构一致性与导航复习组织

用法：noetome structure adopt <family.json> <尚不存在的结构根>
      noetome structure build <结构根> <尚不存在的输出根>
      noetome structure list <结构根>
      noetome structure status <结构根>

adopt：只接受明确作者包与全新目录；核对骨架、概念表、每份笔记的字节与规范化摘要，并按规则预演一次判定，全部通过才建根。
build：从冻结输入重算章节骨架一致性、概念唯一解释位置与两类链接并写出六文件输出根；同一冻结输入的两次构建必须字节相同（输出清单不含时间戳）。
list|status：只读重算并输出一致性计数、链接分类计数与断链数。
同类笔记必须共享同一骨架：缺必填、错位、多余与空节只报告位置，不自动改写正文、不静默补齐。
同一概念只能有一处完整解释：多处完整解释、引用未登记概念、表格内未转义别名竖线都会拒绝整包。
生成链接只指向本族已声明笔记与标题：断链必须为 0，否则不写报告。
不联网、不调用模型、不写项目与Vault；本阶段不判断内容质量与科学正确性。
";

fn structure_command(args: &[OsString]) -> Result<CliCommand, &'static str> {
    const VERBS: [&str; 4] = ["adopt", "build", "list", "status"];
    if args.is_empty() || matches!(args, [help] if help == "--help" || help == "-h") {
        return Ok(CliCommand::StructureHelp);
    }
    let (verb, tail) = args.split_first().ok_or(INVALID_ARGUMENT)?;
    if matches!(tail, [help] if help == "--help" || help == "-h")
        && VERBS.iter().any(|candidate| verb == candidate)
    {
        return Ok(CliCommand::StructureHelp);
    }
    let (tail, separated) = if tail.first().is_some_and(|arg| arg == "--") {
        (&tail[1..], true)
    } else {
        (tail, false)
    };
    let valid = |path: &OsString| {
        !path.is_empty() && (separated || path.as_encoded_bytes().first() != Some(&b'-'))
    };
    match tail {
        [family, root] if verb == "adopt" && valid(family) && valid(root) => {
            Ok(CliCommand::StructureAdopt {
                family: family.into(),
                root: root.into(),
            })
        }
        [root, output] if verb == "build" && valid(root) && valid(output) => {
            Ok(CliCommand::StructureBuild {
                root: root.into(),
                output: output.into(),
            })
        }
        [root] if verb == "list" && valid(root) => {
            Ok(CliCommand::StructureList { root: root.into() })
        }
        [root] if verb == "status" && valid(root) => {
            Ok(CliCommand::StructureStatus { root: root.into() })
        }
        _ => Err(INVALID_ARGUMENT),
    }
}

pub const MIGRATE_HELP: &str = "Noetome 小节提升为独立笔记与链接批量更新

用法：noetome migrate adopt <plan.json> <尚不存在的迁移根>
      noetome migrate build <迁移根> <尚不存在的输出根>
      noetome migrate list <迁移根>
      noetome migrate status <迁移根>

adopt：只接受明确迁移计划与全新目录；核对计划、知识对象表与两侧笔记字节，按规则预演一次判定，全部通过才建根。
build：从冻结输入重算两侧链接解析、知识对象身份保持与每份笔记的核对结果并写出五文件输出根；同一冻结输入的两次构建必须字节相同（输出清单不含时间戳）。
list|status：只读重算并输出迁移前/后链接计数、规范位置与重定向位置条数、重写目标条数与断链数。
引用按知识对象ID而不是文件名核对：小节提升后概念与原子ID不变，重定向位置也算指向同一对象。
无法安全迁移时整包拒绝：目标缺失、目标有歧义、表格内未转义别名竖线、声明位置不存在、身份丢失或断链非零都不写报告。
不联网、不调用模型、不写项目与Vault；本阶段不改写作者包、不搬动文件，也不判断内容质量与科学正确性。
";

fn migrate_command(args: &[OsString]) -> Result<CliCommand, &'static str> {
    const VERBS: [&str; 4] = ["adopt", "build", "list", "status"];
    if args.is_empty() || matches!(args, [help] if help == "--help" || help == "-h") {
        return Ok(CliCommand::MigrateHelp);
    }
    let (verb, tail) = args.split_first().ok_or(INVALID_ARGUMENT)?;
    if matches!(tail, [help] if help == "--help" || help == "-h")
        && VERBS.iter().any(|candidate| verb == candidate)
    {
        return Ok(CliCommand::MigrateHelp);
    }
    let (tail, separated) = if tail.first().is_some_and(|arg| arg == "--") {
        (&tail[1..], true)
    } else {
        (tail, false)
    };
    let valid = |path: &OsString| {
        !path.is_empty() && (separated || path.as_encoded_bytes().first() != Some(&b'-'))
    };
    match tail {
        [plan, root] if verb == "adopt" && valid(plan) && valid(root) => {
            Ok(CliCommand::MigrateAdopt {
                plan: plan.into(),
                root: root.into(),
            })
        }
        [root, output] if verb == "build" && valid(root) && valid(output) => {
            Ok(CliCommand::MigrateBuild {
                root: root.into(),
                output: output.into(),
            })
        }
        [root] if verb == "list" && valid(root) => {
            Ok(CliCommand::MigrateList { root: root.into() })
        }
        [root] if verb == "status" && valid(root) => {
            Ok(CliCommand::MigrateStatus { root: root.into() })
        }
        _ => Err(INVALID_ARGUMENT),
    }
}

pub const INDEX_HELP: &str = "Noetome 可移植来源索引与原始资料重定位

用法：noetome index adopt <工程声明> <尚不存在的索引根>
      noetome index build <索引根> <尚不存在的输出根>
      noetome index relocate <索引根> <目录>
      noetome index list <索引根>
      noetome index status <索引根>

adopt：核对输出区/运行库/临时区互不包含、正文来源负担与来源章节是否真实存在，先按规则预演一次判定，全部通过才建根。
build：在全新输出根里写出 index.json、relocation.json、report.json、report.md 与输出清单；同一冻结输入的两次构建必须字节相同（输出清单不含时间戳）。
relocate：只读扫描一个目录，按内容摘要判断每条来源现在落在哪里；缺失或摘要不符只提示需要重建映射，不指向任何页面。
list|status：只读重算并输出区域、计数、正文负担与重定位结果。
正文保持干净：不插入文献编号、逐句脚注或出处列表，出现即整包拒绝。
运行数据库、缓存、密钥与临时文件不得进入输出区；索引只写相对路径。
不联网、不调用模型、不写项目与Vault；本阶段不做阅读器与编辑器。
";

fn index_command(args: &[OsString]) -> Result<CliCommand, &'static str> {
    const VERBS: [&str; 5] = ["adopt", "build", "relocate", "list", "status"];
    if args.is_empty() || matches!(args, [help] if help == "--help" || help == "-h") {
        return Ok(CliCommand::IndexHelp);
    }
    let (verb, tail) = args.split_first().ok_or(INVALID_ARGUMENT)?;
    if matches!(tail, [help] if help == "--help" || help == "-h")
        && VERBS.iter().any(|candidate| verb == candidate)
    {
        return Ok(CliCommand::IndexHelp);
    }
    let (tail, separated) = if tail.first().is_some_and(|arg| arg == "--") {
        (&tail[1..], true)
    } else {
        (tail, false)
    };
    let valid = |path: &OsString| {
        !path.is_empty() && (separated || path.as_encoded_bytes().first() != Some(&b'-'))
    };
    match tail {
        [project, root] if verb == "adopt" && valid(project) && valid(root) => {
            Ok(CliCommand::IndexAdopt {
                project: project.into(),
                root: root.into(),
            })
        }
        [root, output] if verb == "build" && valid(root) && valid(output) => {
            Ok(CliCommand::IndexBuild {
                root: root.into(),
                output: output.into(),
            })
        }
        [root, directory] if verb == "relocate" && valid(root) && valid(directory) => {
            Ok(CliCommand::IndexRelocate {
                root: root.into(),
                directory: directory.into(),
            })
        }
        [root] if verb == "list" && valid(root) => Ok(CliCommand::IndexList { root: root.into() }),
        [root] if verb == "status" && valid(root) => {
            Ok(CliCommand::IndexStatus { root: root.into() })
        }
        _ => Err(INVALID_ARGUMENT),
    }
}

pub const AUDIT_HELP: &str = "Noetome 全库链接可用性与唯一解释位置审计

用法：noetome audit adopt <工程声明> <尚不存在的审计根>
      noetome audit build <审计根> <尚不存在的输出根>
      noetome audit list <审计根>
      noetome audit status <审计根>

adopt：核对声明、金标准清单与每份笔记，先按规则预演一次审计，只拦声明错误，全部通过才建根。
build：在全新输出根里写出 links.json、explanations.json、report.json、report.md 与输出清单；同一冻结输入的两次构建必须字节相同（输出清单不含时间戳）。
list|status：只读重算并输出结论、口径、召回率与缺口清单。
链接只有在笔记与标题都唯一解析时才算可用：同名标题、同名笔记与目标缺失都记为缺口并逐条列名。
「完整解释」只认行首的 > 完整解释：<概念ID> 标记，同一概念出现两处即记为重复解释。
有缺口时仍然写出完整报告，只把退出码置为非零——审计的产出就是结论本身，缺口必须逐条可见。
不联网、不调用模型、不写项目与Vault；本阶段不判断内容质量与科学正确性。
";

fn audit_command(args: &[OsString]) -> Result<CliCommand, &'static str> {
    const VERBS: [&str; 4] = ["adopt", "build", "list", "status"];
    if args.is_empty() || matches!(args, [help] if help == "--help" || help == "-h") {
        return Ok(CliCommand::AuditHelp);
    }
    let (verb, tail) = args.split_first().ok_or(INVALID_ARGUMENT)?;
    if matches!(tail, [help] if help == "--help" || help == "-h")
        && VERBS.iter().any(|candidate| verb == candidate)
    {
        return Ok(CliCommand::AuditHelp);
    }
    let (tail, separated) = if tail.first().is_some_and(|arg| arg == "--") {
        (&tail[1..], true)
    } else {
        (tail, false)
    };
    let valid = |path: &OsString| {
        !path.is_empty() && (separated || path.as_encoded_bytes().first() != Some(&b'-'))
    };
    match tail {
        [project, root] if verb == "adopt" && valid(project) && valid(root) => {
            Ok(CliCommand::AuditAdopt {
                project: project.into(),
                root: root.into(),
            })
        }
        [root, output] if verb == "build" && valid(root) && valid(output) => {
            Ok(CliCommand::AuditBuild {
                root: root.into(),
                output: output.into(),
            })
        }
        [root] if verb == "list" && valid(root) => Ok(CliCommand::AuditList { root: root.into() }),
        [root] if verb == "status" && valid(root) => {
            Ok(CliCommand::AuditStatus { root: root.into() })
        }
        _ => Err(INVALID_ARGUMENT),
    }
}

pub const RUN_HELP: &str = "Noetome 有限任务与人工环回预算\n\n用法：noetome run start <fixture目录> <尚不存在的运行根> [--paused]\n      noetome run resume <同一fixture目录> <明确拥有的运行根>\n      noetome run status|pause|cancel <运行根>\n      noetome run api init <authority-config.json> <尚不存在的预算根>\n      noetome run api start <run-spec.json> <预算根> <尚不存在的运行根> [--paused]\n      noetome run api resume <同一run-spec.json> <预算根> <运行根>\n      noetome run api status <预算根> <运行根>\n\nfixture start校验明确fixture后建立六单元持久任务；--paused仅建立并暂停。\nfixture resume核对同一输入、版本和检查点，复用确认结果，不重复发布。\nstatus只读核对；fixture pause停止后续调度，cancel保留确认结果且不可继续。\napi仅支持人工数字环回Responses；最多两并发，发送前同时预留Run/共享预算；只重试固定HTTP故障。\napi init拒绝已有根；新Run不重置全局额度。未知远端完成保留完整预留，价格未知为null。\napi resume复用确认原件，原绝对期限与尝试次数不重置；api status不发送或结算。\n输入/策略失效或未知现场不会被接管；仅新测试根，不修改生产Vault。\n动词后可加 -- 分隔以 - 开头的路径；中文和空格路径作为单独参数。\n自造fixture和人工HTTP服务不代表真实模型、账单或科学质量已通过。\n";

fn run_command(args: &[OsString]) -> Result<CliCommand, &'static str> {
    if args.is_empty() || matches!(args,[help] if help=="--help" || help=="-h") {
        return Ok(CliCommand::RunHelp);
    }
    let (verb, tail) = args.split_first().ok_or(INVALID_ARGUMENT)?;
    if verb == "api" {
        return api_run_command(tail);
    }
    if matches!(tail,[help] if help=="--help" || help=="-h")
        && ["start", "resume", "status", "pause", "cancel"]
            .iter()
            .any(|v| verb == v)
    {
        return Ok(CliCommand::RunHelp);
    }
    let (tail, separated) = if tail.first().is_some_and(|p| p == "--") {
        (&tail[1..], true)
    } else {
        (tail, false)
    };
    let valid =
        |p: &OsString| !p.is_empty() && (separated || p.as_encoded_bytes().first() != Some(&b'-'));
    match tail {
        [case, output] if verb == "start" && valid(case) && valid(output) => {
            Ok(CliCommand::RunStart {
                case: case.into(),
                output: output.into(),
                paused: false,
            })
        }
        [case, output, flag]
            if verb == "start" && flag == "--paused" && valid(case) && valid(output) =>
        {
            Ok(CliCommand::RunStart {
                case: case.into(),
                output: output.into(),
                paused: true,
            })
        }
        [case, output] if verb == "resume" && valid(case) && valid(output) => {
            Ok(CliCommand::RunResume {
                case: case.into(),
                output: output.into(),
            })
        }
        [root] if verb == "status" && valid(root) => Ok(CliCommand::RunStatus(root.into())),
        [root] if verb == "pause" && valid(root) => Ok(CliCommand::RunPause(root.into())),
        [root] if verb == "cancel" && valid(root) => Ok(CliCommand::RunCancel(root.into())),
        _ => Err(INVALID_ARGUMENT),
    }
}

fn api_run_command(args: &[OsString]) -> Result<CliCommand, &'static str> {
    if args.is_empty() || matches!(args, [help] if help == "--help" || help == "-h") {
        return Ok(CliCommand::RunHelp);
    }
    let (verb, tail) = args.split_first().ok_or(INVALID_ARGUMENT)?;
    if matches!(tail, [help] if help == "--help" || help == "-h")
        && ["init", "start", "resume", "status"]
            .iter()
            .any(|v| verb == v)
    {
        return Ok(CliCommand::RunHelp);
    }
    let (tail, separated) = if tail.first().is_some_and(|p| p == "--") {
        (&tail[1..], true)
    } else {
        (tail, false)
    };
    let valid =
        |p: &OsString| !p.is_empty() && (separated || p.as_encoded_bytes().first() != Some(&b'-'));
    match tail {
        [config, budget] if verb == "init" && valid(config) && valid(budget) => {
            Ok(CliCommand::ApiRunInit {
                config: config.into(),
                budget: budget.into(),
            })
        }
        [config, budget, output]
            if verb == "start" && valid(config) && valid(budget) && valid(output) =>
        {
            Ok(CliCommand::ApiRunStart {
                config: config.into(),
                budget: budget.into(),
                output: output.into(),
                paused: false,
            })
        }
        [config, budget, output, flag]
            if verb == "start"
                && flag == "--paused"
                && valid(config)
                && valid(budget)
                && valid(output) =>
        {
            Ok(CliCommand::ApiRunStart {
                config: config.into(),
                budget: budget.into(),
                output: output.into(),
                paused: true,
            })
        }
        [config, budget, output]
            if verb == "resume" && valid(config) && valid(budget) && valid(output) =>
        {
            Ok(CliCommand::ApiRunResume {
                config: config.into(),
                budget: budget.into(),
                output: output.into(),
            })
        }
        [budget, output] if verb == "status" && valid(budget) && valid(output) => {
            Ok(CliCommand::ApiRunStatus {
                budget: budget.into(),
                output: output.into(),
            })
        }
        _ => Err(INVALID_ARGUMENT),
    }
}

fn preview_command(args: &[OsString]) -> Result<CliCommand, &'static str> {
    if args.is_empty()
        || matches!(args,[help] if help == "--help" || help == "-h")
        || matches!(args,[verb,help] if (verb == "choices" || verb == "units" || verb == "export") && (help == "--help" || help == "-h"))
    {
        return Ok(CliCommand::PreviewHelp);
    }
    let (verb, tail) = args.split_first().ok_or(INVALID_ARGUMENT)?;
    let (tail, separator) = if tail.first().is_some_and(|arg| arg == "--") {
        (&tail[1..], true)
    } else {
        (tail, false)
    };
    let valid = |value: &OsString| {
        !value.is_empty() && (separator || value.as_encoded_bytes().first() != Some(&b'-'))
    };
    match tail {
        [project, id] if verb == "choices" && valid(project) => Ok(CliCommand::PreviewChoices {
            project: project.into(),
            import_id: id
                .to_str()
                .filter(|s| !s.is_empty())
                .ok_or(INVALID_ARGUMENT)?
                .into(),
        }),
        [project, id, selection] if verb == "units" && valid(project) && valid(selection) => {
            Ok(CliCommand::PreviewUnits {
                project: project.into(),
                import_id: id
                    .to_str()
                    .filter(|s| !s.is_empty())
                    .ok_or(INVALID_ARGUMENT)?
                    .into(),
                selection: selection.into(),
            })
        }
        [project, id, api_root, output]
            if verb == "export" && valid(project) && valid(api_root) && valid(output) =>
        {
            Ok(CliCommand::PreviewExport {
                project: project.into(),
                import_id: id
                    .to_str()
                    .filter(|s| !s.is_empty())
                    .ok_or(INVALID_ARGUMENT)?
                    .into(),
                api_root: api_root.into(),
                output: output.into(),
            })
        }
        _ => Err(INVALID_ARGUMENT),
    }
}

fn extraction_command(args: &[OsString]) -> Result<CliCommand, &'static str> {
    if args.is_empty()
        || matches!(args, [help] if help == "--help" || help == "-h")
        || matches!(args, [verb, help] if (verb == "fake" || verb == "api" || verb == "verify") && (help == "--help" || help == "-h"))
    {
        return Ok(CliCommand::ExtractionHelp);
    }
    let (verb, tail) = args.split_first().ok_or(INVALID_ARGUMENT)?;
    if verb != "fake" && verb != "api" && verb != "verify" {
        return Err(INVALID_ARGUMENT);
    }
    let (tail, separator) = if tail.first().is_some_and(|arg| arg == "--") {
        (&tail[1..], true)
    } else {
        (tail, false)
    };
    let valid_path = |path: &OsString| {
        !path.is_empty() && (separator || path.as_encoded_bytes().first() != Some(&b'-'))
    };
    let (project, id, case, output, cancel_file) = match tail {
        [project, id, case, output] => (project, id, case, output, None),
        [project, id, case, output, flag, signal]
            if flag == "--cancel-file" && valid_path(signal) =>
        {
            (project, id, case, output, Some(PathBuf::from(signal)))
        }
        _ => return Err(INVALID_ARGUMENT),
    };
    if ![project, case, output].iter().all(|path| valid_path(path)) || id.is_empty() {
        return Err(INVALID_ARGUMENT);
    }
    if verb == "verify" {
        if cancel_file.is_some() {
            return Err(INVALID_ARGUMENT);
        }
        return Ok(CliCommand::ExtractVerify {
            project: project.into(),
            import_id: id.to_str().ok_or(INVALID_ARGUMENT)?.into(),
            case: case.into(),
            output: output.into(),
        });
    }
    if verb == "api" {
        return Ok(CliCommand::ExtractApi {
            project: project.into(),
            import_id: id.to_str().ok_or(INVALID_ARGUMENT)?.into(),
            config: case.into(),
            output: output.into(),
            cancel_file,
        });
    }
    Ok(CliCommand::ExtractFake {
        project: project.into(),
        import_id: id.to_str().ok_or(INVALID_ARGUMENT)?.into(),
        case: case.into(),
        output: output.into(),
        cancel_file,
    })
}

fn case_command(args: &[OsString]) -> Result<CliCommand, &'static str> {
    match args {
        [] => Ok(CliCommand::CaseHelp),
        [help] if help == "--help" || help == "-h" => Ok(CliCommand::CaseHelp),
        [verb, help]
            if (verb == "verify" || verb == "compile") && (help == "--help" || help == "-h") =>
        {
            Ok(CliCommand::CaseHelp)
        }
        [verb, path]
            if verb == "verify"
                && !path.is_empty()
                && path.as_encoded_bytes().first() != Some(&b'-') =>
        {
            Ok(CliCommand::CaseVerify(path.into()))
        }
        [verb, separator, path] if verb == "verify" && separator == "--" && !path.is_empty() => {
            Ok(CliCommand::CaseVerify(path.into()))
        }
        [verb, case, output]
            if verb == "compile"
                && [case, output].iter().all(|path| {
                    !path.is_empty() && path.to_str().is_some_and(|value| !value.starts_with('-'))
                }) =>
        {
            Ok(CliCommand::CaseCompile {
                case: PathBuf::from(case),
                output: PathBuf::from(output),
            })
        }
        [verb, separator, case, output]
            if verb == "compile"
                && separator == "--"
                && [case, output]
                    .iter()
                    .all(|path| !path.is_empty() && path.to_str().is_some()) =>
        {
            Ok(CliCommand::CaseCompile {
                case: PathBuf::from(case),
                output: PathBuf::from(output),
            })
        }
        _ => Err(INVALID_ARGUMENT),
    }
}

fn source_command(args: &[OsString]) -> Result<CliCommand, &'static str> {
    if args.is_empty()
        || matches!(args,[help] if help == "--help" || help == "-h")
        || matches!(args,[verb,help] if (verb == "import" || verb == "inspect" || verb == "parse-pdf" || verb == "inspect-pdf") && (help == "--help" || help == "-h"))
    {
        return Ok(CliCommand::SourceHelp);
    }
    let (verb, tail) = args.split_first().ok_or(INVALID_ARGUMENT)?;
    let (tail, separator) = if tail.first().is_some_and(|arg| arg == "--") {
        (&tail[1..], true)
    } else {
        (tail, false)
    };
    let valid_path = |path: &OsString| {
        !path.is_empty() && (separator || path.as_encoded_bytes().first() != Some(&b'-'))
    };
    match tail {
        [project, file, flag]
            if verb == "import"
                && flag == "--reparse"
                && valid_path(project)
                && valid_path(file) =>
        {
            Ok(CliCommand::SourceImportFresh {
                project: project.into(),
                file: file.into(),
                source_id: None,
            })
        }
        [project, file, source_flag, id, fresh]
            if verb == "import"
                && source_flag == "--source-id"
                && fresh == "--reparse"
                && valid_path(project)
                && valid_path(file) =>
        {
            Ok(CliCommand::SourceImportFresh {
                project: project.into(),
                file: file.into(),
                source_id: Some(id.to_str().ok_or(INVALID_ARGUMENT)?.into()),
            })
        }
        [project, file, fresh, source_flag, id]
            if verb == "import"
                && source_flag == "--source-id"
                && fresh == "--reparse"
                && valid_path(project)
                && valid_path(file) =>
        {
            Ok(CliCommand::SourceImportFresh {
                project: project.into(),
                file: file.into(),
                source_id: Some(id.to_str().ok_or(INVALID_ARGUMENT)?.into()),
            })
        }
        [file, output] if verb == "parse-pdf" && valid_path(file) && valid_path(output) => {
            Ok(CliCommand::SourceParsePdf {
                file: file.into(),
                output: output.into(),
            })
        }
        [output] if verb == "inspect-pdf" && valid_path(output) => {
            Ok(CliCommand::SourceInspectPdf(output.into()))
        }
        [project, file] if verb == "import" && valid_path(project) && valid_path(file) => {
            Ok(CliCommand::SourceImport {
                project: project.into(),
                file: file.into(),
                source_id: None,
            })
        }
        [project, file, flag, id]
            if verb == "import"
                && flag == "--source-id"
                && valid_path(project)
                && valid_path(file) =>
        {
            Ok(CliCommand::SourceImport {
                project: project.into(),
                file: file.into(),
                source_id: Some(id.to_str().ok_or(INVALID_ARGUMENT)?.into()),
            })
        }
        [project, id] if verb == "inspect" && valid_path(project) => {
            Ok(CliCommand::SourceInspect {
                project: project.into(),
                import_id: id.to_str().ok_or(INVALID_ARGUMENT)?.into(),
            })
        }
        _ => Err(INVALID_ARGUMENT),
    }
}

fn project_command(
    verb: &OsString,
    path: &OsString,
    after_separator: bool,
) -> Result<CliCommand, &'static str> {
    if path.is_empty() || (!after_separator && path.as_encoded_bytes().first() == Some(&b'-')) {
        return Err(INVALID_ARGUMENT);
    }
    if verb == "init" {
        Ok(CliCommand::ProjectInit(PathBuf::from(path)))
    } else if verb == "inspect" {
        Ok(CliCommand::ProjectInspect(PathBuf::from(path)))
    } else {
        Err(INVALID_ARGUMENT)
    }
}

/// Desktop previews retain a pure help/version API; project operations must be
/// explicitly dispatched by the command-line application, never by this helper.
pub fn render(args: &[OsString]) -> Result<String, &'static str> {
    match args {
        [] => Ok(HELP.to_owned()),
        [arg] if arg == "--help" || arg == "-h" => Ok(HELP.to_owned()),
        [arg] if arg == "--version" || arg == "-V" => Ok(format!("noetome {VERSION}\n")),
        _ => Err(INVALID_ARGUMENT),
    }
}

pub const KNOWLEDGE_HELP:&str="Noetome 有界多来源知识协调与表达

用法：noetome knowledge coordinate <项目路径> <coordination-case> <尚不存在的测试输出根>
      noetome knowledge present <项目路径> <coordination-case> <协调输出根> <尚不存在的测试Vault>
      noetome knowledge process-review <项目路径> <process-case> <尚不存在的review根>
      noetome knowledge flow <项目路径> <process-case> <review根> <profile.json> <尚不存在的测试Vault>
      noetome knowledge formula <formula-case> <profile.json> <尚不存在的测试Vault>
      noetome knowledge image <image-case> <profile.json> <尚不存在的测试Vault>
      noetome knowledge scan-formula <scan-case> <api-config.json> <profile.json> <尚不存在的测试Vault>

只重新核对同项目2至3份成功的单原子来源依据，通过显式组件身份映射保留重复／互补事实和全部出处；严格协调Schema2还保留有来源背景的条件与历史变体。
未知语义差异保持needs_review；不调用模型或研究服务。知识pending，published=false，scientific_quality=not_run。
present：重验严格v1/v2/v3协调输出及完整原始来源，再主动选择比较表或正文，保留原值、单位、条件、例外、版本及来源映射。
process-review：从明确选取的实际文字完整解析有界步骤/关系语法，与独立gold核对；未解析语句无成功过程IR。
flow：从实际来源重建保存的过程核对结果，明确方向、分支、正负作用并保留全部条件与例外；相关性、歧义、循环和缺关系阻断。
已测目标能力支持时生成Mermaid，能力未核验或标签换行不能保真时使用完整有据文字回退；成功仍仅为未发布工程预览。
formula：只核对明确选取的原始TeX/公式区域与来源作者语义说明，逐字保存原件，严格核对负号/上下标/符号定义/单位/前提；不能保真时回退原图，关键语义未知时阻断。无OCR/PDF作者源码恢复或科学正确率宣称。
dfrac的有据规范化由profile明确启用；未启用时回退原图，不表示MathJax本身缺少该宏。
image：只读取明确选定的PNG及作者手工结构声明；保留标签、方向、正负、单位、图例与空间关系，有限SVG不能保真时使用原图，关键语义未知或原图不可辨时阻断。无数据实验曲线只保留原图，不执行OCR或自动图像理解。
scan-formula：只对明确扫描页内的选定PNG区域执行一次有预算视觉Chat识别，原区域、请求正文、原响应和已核实用量分别保存；核对关键负号/下标/符号/单位/前提，未知阻断，不恢复作者TeX。只使用已授权网关deepseek-flash或明确数字环回人工测试，max_requests=0时不读取凭据或联网；不自动重试、跳转或扫描其他资料。
未决冲突或证据不足返回3，保存无成功计划或笔记的失败双报告；成功仅表示未发布的工程预览。
输出必须为已有父目录下的新测试根；以连字符开头的路径在动词后加 --，中文和空格路径作为单独参数。
";
fn knowledge_command(args: &[OsString]) -> Result<CliCommand, &'static str> {
    if args.is_empty()
        || matches!(args,[h] if h=="--help"||h=="-h")
        || matches!(args,[v,h] if (v=="coordinate"||v=="present"||v=="process-review"||v=="flow"||v=="formula"||v=="image"||v=="scan-formula")&&(h=="--help"||h=="-h"))
    {
        return Ok(CliCommand::KnowledgeHelp);
    }
    let (v, tail) = args.split_first().ok_or(INVALID_ARGUMENT)?;
    if v != "coordinate"
        && v != "present"
        && v != "process-review"
        && v != "flow"
        && v != "formula"
        && v != "image"
        && v != "scan-formula"
    {
        return Err(INVALID_ARGUMENT);
    }
    let (tail, sep) = if tail.first().is_some_and(|v| v == "--") {
        (&tail[1..], true)
    } else {
        (tail, false)
    };
    match tail {
        [c, a, p, o]
            if v == "scan-formula"
                && [c, a, p, o].iter().all(|s| {
                    s.to_str()
                        .is_some_and(|s| !s.is_empty() && (sep || !s.starts_with('-')))
                }) =>
        {
            Ok(CliCommand::ScanFormula {
                case: c.into(),
                api_config: a.into(),
                profile: p.into(),
                output: o.into(),
            })
        }
        [c, p, o]
            if v == "image"
                && [c, p, o].iter().all(|s| {
                    s.to_str()
                        .is_some_and(|s| !s.is_empty() && (sep || !s.starts_with('-')))
                }) =>
        {
            Ok(CliCommand::Image {
                case: c.into(),
                profile: p.into(),
                output: o.into(),
            })
        }
        [c, p, o]
            if v == "formula"
                && [c, p, o].iter().all(|s| {
                    s.to_str()
                        .is_some_and(|s| !s.is_empty() && (sep || !s.starts_with('-')))
                }) =>
        {
            Ok(CliCommand::Formula {
                case: c.into(),
                profile: p.into(),
                output: o.into(),
            })
        }
        [p, c, o]
            if v == "process-review"
                && [p, c, o].iter().all(|s| {
                    s.to_str()
                        .is_some_and(|s| !s.is_empty() && (sep || !s.starts_with('-')))
                }) =>
        {
            Ok(CliCommand::ProcessReview {
                project: p.into(),
                case: c.into(),
                output: o.into(),
            })
        }
        [p, c, reviewed, profile, o]
            if v == "flow"
                && [p, c, reviewed, profile, o].iter().all(|s| {
                    s.to_str()
                        .is_some_and(|s| !s.is_empty() && (sep || !s.starts_with('-')))
                }) =>
        {
            Ok(CliCommand::ProcessFlow {
                project: p.into(),
                case: c.into(),
                reviewed: reviewed.into(),
                profile: profile.into(),
                output: o.into(),
            })
        }
        [p, c, o]
            if v == "coordinate"
                && [p, c, o].iter().all(|s| {
                    s.to_str()
                        .is_some_and(|s| !s.is_empty() && (sep || !s.starts_with('-')))
                }) =>
        {
            Ok(CliCommand::KnowledgeCoordinate {
                project: p.into(),
                case: c.into(),
                output: o.into(),
            })
        }
        [p, c, saved, o]
            if v == "present"
                && [p, c, saved, o].iter().all(|s| {
                    s.to_str()
                        .is_some_and(|s| !s.is_empty() && (sep || !s.starts_with('-')))
                }) =>
        {
            Ok(CliCommand::KnowledgePresent {
                project: p.into(),
                case: c.into(),
                coordinated: saved.into(),
                output: o.into(),
            })
        }
        _ => Err(INVALID_ARGUMENT),
    }
}

pub const PLAN_HELP: &str = "Noetome 教材式自适应规划

用法：noetome plan adopt <规划声明> <尚不存在的规划根>
      noetome plan build <规划根> <尚不存在的输出根>
      noetome plan list <规划根>
      noetome plan status <规划根>

adopt：核对主题、阅读目标、概念关系与必要原子，先按规则预演一次规划，成环、拆链无理由或原子无处安放都在建根之前报出。
build：在全新输出根里写出 plan.json、report.json、report.md 与输出清单；同一冻结输入的两次构建必须字节相同（输出清单不含时间戳）。
list|status：只读重算并输出笔记位置、原子安置、反应链与提醒。
每个必要原子恰有一个规划位置；知识图谱可以多重归属，但文件树只选一个主要位置，其余联系用链接表达。
连续反应链被拆到多份笔记时必须给出可检查的拆分理由（chain_too_long、topic_boundary、user_lock），否则整包拒绝。
不联网、不调用模型、不写项目与Vault；本阶段只做结构规划，不生成知识、不判断内容质量与科学正确性。
";

fn plan_command(args: &[OsString]) -> Result<CliCommand, &'static str> {
    const VERBS: [&str; 4] = ["adopt", "build", "list", "status"];
    if args.is_empty() || matches!(args, [help] if help == "--help" || help == "-h") {
        return Ok(CliCommand::PlanHelp);
    }
    let (verb, tail) = args.split_first().ok_or(INVALID_ARGUMENT)?;
    if matches!(tail, [help] if help == "--help" || help == "-h")
        && VERBS.iter().any(|candidate| verb == candidate)
    {
        return Ok(CliCommand::PlanHelp);
    }
    let (tail, separated) = if tail.first().is_some_and(|arg| arg == "--") {
        (&tail[1..], true)
    } else {
        (tail, false)
    };
    let valid = |path: &OsString| {
        !path.is_empty() && (separated || path.as_encoded_bytes().first() != Some(&b'-'))
    };
    match tail {
        [project, root] if verb == "adopt" && valid(project) && valid(root) => {
            Ok(CliCommand::PlanAdopt {
                project: project.into(),
                root: root.into(),
            })
        }
        [root, output] if verb == "build" && valid(root) && valid(output) => {
            Ok(CliCommand::PlanBuild {
                root: root.into(),
                output: output.into(),
            })
        }
        [root] if verb == "list" && valid(root) => Ok(CliCommand::PlanList { root: root.into() }),
        [root] if verb == "status" && valid(root) => {
            Ok(CliCommand::PlanStatus { root: root.into() })
        }
        _ => Err(INVALID_ARGUMENT),
    }
}

pub const TEMPLATE_HELP: &str = "Noetome 主题阅读目标与结构模板

用法：noetome template adopt <结构声明> <尚不存在的结构根>
      noetome template build <结构根> <尚不存在的输出根>
      noetome template list <结构根>
      noetome template status <结构根>

adopt：核对模板、主题、阅读目标与原子归属，先按规则预演一次结构规划，空章节、模板外章节缺理由或章节乱序都在建根之前报出。
build：在全新输出根里写出 plan.json、report.json、report.md 与输出清单；同一冻结输入的两次构建必须字节相同（输出清单不含时间戳）。
list|status：只读重算并输出每份笔记的章节、原子、引用与唯一解释位置。
模板是结构规范而不是填空：不适用的章节可以省略，省略会作为偏差列出；但不得为模板齐全留空章节。
模板外的章节必须给出 special_reason，命名与导航深度偏离必须给出 title_reason 与 depth_reason，否则整包拒绝。
同一类型同一层级的主题必须采用一致的章节顺序；不一致却没有理由时记为缺口而不是静默通过。
每个概念在整份规划里恰好有一处完整解释；其余笔记只能引用或同源嵌入，重复解释记为缺口。
有缺口时仍然写出完整报告，只把退出码置为非零——缺口必须逐条可见。
不联网、不调用模型、不写项目与Vault；本阶段只做结构规划，不生成知识、不判断内容质量与科学正确性。
";

fn template_command(args: &[OsString]) -> Result<CliCommand, &'static str> {
    const VERBS: [&str; 4] = ["adopt", "build", "list", "status"];
    if args.is_empty() || matches!(args, [help] if help == "--help" || help == "-h") {
        return Ok(CliCommand::TemplateHelp);
    }
    let (verb, tail) = args.split_first().ok_or(INVALID_ARGUMENT)?;
    if matches!(tail, [help] if help == "--help" || help == "-h")
        && VERBS.iter().any(|candidate| verb == candidate)
    {
        return Ok(CliCommand::TemplateHelp);
    }
    let (tail, separated) = if tail.first().is_some_and(|arg| arg == "--") {
        (&tail[1..], true)
    } else {
        (tail, false)
    };
    let valid = |path: &OsString| {
        !path.is_empty() && (separated || path.as_encoded_bytes().first() != Some(&b'-'))
    };
    match tail {
        [project, root] if verb == "adopt" && valid(project) && valid(root) => {
            Ok(CliCommand::TemplateAdopt {
                project: project.into(),
                root: root.into(),
            })
        }
        [root, output] if verb == "build" && valid(root) && valid(output) => {
            Ok(CliCommand::TemplateBuild {
                root: root.into(),
                output: output.into(),
            })
        }
        [root] if verb == "list" && valid(root) => {
            Ok(CliCommand::TemplateList { root: root.into() })
        }
        [root] if verb == "status" && valid(root) => {
            Ok(CliCommand::TemplateStatus { root: root.into() })
        }
        _ => Err(INVALID_ARGUMENT),
    }
}

pub const CHANGE_HELP: &str = "Noetome 锁定位置与规划变更的局部影响

用法：
      noetome change adopt <变更声明> <变更根>
      noetome change build <变更根> <输出根>
      noetome change list <变更根>
      noetome change status <变更根>

adopt：冻结变更声明与未受影响笔记的指纹，先把锁定与影响范围预演一遍，全部通过才建根。
build：重新规划并写出 changes.json、report.json、report.md；同一冻结输入的两次构建必须字节相同（输出清单不含时间戳）。
list|status：只读重算并输出已应用、转候选、重构与缺口清单。

锁定冲突只进候选，绝不覆盖已锁定结构：目录、文件、章节、顺序与规范位置都可以锁。
变更必须只影响声明范围：同主题里引用了被触动概念的笔记必须被声明，与之无关的笔记不得被声明。
重构必须列出每一处引用变化并给出完整拆分划分；无法同时更新时保留旧位置并报错。
不联网、不调用模型、不写项目与Vault；本阶段只做位置与影响范围规划，不生成正文、不判断内容质量与科学正确性。
";

fn change_command(args: &[OsString]) -> Result<CliCommand, &'static str> {
    const VERBS: [&str; 4] = ["adopt", "build", "list", "status"];
    if args.is_empty() {
        return Err("缺少子命令：请使用 adopt、build、list 或 status。");
    }
    let verb = args[0].to_str().ok_or("子命令不是合法文本。")?;
    if verb == "--help" || verb == "-h" {
        return Ok(CliCommand::ChangeHelp);
    }
    if !VERBS.contains(&verb) {
        return Err("未知子命令：请使用 adopt、build、list 或 status。");
    }
    let rest: Vec<&OsString> = args[1..]
        .iter()
        .filter(|item| item.as_os_str() != "--")
        .collect();
    let valid = |value: &OsString| !value.is_empty() && !value.to_string_lossy().starts_with('-');
    match verb {
        "adopt" => {
            if rest.len() != 2 || !valid(rest[0]) || !valid(rest[1]) {
                return Err("adopt 需要 <变更声明> <变更根> 两个参数。");
            }
            Ok(CliCommand::ChangeAdopt {
                project: rest[0].into(),
                root: rest[1].into(),
            })
        }
        "build" => {
            if rest.len() != 2 || !valid(rest[0]) || !valid(rest[1]) {
                return Err("build 需要 <变更根> <输出根> 两个参数。");
            }
            Ok(CliCommand::ChangeBuild {
                root: rest[0].into(),
                output: rest[1].into(),
            })
        }
        "list" => {
            if rest.len() != 1 || !valid(rest[0]) {
                return Err("list 需要 <变更根> 一个参数。");
            }
            Ok(CliCommand::ChangeList {
                root: rest[0].into(),
            })
        }
        _ => {
            if rest.len() != 1 || !valid(rest[0]) {
                return Err("status 需要 <变更根> 一个参数。");
            }
            Ok(CliCommand::ChangeStatus {
                root: rest[0].into(),
            })
        }
    }
}

pub const SCALE_HELP: &str = "\
Noetome 合成语料的规模实测

用法：
  noetome scale measure --pages <页数> --headings <每页标题数> --paragraphs <每页段落数> --output <输出根>
  noetome scale --help

说明：
  measure：按参数确定性生成合成页面，真实跑文本分析与章节对齐，写出结构产物、耗时、报告与输出清单。
  结构产物只由参数决定：同一参数两次运行的结构产物字节相同；耗时单独成文件，不参与确定性比较。
  规划动词的声明上限（topics=4、goals=8、concepts=64、atoms=256）会写进报告作为未达标项：
  不绕过上限，也不把上千页的文本处理当成千笔记规划已经达标。
  页面是合成的，不证明真实教材的抽取质量；不联网、不调用模型、不写项目与 Vault。
";

fn scale_command(args: &[OsString]) -> Result<CliCommand, &'static str> {
    if args.is_empty() || args.iter().any(|arg| arg == "--help" || arg == "-h") {
        return Ok(CliCommand::ScaleHelp);
    }
    if args.first().is_none_or(|arg| arg != "measure") {
        return Err(INVALID_ARGUMENT);
    }
    let mut pages: Option<String> = None;
    let mut headings: Option<String> = None;
    let mut paragraphs: Option<String> = None;
    let mut output: Option<PathBuf> = None;
    let mut index = 1;
    while index < args.len() {
        let flag = args[index].to_string_lossy();
        let value = args
            .get(index + 1)
            .ok_or("scale measure 的参数缺少取值。")?;
        match flag.as_ref() {
            "--pages" => pages = Some(value.to_string_lossy().into_owned()),
            "--headings" => headings = Some(value.to_string_lossy().into_owned()),
            "--paragraphs" => paragraphs = Some(value.to_string_lossy().into_owned()),
            "--output" => output = Some(PathBuf::from(value)),
            _ => return Err(INVALID_ARGUMENT),
        }
        index += 2;
    }
    Ok(CliCommand::ScaleMeasure {
        pages: pages.ok_or(INVALID_ARGUMENT)?,
        headings: headings.ok_or(INVALID_ARGUMENT)?,
        paragraphs: paragraphs.ok_or(INVALID_ARGUMENT)?,
        output: output.ok_or(INVALID_ARGUMENT)?,
    })
}

/// Help for the library-scale measurement verb.
pub const BENCH_HELP: &str = "\
Noetome 大库规模实测

用法：
  noetome bench measure --notes 3000 --edits 200 --samples 20 --warmup 2 --output <输出根>
  noetome bench --help

说明：
  measure：按参数确定性生成合成笔记库（含人工改动），逐阶段测量 scan（文本分析取摘要）、detect（与基线比摘要）、plan（汇总共享概念与主题的笔记）、page（渲染变更清单第一页）
  P95 口径写进产物：样本数、预热次数、是否含进程启动、分位数算法逐项固定；毫秒读数只存在 timings.json，不进入结构与报告
  输出根恰四个文件；同一参数两次运行的结构产物与报告必须字节相同（耗时产物除外）
  本轮如实记录未达标项：合成语料不代表真实笔记库，「不因全量渲染冻结界面」属于界面行为，本轮没有界面可测
  不联网、不调用模型、不写项目与 Vault；本阶段只测量，不优化算法。";

fn bench_command(args: &[OsString]) -> Result<CliCommand, &'static str> {
    if args
        .first()
        .is_some_and(|arg| arg == "--help" || arg == "-h")
    {
        return Ok(CliCommand::BenchHelp);
    }
    let verb = args.first().ok_or(INVALID_ARGUMENT)?.to_string_lossy();
    if verb != "measure" {
        return Err(INVALID_ARGUMENT);
    }
    let mut notes: Option<String> = None;
    let mut edits: Option<String> = None;
    let mut samples: Option<String> = None;
    let mut warmup: Option<String> = None;
    let mut output: Option<PathBuf> = None;
    let mut index = 1;
    while index < args.len() {
        let flag = args[index].to_string_lossy();
        let value = args.get(index + 1).ok_or(INVALID_ARGUMENT)?;
        match flag.as_ref() {
            "--notes" => notes = Some(value.to_string_lossy().into_owned()),
            "--edits" => edits = Some(value.to_string_lossy().into_owned()),
            "--samples" => samples = Some(value.to_string_lossy().into_owned()),
            "--warmup" => warmup = Some(value.to_string_lossy().into_owned()),
            "--output" => output = Some(PathBuf::from(value)),
            _ => return Err(INVALID_ARGUMENT),
        }
        index += 2;
    }
    Ok(CliCommand::BenchMeasure {
        notes: notes.ok_or(INVALID_ARGUMENT)?,
        edits: edits.ok_or(INVALID_ARGUMENT)?,
        samples: samples.ok_or(INVALID_ARGUMENT)?,
        warmup: warmup.ok_or(INVALID_ARGUMENT)?,
        output: output.ok_or(INVALID_ARGUMENT)?,
    })
}

pub const SOAK_HELP: &str = "\
Noetome 长时运行 soak

用法：
  noetome soak run --rounds 20 --interval-ms 100 --pause-after-ms 1500 --max-rss-growth-bytes 1073741824 --output <输出根>
  noetome soak --help

说明：
  run：按参数重复执行只读工作单元（对一页合成文本做一次分析），逐轮记录耗时、内存与错误码
  暂停闸门：每轮开始时用距起点的毫秒数判断闸门是否已关；关了就永远关，关闭后仍被调度的轮数必须为 0
  未跑满 24 小时即未达到 NFR-09：报告与产物都写 not_met，不把短时结果当长时结论
  读数只存在 timings.json，结构与报告只由参数决定；输出根恰四个文件
  不联网、不调用模型、不写项目与 Vault；本阶段不做性能优化，只测量与判定。";

fn soak_command(args: &[OsString]) -> Result<CliCommand, &'static str> {
    if args
        .first()
        .is_some_and(|arg| arg == "--help" || arg == "-h")
    {
        return Ok(CliCommand::SoakHelp);
    }
    let verb = args.first().ok_or(INVALID_ARGUMENT)?.to_string_lossy();
    if verb != "run" {
        return Err(INVALID_ARGUMENT);
    }
    let mut rounds: Option<String> = None;
    let mut interval_ms: Option<String> = None;
    let mut pause_after_ms: Option<String> = None;
    let mut pause_seen = false;
    let mut max_rss_growth_bytes: Option<String> = None;
    let mut output: Option<PathBuf> = None;
    let mut index = 1;
    while index < args.len() {
        let flag = args[index].to_string_lossy();
        let value = args.get(index + 1).ok_or(INVALID_ARGUMENT)?;
        match flag.as_ref() {
            "--rounds" => rounds = Some(value.to_string_lossy().into_owned()),
            "--interval-ms" => interval_ms = Some(value.to_string_lossy().into_owned()),
            "--pause-after-ms" => {
                pause_seen = true;
                pause_after_ms = Some(value.to_string_lossy().into_owned());
            }
            "--max-rss-growth-bytes" => {
                max_rss_growth_bytes = Some(value.to_string_lossy().into_owned())
            }
            "--output" => output = Some(PathBuf::from(value)),
            _ => return Err(INVALID_ARGUMENT),
        }
        index += 2;
    }
    Ok(CliCommand::SoakRun {
        rounds: rounds.ok_or(INVALID_ARGUMENT)?,
        interval_ms: interval_ms.ok_or(INVALID_ARGUMENT)?,
        pause_after_ms: if pause_seen { pause_after_ms } else { None },
        max_rss_growth_bytes: max_rss_growth_bytes.ok_or(INVALID_ARGUMENT)?,
        output: output.ok_or(INVALID_ARGUMENT)?,
    })
}

pub const JOURNAL_HELP: &str = "\
Noetome 审计事件日志

用法：
  noetome journal adopt <事件声明> <尚不存在的审计根>
  noetome journal build <审计根> <尚不存在的输出根>
  noetome journal list <审计根>
  noetome journal status <审计根>

adopt：核对声明、四层定位与凭据字样，先按规则预演一次日志，全部通过才建根。
build：把冻结声明渲染成 journal.json、report.json 与 report.md；同一冻结输入的两次构建必须字节相同（输出清单不含时间戳）。
list|status：只读重算日志并输出事件构成、失败定位与任务结果。
每条失败事件都必须能定位到任务、来源、知识对象与受影响文件：定位不到的失败无法复核。
日志只追加、不重写：事件按任务与序号排序，重排声明不会改变日志。
声明里出现疑似凭据字样时整包拒绝——日志与导出会跟着被分享出去，凭据不能进声明。
不联网、不调用模型、不写项目与Vault；本阶段不判断内容质量与科学正确性。
";

fn journal_command(args: &[OsString]) -> Result<CliCommand, &'static str> {
    const VERBS: [&str; 4] = ["adopt", "build", "list", "status"];
    let Some(first) = args.first() else {
        return Err(INVALID_ARGUMENT);
    };
    if first == "--help" || first == "-h" {
        return Ok(CliCommand::JournalHelp);
    }
    let verb = first.to_string_lossy();
    if !VERBS.contains(&verb.as_ref()) {
        return Err(INVALID_ARGUMENT);
    }
    let mut rest: Vec<OsString> = Vec::new();
    let mut index = 1;
    let mut separator = false;
    while index < args.len() {
        let value = &args[index];
        if !separator && value == "--" {
            separator = true;
            index += 1;
            continue;
        }
        rest.push(value.clone());
        index += 1;
    }
    let valid = |value: &OsString| !value.is_empty() && !value.to_string_lossy().starts_with('-');
    match verb.as_ref() {
        "adopt" if rest.len() == 2 && rest.iter().all(valid) => Ok(CliCommand::JournalAdopt {
            project: PathBuf::from(&rest[0]),
            root: PathBuf::from(&rest[1]),
        }),
        "build" if rest.len() == 2 && rest.iter().all(valid) => Ok(CliCommand::JournalBuild {
            root: PathBuf::from(&rest[0]),
            output: PathBuf::from(&rest[1]),
        }),
        "list" if rest.len() == 1 && valid(&rest[0]) => Ok(CliCommand::JournalList {
            root: PathBuf::from(&rest[0]),
        }),
        "status" if rest.len() == 1 && valid(&rest[0]) => Ok(CliCommand::JournalStatus {
            root: PathBuf::from(&rest[0]),
        }),
        _ => Err(INVALID_ARGUMENT),
    }
}

pub const DIAGNOSTICS_HELP: &str = "\
Noetome 诊断导出

用法：
  noetome diagnostics adopt <导出声明> <尚不存在的诊断根>
  noetome diagnostics build <诊断根> <尚不存在的输出根>
  noetome diagnostics list <诊断根>
  noetome diagnostics status <诊断根>

adopt：核对声明、范围类别与同意，先按规则预演一次导出，全部通过才建根。
build：把冻结声明渲染成 export.json、report.json 与 report.md；同一冻结输入的两次构建必须字节相同（输出清单不含时间戳）。
list|status：只读重算导出范围并输出每个类别的条目、字节与是否导出。
导出内容由勾选决定：没勾选的类别如实标为未导出，不静默补全。
默认不含原文与来源正文；勾选敏感范围必须在声明里记录同意，没有同意直接拒绝。
声明里出现绝对路径或疑似凭据字样时整包拒绝，拒绝时不写出任何字节。
预览只是短摘要，不是原文；诊断包会被分享出去，只能写相对路径。
不联网、不调用模型、不写项目与Vault；本阶段不判断内容质量与科学正确性。
";

fn diagnostics_command(args: &[OsString]) -> Result<CliCommand, &'static str> {
    const VERBS: [&str; 4] = ["adopt", "build", "list", "status"];
    let Some(first) = args.first() else {
        return Err(INVALID_ARGUMENT);
    };
    if first == "--help" || first == "-h" {
        return Ok(CliCommand::DiagnosticsHelp);
    }
    let verb = first.to_string_lossy();
    if !VERBS.contains(&verb.as_ref()) {
        return Err(INVALID_ARGUMENT);
    }
    let mut rest: Vec<OsString> = Vec::new();
    let mut index = 1;
    let mut separator = false;
    while index < args.len() {
        let value = &args[index];
        if !separator && value == "--" {
            separator = true;
            index += 1;
            continue;
        }
        rest.push(value.clone());
        index += 1;
    }
    let valid = |value: &OsString| !value.is_empty() && !value.to_string_lossy().starts_with('-');
    match verb.as_ref() {
        "adopt" if rest.len() == 2 && rest.iter().all(valid) => Ok(CliCommand::DiagnosticsAdopt {
            project: PathBuf::from(&rest[0]),
            root: PathBuf::from(&rest[1]),
        }),
        "build" if rest.len() == 2 && rest.iter().all(valid) => Ok(CliCommand::DiagnosticsBuild {
            root: PathBuf::from(&rest[0]),
            output: PathBuf::from(&rest[1]),
        }),
        "list" if rest.len() == 1 && valid(&rest[0]) => Ok(CliCommand::DiagnosticsList {
            root: PathBuf::from(&rest[0]),
        }),
        "status" if rest.len() == 1 && valid(&rest[0]) => Ok(CliCommand::DiagnosticsStatus {
            root: PathBuf::from(&rest[0]),
        }),
        _ => Err(INVALID_ARGUMENT),
    }
}

pub const RETENTION_HELP: &str = "\
Noetome 保留与清理

用法：
  noetome retention adopt <保留声明> <尚不存在的保留根>
  noetome retention build <保留根> <尚不存在的输出根>
  noetome retention list <保留根>
  noetome retention status <保留根>

adopt：核对保留策略与已发布版本，先按规则预演一次清理计划，全部通过才建根。
build：把冻结声明渲染成 plan.json、report.json 与 report.md；同一冻结输入的两次构建必须字节相同（输出清单不含时间戳）。
list|status：只读重算清理计划，输出逐条动作、影响说明与受保护条目。
只生成计划，不删除任何文件：清理动作由调用方按计划执行。
已发布版本必需的索引与追溯条目一律受保护；若当前保留期会让它们过期，整条拒绝而不是跳过它们继续清理。
清理前必须说明会丧失哪些恢复能力、哪些追溯链会因此变短。
不联网、不调用模型、不写项目与Vault；本阶段不判断内容质量与科学正确性。
";

fn retention_command(args: &[OsString]) -> Result<CliCommand, &'static str> {
    const VERBS: [&str; 4] = ["adopt", "build", "list", "status"];
    let Some(first) = args.first() else {
        return Err(INVALID_ARGUMENT);
    };
    if first == "--help" || first == "-h" {
        return Ok(CliCommand::RetentionHelp);
    }
    let verb = first.to_string_lossy();
    if !VERBS.contains(&verb.as_ref()) {
        return Err(INVALID_ARGUMENT);
    }
    let mut rest: Vec<OsString> = Vec::new();
    let mut index = 1;
    let mut separator = false;
    while index < args.len() {
        let value = &args[index];
        if !separator && value == "--" {
            separator = true;
            index += 1;
            continue;
        }
        rest.push(value.clone());
        index += 1;
    }
    let valid = |value: &OsString| !value.is_empty() && !value.to_string_lossy().starts_with('-');
    match verb.as_ref() {
        "adopt" if rest.len() == 2 && rest.iter().all(valid) => Ok(CliCommand::RetentionAdopt {
            project: PathBuf::from(&rest[0]),
            root: PathBuf::from(&rest[1]),
        }),
        "build" if rest.len() == 2 && rest.iter().all(valid) => Ok(CliCommand::RetentionBuild {
            root: PathBuf::from(&rest[0]),
            output: PathBuf::from(&rest[1]),
        }),
        "list" if rest.len() == 1 && valid(&rest[0]) => Ok(CliCommand::RetentionList {
            root: PathBuf::from(&rest[0]),
        }),
        "status" if rest.len() == 1 && valid(&rest[0]) => Ok(CliCommand::RetentionStatus {
            root: PathBuf::from(&rest[0]),
        }),
        _ => Err(INVALID_ARGUMENT),
    }
}

#[cfg(test)]
mod preview_cli_tests {
    use super::*;
    fn args(items: &[&str]) -> Vec<OsString> {
        items.iter().map(OsString::from).collect()
    }
    #[test]
    fn preview_help_navigation_and_subcommands_are_pure_parsing() {
        for items in [
            &["preview"][..],
            &["preview", "--help"],
            &["preview", "units", "--help"],
            &["preview", "choices", "-h"],
            &["preview", "export", "--help"],
        ] {
            assert!(matches!(
                parse(&args(items)).unwrap(),
                CliCommand::PreviewHelp
            ));
        }
        for verb in ["choices", "units", "export"] {
            assert!(HELP.contains(&format!("noetome preview {verb}")));
            assert!(PREVIEW_HELP.contains(&format!("noetome preview {verb}")));
        }
        assert!(
            matches!(parse(&args(&["preview","choices","中文项目","id"])).unwrap(),CliCommand::PreviewChoices{project,import_id} if project==PathBuf::from("中文项目") && import_id=="id")
        );
        assert!(
            matches!(parse(&args(&["preview","units","--","-项目","id","-选择.json"])).unwrap(),CliCommand::PreviewUnits{project,import_id,selection} if project==PathBuf::from("-项目") && import_id=="id" && selection==PathBuf::from("-选择.json"))
        );
        assert!(
            matches!(parse(&args(&["preview","export","项目","id","api","new"])).unwrap(),CliCommand::PreviewExport{api_root,output,..} if api_root==PathBuf::from("api") && output==PathBuf::from("new"))
        );
    }
    #[test]
    fn preview_rejects_unknown_missing_extra_or_ambiguous_arguments() {
        for items in [
            &["preview", "publish"][..],
            &["preview", "units", "p", "i"],
            &["preview", "choices", "p", "i", "extra"],
            &["preview", "export", "p", "i", "api"],
            &["preview", "choices", "-p", "i"],
            &["preview", "units", "p", "i", "-selection"],
            &["preview", "choices", "p", ""],
            &["preview", "units", "p", "i", ""],
        ] {
            assert!(parse(&args(items)).is_err(), "{items:?}");
        }
    }
    #[cfg(unix)]
    #[test]
    fn preview_paths_remain_native_but_non_unicode_ids_are_refused() {
        use std::os::unix::ffi::OsStringExt;
        let native = OsString::from_vec(vec![0xff]);
        let parsed = parse(&[
            "preview".into(),
            "choices".into(),
            native.clone(),
            "id".into(),
        ])
        .unwrap();
        assert!(
            matches!(parsed,CliCommand::PreviewChoices{project,..} if project==PathBuf::from(&native))
        );
        assert!(parse(&["preview".into(), "choices".into(), "p".into(), native]).is_err());
    }
}
