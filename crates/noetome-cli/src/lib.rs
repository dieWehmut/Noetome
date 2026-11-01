use std::ffi::OsString;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const INVALID_ARGUMENT: &str = "CLI_INVALID_ARGUMENT：参数无效。当前每次只支持 --help（-h）或 --version（-V）；请运行 noetome --help 查看用法。";

pub const HELP: &str = "Noetome — 个人学科学习知识编译器

用法：noetome [选项]

当前支持：
  -h, --help       显示帮助
  -V, --version    显示版本

不传参数时显示帮助；每次只接受一个选项。
当前为命令行骨架，项目初始化和资料导入尚未实现。
";

/// The command line and desktop preview share exactly the same pure behavior.
/// OS-native arguments stay intact so malformed Unicode is an input error.
pub fn render(args: &[OsString]) -> Result<String, &'static str> {
    match args {
        [] => Ok(HELP.to_owned()),
        [arg] if arg == "--help" || arg == "-h" => Ok(HELP.to_owned()),
        [arg] if arg == "--version" || arg == "-V" => Ok(format!("noetome {VERSION}\n")),
        _ => Err(INVALID_ARGUMENT),
    }
}
