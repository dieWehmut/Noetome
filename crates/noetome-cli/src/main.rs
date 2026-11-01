use std::env;
use std::io::{self, Write};
use std::process::ExitCode;

use noetome_cli::{CliCommand, project};

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

    let output = match command {
        CliCommand::Help => noetome_cli::HELP.to_owned(),
        CliCommand::ProjectHelp => noetome_cli::PROJECT_HELP.to_owned(),
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
    };

    if io::stdout().lock().write_all(output.as_bytes()).is_err() {
        let _ = writeln!(io::stderr().lock(), "CLI_OUTPUT_ERROR：无法写入标准输出。");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

fn project_summary(result: &str, path: &std::path::Path, info: &project::ProjectInfo) -> String {
    format!(
        "结果：{result}\n项目：{}\n项目 ID：{}\nSchema 版本：{}\n创建时间：{}\n状态：{}（空项目）\n配置：{}\n数据库：{}\n资料导入与知识生成尚未实现。\n",
        path.display(),
        info.project_id,
        info.schema_version,
        info.created_at,
        info.state,
        path.join("project.json").display(),
        path.join(&info.database).display(),
    )
}
