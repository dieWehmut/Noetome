#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::ffi::OsString;

use serde::Serialize;

#[derive(Serialize)]
struct RuntimeInfo {
    app_version: &'static str,
    cli_version: &'static str,
    target: &'static str,
    capabilities: [&'static str; 2],
    cli_help: &'static str,
}

#[tauri::command]
fn runtime_info() -> RuntimeInfo {
    RuntimeInfo {
        app_version: env!("CARGO_PKG_VERSION"),
        cli_version: noetome_cli::VERSION,
        target: env!("NOETOME_TARGET"),
        capabilities: ["help", "version"],
        cli_help: noetome_cli::HELP,
    }
}

#[tauri::command]
fn cli_preview(option: &str) -> Result<String, String> {
    let flag = match option {
        "help" => "--help",
        "version" => "--version",
        _ => return Err(noetome_cli::INVALID_ARGUMENT.to_owned()),
    };
    noetome_cli::render(&[OsString::from(flag)]).map_err(str::to_owned)
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![runtime_info, cli_preview])
        .run(tauri::generate_context!())
        .expect("DESKTOP_START_FAILED：Noetome 桌面应用启动失败");
}

#[cfg(test)]
mod tests {
    use super::{cli_preview, runtime_info};

    #[test]
    fn desktop_and_cli_share_versions_and_supported_capabilities() {
        let info = runtime_info();
        assert_eq!(info.app_version, info.cli_version);
        assert_eq!(info.capabilities, ["help", "version"]);
        assert_eq!(info.cli_help, cli_preview("help").unwrap());
        assert_eq!(
            cli_preview("version").unwrap(),
            format!("noetome {}\n", info.cli_version)
        );
    }

    #[test]
    fn desktop_does_not_accept_arbitrary_cli_commands() {
        for option in [
            "",
            "project init",
            "--help",
            "source",
            "shell",
            "version; whoami",
        ] {
            let error = cli_preview(option).unwrap_err();
            assert!(error.starts_with("CLI_INVALID_ARGUMENT"));
        }
    }
}
