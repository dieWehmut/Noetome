use std::env;
use std::io::{self, Write};
use std::process::ExitCode;

use noetome_cli::{
    CliCommand, extraction, extraction_api, extraction_verify, fixture, fixture_compile,
    outline_store, pdf_parse, project, research_store, retrieval_store, revision_store, source,
    vault_store, vault_update,
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
        CliCommand::RunHelp => noetome_cli::RUN_HELP.to_owned(),
        CliCommand::ApiRunInit { config, budget } => api_run_result(
            noetome_cli::request_run::init(&config, &budget),
            &mut result_code,
        ),
        CliCommand::ApiRunStart {
            config,
            budget,
            output,
            paused,
        } => api_run_result(
            noetome_cli::request_run::start(&config, &budget, &output, paused),
            &mut result_code,
        ),
        CliCommand::ApiRunResume {
            config,
            budget,
            output,
        } => api_run_result(
            noetome_cli::request_run::resume(&config, &budget, &output),
            &mut result_code,
        ),
        CliCommand::ApiRunStatus { budget, output } => api_run_result(
            noetome_cli::request_run::status(&budget, &output),
            &mut result_code,
        ),
        CliCommand::RunStart {
            case,
            output,
            paused,
        } => match noetome_cli::fixture_run::start(&case, &output, paused) {
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
        CliCommand::RunResume { case, output } => {
            match noetome_cli::fixture_run::resume(&case, &output) {
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
            }
        }
        CliCommand::RunStatus(root) => match noetome_cli::fixture_run::status(&root) {
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
        CliCommand::RunPause(root) => match noetome_cli::fixture_run::control(&root, "pause") {
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
        CliCommand::RunCancel(root) => match noetome_cli::fixture_run::control(&root, "cancel") {
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
        CliCommand::KnowledgeHelp => noetome_cli::KNOWLEDGE_HELP.to_owned(),
        CliCommand::ScanFormula {
            case,
            api_config,
            profile,
            output,
        } => match noetome_cli::scan_formula::run(&case, &api_config, &profile, &output) {
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
        CliCommand::Image {
            case,
            profile,
            output,
        } => match noetome_cli::image_fidelity::run(&case, &profile, &output) {
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
        CliCommand::Formula {
            case,
            profile,
            output,
        } => match noetome_cli::formula::run(&case, &profile, &output) {
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
        CliCommand::VaultHelp => noetome_cli::VAULT_HELP.to_owned(),
        CliCommand::VaultAdopt { author, root } => {
            vault_result(vault_store::adopt(&author, &root), &mut result_code)
        }
        CliCommand::VaultPlan {
            root,
            candidate,
            plan,
        } => vault_result(
            vault_update::plan(&root, &candidate, &plan),
            &mut result_code,
        ),
        CliCommand::VaultApprove {
            root,
            plan,
            approval,
        } => vault_result(
            vault_update::approve(&root, &plan, &approval),
            &mut result_code,
        ),
        CliCommand::VaultPublish { root, approval } => {
            vault_result(vault_update::publish(&root, &approval), &mut result_code)
        }
        CliCommand::VaultStatus { root } => {
            vault_result(vault_store::status(&root), &mut result_code)
        }
        CliCommand::VaultRollback { root, plan } => {
            vault_result(vault_update::rollback(&root, &plan), &mut result_code)
        }
        CliCommand::ResearchHelp => noetome_cli::RESEARCH_HELP.to_owned(),
        CliCommand::ResearchAdopt { author, root } => {
            vault_result(research_store::adopt(&author, &root), &mut result_code)
        }
        CliCommand::ResearchObserve { root, file } => {
            vault_result(research_store::observe(&root, &file), &mut result_code)
        }
        CliCommand::ResearchDecide { root, file } => {
            vault_result(research_store::decide(&root, &file), &mut result_code)
        }
        CliCommand::ResearchList { root } => {
            vault_result(research_store::list(&root), &mut result_code)
        }
        CliCommand::ResearchStatus { root } => {
            vault_result(research_store::status(&root), &mut result_code)
        }
        CliCommand::RetrieveHelp => noetome_cli::RETRIEVAL_HELP.to_owned(),
        CliCommand::RetrieveAdopt { plan, root } => {
            vault_result(retrieval_store::adopt(&plan, &root), &mut result_code)
        }
        CliCommand::RetrieveStep { root } => {
            vault_result(retrieval_store::step_text(&root), &mut result_code)
        }
        CliCommand::RetrieveRun { root } => {
            vault_result(retrieval_store::run(&root), &mut result_code)
        }
        CliCommand::RetrievePause { root } => {
            vault_result(retrieval_store::pause(&root), &mut result_code)
        }
        CliCommand::RetrieveResume { root } => {
            vault_result(retrieval_store::resume(&root), &mut result_code)
        }
        CliCommand::RetrieveFinish { root, reason } => {
            vault_result(retrieval_store::finish(&root, &reason), &mut result_code)
        }
        CliCommand::RetrieveList { root } => {
            vault_result(retrieval_store::list(&root), &mut result_code)
        }
        CliCommand::RetrieveStatus { root } => {
            vault_result(retrieval_store::status(&root), &mut result_code)
        }
        CliCommand::RevisionHelp => noetome_cli::REVISION_HELP.to_owned(),
        CliCommand::RevisionAdopt { author, root } => {
            vault_result(revision_store::adopt(&author, &root), &mut result_code)
        }
        CliCommand::RevisionGap { root, file } => {
            vault_result(revision_store::gap(&root, &file), &mut result_code)
        }
        CliCommand::RevisionRevise { root, file } => {
            vault_result(revision_store::revise(&root, &file), &mut result_code)
        }
        CliCommand::RevisionList { root } => {
            vault_result(revision_store::list(&root), &mut result_code)
        }
        CliCommand::RevisionStatus { root } => {
            vault_result(revision_store::status(&root), &mut result_code)
        }
        CliCommand::OutlineHelp => noetome_cli::OUTLINE_HELP.to_owned(),
        CliCommand::OutlineAdopt { author, root } => {
            vault_result(outline_store::adopt(&author, &root), &mut result_code)
        }
        CliCommand::OutlineBuild { root, output } => {
            vault_result(outline_store::build(&root, &output), &mut result_code)
        }
        CliCommand::OutlineList { root } => {
            vault_result(outline_store::list(&root), &mut result_code)
        }
        CliCommand::OutlineStatus { root } => {
            vault_result(outline_store::status(&root), &mut result_code)
        }
    };

    if io::stdout().lock().write_all(output.as_bytes()).is_err() {
        let _ = writeln!(io::stderr().lock(), "CLI_OUTPUT_ERROR：无法写入标准输出。");
        return ExitCode::from(1);
    }
    ExitCode::from(result_code)
}

fn api_run_result(
    result: Result<source::CommandOutput, project::ProjectError>,
    result_code: &mut u8,
) -> String {
    match result {
        Ok(output) => {
            if let Some((code, message)) = output.failure {
                let _ = writeln!(io::stderr().lock(), "{code}：{message}");
                *result_code = 3;
            }
            output.json
        }
        Err(error) => {
            let _ = writeln!(io::stderr().lock(), "{error}");
            // main retains the single common stdout/exit path.
            *result_code = error.exit_code();
            String::new()
        }
    }
}

fn vault_result(result: Result<String, project::ProjectError>, result_code: &mut u8) -> String {
    match result {
        Ok(text) => {
            if text.ends_with('\n') {
                text
            } else {
                format!("{text}\n")
            }
        }
        Err(error) => {
            let _ = writeln!(io::stderr().lock(), "{error}");
            *result_code = error.exit_code();
            String::new()
        }
    }
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
