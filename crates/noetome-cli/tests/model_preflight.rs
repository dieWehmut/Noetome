//! Read-only complete-source CLI preflight, before any model configuration/call.
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use uuid::Uuid;

struct Sandbox {
    root: PathBuf,
    project: PathBuf,
    version: String,
    source: PathBuf,
}
impl Sandbox {
    fn new(raw: &[u8]) -> Self {
        let base =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.artifacts/B04-T02-cli-preflight");
        fs::create_dir_all(&base).unwrap();
        let root = base
            .canonicalize()
            .unwrap()
            .join(Uuid::new_v4().to_string());
        fs::create_dir(&root).unwrap();
        let project = root.join("明确项目 中文 spaces");
        let source = root.join("独立 source.md");
        fs::write(&source, raw).unwrap();
        fs::write(root.join("external-sentinel.txt"), b"external protected\n").unwrap();
        noetome_cli::project::initialize(&project).unwrap();
        let imported = noetome_cli::source::import(&project, &source, None).unwrap();
        let value: Value = serde_json::from_str(&imported.json).unwrap();
        Self {
            root,
            project,
            source,
            version: value["version_id"].as_str().unwrap().into(),
        }
    }
    fn run(&self) -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_noetome"))
            .arg("knowledge")
            .arg("preflight")
            .arg(&self.project)
            .arg(&self.version)
            .output()
            .unwrap()
    }
}
fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(base: &Path, path: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if entry.file_type().unwrap().is_dir() {
                visit(base, &path, out);
            } else {
                out.insert(
                    path.strip_prefix(base).unwrap().into(),
                    fs::read(&path).unwrap(),
                );
            }
        }
    }
    let mut out = BTreeMap::new();
    visit(root, root, &mut out);
    out
}

#[test]
fn full_source_metadata_preflight_keeps_blank_bytes_and_is_read_only() {
    let raw = b"\xef\xbb\xbf# Header\r\n\r\nChinese: \xe4\xb8\xad\xe6\x96\x87.\r\n\r\nEnd  \r\n";
    let sandbox = Sandbox::new(raw);
    fs::rename(&sandbox.source, sandbox.root.join("原件已移走.md")).unwrap();
    let before = snapshot(&sandbox.root);
    let actual = sandbox.run();
    assert!(
        actual.status.success(),
        "{}",
        String::from_utf8_lossy(&actual.stderr)
    );
    assert!(actual.stderr.is_empty());
    let value: Value = serde_json::from_slice(&actual.stdout).unwrap();
    assert_eq!(value["result"], "source_ready");
    assert_eq!(value["api_status"], "not_called");
    assert_eq!(value["externalization_authorized"], false);
    let normalized = "# Header\n\nChinese: 中文.\n\nEnd  \n";
    assert_eq!(value["source"]["raw_byte_length"], raw.len());
    assert_eq!(value["source"]["normalized_byte_length"], normalized.len());
    assert_eq!(
        value["source"]["raw_sha256"],
        noetome_cli::text::sha256(raw)
    );
    assert_eq!(
        value["source"]["normalized_sha256"],
        noetome_cli::text::sha256(normalized.as_bytes())
    );
    assert!(
        value["source"]["span_count"].as_u64().unwrap()
            > value["source"]["evidence_span_count"].as_u64().unwrap()
    );
    assert!(
        !String::from_utf8(actual.stdout)
            .unwrap()
            .contains("Chinese:")
    );
    assert_eq!(snapshot(&sandbox.root), before);
}

#[test]
fn exact_complete_utf8_input_limit_is_accepted_without_raw_truncation() {
    let raw = [
        b"\xef\xbb\xbf".as_slice(),
        "a\r\n".repeat(4095).as_bytes(),
        b"ab",
    ]
    .concat();
    let sandbox = Sandbox::new(&raw);
    let before = snapshot(&sandbox.root);
    let actual = sandbox.run();
    assert!(
        actual.status.success(),
        "{}",
        String::from_utf8_lossy(&actual.stderr)
    );
    let value: Value = serde_json::from_slice(&actual.stdout).unwrap();
    assert_eq!(value["source"]["normalized_byte_length"], 8192);
    assert!(value["source"]["raw_byte_length"].as_u64().unwrap() > 8192);
    assert_eq!(snapshot(&sandbox.root), before);
}

#[test]
fn over_limit_complete_input_never_produces_a_partial_preview() {
    let sandbox = Sandbox::new("a".repeat(8193).as_bytes());
    let before = snapshot(&sandbox.root);
    let actual = sandbox.run();
    assert_eq!(actual.status.code(), Some(2));
    assert!(actual.stdout.is_empty());
    assert!(String::from_utf8_lossy(&actual.stderr).contains("MODEL_"));
    assert_eq!(snapshot(&sandbox.root), before);
}
