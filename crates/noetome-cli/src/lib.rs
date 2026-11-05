use std::ffi::OsString;
use std::path::PathBuf;

mod compile_model;
mod compile_report;
mod compile_store;
pub mod extraction;
pub mod extraction_api;
mod extraction_api_config;
mod extraction_api_schema;
mod extraction_api_transport;
mod extraction_api_wire;
mod extraction_fake;
mod extraction_model;
mod extraction_output;
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
pub mod formula;
mod formula_input;
mod formula_model;
mod formula_parser;
mod formula_render;
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
mod pdf_model;
pub mod pdf_parse;
mod pdf_source;
mod pdf_source_model;
mod pdf_validate;
mod pdf_workspace;
pub mod process_flow;
mod process_flow_model;
mod process_flow_plan;
mod process_flow_render;
mod process_input;
mod process_model;
mod process_parser;
pub mod process_review;
pub mod project;
pub mod sidecar_model;
pub mod sidecar_process;
mod sidecar_workspace;
pub mod source;
mod source_model;
mod source_schema;
mod source_write;
pub mod text;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const INVALID_ARGUMENT: &str = "CLI_INVALID_ARGUMENT：参数无效或缺少项目路径。请运行 noetome --help 查看用法；包含空格的路径请作为一个参数传入。";

pub const HELP: &str = "Noetome — 个人学科学习知识编译器

用法：noetome [选项]
      noetome project <init|inspect> <路径>
      noetome source import <项目路径> <MD/TXT/PDF文件> [--source-id <既有ID>] [--reparse]
      noetome source inspect <项目路径> <import-id>
      noetome source parse-pdf <PDF文件> <尚不存在的测试根>
      noetome source inspect-pdf <PDF测试根>
      noetome extract fake <项目路径> <import-id> <fake目录> <尚不存在的测试输出根>
      noetome extract api <项目路径> <import-id> <config.json> <尚不存在的测试输出根>
      noetome extract verify <项目路径> <import-id> <review目录> <尚不存在的测试输出根>
      noetome knowledge coordinate <项目路径> <coordination-case> <尚不存在的测试输出根>
      noetome knowledge present <项目路径> <coordination-case> <协调输出根> <尚不存在的测试Vault>
      noetome knowledge formula <formula-case> <profile.json> <尚不存在的测试Vault>
      noetome case verify <fixture目录>
      noetome case compile <fixture目录> <尚不存在的测试输出根>

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
  case verify             只读校验明确fixture的来源、IR、计划与独立预期
  case compile            将已校验fixture编译到全新临时Vault并保存双报告

不传参数时显示帮助；project --help、source --help 或 extract --help 查看命令说明。
当前来源由用户明确选取；候选知识保持pending。API本地HTTP假服务仅验证工程契约，实际服务冒烟与科学质量验收仍待执行。
完整知识编译与生产Vault增量发布尚未实现。
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
    Help,
    KnowledgeHelp,
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
}

/// Parse without touching the filesystem. Paths remain native OS arguments.
pub fn parse(args: &[OsString]) -> Result<CliCommand, &'static str> {
    if args.first().is_some_and(|arg| arg == "knowledge") {
        return knowledge_command(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "case") {
        return case_command(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "source") {
        return source_command(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "extract") {
        return extraction_command(&args[1..]);
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

只重新核对同项目2至3份成功的单原子来源依据，通过显式组件身份映射保留重复／互补事实和全部出处；严格协调Schema2还保留有来源背景的条件与历史变体。
未知语义差异保持needs_review；不调用模型或研究服务。知识pending，published=false，scientific_quality=not_run。
present：重验严格v1/v2/v3协调输出及完整原始来源，再主动选择比较表或正文，保留原值、单位、条件、例外、版本及来源映射。
process-review：从明确选取的实际文字完整解析有界步骤/关系语法，与独立gold核对；未解析语句无成功过程IR。
flow：从实际来源重建保存的过程核对结果，明确方向、分支、正负作用并保留全部条件与例外；相关性、歧义、循环和缺关系阻断。
已测目标能力支持时生成Mermaid，能力未核验或标签换行不能保真时使用完整有据文字回退；成功仍仅为未发布工程预览。
formula：只核对明确选取的原始TeX/公式区域与来源作者语义说明，逐字保存原件，严格核对负号/上下标/符号定义/单位/前提；不能保真时回退原图，关键语义未知时阻断。无OCR/PDF作者源码恢复或科学正确率宣称。
dfrac的有据规范化由profile明确启用；未启用时回退原图，不表示MathJax本身缺少该宏。
未决冲突或证据不足返回3，保存无成功计划或笔记的失败双报告；成功仅表示未发布的工程预览。
输出必须为已有父目录下的新测试根；以连字符开头的路径在动词后加 --，中文和空格路径作为单独参数。
";
fn knowledge_command(args: &[OsString]) -> Result<CliCommand, &'static str> {
    if args.is_empty()
        || matches!(args,[h] if h=="--help"||h=="-h")
        || matches!(args,[v,h] if (v=="coordinate"||v=="present"||v=="process-review"||v=="flow"||v=="formula")&&(h=="--help"||h=="-h"))
    {
        return Ok(CliCommand::KnowledgeHelp);
    }
    let (v, tail) = args.split_first().ok_or(INVALID_ARGUMENT)?;
    if v != "coordinate" && v != "present" && v != "process-review" && v != "flow" && v != "formula"
    {
        return Err(INVALID_ARGUMENT);
    }
    let (tail, sep) = if tail.first().is_some_and(|v| v == "--") {
        (&tail[1..], true)
    } else {
        (tail, false)
    };
    match tail {
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
