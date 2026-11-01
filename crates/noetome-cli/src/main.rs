use std::env;
use std::io::{self, Write};
use std::process::ExitCode;

const HELP: &str = "Noetome — 个人学科学习知识编译器

用法：noetome [选项]

当前支持：
  -h, --help       显示帮助
  -V, --version    显示版本

不传参数时显示帮助；每次只接受一个选项。
当前为命令行骨架，项目初始化和资料导入尚未实现。
";

fn main() -> ExitCode {
    // Keep OS-native arguments: malformed Unicode is an input error, not a panic.
    let args: Vec<_> = env::args_os().skip(1).collect();
    let output = match args.as_slice() {
        [] => HELP.to_owned(),
        [arg] if arg == "--help" || arg == "-h" => HELP.to_owned(),
        [arg] if arg == "--version" || arg == "-V" => {
            format!("noetome {}\n", env!("CARGO_PKG_VERSION"))
        }
        _ => {
            let _ = writeln!(
                io::stderr().lock(),
                "CLI_INVALID_ARGUMENT：参数无效。当前每次只支持 --help（-h）或 --version（-V）；请运行 noetome --help 查看用法。"
            );
            return ExitCode::from(2);
        }
    };

    if io::stdout().lock().write_all(output.as_bytes()).is_err() {
        let _ = writeln!(io::stderr().lock(), "CLI_OUTPUT_ERROR：无法写入标准输出。");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}
