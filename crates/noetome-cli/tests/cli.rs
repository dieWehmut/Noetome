use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_CASE: AtomicU64 = AtomicU64::new(0);

struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new() -> Self {
        let root = test_temp_path();
        eprintln!("CLI test sandbox: {}", root.display());
        fs::create_dir(&root).expect("create exclusive test directory");
        let sandbox = Self { root };
        for directory in ["工作目录 with spaces", "home", "appdata", "localappdata"] {
            fs::create_dir(sandbox.root.join(directory)).unwrap();
        }
        fs::write(sandbox.root.join("sentinel.txt"), b"preserve outside cwd").unwrap();
        fs::write(
            sandbox.root.join("工作目录 with spaces/人工内容.md"),
            "# 已有内容\n保持原样。\n",
        )
        .unwrap();
        sandbox
    }

    fn run(&self, args: &[OsString]) -> Output {
        let before = snapshot(&self.root);
        let output = Command::new(env!("CARGO_BIN_EXE_noetome"))
            .args(args)
            .current_dir(self.root.join("工作目录 with spaces"))
            .env("HOME", self.root.join("home"))
            .env("USERPROFILE", self.root.join("home"))
            .env("APPDATA", self.root.join("appdata"))
            .env("LOCALAPPDATA", self.root.join("localappdata"))
            .env("TMP", &self.root)
            .env("TEMP", &self.root)
            .env("TMPDIR", &self.root)
            .output()
            .expect("execute compiled CLI");
        assert_eq!(
            snapshot(&self.root),
            before,
            "CLI changed files or directories for {args:?}"
        );
        output
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        // This path is exclusively created by this test, never supplied by a user.
        fs::remove_dir_all(&self.root).expect("remove test-owned sandbox");
    }
}

fn test_temp_path() -> PathBuf {
    // The Windows system temp directory can be inaccessible to the test process.
    // Put test-owned workspaces in the ignored build tree instead.
    let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/cli-tests");
    fs::create_dir_all(&parent).expect("create test workspace parent");
    let parent = parent
        .canonicalize()
        .expect("resolve test workspace parent");
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    parent.join(format!(
        "noetome-cli-{}-{nonce}-{}",
        std::process::id(),
        NEXT_CASE.fetch_add(1, Ordering::Relaxed)
    ))
}

// Include directories and original bytes: additions, deletion and edits all fail.
fn snapshot(root: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
    fn visit(root: &Path, path: &Path, entries: &mut BTreeMap<PathBuf, Option<Vec<u8>>>) {
        for entry in fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let relative = path.strip_prefix(root).unwrap().to_path_buf();
            if entry.file_type().unwrap().is_dir() {
                entries.insert(relative, None);
                visit(root, &path, entries);
            } else {
                entries.insert(relative, Some(fs::read(path).unwrap()));
            }
        }
    }
    let mut entries = BTreeMap::new();
    visit(root, root, &mut entries);
    entries
}

fn args(values: &[&str]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}

#[test]
fn api_help_and_missing_source_admission_do_not_touch_files() {
    let sandbox = Sandbox::new();
    for values in [
        vec!["api"],
        vec!["api", "--help"],
        vec!["api", "extract", "--help"],
        vec!["api", "inspect", "--help"],
    ] {
        let output = sandbox.run(&args(&values));
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        let text = String::from_utf8(output.stdout).unwrap();
        for expected in [
            "api preflight",
            "api extract",
            "api inspect",
            "unverified",
            "没有命令行参数",
        ] {
            assert!(text.contains(expected), "missing {expected}");
        }
    }
    let runtime = sandbox.root.join("not-created-python.exe");
    for verb in ["preflight", "extract"] {
        let output = sandbox.run(&args(&[
            "api",
            verb,
            "missing project",
            "version",
            "missing config",
            "missing auth",
            "new output",
            "--python",
            runtime.to_str().unwrap(),
        ]));
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(error.contains("MODEL_"));
        assert!(!error.contains("CLI_INVALID_ARGUMENT"));
    }
}

#[test]
fn help_succeeds_without_writing_files() {
    let sandbox = Sandbox::new();
    for values in [&[][..], &["--help"][..], &["-h"][..]] {
        let output = sandbox.run(&args(values));
        assert_eq!(output.status.code(), Some(0));
        assert!(output.stderr.is_empty());
        let stdout = String::from_utf8(output.stdout).unwrap();
        for expected in ["用法：noetome", "--help", "--version", "尚未实现"] {
            assert!(stdout.contains(expected), "missing {expected}");
        }
        assert!(!stdout.contains('\u{1b}'), "unexpected ANSI control code");
    }
}

#[test]
fn version_matches_package_without_writing_files() {
    let sandbox = Sandbox::new();
    for value in ["--version", "-V"] {
        let output = sandbox.run(&args(&[value]));
        assert_eq!(output.status.code(), Some(0));
        assert!(output.stderr.is_empty());
        assert_eq!(
            output.stdout,
            format!("noetome {}\n", env!("CARGO_PKG_VERSION")).as_bytes()
        );
    }
}

#[test]
fn invalid_arguments_are_actionable_input_errors_without_writing_files() {
    let sandbox = Sandbox::new();
    for values in [
        vec!["--unknown"],
        vec!["project", "init"],
        vec!["source", "add", "资料.md"],
        vec!["--help", "--unknown"],
        vec!["--version", "--unknown"],
        vec!["--help", "--version"],
        vec!["--version", "--help"],
        vec!["--help", "--help"],
        vec!["--"],
        vec!["--help=yes"],
        vec!["-hV"],
        vec![""],
    ] {
        let output = sandbox.run(&args(&values));
        assert_input_error(&output);
    }
}

fn assert_input_error(output: &Output) {
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr.clone()).unwrap();
    assert!(stderr.contains("CLI_INVALID_ARGUMENT"));
    assert!(stderr.contains("noetome --help"));
    assert!(!stderr.contains('\u{1b}'), "unexpected ANSI control code");
}

#[cfg(windows)]
#[test]
fn non_unicode_windows_argument_is_rejected_without_panicking_or_writing_files() {
    use std::os::windows::ffi::OsStringExt;

    let sandbox = Sandbox::new();
    let output = sandbox.run(&[OsString::from_wide(&[0xd800])]);
    assert_input_error(&output);
}
