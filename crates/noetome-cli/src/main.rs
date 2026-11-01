use std::env;
use std::io::{self, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    // Keep OS-native arguments: malformed Unicode is an input error, not a panic.
    let args: Vec<_> = env::args_os().skip(1).collect();
    let output = match noetome_cli::render(&args) {
        Ok(output) => output,
        Err(error) => {
            let _ = writeln!(io::stderr().lock(), "{error}");
            return ExitCode::from(2);
        }
    };

    if io::stdout().lock().write_all(output.as_bytes()).is_err() {
        let _ = writeln!(io::stderr().lock(), "CLI_OUTPUT_ERROR：无法写入标准输出。");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}
