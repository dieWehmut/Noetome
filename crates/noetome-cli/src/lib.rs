use std::ffi::OsString;
use std::path::PathBuf;

mod compile_model;
mod compile_report;
mod compile_store;
pub mod fixture;
pub mod fixture_compile;
mod fixture_input;
mod fixture_knowledge;
mod fixture_model;
mod fixture_plan;
mod fixture_render;
mod pdf_model;
pub mod pdf_parse;
mod pdf_validate;
mod pdf_workspace;
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
      noetome source import <项目路径> <MD/TXT文件> [--source-id <既有ID>]
      noetome source inspect <项目路径> <import-id>
      noetome source parse-pdf <PDF文件> <尚不存在的测试根>
      noetome source inspect-pdf <PDF测试根>
      noetome case verify <fixture目录>
      noetome case compile <fixture目录> <尚不存在的测试输出根>

当前支持：
  -h, --help       显示帮助
  -V, --version    显示版本
  project init <路径>     在尚不存在的目录初始化空项目
  project inspect <路径>  只读检查已有项目
  source import           登记单份UTF8文本、不可变版本及完整位置清单
  source inspect          只读核对导入记录和真实快照
  source parse-pdf        单份PDF解析到全新测试根，完整物理页/区域及需复核原因
  source inspect-pdf      只读核对已保存PDF、文本、位置和双报告
  case verify             只读校验明确fixture的来源、IR、计划与独立预期
  case compile            将已校验fixture编译到全新临时Vault并保存双报告

不传参数时显示帮助；project --help 或 source --help 查看命令说明。
当前来源由用户明确选取，fixture知识与计划由作者声明；PDF先在独立新根解析，项目PDF登记和知识自动抽取尚未实现。
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

用法：noetome source import <项目路径> <MD/TXT文件> [--source-id <既有ID>]
      noetome source inspect <项目路径> <import-id>
      noetome source parse-pdf <PDF文件> <尚不存在的测试根>
      noetome source inspect-pdf <PDF测试根>

import：只读选取的UTF8文件（最多64 MiB），保存原始/规范快照、来源版本和完整Span清单。
初次写入将Schema1空项目升级为Schema2，保持项目身份和创建时间；已有版本保留。
相同内容提示复用；显式source-id只在该来源身份内换版，不按名称合并。
inspect：使用导入结果的import-id只读核对持久记录、快照与位置，不迁移或修复。
结果为JSON；知识状态仍为pending。UTF8无效或文件未能完整读取会记录真实失败并返回非零。
以 - 开头的路径可在动词后加 -- 分隔符；中文和空格路径须作为单独参数。
项目写入中断保留标记、候选与备份；请检查旧目录并用新的测试项目重试。
parse-pdf：用NOETOME_PDF_PYTHON指定的锁定Python解释器解析一份明确PDF，仅写全新测试根。
保存原始PDF、完整物理页/区域、逐页UTF8及双报告；扫描/空白/复杂版式需复核，返回非零。
inspect-pdf：只读重查测试根的实际SHA、位置、正文与报告，原文件移动后仍可核对；不修复残留。
PDF物理索引从0开始、显示页为索引+1，印刷页号未知；不以行号或文件名代替页码。
source导入不生成IR或Vault；已审阅fixture通过case compile输出临时Vault。项目PDF登记和模型抽取尚未实现。
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

#[derive(Debug)]
pub enum CliCommand {
    Help,
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
    SourceInspect {
        project: PathBuf,
        import_id: String,
    },
    SourceParsePdf {
        file: PathBuf,
        output: PathBuf,
    },
    SourceInspectPdf(PathBuf),
    CaseHelp,
    CaseVerify(PathBuf),
    CaseCompile {
        case: PathBuf,
        output: PathBuf,
    },
}

/// Parse without touching the filesystem. Paths remain native OS arguments.
pub fn parse(args: &[OsString]) -> Result<CliCommand, &'static str> {
    if args.first().is_some_and(|arg| arg == "case") {
        return case_command(&args[1..]);
    }
    if args.first().is_some_and(|arg| arg == "source") {
        return source_command(&args[1..]);
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
