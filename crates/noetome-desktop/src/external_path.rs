//! Paths crossing the CLI/PowerShell boundary must use ordinary absolute syntax.
use std::{ffi::OsString, path::Path};

pub fn argument(path: &Path) -> Result<OsString, String> {
    let value = path
        .to_str()
        .ok_or("DESKTOP_PATH_INVALID：路径不是有效Unicode。")?;
    normalize(value).map(OsString::from)
}

pub fn normalize(value: &str) -> Result<String, String> {
    let invalid = || "DESKTOP_PATH_INVALID：外部进程路径必须是合法绝对路径。".to_owned();
    let normalized = if let Some(rest) = value.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else {
        value.strip_prefix(r"\\?\").unwrap_or(value).to_owned()
    };
    let bytes = normalized.as_bytes();
    let drive = bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/');
    let unc = normalized.starts_with(r"\\");
    let windows = drive || unc;
    if normalized.is_empty() || normalized.chars().any(|c| c.is_control()) {
        return Err(invalid());
    }
    if windows {
        let rest = if drive {
            &normalized[3..]
        } else {
            &normalized[2..]
        };
        if rest.contains([':', '<', '>', '"', '|', '?', '*']) {
            return Err(invalid());
        }
        let parts: Vec<_> = rest.split(['\\', '/']).collect();
        if unc && (parts.len() < 2 || parts[0].is_empty() || parts[1].is_empty()) {
            return Err(invalid());
        }
        if parts.iter().any(|part| {
            let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
            *part == "."
                || *part == ".."
                || part.ends_with(['.', ' '])
                || matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
                || (stem.len() == 4
                    && (stem.starts_with("COM") || stem.starts_with("LPT"))
                    && matches!(stem.as_bytes()[3], b'1'..=b'9'))
        }) {
            return Err(invalid());
        }
    } else if !normalized.starts_with('/') || normalized.contains('\\') {
        return Err(invalid());
    }
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ordinary_and_verbatim_paths() {
        for (input, expected) in [
            (r"\\?\D:\books\项目", r"D:\books\项目"),
            (r"\\?\UNC\server\share\book", r"\\server\share\book"),
            (r"\\server\share\book", r"\\server\share\book"),
            (r"D:\books", r"D:\books"),
            ("/tmp/project", "/tmp/project"),
        ] {
            assert_eq!(normalize(input).unwrap(), expected);
        }
    }
    #[test]
    fn rejects_unsafe_paths() {
        for input in [
            "",
            "relative",
            "D:book",
            r"\\?\GLOBALROOT\Device",
            r"\\server",
            r"D:\a\..\b",
            r"D:\a:stream",
            r"D:\NUL",
            "D:\\bad\n",
            r"D:\bad.",
        ] {
            assert!(normalize(input).is_err(), "{input:?}");
        }
    }
}

#[cfg(all(test, windows))]
mod helper_tests {
    #[test]
    fn normalized_verbatim_passes_selected_path() {
        let root = std::env::temp_dir().canonicalize().unwrap();
        let normalized = super::argument(&root).unwrap();
        let support = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scripts/text_api_support.ps1");
        let output = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-Command",
                "$ErrorActionPreference='Stop'; . $env:NOETOME_TEST_SUPPORT; try { $null=Selected-Path $env:NOETOME_TEST_PATH } finally { foreach($guard in $guards) { $guard.Dispose() } }"])
            .env("NOETOME_TEST_SUPPORT", support)
            .env("NOETOME_TEST_PATH", normalized)
            .output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
