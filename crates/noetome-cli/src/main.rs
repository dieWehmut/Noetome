use std::env;
use std::io::{self, Write};
use std::process::ExitCode;

use noetome_cli::{
    CliCommand, extraction, extraction_api, extraction_verify, fixture, fixture_compile, pdf_parse,
    project, source,
};

fn main() -> ExitCode {
    // Keep OS-native arguments: malformed Unicode is an input error, not a panic.
    let args: Vec<_> = env::args_os().skip(1).collect();
    let command = match noetome_cli::parse(&args) {
        Ok(command) => command,
        Err(error) => {
            let _ = writeln!(io::stderr().lock(), "{error}");
            return ExitCode::from(2);
        }
    };

    let mut result_code = 0;
    let output = match command {
        CliCommand::KnowledgeHelp => noetome_cli::KNOWLEDGE_HELP.to_owned(),
        CliCommand::ProcessReview {
            project,
            case,
            output,
        } => match noetome_cli::process_review::run(&project, &case, &output) {
            Ok(output) => {
                if output.failed {
                    result_code = 3;
                }
                output.json
            }
            Err(error) => {
                let _ = writeln!(io::stderr().lock(), "{error}");
                return ExitCode::from(error.exit_code());
            }
        },
        CliCommand::ProcessFlow {
            project,
            case,
            reviewed,
            profile,
            output,
        } => match noetome_cli::process_flow::run(&project, &case, &reviewed, &profile, &output) {
            Ok(output) => {
                if output.failed {
                    result_code = 3;
                }
                output.json
            }
            Err(error) => {
                let _ = writeln!(io::stderr().lock(), "{error}");
                return ExitCode::from(error.exit_code());
            }
        },
        CliCommand::KnowledgePresent {
            project,
            case,
            coordinated,
            output,
        } => match noetome_cli::knowledge_presentation::run(&project, &case, &coordinated, &output)
        {
            Ok(output) => {
                if output.failed {
                    result_code = 3;
                }
                output.json
            }
            Err(error) => {
                let _ = writeln!(io::stderr().lock(), "{error}");
                return ExitCode::from(error.exit_code());
            }
        },
        CliCommand::KnowledgeCoordinate {
            project,
            case,
            output,
        } => match noetome_cli::knowledge_coordinate::run(&project, &case, &output) {
            Ok(output) => {
                if output.failed {
                    result_code = 3;
                }
                output.json
            }
            Err(error) => {
                let _ = writeln!(io::stderr().lock(), "{error}");
                return ExitCode::from(error.exit_code());
            }
        },
        CliCommand::Help => noetome_cli::HELP.to_owned(),
        CliCommand::ProjectHelp => noetome_cli::PROJECT_HELP.to_owned(),
        CliCommand::SourceHelp => noetome_cli::SOURCE_HELP.to_owned(),
        CliCommand::ExtractionHelp => noetome_cli::EXTRACTION_HELP.to_owned(),
        CliCommand::ExtractVerify {
            project,
            import_id,
            case,
            output,
        } => match extraction_verify::run(&project, &import_id, &case, &output) {
            Ok(output) => {
                if output.failed {
                    result_code = 3;
                }
                output.json
            }
            Err(error) => {
                let _ = writeln!(io::stderr().lock(), "{error}");
                return ExitCode::from(error.exit_code());
            }
        },
        CliCommand::ExtractApi {
            project,
            import_id,
            config,
            output,
            cancel_file,
        } => match extraction_api::run(
            &project,
            &import_id,
            &config,
            &output,
            cancel_file.as_deref(),
        ) {
            Ok(output) => {
                if let Some((code, message)) = output.failure {
                    let _ = writeln!(io::stderr().lock(), "{code}：{message}");
                    result_code = 3;
                }
                output.json
            }
            Err(error) => {
                let _ = writeln!(io::stderr().lock(), "{error}");
                return ExitCode::from(error.exit_code());
            }
        },
        CliCommand::ExtractFake {
            project,
            import_id,
            case,
            output,
            cancel_file,
        } => match extraction::fake(&project, &import_id, &case, &output, cancel_file.as_deref()) {
            Ok(output) => {
                if let Some((code, message)) = output.failure {
                    let _ = writeln!(io::stderr().lock(), "{code}：{message}");
                    result_code = 3;
                }
                output.json
            }
            Err(error) => {
                let _ = writeln!(io::stderr().lock(), "{error}");
                return ExitCode::from(error.exit_code());
            }
        },
        CliCommand::CaseHelp => noetome_cli::CASE_HELP.to_owned(),
        CliCommand::CaseCompile { case, output } => {
            match fixture_compile::compile(&case, &output) {
                Ok(result) => {
                    if result.failed {
                        let _ = writeln!(
                            io::stderr().lock(),
                            "CASE_COMPILATION_FAILED：fixture编译未完成，请查看JSON问题及保留的阶段现场。已有目标不自动接管。"
                        );
                        result_code = 3;
                    }
                    result.json
                }
                Err(error) => {
                    let _ = writeln!(io::stderr().lock(), "{error}");
                    return ExitCode::from(error.exit_code());
                }
            }
        }
        CliCommand::CaseVerify(path) => match fixture::verify(&path) {
            Ok(output) => {
                if output.failed {
                    let _ = writeln!(
                        io::stderr().lock(),
                        "CASE_VERIFICATION_FAILED：fixture契约校验未通过，请查看JSON中的规则和对象；没有发布产物。"
                    );
                    result_code = 3;
                }
                output.json
            }
            Err(error) => {
                let _ = writeln!(io::stderr().lock(), "{error}");
                return ExitCode::from(error.exit_code());
            }
        },
        CliCommand::SourceParsePdf { file, output } => match pdf_parse::parse(&file, &output) {
            Ok(output) => {
                if let Some((code, message)) = output.failure {
                    let _ = writeln!(io::stderr().lock(), "{code}: {message}");
                    result_code = 3;
                }
                output.json
            }
            Err(error) => {
                let _ = writeln!(io::stderr().lock(), "{error}");
                result_code = 3;
                pdf_parse::failure_json(&error)
            }
        },
        CliCommand::SourceInspectPdf(path) => match pdf_parse::inspect(&path) {
            Ok(output) => {
                if let Some((code, message)) = output.failure {
                    let _ = writeln!(io::stderr().lock(), "{code}: {message}");
                    result_code = 3;
                }
                output.json
            }
            Err(error) => {
                let _ = writeln!(io::stderr().lock(), "{error}");
                result_code = 3;
                pdf_parse::failure_json(&error)
            }
        },
        CliCommand::Version => format!("noetome {}\n", noetome_cli::VERSION),
        CliCommand::ProjectInit(path) => match project::initialize(&path) {
            Ok(info) => project_summary("空项目初始化完成", &path, &info),
            Err(error) => {
                let _ = writeln!(io::stderr().lock(), "{error}");
                return ExitCode::from(error.exit_code());
            }
        },
        CliCommand::ProjectInspect(path) => match project::inspect(&path) {
            Ok(info) => project_summary("项目检查通过（只读）", &path, &info),
            Err(error) => {
                let _ = writeln!(io::stderr().lock(), "{error}");
                return ExitCode::from(error.exit_code());
            }
        },
        CliCommand::SourceImport {
            project,
            file,
            source_id,
        } => match source::import(&project, &file, source_id.as_deref()) {
            Ok(output) => {
                if let Some((code, message)) = output.failure {
                    let _ = writeln!(io::stderr().lock(), "{code}：{message}");
                    result_code = 3;
                }
                output.json
            }
            Err(error) => {
                let _ = writeln!(io::stderr().lock(), "{error}");
                return ExitCode::from(error.exit_code());
            }
        },
        CliCommand::SourceImportFresh {
            project,
            file,
            source_id,
        } => match source::import_with_options(&project, &file, source_id.as_deref(), true) {
            Ok(output) => {
                if let Some((code, message)) = output.failure {
                    let _ = writeln!(io::stderr().lock(), "{code}：{message}");
                    result_code = 3;
                }
                output.json
            }
            Err(error) => {
                let _ = writeln!(io::stderr().lock(), "{error}");
                return ExitCode::from(error.exit_code());
            }
        },
        CliCommand::SourceInspect { project, import_id } => {
            match source::inspect_output(&project, &import_id) {
                Ok(output) => {
                    if let Some((code, message)) = output.failure {
                        let _ = writeln!(io::stderr().lock(), "{code}：{message}");
                        result_code = 3;
                    }
                    output.json
                }
                Err(error) => {
                    let _ = writeln!(io::stderr().lock(), "{error}");
                    return ExitCode::from(error.exit_code());
                }
            }
        }
    };

    if io::stdout().lock().write_all(output.as_bytes()).is_err() {
        let _ = writeln!(io::stderr().lock(), "CLI_OUTPUT_ERROR：无法写入标准输出。");
        return ExitCode::from(1);
    }
    ExitCode::from(result_code)
}

fn project_summary(result: &str, path: &std::path::Path, info: &project::ProjectInfo) -> String {
    let stage = if info.schema_version == 1 {
        "空项目，尚未登记来源"
    } else {
        "来源登记Schema；此状态只表示项目建立完成"
    };
    format!(
        "结果：{result}\n项目：{}\n项目 ID：{}\nSchema 版本：{}\n创建时间：{}\n状态：{}（{stage}）\n配置：{}\n数据库：{}\n单份UTF8文本通过source import登记；知识抽取和笔记生成尚未实现。\n",
        path.display(),
        info.project_id,
        info.schema_version,
        info.created_at,
        info.state,
        path.join("project.json").display(),
        path.join(&info.database).display(),
    )
}
