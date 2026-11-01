//! Independent author checks over persisted SQL rows and actual owned bytes.
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Instant, SystemTime};

use noetome_cli::pdf_parse::{PdfPolicy, PdfRuntime};
use noetome_cli::sidecar::ArtifactPolicy;
use rusqlite::{Connection, OpenFlags, types::ValueRef};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub static CLI_CALLS: AtomicUsize = AtomicUsize::new(0);
const EXPECTED_SHA: &str = "b9ee562d3b180754cdde8aca5de76cb79baf877204ed5384bd35652c63caca77";

pub fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}
pub fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
pub fn write_json(path: &Path, value: &Value) {
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}
pub fn python() -> PathBuf {
    selected_python("NOETOME_TEST_PDF_PYTHON")
}
pub fn different_python() -> PathBuf {
    let path = selected_python("NOETOME_TEST_PDF_MISMATCH_PYTHON");
    assert_ne!(
        path.canonicalize().unwrap(),
        python().canonicalize().unwrap()
    );
    path
}
fn selected_python(variable: &str) -> PathBuf {
    let path = PathBuf::from(std::env::var_os(variable).unwrap_or_else(|| {
        panic!("{variable} must explicitly select an actual Python executable")
    }));
    assert!(
        path.is_absolute() && path.is_file(),
        "{variable} must be an existing absolute executable"
    );
    path
}
pub fn case(id: &str) -> Value {
    let bytes = fs::read(repo().join("fixtures/pdf-text-v1/expected.json")).unwrap();
    assert_eq!(
        sha(&bytes),
        EXPECTED_SHA,
        "independent author golden changed"
    );
    let manifest: Value = serde_json::from_slice(&bytes).unwrap();
    manifest["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["case_id"] == id)
        .unwrap()
        .clone()
}
pub fn fixture(id: &str) -> PathBuf {
    let expected = case(id);
    let path = repo()
        .join("fixtures/pdf-text-v1")
        .join(expected["raw"]["relative_path"].as_str().unwrap());
    let bytes = fs::read(&path).unwrap();
    assert_eq!(
        bytes.len() as u64,
        expected["raw"]["byte_length"].as_u64().unwrap()
    );
    assert_eq!(sha(&bytes), expected["raw"]["sha256"].as_str().unwrap());
    path
}
pub fn uuid(value: &Value) {
    let text = value.as_str().unwrap();
    let parsed = Uuid::parse_str(text).unwrap();
    assert_eq!(parsed.to_string(), text);
    assert_eq!(parsed.get_version_num(), 4);
}
pub fn exact_keys(value: &Value, keys: &[&str]) {
    let actual: BTreeSet<_> = value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        actual,
        keys.iter().copied().collect(),
        "unexpected/missing JSON fields"
    );
}

pub struct Sandbox {
    pub evidence: PathBuf,
    pub root: PathBuf,
    calls: AtomicUsize,
}
impl Sandbox {
    pub fn new(name: &str) -> Self {
        let evidence = repo()
            .join(".artifacts/B03-T03-tests")
            .join(format!("{name}-{}", Uuid::new_v4()));
        let root = evidence.join("tree");
        for path in [
            &root,
            &root.join("cwd with spaces"),
            &root.join("home"),
            &root.join("appdata"),
            &root.join("localappdata"),
        ] {
            fs::create_dir_all(path).unwrap();
        }
        fs::write(
            root.join("人工外部哨兵.md"),
            "外部人工正文，来源导入不得改写。\n",
        )
        .unwrap();
        println!("evidence: {}", evidence.display());
        Self {
            evidence,
            root,
            calls: AtomicUsize::new(0),
        }
    }
    pub fn run(&self, arguments: &[OsString]) -> Output {
        let sequence = self.calls.fetch_add(1, Ordering::Relaxed);
        CLI_CALLS.fetch_add(1, Ordering::Relaxed);
        let before_sentinel = sha(&fs::read(self.root.join("人工外部哨兵.md")).unwrap());
        let started = Instant::now();
        let output = Command::new(env!("CARGO_BIN_EXE_noetome"))
            .args(arguments)
            .current_dir(self.root.join("cwd with spaces"))
            .env("HOME", self.root.join("home"))
            .env("USERPROFILE", self.root.join("home"))
            .env("APPDATA", self.root.join("appdata"))
            .env("LOCALAPPDATA", self.root.join("localappdata"))
            .env("TMP", &self.root)
            .env("TEMP", &self.root)
            .env("TMPDIR", &self.root)
            .output()
            .expect("execute current compiled real source CLI");
        let evidence = json!({"argv":arguments.iter().map(|v|v.to_string_lossy().into_owned()).collect::<Vec<_>>(),"executable":env!("CARGO_BIN_EXE_noetome"),"exit_code":output.status.code(),"elapsed_ms":started.elapsed().as_millis(),"stdout":String::from_utf8_lossy(&output.stdout),"stderr":String::from_utf8_lossy(&output.stderr),"stdout_hex":hex(&output.stdout),"stderr_hex":hex(&output.stderr),"sentinel_sha256":before_sentinel});
        write_json(
            &self.evidence.join(format!("command-{sequence:03}.json")),
            &evidence,
        );
        assert_eq!(
            sha(&fs::read(self.root.join("人工外部哨兵.md")).unwrap()),
            before_sentinel,
            "external sentinel changed"
        );
        output
    }
    pub fn project(&self, name: &str) -> PathBuf {
        let path = self.root.join(name);
        let output = self.run(&["project".into(), "init".into(), path.as_os_str().to_owned()]);
        assert!(output.status.success() && output.stderr.is_empty());
        assert!(!output.stdout.is_empty());
        assert_eq!(read_json(&path.join("project.json"))["schema_version"], 1);
        path
    }
    pub fn selected(&self, id: &str, name: &str) -> PathBuf {
        let path = self.root.join(name);
        fs::copy(fixture(id), &path).unwrap();
        path
    }
    pub fn text(&self, project: &Path, source: &Path, source_id: Option<&str>) -> Output {
        let mut args = vec![
            "source".into(),
            "import".into(),
            project.as_os_str().to_owned(),
            source.as_os_str().to_owned(),
        ];
        if let Some(id) = source_id {
            args.extend(["--source-id".into(), id.into()]);
        }
        self.run(&args)
    }
    pub fn pdf(&self, project: &Path, source: &Path, selected: &Path, extra: &[&str]) -> Output {
        let mut args = vec![
            "source".into(),
            "import".into(),
            project.as_os_str().to_owned(),
            source.as_os_str().to_owned(),
            "--python".into(),
            selected.as_os_str().to_owned(),
        ];
        args.extend(extra.iter().map(OsString::from));
        self.run(&args)
    }
    pub fn import(&self, project: &Path, id: &str, name: &str) -> Value {
        let source = self.selected(id, name);
        let expected = case(id);
        let report = json_output(
            self.pdf(project, &source, &python(), &[]),
            expected["expected_exit_code"].as_i64().unwrap() as i32,
        );
        verify_persisted(project, &report, Some(id));
        report
    }
    pub fn inspect(&self, project: &Path, report: &Value) -> Value {
        let before = snapshot(&self.root);
        let actual = json_output(
            self.run(&[
                "source".into(),
                "inspect".into(),
                project.as_os_str().to_owned(),
                report["import_id"].as_str().unwrap().into(),
            ]),
            0,
        );
        assert_eq!(
            actual, *report,
            "inspect must return original immutable acquisition/parse event"
        );
        assert_eq!(
            snapshot(&self.root),
            before,
            "inspect wrote bytes, timestamps, or side files"
        );
        actual
    }
    pub fn rejected_inspect(&self, project: &Path, report: &Value) {
        let before = snapshot(&self.root);
        let output = self.run(&[
            "source".into(),
            "inspect".into(),
            project.as_os_str().to_owned(),
            report["import_id"].as_str().unwrap().into(),
        ]);
        assert!(!output.status.success(), "corrupt project accepted");
        assert!(output.stdout.is_empty() && !output.stderr.is_empty());
        assert_eq!(
            snapshot(&self.root),
            before,
            "invalid inspect changed evidence"
        );
    }
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|v| format!("{v:02x}")).collect()
}
pub fn json_output(output: Output, code: i32) -> Value {
    assert_eq!(
        output.status.code(),
        Some(code),
        "actual CLI exit differs; stderr={} stdout={}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    let report: Value =
        serde_json::from_slice(&output.stdout).expect("one actual pure UTF8 JSON report");
    if code == 0 {
        assert!(
            output.stderr.is_empty(),
            "successful report contaminated stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    } else {
        let error = report["error"]["code"]
            .as_str()
            .expect("persisted failure has stable error code");
        assert!(!error.is_empty() && !report["error"]["message"].as_str().unwrap().is_empty());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(error),
            "stderr must reflect the exact persisted failure code"
        );
    }
    report
}
pub fn rejected(output: Output, code: i32, error: &str) {
    assert_eq!(output.status.code(), Some(code));
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(error),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[derive(Debug, PartialEq, Eq)]
pub struct Entry {
    bytes: Option<Vec<u8>>,
    modified: Option<SystemTime>,
    readonly: bool,
}
pub fn snapshot(root: &Path) -> BTreeMap<PathBuf, Entry> {
    fn visit(root: &Path, path: &Path, result: &mut BTreeMap<PathBuf, Entry>) {
        let metadata = fs::symlink_metadata(path).unwrap();
        let link = is_link(&metadata);
        result.insert(
            path.strip_prefix(root).unwrap().to_owned(),
            Entry {
                bytes: if metadata.is_file() && !link {
                    Some(fs::read(path).unwrap())
                } else {
                    None
                },
                modified: metadata.modified().ok(),
                readonly: metadata.permissions().readonly(),
            },
        );
        if metadata.is_dir() && !link {
            for entry in fs::read_dir(path).unwrap() {
                visit(root, &entry.unwrap().path(), result);
            }
        }
    }
    let mut result = BTreeMap::new();
    visit(root, root, &mut result);
    result
}
#[cfg(windows)]
fn is_link(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
}
#[cfg(not(windows))]
fn is_link(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}
pub fn content(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    snapshot(root)
        .into_iter()
        .filter_map(|(path, entry)| entry.bytes.map(|v| (path, v)))
        .collect()
}
pub fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).unwrap();
        assert!(!is_link(&metadata));
        let destination = to.join(entry.file_name());
        if metadata.is_dir() {
            copy_tree(&path, &destination);
        } else {
            fs::copy(&path, destination).unwrap();
        }
    }
}
pub fn database(project: &Path) -> Connection {
    Connection::open_with_flags(
        project.join("project.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap()
}
pub fn rows(connection: &Connection, sql: &str) -> Vec<Value> {
    let mut statement = connection.prepare(sql).unwrap();
    let names: Vec<String> = statement.column_names().map(str::to_owned).collect();
    statement
        .query_map([], |row| {
            let mut object = serde_json::Map::new();
            for (i, name) in names.iter().enumerate() {
                let value = match row.get_ref(i)? {
                    ValueRef::Null => Value::Null,
                    ValueRef::Integer(v) => json!(v),
                    ValueRef::Real(v) => json!(v),
                    ValueRef::Text(v) => json!(std::str::from_utf8(v).unwrap()),
                    ValueRef::Blob(v) => json!({"blob_hex":hex(v)}),
                };
                object.insert(name.clone(), value);
            }
            Ok(Value::Object(object))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}
pub fn table(connection: &Connection, name: &str) -> Vec<Value> {
    assert!(
        [
            "sources",
            "source_versions",
            "source_spans",
            "source_imports",
            "pdf_parses",
            "pdf_artifacts",
            "pdf_pages",
            "pdf_regions",
            "pdf_imports"
        ]
        .contains(&name)
    );
    rows(connection, &format!("SELECT * FROM {name} ORDER BY rowid"))
}
pub fn count(connection: &Connection, name: &str) -> usize {
    table(connection, name).len()
}
pub fn owned(root: &Path, relative: &str) -> PathBuf {
    let path = Path::new(relative);
    assert!(!path.is_absolute() && path.components().all(|v| matches!(v, Component::Normal(_))));
    let full = root.join(path);
    assert!(
        full.canonicalize()
            .unwrap()
            .starts_with(root.canonicalize().unwrap())
    );
    #[cfg(windows)]
    if full.is_file() {
        assert_eq!(
            noetome_sidecar_platform::file_link_count(&fs::File::open(&full).unwrap()).unwrap(),
            1,
            "PDF owned snapshot has another link"
        );
    }
    full
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CacheConfig {
    protocol_schema: u32,
    protocol: String,
    adapter_id: String,
    adapter_version: String,
    options_version: String,
    runtime: PdfRuntime,
    pdf_policy: PdfPolicy,
    artifact_policy: ArtifactPolicy,
}
#[derive(Serialize)]
struct CacheKey<'a> {
    version_id: &'a str,
    raw_sha256: &'a str,
    config: &'a CacheConfig,
}

/// The response pages/artifacts are rebuilt from SQL, then checked against the
/// authored full UTF8, glyph streams, every region, and point coordinate ranges.
pub fn verify_persisted(project: &Path, report: &Value, id: Option<&str>) {
    exact_keys(
        report,
        &[
            "schema_version",
            "project_id",
            "import_id",
            "imported_at",
            "selected_path",
            "source_id",
            "version_id",
            "status",
            "exit_code",
            "raw_acquisition",
            "pdf_parse",
            "knowledge_status",
            "error",
        ],
    );
    assert_eq!(report["schema_version"], 2);
    assert_eq!(report["knowledge_status"], "pending");
    uuid(&report["project_id"]);
    uuid(&report["import_id"]);
    exact_keys(
        &report["raw_acquisition"],
        &["status", "reused", "version", "error"],
    );
    exact_keys(
        &report["pdf_parse"],
        &[
            "status",
            "reused",
            "parse_id",
            "config",
            "report_path",
            "report_sha256",
            "evidence",
        ],
    );
    let config_json = read_json(&project.join("project.json"));
    exact_keys(
        &config_json,
        &[
            "project_id",
            "schema_version",
            "created_at",
            "state",
            "database",
        ],
    );
    assert_eq!(config_json["schema_version"], 3);
    assert_eq!(config_json["project_id"], report["project_id"]);
    let db = database(project);
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        3
    );
    assert_eq!(
        db.query_row("PRAGMA application_id", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0x4e4f4554
    );
    assert!(rows(&db, "PRAGMA foreign_key_check").is_empty());
    let table_names: BTreeSet<_> = rows(
        &db,
        "SELECT name FROM sqlite_schema WHERE type='table' AND name NOT GLOB 'sqlite_*'",
    )
    .into_iter()
    .map(|v| v["name"].as_str().unwrap().to_owned())
    .collect();
    assert_eq!(
        table_names,
        [
            "project_metadata",
            "sources",
            "source_versions",
            "source_spans",
            "source_imports",
            "pdf_parses",
            "pdf_artifacts",
            "pdf_pages",
            "pdf_regions",
            "pdf_imports"
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
        "unexpected knowledge or schema tables were created"
    );
    let imports = table(&db, "pdf_imports");
    let legacy_imports = table(&db, "source_imports");
    let all_import_ids: BTreeSet<_> = imports
        .iter()
        .chain(&legacy_imports)
        .map(|v| v["import_id"].as_str().unwrap())
        .collect();
    assert_eq!(
        all_import_ids.len(),
        imports.len() + legacy_imports.len(),
        "import IDs collide between text and PDF histories"
    );
    let import = imports
        .iter()
        .find(|v| v["import_id"] == report["import_id"])
        .unwrap();
    for key in [
        "source_id",
        "version_id",
        "imported_at",
        "selected_path",
        "exit_code",
        "knowledge_status",
    ] {
        assert_eq!(import[key], report[key]);
    }
    assert_eq!(import["parse_id"], report["pdf_parse"]["parse_id"]);
    assert_eq!(import["raw_status"], report["raw_acquisition"]["status"]);
    assert_eq!(import["parse_status"], report["pdf_parse"]["status"]);
    assert_eq!(
        import["reused_version"].as_i64().unwrap() != 0,
        report["raw_acquisition"]["reused"].as_bool().unwrap()
    );
    assert_eq!(
        import["reused_parse"].as_i64().unwrap() != 0,
        report["pdf_parse"]["reused"].as_bool().unwrap()
    );
    if report["version_id"].is_null() {
        assert!(report["raw_acquisition"]["version"].is_null());
        assert_eq!(report["raw_acquisition"]["status"], "failed");
        assert_eq!(report["pdf_parse"]["status"], "not_run");
        for key in [
            "parse_id",
            "config",
            "report_path",
            "report_sha256",
            "evidence",
        ] {
            assert!(report["pdf_parse"][key].is_null());
        }
        assert!(report["pdf_parse"]["reused"] == false);
        return;
    }
    uuid(&report["source_id"]);
    uuid(&report["version_id"]);
    uuid(&report["pdf_parse"]["parse_id"]);
    let version = &report["raw_acquisition"]["version"];
    let versions = table(&db, "source_versions");
    let persisted_version = versions
        .iter()
        .find(|v| v["version_id"] == report["version_id"])
        .unwrap();
    for key in [
        "source_id",
        "version_id",
        "revision",
        "created_at",
        "media_type",
        "display_name",
        "content_sha256",
        "byte_length",
        "normalized_sha256",
        "normalization_version",
        "ingest_status",
        "raw_snapshot",
        "normalized_snapshot",
    ] {
        assert_eq!(
            persisted_version[key], version[key],
            "SQL raw version field {key} differs"
        );
    }
    assert!(
        persisted_version["error_code"].is_null() && persisted_version["error_message"].is_null()
    );
    assert_eq!(version["source_id"], report["source_id"]);
    assert_eq!(version["version_id"], report["version_id"]);
    assert_eq!(version["media_type"], "application/pdf");
    assert_eq!(version["ingest_status"], "acquired");
    assert_eq!(report["raw_acquisition"]["status"], "acquired");
    for key in [
        "normalized_sha256",
        "normalization_version",
        "normalized_snapshot",
        "error",
    ] {
        assert!(version[key].is_null());
    }
    let raw_relative = format!(
        ".noetome-sources/{}/raw.bin",
        report["version_id"].as_str().unwrap()
    );
    assert_eq!(version["raw_snapshot"], raw_relative);
    let raw = fs::read(owned(project, &raw_relative)).unwrap();
    let version_dir = project.join(format!(
        ".noetome-sources/{}",
        report["version_id"].as_str().unwrap()
    ));
    let names: BTreeSet<_> = fs::read_dir(version_dir)
        .unwrap()
        .map(|v| v.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["raw.bin".to_owned()].into_iter().collect());
    assert_eq!(sha(&raw), version["content_sha256"].as_str().unwrap());
    assert_eq!(raw.len() as u64, version["byte_length"].as_u64().unwrap());
    assert!(
        !project
            .join(format!(
                ".noetome-sources/{}/normalized.txt",
                report["version_id"].as_str().unwrap()
            ))
            .exists()
    );
    assert!(
        !table(&db, "source_spans")
            .iter()
            .any(|v| v["source_version_id"] == report["version_id"])
    );
    let parse_id = report["pdf_parse"]["parse_id"].as_str().unwrap();
    let parse_relative = format!(".noetome-pdf-parses/{parse_id}/report.json");
    assert_eq!(report["pdf_parse"]["report_path"], parse_relative);
    let evidence_bytes = fs::read(owned(project, &parse_relative)).unwrap();
    assert!(evidence_bytes.len() <= 16 * 1024 * 1024);
    assert_eq!(
        sha(&evidence_bytes),
        report["pdf_parse"]["report_sha256"].as_str().unwrap()
    );
    let evidence: Value = serde_json::from_slice(&evidence_bytes).unwrap();
    assert_eq!(evidence, report["pdf_parse"]["evidence"]);
    exact_keys(
        &evidence,
        &[
            "schema_version",
            "input",
            "config",
            "status",
            "exit_code",
            "host_report",
            "failure",
        ],
    );
    assert_eq!(evidence["schema_version"], 1);
    assert_eq!(evidence["input"]["sha256"], version["content_sha256"]);
    assert_eq!(evidence["input"]["byte_length"], version["byte_length"]);
    assert_eq!(evidence["config"], report["pdf_parse"]["config"]);
    assert_eq!(evidence["status"], report["pdf_parse"]["status"]);
    assert_eq!(evidence["exit_code"], report["exit_code"]);
    if let Some(id) = id {
        let expected = case(id);
        assert_eq!(version["content_sha256"], expected["raw"]["sha256"]);
        assert_eq!(version["byte_length"], expected["raw"]["byte_length"]);
        assert_eq!(report["pdf_parse"]["status"], expected["expected_label"]);
        assert_eq!(report["exit_code"], expected["expected_exit_code"]);
        assert!(
            evidence["host_report"]["response"].is_object(),
            "fixture acceptance requires an actual parser response, not a pre-parser failure"
        );
        assert!(evidence["host_report"]["pid"].as_u64().is_some());
        assert_eq!(evidence["host_report"]["native_exit_code"], 0);
    }
    let config: CacheConfig = serde_json::from_value(evidence["config"].clone()).unwrap();
    let key = sha(&serde_json::to_vec(&CacheKey {
        version_id: report["version_id"].as_str().unwrap(),
        raw_sha256: version["content_sha256"].as_str().unwrap(),
        config: &config,
    })
    .unwrap());
    let parses = table(&db, "pdf_parses");
    let parse = parses.iter().find(|v| v["parse_id"] == parse_id).unwrap();
    assert_eq!(parse["version_id"], report["version_id"]);
    assert_eq!(parse["cache_key"], key);
    assert_eq!(parse["status"], report["pdf_parse"]["status"]);
    assert_eq!(parse["report_path"], parse_relative);
    assert_eq!(parse["report_sha256"], sha(&evidence_bytes));
    assert_eq!(parse["report_byte_length"], evidence_bytes.len() as u64);
    assert_eq!(
        serde_json::from_str::<Value>(parse["config_json"].as_str().unwrap()).unwrap(),
        evidence["config"]
    );
    let parse_root = project.join(format!(".noetome-pdf-parses/{parse_id}"));
    let actual_names: BTreeSet<_> = fs::read_dir(&parse_root)
        .unwrap()
        .map(|v| v.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        actual_names,
        ["artifacts".to_owned(), "report.json".to_owned()]
            .into_iter()
            .collect()
    );
    let artifact_rows: Vec<_> = table(&db, "pdf_artifacts")
        .into_iter()
        .filter(|v| v["parse_id"] == parse_id)
        .collect();
    let page_rows: Vec<_> = table(&db, "pdf_pages")
        .into_iter()
        .filter(|v| v["parse_id"] == parse_id)
        .collect();
    let region_rows: Vec<_> = table(&db, "pdf_regions")
        .into_iter()
        .filter(|v| v["parse_id"] == parse_id)
        .collect();
    let response = &evidence["host_report"]["response"];
    if response.is_null() {
        assert!(artifact_rows.is_empty() && page_rows.is_empty() && region_rows.is_empty());
        assert!(!evidence["failure"].is_null());
        assert!(content(&parse_root.join("artifacts")).is_empty());
        return;
    }
    let mut rebuilt = response.clone();
    let mut resource_bytes = BTreeMap::new();
    let mut artifacts = Vec::new();
    for artifact in &artifact_rows {
        let relative = artifact["relative_path"].as_str().unwrap();
        assert!(relative.starts_with("artifacts/"));
        let bytes = fs::read(owned(&parse_root, relative)).unwrap();
        assert_eq!(
            bytes.len() as u64,
            artifact["byte_length"].as_u64().unwrap()
        );
        assert_eq!(sha(&bytes), artifact["sha256"].as_str().unwrap());
        resource_bytes.insert(relative.to_owned(), bytes);
        artifacts.push(json!({"artifact_id":artifact["artifact_id"],"kind":artifact["kind"],"relative_path":artifact["relative_path"],"byte_length":artifact["byte_length"],"sha256":artifact["sha256"]}));
    }
    let declared: BTreeSet<_> = artifact_rows
        .iter()
        .map(|v| {
            PathBuf::from(
                v["relative_path"]
                    .as_str()
                    .unwrap()
                    .strip_prefix("artifacts/")
                    .unwrap(),
            )
        })
        .collect();
    assert_eq!(
        content(&parse_root.join("artifacts"))
            .into_keys()
            .collect::<BTreeSet<_>>(),
        declared,
        "parse resources differ from declared exact file list"
    );
    // Stable order is the actual T02 manifest order, checked for a bijection.
    let ordered: Vec<_> = response["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            artifacts
                .iter()
                .find(|a| a["artifact_id"] == v["artifact_id"])
                .unwrap()
                .clone()
        })
        .collect();
    assert_eq!(ordered.len(), artifacts.len());
    rebuilt["artifacts"] = json!(ordered);
    let mut pages = Vec::new();
    for page in &page_rows {
        let index = page["physical_page_index"].as_u64().unwrap();
        let mut regions:Vec<_>=region_rows.iter().filter(|v|v["physical_page_index"]==index).map(|v|json!({"region_id":v["region_id"],"ordinal":v["ordinal"],"locator":{"kind":"pdf_page_region","physical_page_index":index,"bbox_pt":serde_json::from_str::<Value>(v["bbox_json"].as_str().unwrap()).unwrap()},"content":{"artifact_id":v["artifact_id"],"sha256":v["sha256"],"start_byte":v["start_byte"],"end_byte":v["end_byte"]},"glyph_ordinals":serde_json::from_str::<Value>(v["glyph_ordinals_json"].as_str().unwrap()).unwrap()})).collect();
        regions.sort_by_key(|v| v["ordinal"].as_u64().unwrap());
        let text = if page["text_artifact_id"].is_null() {
            Value::Null
        } else {
            json!({"artifact_id":page["text_artifact_id"],"sha256":page["text_artifact_sha256"]})
        };
        let geometry = page["geometry_json"]
            .as_str()
            .map(|s| serde_json::from_str::<Value>(s).unwrap())
            .unwrap_or(Value::Null);
        pages.push(json!({"physical_page_index":index,"printed_page_label":page["printed_page_label"],"status":page["status"],"geometry":geometry,"glyph_count":page["glyph_count"],"excluded_glyph_ordinals":serde_json::from_str::<Value>(page["excluded_glyph_ordinals_json"].as_str().unwrap()).unwrap(),"text_artifact":text,"regions":regions}));
    }
    pages.sort_by_key(|v| v["physical_page_index"].as_u64().unwrap());
    rebuilt["pages"] = json!(pages);
    rebuilt["page_count"] = parse["page_count"].clone();
    assert_eq!(
        rebuilt, *response,
        "persisted SQL projection diverges from actual verified T02 response"
    );
    if let Some(id) = id {
        verify_author(&rebuilt, &resource_bytes, &case(id)).unwrap_or_else(|e| panic!("{id}: {e}"));
    }
}

fn artifact_path(response: &Value, id: &str) -> String {
    response["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["artifact_id"] == id)
        .unwrap()["relative_path"]
        .as_str()
        .unwrap()
        .to_owned()
}
fn read_inventory(response: &Value, bytes: &BTreeMap<String, Vec<u8>>) -> Value {
    let path = artifact_path(
        response,
        response["inventory"]["artifact_id"].as_str().unwrap(),
    );
    serde_json::from_slice(&bytes[&path]).unwrap()
}
fn numbers(value: &Value) -> Vec<f64> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect()
}
fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.000001
}
fn raw_to_media(x: f64, y: f64, media: &[f64], rotation: u64) -> (f64, f64) {
    let u = x - media[0];
    let v = y - media[1];
    let width = media[2] - media[0];
    let height = media[3] - media[1];
    match rotation {
        0 => (u, height - v),
        90 => (v, u),
        180 => (width - u, v),
        270 => (height - v, width - u),
        _ => panic!("invalid authored rotation"),
    }
}
fn crop_offset(page: &Value) -> (f64, f64) {
    let media = numbers(&page["raw_media_box"]);
    let crop = numbers(&page["raw_crop_box"]);
    let mut left = f64::INFINITY;
    let mut top = f64::INFINITY;
    for x in [crop[0], crop[2]] {
        for y in [crop[1], crop[3]] {
            let (a, b) = raw_to_media(x, y, &media, page["rotation"].as_u64().unwrap());
            left = left.min(a);
            top = top.min(b);
        }
    }
    (left, top)
}

/// Uses author literals/coordinates. This does not create expected from output.
fn verify_author(
    response: &Value,
    bytes: &BTreeMap<String, Vec<u8>>,
    expected: &Value,
) -> Result<(), String> {
    macro_rules! require {
        ($ok:expr,$message:expr) => {
            if !$ok {
                return Err($message.to_owned());
            }
        };
    }
    require!(
        response["input"]["sha256"] == expected["raw"]["sha256"],
        "raw SHA differs from immutable author input"
    );
    require!(
        response["page_count"] == expected["expected_confirmed_page_count"],
        "full confirmed page count differs from author expected"
    );
    require!(
        response["status"] == expected["expected_response_status"],
        "response status differs from author policy"
    );
    let pages = response["pages"].as_array().unwrap();
    let expected_pages = expected["pages"].as_array().unwrap();
    require!(pages.len() == expected_pages.len(), "incomplete page list");
    if pages.is_empty() {
        require!(
            bytes.is_empty(),
            "unconfirmed page tree must not fabricate artifacts"
        );
        return Ok(());
    }
    let inventory = read_inventory(response, bytes);
    require!(
        inventory["page_count"] == expected["expected_confirmed_page_count"],
        "primitive page count differs"
    );
    for (index, (actual, want)) in pages.iter().zip(expected_pages).enumerate() {
        require!(
            actual["physical_page_index"] == index as u64,
            "wrong physical page index"
        );
        require!(
            actual["printed_page_label"].is_null(),
            "fabricated printed label"
        );
        require!(
            actual["status"] == want["expected_status"],
            "wrong per-page supported/review/failure state"
        );
        require!(
            actual["glyph_count"] == want["expected_glyph_count"],
            "missing or fabricated glyphs"
        );
        let geometry = &actual["geometry"];
        require!(
            numbers(&geometry["media_box"]) == numbers(&want["raw_media_box"]),
            "raw MediaBox differs"
        );
        require!(
            numbers(&geometry["crop_box"]) == numbers(&want["raw_crop_box"]),
            "raw CropBox differs"
        );
        require!(
            geometry["rotation"] == want["rotation"]
                && close(
                    geometry["user_unit"].as_f64().unwrap(),
                    want["user_unit"].as_f64().unwrap()
                ),
            "rotation/UserUnit differs"
        );
        let size = numbers(&want["display_size_pt"]);
        require!(
            close(geometry["display_width_pt"].as_f64().unwrap(), size[0])
                && close(geometry["display_height_pt"].as_f64().unwrap(), size[1]),
            "display pt dimensions differ"
        );
        let source = &inventory["pages"][index];
        if actual["status"] == "failed" {
            require!(
                actual["text_artifact"].is_null()
                    && actual["regions"].as_array().unwrap().is_empty(),
                "failed page fabricated text/regions"
            );
            require!(
                source["glyphs"].is_null()
                    && source["decode_error"]
                        .as_str()
                        .is_some_and(|v| !v.is_empty()),
                "failed page missing real decode diagnostic"
            );
            continue;
        }
        let glyphs = source["glyphs"].as_array().unwrap();
        let primitive_text: String = glyphs
            .iter()
            .map(|glyph| glyph["text"].as_str().unwrap())
            .collect();
        require!(
            primitive_text == want["authored_glyph_stream_utf8"].as_str().unwrap(),
            "author stream text was lost/reordered/fabricated"
        );
        if want["visual_objects_expected"] == true {
            let object_count: u64 = source["visual_objects"]
                .as_object()
                .unwrap()
                .values()
                .map(|v| v.as_u64().unwrap())
                .sum();
            require!(object_count > 0, "visual source evidence omitted");
        }
        // Check every original author line using its primitive ordinals, even
        // for table/multicolumn text whose semantic line grouping is unverified.
        let (crop_left, crop_top) = crop_offset(want);
        let unit = want["user_unit"].as_f64().unwrap();
        let mut cursor = 0;
        for anchor in want["anchors"].as_array().unwrap() {
            let count = anchor["excerpt_utf8"].as_str().unwrap().len();
            let mut bbox = [
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            ];
            for glyph in &glyphs[cursor..cursor + count] {
                let v = numbers(&glyph["media_bbox"]);
                bbox[0] = bbox[0].min((v[0] - crop_left) * unit);
                bbox[1] = bbox[1].min((v[1] - crop_top) * unit);
                bbox[2] = bbox[2].max((v[2] - crop_left) * unit);
                bbox[3] = bbox[3].max((v[3] - crop_top) * unit);
            }
            cursor += count;
            if let Some(windows) = anchor["endpoint_windows_pt"].as_object() {
                for (axis, key) in ["left", "top", "right", "bottom"].iter().enumerate() {
                    let window = numbers(&windows[*key]);
                    require!(
                        bbox[axis] >= window[0] - 0.000001 && bbox[axis] <= window[1] + 0.000001,
                        "glyph bbox points to a different author region"
                    );
                }
            } else {
                let envelope = numbers(&anchor["display_envelope_pt"]);
                require!(
                    bbox[0] >= envelope[0]
                        && bbox[1] >= envelope[1]
                        && bbox[2] <= envelope[2]
                        && bbox[3] <= envelope[3],
                    "rotated glyph position is outside author envelope"
                );
            }
        }
        if !want["text"].is_null() {
            let id = actual["text_artifact"]["artifact_id"].as_str().unwrap();
            let path = artifact_path(response, id);
            let actual_text = &bytes[&path];
            let expected_text = &want["text"];
            require!(
                actual_text == expected_text["utf8"].as_str().unwrap().as_bytes(),
                "independent full page text/order differs"
            );
            require!(
                sha(actual_text) == expected_text["sha256"].as_str().unwrap(),
                "independent text SHA differs"
            );
            let ranges = expected_text["line_ranges"].as_array().unwrap();
            let regions = actual["regions"].as_array().unwrap();
            require!(
                regions.len() == ranges.len(),
                "line/region count differs from author text"
            );
            for (region, range) in regions.iter().zip(ranges) {
                require!(
                    region["content"]["start_byte"] == range["start_byte"]
                        && region["content"]["end_byte"] == range["end_byte"],
                    "author UTF8 half-open range differs"
                );
                require!(
                    region["locator"]["physical_page_index"] == index as u64,
                    "region assigned to wrong page"
                );
                let ordinal = range["ordinal"].as_u64().unwrap() as usize;
                let bounds = &want["anchors"][ordinal]["endpoint_windows_pt"];
                let bbox = numbers(&region["locator"]["bbox_pt"]);
                for (axis, key) in ["left", "top", "right", "bottom"].iter().enumerate() {
                    let window = numbers(&bounds[*key]);
                    require!(
                        bbox[axis] >= window[0] - 0.000001 && bbox[axis] <= window[1] + 0.000001,
                        "line region bbox points to wrong author location"
                    );
                }
            }
        }
    }
    Ok(())
}
