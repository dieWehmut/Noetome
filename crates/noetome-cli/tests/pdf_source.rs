//! Ordinary CLI acceptance for Schema3 PDF sources, using frozen authoring facts.
//! No production validator, parser helper, or fake runtime supplies the oracle.
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs::{self, Metadata};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output};
use std::time::SystemTime;

use rusqlite::{Connection, OpenFlags, types::ValueRef};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

const TABLES: [&str; 10] = [
    "project_metadata",
    "sources",
    "source_versions",
    "source_spans",
    "source_imports",
    "pdf_parses",
    "pdf_pages",
    "pdf_regions",
    "pdf_resources",
    "pdf_imports",
];
const SENTINEL: &[u8] = b"Human bytes outside every selected project.\r\n";
const OLD_RAW: &str = "\u{feff}# 标题\r\n\r\n正文 α\r第二行\n```rust\r\nlet x = 1;\r\n```\r\n";
const OLD_NORMAL: &str = "# 标题\n\n正文 α\n第二行\n```rust\nlet x = 1;\n```\n";

struct Scratch {
    root: PathBuf,
    parent: PathBuf,
    owner: Vec<u8>,
    identity: same_file::Handle,
}

impl Scratch {
    fn new() -> Self {
        let parent = fs::canonicalize(std::env::temp_dir()).unwrap();
        assert!(!is_link(&fs::symlink_metadata(&parent).unwrap()));
        let nonce = Uuid::new_v4();
        let root = parent.join(format!("noetome-pdf-source-test-{nonce}"));
        fs::create_dir(&root).unwrap();
        let owner = serde_json::to_vec(&json!({"nonce": nonce.to_string(), "root": root})).unwrap();
        fs::write(root.join(".test-owner.json"), &owner).unwrap();
        fs::write(root.join("external-sentinel.bin"), SENTINEL).unwrap();
        let identity = same_file::Handle::from_path(&root).unwrap();
        Self {
            root,
            parent,
            owner,
            identity,
        }
    }

    fn command(&self, arguments: &[OsString], runtime: bool) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_noetome"));
        command.args(arguments).current_dir(&self.root);
        if !runtime {
            command.env_remove("NOETOME_PDF_PYTHON");
        }
        command.output().expect("execute actual ordinary CLI")
    }

    fn project(&self, name: &str) -> (PathBuf, Value) {
        let project = self.root.join(name);
        let initialized = self.command(
            &[
                "project".into(),
                "init".into(),
                project.clone().into_os_string(),
            ],
            true,
        );
        assert_eq!(
            initialized.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&initialized.stderr)
        );
        assert!(initialized.stderr.is_empty());
        let config = read_json(&project.join("project.json"));
        assert_eq!(config["schema_version"], 1);
        (project, config)
    }

    fn import(
        &self,
        project: &Path,
        file: &Path,
        id: Option<&str>,
        reparse: bool,
        runtime: bool,
    ) -> Output {
        let mut args = vec![
            "source".into(),
            "import".into(),
            project.as_os_str().to_owned(),
            file.as_os_str().to_owned(),
        ];
        if let Some(id) = id {
            args.extend(["--source-id".into(), id.into()]);
        }
        if reparse {
            args.push("--reparse".into());
        }
        self.command(&args, runtime)
    }

    fn inspect(&self, project: &Path, report: &Value, expected_exit: i32) {
        let before = snapshot(project);
        let sentinel_before = snapshot(&self.root.join("external-sentinel.bin"));
        let output = self.command(
            &[
                "source".into(),
                "inspect".into(),
                project.as_os_str().to_owned(),
                report["import_id"].as_str().unwrap().into(),
            ],
            false,
        );
        assert_eq!(
            output.status.code(),
            Some(expected_exit),
            "read-only inspect: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(json_output(&output), *report);
        assert_eq!(
            snapshot(project),
            before,
            "inspect changed bytes, mtimes, or entries"
        );
        assert_eq!(
            snapshot(&self.root.join("external-sentinel.bin")),
            sentinel_before
        );
        self.sentinel();
    }

    fn sentinel(&self) {
        assert_eq!(
            fs::read(self.root.join("external-sentinel.bin")).unwrap(),
            SENTINEL
        );
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        // A replaced root, wrong owner, or reparse point is evidence to preserve.
        // Never recursively clean an unchecked path, even when a test panics.
        let safe = (|| -> std::io::Result<bool> {
            let metadata = fs::symlink_metadata(&self.root)?;
            if !metadata.is_dir()
                || is_link(&metadata)
                || fs::canonicalize(&self.root)?.parent() != Some(self.parent.as_path())
            {
                return Ok(false);
            }
            if fs::read(self.root.join(".test-owner.json"))? != self.owner {
                return Ok(false);
            }
            if same_file::Handle::from_path(&self.root)? != self.identity {
                return Ok(false);
            }
            fn ordinary(path: &Path) -> std::io::Result<bool> {
                let metadata = fs::symlink_metadata(path)?;
                if is_link(&metadata) {
                    return Ok(false);
                }
                if metadata.is_dir() {
                    for entry in fs::read_dir(path)? {
                        if !ordinary(&entry?.path())? {
                            return Ok(false);
                        }
                    }
                }
                Ok(true)
            }
            ordinary(&self.root)
        })();
        if matches!(safe, Ok(true)) {
            let _ = fs::remove_dir_all(&self.root);
        } else {
            eprintln!(
                "Preserved unverified PDF test root: {}",
                self.root.display()
            );
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Entry {
    bytes: Option<Vec<u8>>,
    modified: Option<SystemTime>,
    readonly: bool,
}

fn is_link(metadata: &Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        return metadata.file_attributes() & 0x400 != 0;
    }
    #[cfg(not(windows))]
    false
}

fn snapshot(root: &Path) -> BTreeMap<PathBuf, Entry> {
    fn visit(root: &Path, path: &Path, out: &mut BTreeMap<PathBuf, Entry>) {
        let meta = fs::symlink_metadata(path).unwrap();
        assert!(
            !is_link(&meta),
            "snapshot must not follow a link: {}",
            path.display()
        );
        out.insert(
            path.strip_prefix(root).unwrap().to_owned(),
            Entry {
                bytes: meta.is_file().then(|| fs::read(path).unwrap()),
                modified: meta.modified().ok(),
                readonly: meta.permissions().readonly(),
            },
        );
        if meta.is_dir() {
            for entry in fs::read_dir(path).unwrap() {
                visit(root, &entry.unwrap().path(), out);
            }
        }
    }
    let mut out = BTreeMap::new();
    visit(root, root, &mut out);
    out
}

fn fixture(name: &str) -> PathBuf {
    fs::canonicalize(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/pdf-text-v1")
            .join(name),
    )
    .unwrap()
}
fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn sha(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}
fn uuid4(value: &Value) {
    let s = value.as_str().unwrap();
    let id = Uuid::parse_str(s).unwrap();
    assert_eq!(id.get_version_num(), 4);
    assert_eq!(id.to_string(), s);
}
fn json_output(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "JSON {error}; stdout={}; stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}
fn success(output: &Output) -> Value {
    assert_eq!(
        output.status.code(),
        Some(0),
        "missing or failed ordinary source CLI: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    json_output(output)
}
fn incomplete(output: &Output, state: &str) -> Value {
    assert_eq!(
        output.status.code(),
        Some(3),
        "PDF failure must be a saved incomplete report: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report = json_output(output);
    assert_eq!(report["status"], state);
    assert_eq!(report["knowledge_status"], "pending");
    if state == "failed" {
        assert!(!report["error"]["code"].as_str().unwrap().is_empty());
        assert!(!report["error"]["message"].as_str().unwrap().is_empty());
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains(report["error"]["code"].as_str().unwrap())
        );
    }
    report
}
fn database(project: &Path) -> Connection {
    Connection::open_with_flags(
        project.join("project.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap()
}
fn rows(connection: &Connection, query: &str) -> Vec<Vec<Value>> {
    let mut stmt = connection.prepare(query).unwrap();
    let count = stmt.column_count();
    stmt.query_map([], |row| {
        (0..count)
            .map(|i| {
                Ok(match row.get_ref(i)? {
                    ValueRef::Null => Value::Null,
                    ValueRef::Integer(n) => json!(n),
                    ValueRef::Real(n) => json!(n),
                    ValueRef::Text(s) => json!(std::str::from_utf8(s).unwrap()),
                    ValueRef::Blob(b) => json!(b),
                })
            })
            .collect::<rusqlite::Result<Vec<_>>>()
    })
    .unwrap()
    .map(Result::unwrap)
    .collect()
}
fn counts(project: &Path) -> BTreeMap<String, u64> {
    let db = database(project);
    let actual: BTreeSet<String> = rows(
        &db,
        "SELECT name FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%'",
    )
    .iter()
    .map(|r| r[0].as_str().unwrap().to_owned())
    .collect();
    assert_eq!(
        actual,
        TABLES.iter().map(|s| (*s).to_owned()).collect(),
        "Schema3 has exactly ten application tables"
    );
    assert_eq!(rows(&db, "PRAGMA user_version"), vec![vec![json!(3)]]);
    assert_eq!(
        rows(&db, "PRAGMA application_id"),
        vec![vec![json!(0x4e4f4554u32)]]
    );
    assert!(rows(&db, "PRAGMA foreign_key_check").is_empty());
    TABLES
        .iter()
        .map(|name| {
            (
                (*name).to_owned(),
                rows(&db, &format!("SELECT count(*) FROM {name}"))[0][0]
                    .as_u64()
                    .unwrap(),
            )
        })
        .collect()
}
fn managed(project: &Path, relative: &Value) -> PathBuf {
    let relative = relative.as_str().unwrap();
    assert!(!relative.contains('\\') && !relative.contains(':'));
    let path = Path::new(relative);
    assert!(!path.is_absolute() && path.components().all(|c| matches!(c, Component::Normal(_))));
    let result = project.join(path);
    assert!(
        fs::canonicalize(&result)
            .unwrap()
            .starts_with(fs::canonicalize(project).unwrap())
    );
    result
}
fn authoring_case(name: &str) -> Value {
    read_json(&fixture("expected.json"))["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["file"] == name)
        .unwrap()
        .clone()
}

fn pdf_version(project: &Path, report: &Value, raw: &[u8]) {
    let keys: BTreeSet<&str> = report
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        [
            "schema_version",
            "report_kind",
            "project_id",
            "import_id",
            "imported_at",
            "selected_path",
            "source_id",
            "version_id",
            "version",
            "status",
            "reused",
            "acquisition_status",
            "pdf",
            "pending_extraction_count",
            "knowledge_status",
            "error"
        ]
        .into_iter()
        .collect()
    );
    assert_eq!(report["schema_version"], 2);
    assert_eq!(report["report_kind"], "source_pdf_import");
    assert_eq!(report["acquisition_status"], "acquired");
    assert_eq!(report["knowledge_status"], "pending");
    for key in ["project_id", "import_id", "source_id", "version_id"] {
        uuid4(&report[key]);
    }
    uuid4(&report["pdf"]["parse_id"]);
    let version = &report["version"];
    assert_eq!(version["source_id"], report["source_id"]);
    assert_eq!(version["version_id"], report["version_id"]);
    assert_eq!(version["media_type"], "application/pdf");
    assert_eq!(version["ingest_status"], "acquired");
    assert_eq!(version["content_sha256"], sha(raw));
    assert_eq!(version["byte_length"], raw.len());
    for key in [
        "normalized_sha256",
        "normalization_version",
        "normalized_snapshot",
        "error",
    ] {
        assert_eq!(
            version[key],
            Value::Null,
            "PDF raw must not become old normalized text: {key}"
        );
    }
    assert_eq!(
        version["raw_snapshot"],
        format!(
            ".noetome-sources/{}/raw.pdf",
            report["version_id"].as_str().unwrap()
        )
    );
    let path = managed(project, &version["raw_snapshot"]);
    assert_eq!(fs::read(&path).unwrap(), raw);
    assert!(!path.parent().unwrap().join("normalized.txt").exists());
    let db = database(project);
    assert_eq!(
        rows(
            &db,
            &format!(
                "SELECT count(*) FROM source_spans WHERE source_version_id='{}'",
                report["version_id"].as_str().unwrap()
            )
        ),
        vec![vec![json!(0)]]
    );
}

fn check_authoring(project: &Path, report: &Value, case: &Value) {
    let parse = &report["pdf"];
    assert_eq!(parse["status"], case["status"]);
    assert_eq!(parse["page_count"], case["parse_page_count"]);
    assert_eq!(parse["outcome_kind"], "validated");
    assert_eq!(parse["protocol_version"], 1);
    assert_eq!(parse["parser"]["engine"], "pdfplumber");
    assert_eq!(parse["parser"]["engine_version"], "0.11.9");
    assert_eq!(parse["requested_parser"], parse["parser"]);
    let root = managed(project, &parse["snapshot_path"]);
    assert_eq!(
        parse["snapshot_path"],
        format!(
            ".noetome-pdf-parses/{}",
            parse["parse_id"].as_str().unwrap()
        )
    );
    let manifest_bytes = fs::read(root.join("parser/pdf-manifest.json")).unwrap();
    assert_eq!(parse["manifest_sha256"], sha(&manifest_bytes));
    let manifest: Value = serde_json::from_slice(&manifest_bytes).unwrap();
    assert_eq!(manifest["input_fingerprint"]["sha256"], case["raw_sha256"]);
    assert_eq!(
        manifest["input_fingerprint"]["byte_length"],
        case["byte_length"]
    );
    assert_eq!(
        fs::read(root.join("parser/raw.pdf")).unwrap(),
        fs::read(fixture(case["file"].as_str().unwrap())).unwrap()
    );
    let actual = manifest["pages"].as_array().unwrap();
    let expected = case["pages"].as_array().unwrap();
    assert_eq!(
        actual.len(),
        expected.len(),
        "no physical page may disappear"
    );
    assert_eq!(parse["confirmed_page_count"], expected.len());
    for (state, field) in [
        ("parsed", "parsed_page_count"),
        ("needs_review", "needs_review_page_count"),
        ("failed", "failed_page_count"),
    ] {
        assert_eq!(
            parse[field],
            expected.iter().filter(|p| p["status"] == state).count()
        );
    }
    let db = database(project);
    let id = parse["parse_id"].as_str().unwrap();
    let pages = rows(
        &db,
        &format!(
            "SELECT physical_page_index,display_page_number,printed_page_label,status,record_json FROM pdf_pages WHERE parse_id='{id}' ORDER BY physical_page_index"
        ),
    );
    assert_eq!(pages.len(), expected.len());
    let mut region_count = 0;
    for ((page, want), stored) in actual.iter().zip(expected).zip(&pages) {
        for field in [
            "physical_page_index",
            "display_page_number",
            "printed_page_label",
            "status",
        ] {
            assert_eq!(page[field], want[field], "{field}");
        }
        assert_eq!(
            &stored[..4],
            &[
                want["physical_page_index"].clone(),
                want["display_page_number"].clone(),
                want["printed_page_label"].clone(),
                want["status"].clone()
            ]
        );
        assert_eq!(
            serde_json::from_str::<Value>(stored[4].as_str().unwrap()).unwrap(),
            *page
        );
        region_count += page["regions"].as_array().unwrap().len();
        if want["status"] == "failed" {
            assert!(
                page["text"].is_null()
                    && page["glyphs"].as_array().unwrap().is_empty()
                    && page["regions"].as_array().unwrap().is_empty()
            );
            continue;
        }
        let glyphs = page["glyphs"].as_array().unwrap();
        let want_glyphs = want["glyph_positions"].as_array().unwrap();
        assert_eq!(glyphs.len(), want_glyphs.len());
        for (glyph, want_glyph) in glyphs.iter().zip(want_glyphs) {
            assert_eq!(glyph["ordinal"], want_glyph["ordinal"]);
            assert_eq!(glyph["text"], want_glyph["text"]);
            for axis in 0..4 {
                assert!(
                    (glyph["bbox"][axis].as_f64().unwrap()
                        - want_glyph["bbox"][axis].as_f64().unwrap())
                    .abs()
                        <= want_glyph["tolerance_pt"].as_f64().unwrap()
                );
            }
        }
        let lines = want["lines"].as_array().unwrap();
        if lines.is_empty() {
            continue;
        }
        let text = fs::read(root.join(format!(
            "parser/pages/{:06}.txt",
            want["physical_page_index"].as_u64().unwrap()
        )))
        .unwrap();
        let literal: String = lines
            .iter()
            .map(|line| format!("{}\n", line["text"].as_str().unwrap()))
            .collect();
        assert_eq!(text, literal.as_bytes());
        let regions = page["regions"].as_array().unwrap();
        assert_eq!(regions.len(), lines.len());
        let mut cursor = 0;
        for (region, line) in regions.iter().zip(lines) {
            assert_eq!(region["locator_kind"], "pdf_page_region");
            assert_eq!(region["physical_page_index"], want["physical_page_index"]);
            let start = region["text"]["start_byte"].as_u64().unwrap() as usize;
            let end = region["text"]["end_byte"].as_u64().unwrap() as usize;
            assert_eq!(start, cursor);
            assert_eq!(&text[start..end], line["text"].as_str().unwrap().as_bytes());
            assert_eq!(text[end], b'\n');
            cursor = end + 1;
            assert_eq!(region["text"]["resource_sha256"], sha(&text));
            for axis in 0..4 {
                assert!(
                    (region["bbox"][axis].as_f64().unwrap() - line["bbox"][axis].as_f64().unwrap())
                        .abs()
                        <= line["tolerance_pt"].as_f64().unwrap()
                );
            }
            let indexed = rows(
                &db,
                &format!(
                    "SELECT physical_page_index,locator_kind,bbox_left,bbox_top,bbox_right,bbox_bottom,text_resource_id,text_resource_sha256,start_byte,end_byte,glyph_indices_json FROM pdf_regions WHERE parse_id='{id}' AND region_id='{}'",
                    region["region_id"].as_str().unwrap()
                ),
            );
            assert_eq!(indexed.len(), 1);
            assert_eq!(indexed[0][0], want["physical_page_index"]);
            assert_eq!(indexed[0][1], "pdf_page_region");
            for axis in 0..4 {
                assert_eq!(indexed[0][2 + axis].as_f64(), region["bbox"][axis].as_f64());
            }
            for (i, field) in [
                (6, "resource_id"),
                (7, "resource_sha256"),
                (8, "start_byte"),
                (9, "end_byte"),
            ] {
                assert_eq!(indexed[0][i], region["text"][field]);
            }
            assert_eq!(
                serde_json::from_str::<Value>(indexed[0][10].as_str().unwrap()).unwrap(),
                region["glyph_indices"]
            );
        }
        assert_eq!(cursor, text.len());
    }
    assert_eq!(parse["region_count"], region_count);
    assert_eq!(report["pending_extraction_count"], region_count);
    let resources = rows(
        &db,
        &format!(
            "SELECT relative_path,sha256,byte_length,confidence FROM pdf_resources WHERE parse_id='{id}'"
        ),
    );
    let mut names = BTreeSet::new();
    for row in &resources {
        let path = managed(&root, &row[0]);
        let data = fs::read(&path).unwrap();
        assert_eq!(row[1], sha(&data));
        assert_eq!(row[2], data.len());
        let relative = row[0].as_str().unwrap();
        let confidence = if relative.starts_with("parser/") {
            "confirmed"
        } else {
            assert!(matches!(relative, "parse-result.json" | "parse-result.md"));
            "diagnostic"
        };
        assert_eq!(row[3], confidence);
        assert!(names.insert(PathBuf::from(row[0].as_str().unwrap())));
    }
    let files: BTreeSet<PathBuf> = snapshot(&root)
        .into_iter()
        .filter_map(|(path, entry)| entry.bytes.map(|_| path))
        .collect();
    assert_eq!(
        names, files,
        "every snapshot file needs a registered resource, both ways"
    );
    assert_eq!(
        parse["outcome_sha256"],
        sha(&fs::read(root.join("parse-result.json")).unwrap())
    );
}

#[test]
fn schema1_pdf_import_registers_authored_pages_and_exact_ten_tables() {
    // Removing dispatch, inventing a normalized PDF, or dropping pages must fail.
    let scratch = Scratch::new();
    let (project, initial) = scratch.project("empty project");
    let input = fixture("multiple-pages.pdf");
    let raw = fs::read(&input).unwrap();
    let report = success(&scratch.import(&project, &input, None, false, true));
    let config = read_json(&project.join("project.json"));
    assert_eq!(config["schema_version"], 3);
    assert_eq!(config["project_id"], initial["project_id"]);
    assert_eq!(config["created_at"], initial["created_at"]);
    pdf_version(&project, &report, &raw);
    check_authoring(&project, &report, &authoring_case("multiple-pages.pdf"));
    let observed = counts(&project);
    for (table, count) in [
        ("project_metadata", 1),
        ("sources", 1),
        ("source_versions", 1),
        ("source_spans", 0),
        ("source_imports", 0),
        ("pdf_parses", 1),
        ("pdf_pages", 3),
        ("pdf_regions", 6),
        ("pdf_resources", 7),
        ("pdf_imports", 1),
    ] {
        assert_eq!(observed[table], count, "{table}");
    }
    scratch.inspect(&project, &report, 0);
    assert_eq!(fs::read(input).unwrap(), raw);
    scratch.sentinel();
}

#[test]
fn schema2_upgrade_preserves_all_old_rows_reports_snapshots_and_text_semantics() {
    // A migration must preserve every old identity and historical reused flag.
    let scratch = Scratch::new();
    let (project, initial) = scratch.project("old text project");
    let md = scratch.root.join("old source.MD");
    fs::write(&md, OLD_RAW.as_bytes()).unwrap();
    let first = success(&scratch.import(&project, &md, None, false, true));
    assert_eq!(
        first["version"]["normalized_sha256"],
        sha(OLD_NORMAL.as_bytes())
    );
    assert_eq!(
        fs::read(managed(&project, &first["version"]["normalized_snapshot"])).unwrap(),
        OLD_NORMAL.as_bytes()
    );
    let wanted = [
        ("heading", 0, 9, 1, 1),
        ("formatting", 9, 10, 2, 2),
        ("text", 10, 30, 3, 4),
        ("code", 30, 53, 5, 7),
    ];
    for (span, (kind, start, end, line, last)) in
        first["spans"].as_array().unwrap().iter().zip(wanted)
    {
        assert_eq!(
            (
                &span["kind"],
                &span["start_byte"],
                &span["end_byte"],
                &span["start_line"],
                &span["end_line"]
            ),
            (
                &json!(kind),
                &json!(start),
                &json!(end),
                &json!(line),
                &json!(last)
            )
        );
        assert_eq!(span["parser_version"], "blocks-v1");
        assert_eq!(span["text_sha256"], sha(&OLD_NORMAL.as_bytes()[start..end]));
    }
    let repeat = success(&scratch.import(&project, &md, None, false, true));
    fs::write(&md, b"changed\r\n").unwrap();
    let changed = success(&scratch.import(&project, &md, None, false, true));
    fs::write(&md, OLD_RAW.as_bytes()).unwrap();
    let back = success(&scratch.import(&project, &md, None, false, true));
    assert_eq!(back["version_id"], first["version_id"]);
    assert_eq!(back["spans"], first["spans"]);
    let invalid = scratch.root.join("invalid.txt");
    fs::write(&invalid, b"\xff\x00old actual raw").unwrap();
    let invalid_report = incomplete(
        &scratch.import(&project, &invalid, None, false, true),
        "failed",
    );
    let missing_report = incomplete(
        &scratch.import(
            &project,
            &scratch.root.join("absent.txt"),
            None,
            false,
            true,
        ),
        "failed",
    );
    let old_reports = [first, repeat, changed, back, invalid_report, missing_report];
    assert_eq!(
        read_json(&project.join("project.json"))["schema_version"],
        2
    );
    let old_rows = {
        let db = database(&project);
        [
            "sources",
            "source_versions",
            "source_spans",
            "source_imports",
        ]
        .map(|table| {
            (
                table,
                rows(&db, &format!("SELECT rowid,* FROM {table} ORDER BY rowid")),
            )
        })
    };
    let old_files = snapshot(&project.join(".noetome-sources"));
    let pdf = success(&scratch.import(&project, &fixture("single-page.pdf"), None, false, true));
    assert_eq!(
        read_json(&project.join("project.json"))["schema_version"],
        3
    );
    assert_eq!(
        read_json(&project.join("project.json"))["project_id"],
        initial["project_id"]
    );
    assert_eq!(
        read_json(&project.join("project.json"))["created_at"],
        initial["created_at"]
    );
    let db = database(&project);
    for (table, original) in old_rows {
        let now = rows(&db, &format!("SELECT rowid,* FROM {table} ORDER BY rowid"));
        assert_eq!(
            &now[..original.len()],
            original,
            "old rows changed: {table}"
        );
    }
    let now = snapshot(&project.join(".noetome-sources"));
    for (path, original) in old_files {
        if original.bytes.is_some() {
            assert_eq!(
                now[&path],
                original,
                "old immutable snapshot changed: {}",
                path.display()
            );
        }
    }
    drop(db);
    for old in &old_reports {
        scratch.inspect(&project, old, 0);
    }
    check_authoring(&project, &pdf, &authoring_case("single-page.pdf"));
    let after_text = scratch.root.join("after.txt");
    fs::write(&after_text, b"still\r\nplain\rtext").unwrap();
    let text = success(&scratch.import(&project, &after_text, None, false, true));
    assert_eq!(text["schema_version"], 1);
    assert_eq!(text["version"]["normalization_version"], "utf8-newlines-v1");
    assert_eq!(
        fs::read(managed(&project, &text["version"]["normalized_snapshot"])).unwrap(),
        b"still\nplain\ntext"
    );
    assert_eq!(
        read_json(&project.join("project.json"))["schema_version"],
        3
    );
    scratch.inspect(&project, &text, 0);
    // Existing MD/TXT byte identity is unchanged after the PDF migration:
    // changing the extension preserves the original Version and media label.
    let same_text_markdown = scratch.root.join("same-text-renamed.md");
    fs::write(&same_text_markdown, b"still\r\nplain\rtext").unwrap();
    let renamed_text = success(&scratch.import(&project, &same_text_markdown, None, false, true));
    assert_eq!(renamed_text["version_id"], text["version_id"]);
    assert_eq!(renamed_text["version"], text["version"]);
    assert_eq!(renamed_text["reused"], true);
    scratch.inspect(&project, &renamed_text, 0);
    scratch.sentinel();
}

#[test]
fn same_bytes_repeat_and_rename_reuse_version_parse_without_new_evidence() {
    // Reparse-on-repeat or global-SHA merging corrupts persistent identity.
    let scratch = Scratch::new();
    let (project, _) = scratch.project("reuse project");
    let selected = scratch.root.join("第88章 page999.pdf");
    let original = fs::read(fixture("single-page.pdf")).unwrap();
    fs::write(&selected, &original).unwrap();
    let first = success(&scratch.import(&project, &selected, None, false, true));
    let before = counts(&project);
    let snapshots = snapshot(&managed(&project, &first["pdf"]["snapshot_path"]));
    let repeat = success(&scratch.import(&project, &selected, None, false, true));
    let renamed = scratch.root.join("renamed.pdf");
    fs::rename(&selected, &renamed).unwrap();
    let rename = success(&scratch.import(&project, &renamed, None, false, true));
    for report in [&repeat, &rename] {
        for key in ["source_id", "version_id"] {
            assert_eq!(report[key], first[key]);
        }
        assert_ne!(report["import_id"], first["import_id"]);
        assert_eq!(report["reused"], true);
        assert_eq!(report["pdf"]["parse_id"], first["pdf"]["parse_id"]);
        assert_eq!(report["pdf"]["reused"], true);
        assert_eq!(report["version"], first["version"]);
        scratch.inspect(&project, report, 0);
    }
    assert_ne!(repeat["import_id"], rename["import_id"]);
    let mut want = before;
    want.insert("pdf_imports".to_owned(), 3);
    assert_eq!(counts(&project), want);
    assert_eq!(
        snapshot(&managed(&project, &first["pdf"]["snapshot_path"])),
        snapshots
    );
    fs::write(&renamed, fs::read(fixture("multiple-pages.pdf")).unwrap()).unwrap();
    let changed = success(&scratch.import(&project, &renamed, None, false, true));
    assert_eq!(changed["source_id"], first["source_id"]);
    assert_ne!(changed["version_id"], first["version_id"]);
    assert_eq!(changed["version"]["revision"], 2);
    fs::write(&renamed, &original).unwrap();
    let back = success(&scratch.import(&project, &renamed, None, false, true));
    assert_eq!(back["version_id"], first["version_id"]);
    assert_eq!(back["pdf"]["parse_id"], first["pdf"]["parse_id"]);
    let old = snapshot(&managed(&project, &first["pdf"]["snapshot_path"]));
    let fresh = success(&scratch.import(&project, &renamed, None, true, true));
    assert_eq!(fresh["version_id"], first["version_id"]);
    assert_eq!(fresh["reused"], true);
    assert_ne!(fresh["pdf"]["parse_id"], first["pdf"]["parse_id"]);
    assert_eq!(fresh["pdf"]["reused"], false);
    assert_ne!(fresh["pdf"]["cache_key"], first["pdf"]["cache_key"]);
    assert_eq!(
        fresh["pdf"]["import_options_version"],
        "pdf-import-fresh-v1"
    );
    let second_fresh = success(&scratch.import(&project, &renamed, None, true, true));
    assert_eq!(second_fresh["version_id"], first["version_id"]);
    assert_eq!(second_fresh["pdf"]["cache_key"], fresh["pdf"]["cache_key"]);
    assert_ne!(second_fresh["pdf"]["parse_id"], fresh["pdf"]["parse_id"]);
    assert_eq!(second_fresh["pdf"]["reused"], false);
    assert_eq!(
        snapshot(&managed(&project, &first["pdf"]["snapshot_path"])),
        old
    );
    assert_eq!(
        fs::read(managed(&project, &first["version"]["raw_snapshot"])).unwrap(),
        original
    );
    scratch.inspect(&project, &first, 0);
    scratch.inspect(&project, &fresh, 0);
    scratch.inspect(&project, &second_fresh, 0);
    scratch.sentinel();
}

#[test]
fn damaged_encrypted_partial_and_review_keep_raw_and_honest_page_counts() {
    // Failed parses cannot be reused as ready or lose confirmed physical pages.
    let scratch = Scratch::new();
    for name in [
        "damaged.pdf",
        "encrypted.pdf",
        "partial-page-failure.pdf",
        "scanned-page.pdf",
    ] {
        let (project, _) = scratch.project(name);
        let input = fixture(name);
        let raw = fs::read(&input).unwrap();
        let case = authoring_case(name);
        let first = incomplete(
            &scratch.import(&project, &input, None, false, true),
            if case["status"] == "failed" {
                "failed"
            } else {
                "needs_review"
            },
        );
        pdf_version(&project, &first, &raw);
        check_authoring(&project, &first, &case);
        scratch.inspect(&project, &first, 3);
        let repeat = incomplete(
            &scratch.import(&project, &input, None, false, true),
            if case["status"] == "failed" {
                "failed"
            } else {
                "needs_review"
            },
        );
        assert_eq!(repeat["version_id"], first["version_id"]);
        assert_ne!(repeat["import_id"], first["import_id"]);
        if case["status"] == "failed" {
            assert_ne!(repeat["pdf"]["parse_id"], first["pdf"]["parse_id"]);
            assert_eq!(repeat["pdf"]["reused"], false);
        } else {
            assert_eq!(repeat["pdf"]["parse_id"], first["pdf"]["parse_id"]);
            assert_eq!(repeat["pdf"]["reused"], true);
        }
        let valid =
            success(&scratch.import(&project, &fixture("single-page.pdf"), None, false, true));
        assert_eq!(valid["status"], "ready");
        scratch.inspect(&project, &first, 3);
        assert_eq!(fs::read(input).unwrap(), raw);
    }
    scratch.sentinel();
}

#[test]
fn missing_runtime_keeps_acquired_raw_and_failed_attempt_then_recovers_readonly() {
    // Acquisition is independent of interpreter availability; inspect uses saved bytes.
    let scratch = Scratch::new();
    let (project, _) = scratch.project("runtime recovery");
    let selected = scratch.root.join("selected.pdf");
    let raw = fs::read(fixture("single-page.pdf")).unwrap();
    fs::write(&selected, &raw).unwrap();
    let failed = incomplete(
        &scratch.import(&project, &selected, None, false, false),
        "failed",
    );
    pdf_version(&project, &failed, &raw);
    assert_eq!(failed["pdf"]["outcome_kind"], "host_failed");
    assert_eq!(failed["pdf"]["parser"], Value::Null);
    assert_eq!(failed["pdf"]["page_count"], Value::Null);
    for key in [
        "confirmed_page_count",
        "parsed_page_count",
        "needs_review_page_count",
        "failed_page_count",
        "region_count",
    ] {
        assert_eq!(failed["pdf"][key], 0);
    }
    assert_eq!(failed["error"]["code"], "PDF_RUNTIME");
    let failed_snapshot = snapshot(&managed(&project, &failed["pdf"]["snapshot_path"]));
    let failed_again = incomplete(
        &scratch.import(&project, &selected, None, false, false),
        "failed",
    );
    assert_eq!(failed_again["version_id"], failed["version_id"]);
    assert_ne!(failed_again["import_id"], failed["import_id"]);
    assert_ne!(failed_again["pdf"]["parse_id"], failed["pdf"]["parse_id"]);
    assert_eq!(failed_again["pdf"]["reused"], false);
    let valid = success(&scratch.import(&project, &selected, None, false, true));
    assert_eq!(valid["version_id"], failed["version_id"]);
    assert_ne!(valid["pdf"]["parse_id"], failed["pdf"]["parse_id"]);
    assert_eq!(valid["reused"], true);
    assert_eq!(valid["pdf"]["reused"], false);
    check_authoring(&project, &valid, &authoring_case("single-page.pdf"));
    fs::rename(&selected, scratch.root.join("original moved away.pdf")).unwrap();
    scratch.inspect(&project, &failed, 3);
    scratch.inspect(&project, &failed_again, 3);
    scratch.inspect(&project, &valid, 0);
    assert_eq!(
        snapshot(&managed(&project, &failed["pdf"]["snapshot_path"])),
        failed_snapshot
    );
    assert_eq!(counts(&project)["source_versions"], 1);
    assert_eq!(counts(&project)["pdf_parses"], 3);
    scratch.sentinel();
}

#[test]
fn reparse_on_schema1_creates_one_fresh_real_attempt() {
    // Omitting the ordinary --reparse flag must fail before parser dispatch.
    let scratch = Scratch::new();
    let (project, _) = scratch.project("fresh option project");
    let selected = fixture("single-page.pdf");
    let report = success(&scratch.import(&project, &selected, None, true, true));
    pdf_version(&project, &report, &fs::read(&selected).unwrap());
    check_authoring(&project, &report, &authoring_case("single-page.pdf"));
    assert_eq!(
        report["pdf"]["import_options_version"],
        "pdf-import-fresh-v1"
    );
    assert_eq!(report["pdf"]["reused"], false);
    assert_eq!(report["reused"], false);
    assert_eq!(counts(&project)["pdf_parses"], 1);
    scratch.sentinel();
}

#[test]
fn explicit_source_id_isolates_same_sha_versions_and_parse_evidence() {
    // A global content cache must never merge a caller-selected existing Source.
    let scratch = Scratch::new();
    let (project, _) = scratch.project("explicit source project");
    let alpha_path = scratch.root.join("alpha.pdf");
    let beta_path = scratch.root.join("beta.pdf");
    let alpha_raw = fs::read(fixture("single-page.pdf")).unwrap();
    fs::write(&alpha_path, &alpha_raw).unwrap();
    fs::write(&beta_path, fs::read(fixture("multiple-pages.pdf")).unwrap()).unwrap();
    let alpha = success(&scratch.import(&project, &alpha_path, None, false, true));
    let beta = success(&scratch.import(&project, &beta_path, None, false, true));
    assert_ne!(alpha["source_id"], beta["source_id"]);
    let alpha_snapshot = snapshot(&managed(&project, &alpha["pdf"]["snapshot_path"]));
    fs::write(&beta_path, &alpha_raw).unwrap();
    let beta_same = success(&scratch.import(
        &project,
        &beta_path,
        beta["source_id"].as_str(),
        false,
        true,
    ));
    assert_eq!(beta_same["source_id"], beta["source_id"]);
    assert_eq!(beta_same["version"]["revision"], 2);
    assert_eq!(
        beta_same["version"]["content_sha256"],
        alpha["version"]["content_sha256"]
    );
    assert_ne!(beta_same["version_id"], alpha["version_id"]);
    assert_ne!(beta_same["pdf"]["parse_id"], alpha["pdf"]["parse_id"]);
    assert_eq!(beta_same["reused"], false);
    assert_eq!(beta_same["pdf"]["reused"], false);
    let before = counts(&project);
    let repeated = success(&scratch.import(
        &project,
        &beta_path,
        beta["source_id"].as_str(),
        false,
        true,
    ));
    assert_eq!(repeated["version_id"], beta_same["version_id"]);
    assert_eq!(repeated["pdf"]["parse_id"], beta_same["pdf"]["parse_id"]);
    assert_eq!(repeated["reused"], true);
    assert_eq!(repeated["pdf"]["reused"], true);
    let mut expected = before;
    *expected.get_mut("pdf_imports").unwrap() += 1;
    assert_eq!(counts(&project), expected);
    assert_eq!(counts(&project)["sources"], 2);
    assert_eq!(counts(&project)["source_versions"], 3);
    assert_eq!(
        snapshot(&managed(&project, &alpha["pdf"]["snapshot_path"])),
        alpha_snapshot
    );
    scratch.inspect(&project, &alpha, 0);
    scratch.inspect(&project, &beta, 0);
    scratch.inspect(&project, &beta_same, 0);
    scratch.sentinel();
}

fn no_acquired_version(project: &Path, report: &Value) {
    assert_eq!(report["schema_version"], 2);
    assert_eq!(report["report_kind"], "source_pdf_import");
    assert_eq!(report["acquisition_status"], "failed");
    assert_eq!(report["status"], "failed");
    assert_eq!(report["reused"], false);
    for field in ["source_id", "version_id", "version"] {
        assert_eq!(report[field], Value::Null);
    }
    let pdf = &report["pdf"];
    assert_eq!(pdf["status"], "not_attempted");
    assert_eq!(pdf["reused"], false);
    for field in [
        "parse_id",
        "requested_parser",
        "parser",
        "protocol_version",
        "import_options_version",
        "adapter_sha256",
        "cache_key",
        "page_count",
        "snapshot_path",
        "outcome_kind",
        "outcome_sha256",
        "manifest_sha256",
        "error",
    ] {
        assert_eq!(
            pdf[field],
            Value::Null,
            "unread input fabricated PDF metadata: {field}"
        );
    }
    for field in [
        "confirmed_page_count",
        "parsed_page_count",
        "needs_review_page_count",
        "failed_page_count",
        "region_count",
    ] {
        assert_eq!(pdf[field], 0);
    }
    assert_eq!(pdf["issues"], json!([]));
    assert_eq!(report["pending_extraction_count"], 0);
    let observed = counts(project);
    for table in [
        "sources",
        "source_versions",
        "source_spans",
        "source_imports",
        "pdf_parses",
        "pdf_pages",
        "pdf_regions",
        "pdf_resources",
    ] {
        assert_eq!(observed[table], 0, "unread input invented {table}");
    }
    assert_eq!(observed["pdf_imports"], 1);
    for cache in [".noetome-sources", ".noetome-pdf-parses"] {
        if project.join(cache).exists() {
            let files: Vec<_> = fs::read_dir(project.join(cache))
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .collect();
            assert!(
                files.iter().all(|name| name == ".owner.json"),
                "unread input produced an evidence tree"
            );
        }
    }
}

#[test]
fn missing_and_oversized_pdf_record_only_failed_import_then_valid_input_recovers() {
    // No truncated fingerprint, Version, empty normalized text, or locator is real.
    let scratch = Scratch::new();
    for too_large in [false, true] {
        let (project, _) = scratch.project(if too_large {
            "large acquisition"
        } else {
            "missing acquisition"
        });
        let selected = scratch.root.join(if too_large {
            "too-large.pdf"
        } else {
            "missing.pdf"
        });
        if too_large {
            let file = fs::File::create(&selected).unwrap();
            file.set_len(64 * 1024 * 1024 + 1).unwrap();
        }
        let metadata = too_large.then(|| {
            let m = fs::metadata(&selected).unwrap();
            (m.len(), m.modified().unwrap())
        });
        let failed = incomplete(
            &scratch.import(&project, &selected, None, false, true),
            "failed",
        );
        no_acquired_version(&project, &failed);
        if too_large {
            assert_eq!(failed["error"]["code"], "SOURCE_TOO_LARGE");
        }
        scratch.inspect(&project, &failed, 3);
        if let Some(original) = metadata {
            let now = fs::metadata(&selected).unwrap();
            assert_eq!((now.len(), now.modified().unwrap()), original);
        }
        let valid =
            success(&scratch.import(&project, &fixture("single-page.pdf"), None, false, true));
        assert_eq!(valid["status"], "ready");
        assert_eq!(counts(&project)["source_versions"], 1);
        assert_eq!(counts(&project)["pdf_imports"], 2);
        scratch.inspect(&project, &failed, 3);
    }
    scratch.sentinel();
}

#[cfg(windows)]
#[test]
fn actual_windows_exclusive_pdf_lock_creates_no_version_and_unlock_recovers() {
    use std::os::windows::fs::OpenOptionsExt;
    let scratch = Scratch::new();
    let (project, _) = scratch.project("locked acquisition");
    let selected = scratch.root.join("locked.pdf");
    let raw = fs::read(fixture("single-page.pdf")).unwrap();
    fs::write(&selected, &raw).unwrap();
    let guard = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&selected)
        .unwrap();
    let failed = incomplete(
        &scratch.import(&project, &selected, None, false, true),
        "failed",
    );
    no_acquired_version(&project, &failed);
    drop(guard);
    assert_eq!(fs::read(&selected).unwrap(), raw);
    scratch.inspect(&project, &failed, 3);
    let valid = success(&scratch.import(&project, &selected, None, false, true));
    pdf_version(&project, &valid, &raw);
    scratch.inspect(&project, &valid, 0);
    scratch.sentinel();
}

fn rejected_inspect(scratch: &Scratch, project: &Path, report: &Value, diagnostic: Option<&str>) {
    let before = snapshot(project);
    let sentinel = snapshot(&scratch.root.join("external-sentinel.bin"));
    let output = scratch.command(
        &[
            "source".into(),
            "inspect".into(),
            project.as_os_str().to_owned(),
            report["import_id"].as_str().unwrap().into(),
        ],
        false,
    );
    assert!(
        !output.status.success(),
        "accepted corrupted registered evidence"
    );
    assert!(
        output.stdout.is_empty(),
        "corrupt evidence must not yield a saved success report"
    );
    assert!(!output.stderr.is_empty());
    if let Some(code) = diagnostic {
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(code),
            "corruption failed for an unrelated stale-hash/report reason: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert_eq!(
        snapshot(project),
        before,
        "rejected inspect mutated or cleaned the scene"
    );
    assert_eq!(
        snapshot(&scratch.root.join("external-sentinel.bin")),
        sentinel
    );
}

#[test]
fn changed_raw_and_sql_resource_or_page_index_are_rejected_without_writes() {
    // Schema shape plus outer fingerprint checks cannot replace cross-index validation.
    let scratch = Scratch::new();
    for mutation in ["raw", "resource_index", "page_json", "region_index"] {
        let (project, _) = scratch.project(mutation);
        let report =
            success(&scratch.import(&project, &fixture("single-page.pdf"), None, false, true));
        let id = report["pdf"]["parse_id"].as_str().unwrap();
        match mutation {
            "raw" => {
                let path = managed(&project, &report["version"]["raw_snapshot"]);
                let mut bytes = fs::read(&path).unwrap();
                bytes[0] = b'!';
                fs::write(path, bytes).unwrap();
            }
            "resource_index" => {
                let db = Connection::open(project.join("project.sqlite")).unwrap();
                assert_eq!(db.execute("UPDATE pdf_resources SET byte_length=byte_length+1 WHERE parse_id=?1 AND relative_path='parser/pages/000000.txt'", [id]).unwrap(), 1);
            }
            "page_json" => {
                let db = Connection::open(project.join("project.sqlite")).unwrap();
                let bytes: String = db
                    .query_row(
                        "SELECT record_json FROM pdf_pages WHERE parse_id=?1",
                        [id],
                        |r| r.get(0),
                    )
                    .unwrap();
                let mut page: Value = serde_json::from_str(&bytes).unwrap();
                page["display_page_number"] = json!(88);
                db.execute(
                    "UPDATE pdf_pages SET record_json=?1 WHERE parse_id=?2",
                    rusqlite::params![serde_json::to_string(&page).unwrap(), id],
                )
                .unwrap();
            }
            "region_index" => {
                let db = Connection::open(project.join("project.sqlite")).unwrap();
                db.execute("UPDATE pdf_regions SET bbox_left=bbox_left+1,bbox_right=bbox_right+1 WHERE parse_id=?1", [id]).unwrap();
            }
            _ => unreachable!(),
        }
        rejected_inspect(&scratch, &project, &report, None);
    }
    scratch.sentinel();
}

fn sync_modified_manifest(project: &Path, report: &Value, manifest: &Value) {
    // Only mutation setup: synchronize outer hashes and SQL so the independent
    // glyph/text semantics, rather than an out-of-date checksum, must reject.
    let root = managed(project, &report["pdf"]["snapshot_path"]);
    let id = report["pdf"]["parse_id"].as_str().unwrap();
    let bytes = serde_json::to_vec_pretty(manifest).unwrap();
    let digest = sha(&bytes);
    fs::write(root.join("parser/pdf-manifest.json"), &bytes).unwrap();
    let mut outcome = read_json(&root.join("parse-result.json"));
    let old_outcome = outcome.clone();
    outcome["manifest_sha256"] = json!(digest);
    for saved in manifest["resources"].as_array().unwrap() {
        if let Some(indexed) = outcome["sidecar"]["resources"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|r| r["resource_id"] == saved["resource_id"])
        {
            indexed["fingerprint"] = saved["fingerprint"].clone();
        }
    }
    let resource = outcome["sidecar"]["resources"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|r| r["resource_id"] == "pdf-manifest")
        .unwrap();
    resource["fingerprint"]["sha256"] = json!(digest);
    resource["fingerprint"]["byte_length"] = json!(bytes.len());
    outcome["resource_bytes"] = json!(
        outcome["sidecar"]["resources"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["fingerprint"]["byte_length"].as_u64().unwrap())
            .sum::<u64>()
    );
    let outcome_bytes = serde_json::to_vec_pretty(&outcome).unwrap();
    fs::write(root.join("parse-result.json"), &outcome_bytes).unwrap();
    let markdown = fs::read_to_string(root.join("parse-result.md"))
        .unwrap()
        .replace(old_outcome["manifest_sha256"].as_str().unwrap(), &digest)
        .replace(
            &format!("| resource_bytes | {} |", old_outcome["resource_bytes"]),
            &format!("| resource_bytes | {} |", outcome["resource_bytes"]),
        );
    fs::write(root.join("parse-result.md"), markdown.as_bytes()).unwrap();
    let db = Connection::open(project.join("project.sqlite")).unwrap();
    db.execute(
        "UPDATE pdf_parses SET manifest_sha256=?1,outcome_sha256=?2 WHERE parse_id=?3",
        rusqlite::params![digest, sha(&outcome_bytes), id],
    )
    .unwrap();
    for (path, data) in [
        ("parser/pdf-manifest.json", bytes),
        ("parse-result.json", outcome_bytes),
        ("parse-result.md", markdown.into_bytes()),
    ] {
        assert_eq!(db.execute("UPDATE pdf_resources SET sha256=?1,byte_length=?2 WHERE parse_id=?3 AND relative_path=?4", rusqlite::params![sha(&data), data.len() as i64, id, path]).unwrap(), 1);
    }
    for resource in manifest["resources"].as_array().unwrap() {
        assert_eq!(db.execute("UPDATE pdf_resources SET sha256=?1,byte_length=?2 WHERE parse_id=?3 AND resource_id=?4", rusqlite::params![resource["fingerprint"]["sha256"].as_str().unwrap(),resource["fingerprint"]["byte_length"].as_u64().unwrap() as i64,id,resource["resource_id"].as_str().unwrap()]).unwrap(),1);
    }
    for page in manifest["pages"].as_array().unwrap() {
        let index = page["physical_page_index"].as_u64().unwrap() as i64;
        db.execute(
            "UPDATE pdf_pages SET record_json=?1 WHERE parse_id=?2 AND physical_page_index=?3",
            rusqlite::params![serde_json::to_string(page).unwrap(), id, index],
        )
        .unwrap();
        for region in page["regions"].as_array().unwrap() {
            db.execute("UPDATE pdf_regions SET bbox_left=?1,bbox_top=?2,bbox_right=?3,bbox_bottom=?4,start_byte=?5,end_byte=?6,glyph_indices_json=?7,text_resource_sha256=?10 WHERE parse_id=?8 AND region_id=?9", rusqlite::params![region["bbox"][0].as_f64().unwrap(), region["bbox"][1].as_f64().unwrap(), region["bbox"][2].as_f64().unwrap(), region["bbox"][3].as_f64().unwrap(), region["text"]["start_byte"].as_u64().unwrap() as i64, region["text"]["end_byte"].as_u64().unwrap() as i64, serde_json::to_string(&region["glyph_indices"]).unwrap(), id, region["region_id"].as_str().unwrap(), region["text"]["resource_sha256"].as_str().unwrap()]).unwrap();
        }
    }
}

#[test]
fn semantic_region_and_utf8_tamper_fail_even_when_all_outer_hashes_match() {
    let scratch = Scratch::new();
    for mutation in [
        "wrong_in_bounds_bbox",
        "wrong_geometry",
        "cut_utf8",
        "changed_body",
        "missing_region",
    ] {
        let (project, _) = scratch.project(mutation);
        let report =
            success(&scratch.import(&project, &fixture("single-page.pdf"), None, false, true));
        let root = managed(&project, &report["pdf"]["snapshot_path"]);
        let mut manifest = read_json(&root.join("parser/pdf-manifest.json"));
        match mutation {
            "wrong_in_bounds_bbox" => {
                for axis in [0, 2] {
                    manifest["pages"][0]["regions"][0]["bbox"][axis] = json!(
                        manifest["pages"][0]["regions"][0]["bbox"][axis]
                            .as_f64()
                            .unwrap()
                            + 10.0
                    );
                }
            }
            "wrong_geometry" => {
                manifest["pages"][0]["geometry"]["width_pt"] = json!(
                    manifest["pages"][0]["geometry"]["width_pt"]
                        .as_f64()
                        .unwrap()
                        + 10.0
                );
            }
            "changed_body" => {
                let path = root.join("parser/pages/000000.txt");
                let mut text = fs::read(&path).unwrap();
                text[0] = b'X';
                fs::write(path, &text).unwrap();
                let digest = sha(&text);
                manifest["pages"][0]["text"]["sha256"] = json!(digest);
                for region in manifest["pages"][0]["regions"].as_array_mut().unwrap() {
                    region["text"]["resource_sha256"] = json!(digest);
                }
                let resource = manifest["resources"]
                    .as_array_mut()
                    .unwrap()
                    .iter_mut()
                    .find(|r| r["resource_id"] == "page-text-000000")
                    .unwrap();
                resource["fingerprint"]["sha256"] = json!(digest);
            }
            "cut_utf8" => {
                let start = manifest["pages"][0]["regions"][2]["text"]["start_byte"]
                    .as_u64()
                    .unwrap();
                manifest["pages"][0]["regions"][2]["text"]["start_byte"] = json!(start + 4);
            }
            "missing_region" => {
                let removed = manifest["pages"][0]["regions"]
                    .as_array_mut()
                    .unwrap()
                    .remove(1);
                let db = Connection::open(project.join("project.sqlite")).unwrap();
                db.execute(
                    "DELETE FROM pdf_regions WHERE parse_id=?1 AND region_id=?2",
                    rusqlite::params![
                        report["pdf"]["parse_id"].as_str().unwrap(),
                        removed["region_id"].as_str().unwrap()
                    ],
                )
                .unwrap();
                db.execute(
                    "UPDATE pdf_parses SET region_count=region_count-1 WHERE parse_id=?1",
                    [report["pdf"]["parse_id"].as_str().unwrap()],
                )
                .unwrap();
            }
            _ => unreachable!(),
        }
        sync_modified_manifest(&project, &report, &manifest);
        rejected_inspect(
            &scratch,
            &project,
            &report,
            Some(match mutation {
                "wrong_in_bounds_bbox" => "PDF_LOCATOR",
                "wrong_geometry" => "PDF_LOCATOR",
                _ => "PDF_TEXT",
            }),
        );
        assert_eq!(
            fs::read(root.join("parser/raw.pdf")).unwrap(),
            fs::read(fixture("single-page.pdf")).unwrap()
        );
    }
    scratch.sentinel();
}

#[test]
fn unknown_pdf_cache_and_snapshot_entries_are_never_accepted_or_cleaned() {
    let scratch = Scratch::new();
    for mutation in [
        "extra_parse",
        "extra_file",
        "extra_old_version",
        "unknown_table",
    ] {
        let (project, _) = scratch.project(mutation);
        let report =
            success(&scratch.import(&project, &fixture("single-page.pdf"), None, false, true));
        match mutation {
            "extra_parse" => {
                let path = project
                    .join(".noetome-pdf-parses")
                    .join(Uuid::new_v4().to_string());
                fs::create_dir(&path).unwrap();
                fs::write(path.join("human.txt"), b"unknown parse is not owned").unwrap();
            }
            "extra_file" => {
                let root = managed(&project, &report["pdf"]["snapshot_path"]);
                fs::write(
                    root.join("parser/human.txt"),
                    b"unknown artifact is not owned",
                )
                .unwrap();
            }
            "extra_old_version" => {
                let path = project
                    .join(".noetome-sources")
                    .join(Uuid::new_v4().to_string());
                fs::create_dir(&path).unwrap();
                fs::write(path.join("raw.pdf"), SENTINEL).unwrap();
            }
            "unknown_table" => {
                Connection::open(project.join("project.sqlite"))
                    .unwrap()
                    .execute_batch("CREATE TABLE human_notes (note TEXT)")
                    .unwrap();
            }
            _ => unreachable!(),
        }
        rejected_inspect(&scratch, &project, &report, None);
        let before = snapshot(&project);
        let output = scratch.import(&project, &fixture("single-page.pdf"), None, false, true);
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert_eq!(
            snapshot(&project),
            before,
            "import adopted or cleaned unknown evidence"
        );
    }
    scratch.sentinel();
}

#[test]
fn mixed_text_and_pdf_imports_share_a_unique_increasing_sql_rowid_sequence() {
    // Wall-clock timestamp ties must not erase the actual import order.
    let scratch = Scratch::new();
    let (project, _) = scratch.project("mixed import sequence");
    let first_text = scratch.root.join("first.md");
    let second_text = scratch.root.join("second.txt");
    fs::write(&first_text, b"# First\r\n").unwrap();
    fs::write(&second_text, b"second\r\n").unwrap();
    let a = success(&scratch.import(&project, &first_text, None, false, true));
    let b = success(&scratch.import(&project, &fixture("single-page.pdf"), None, false, true));
    let c = success(&scratch.import(&project, &second_text, None, false, true));
    let d = success(&scratch.import(&project, &fixture("single-page.pdf"), None, false, true));
    let db = database(&project);
    let sequence = rows(
        &db,
        "SELECT rowid,'text',import_id FROM source_imports UNION ALL SELECT rowid,'pdf',import_id FROM pdf_imports ORDER BY rowid",
    );
    assert_eq!(
        sequence,
        vec![
            vec![json!(1), json!("text"), a["import_id"].clone()],
            vec![json!(2), json!("pdf"), b["import_id"].clone()],
            vec![json!(3), json!("text"), c["import_id"].clone()],
            vec![json!(4), json!("pdf"), d["import_id"].clone()]
        ]
    );
    drop(db);
    for report in [&a, &b, &c, &d] {
        scratch.inspect(&project, report, 0);
    }
    scratch.sentinel();
}

#[test]
fn cross_table_duplicate_import_rowid_or_uuid_cannot_hide_behind_text_inspect() {
    let scratch = Scratch::new();
    for mutation in ["duplicate_rowid", "duplicate_import_id"] {
        let (project, _) = scratch.project(mutation);
        let text = scratch.root.join(format!("{mutation}.md"));
        fs::write(&text, b"# Original text\n").unwrap();
        let old = success(&scratch.import(&project, &text, None, false, true));
        let pdf =
            success(&scratch.import(&project, &fixture("single-page.pdf"), None, false, true));
        let db = Connection::open(project.join("project.sqlite")).unwrap();
        if mutation == "duplicate_rowid" {
            assert_eq!(db.execute("UPDATE pdf_imports SET rowid=(SELECT min(rowid) FROM source_imports) WHERE import_id=?1", [pdf["import_id"].as_str().unwrap()]).unwrap(), 1);
        } else {
            assert_eq!(
                db.execute(
                    "UPDATE pdf_imports SET import_id=?1 WHERE import_id=?2",
                    rusqlite::params![
                        old["import_id"].as_str().unwrap(),
                        pdf["import_id"].as_str().unwrap()
                    ]
                )
                .unwrap(),
                1
            );
        }
        drop(db);
        // Looking up the old text ID would succeed if only its own table were
        // validated. It must reject the whole incoherent Source/PDF graph.
        rejected_inspect(&scratch, &project, &old, None);
    }
    scratch.sentinel();
}

#[test]
fn configuration_schema_mismatch_or_unknown_schema_never_implicitly_migrates() {
    let scratch = Scratch::new();
    for version in [2, 99] {
        let (project, _) = scratch.project(&format!("bad schema {version}"));
        let report =
            success(&scratch.import(&project, &fixture("single-page.pdf"), None, false, true));
        let path = project.join("project.json");
        let mut config = read_json(&path);
        config["schema_version"] = json!(version);
        fs::write(&path, serde_json::to_vec_pretty(&config).unwrap()).unwrap();
        rejected_inspect(&scratch, &project, &report, None);
        let before = snapshot(&project);
        let output = scratch.command(
            &[
                "project".into(),
                "inspect".into(),
                project.as_os_str().to_owned(),
            ],
            false,
        );
        assert!(!output.status.success());
        assert_eq!(snapshot(&project), before);
    }
    scratch.sentinel();
}

#[cfg(windows)]
#[test]
fn real_windows_database_replace_lock_preserves_old_text_and_config_bytes() {
    use std::os::windows::fs::OpenOptionsExt;
    let scratch = Scratch::new();
    let (project, _) = scratch.project("upgrade publication lock");
    let text = scratch.root.join("history.md");
    fs::write(&text, OLD_RAW.as_bytes()).unwrap();
    let old = success(&scratch.import(&project, &text, None, false, true));
    let before = snapshot(&project);
    let guard = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(project.join("project.sqlite"))
        .unwrap();
    let output = scratch.import(&project, &fixture("single-page.pdf"), None, false, true);
    assert_eq!(
        output.status.code(),
        Some(3),
        "deny-delete publication must fail: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    drop(guard);
    for (path, entry) in before {
        if let Some(bytes) = entry.bytes {
            assert_eq!(
                fs::read(project.join(&path)).unwrap(),
                bytes,
                "publication lock changed old bytes: {}",
                path.display()
            );
        }
    }
    assert_eq!(
        read_json(&project.join("project.json"))["schema_version"],
        2
    );
    if project.join(".noetome-writing").exists() {
        rejected_inspect(&scratch, &project, &old, Some("SOURCE_WRITE_INCOMPLETE"));
    } else {
        scratch.inspect(&project, &old, 0);
    }
    scratch.sentinel();
}

#[cfg(windows)]
#[test]
fn actual_hardlinked_pdf_resource_is_rejected_even_with_identical_bytes() {
    let scratch = Scratch::new();
    let (project, _) = scratch.project("hardlink protection");
    let report = success(&scratch.import(&project, &fixture("single-page.pdf"), None, false, true));
    let root = managed(&project, &report["pdf"]["snapshot_path"]);
    let path = root.join("parser/pages/000000.txt");
    let before = fs::read(&path).unwrap();
    fs::hard_link(&path, scratch.root.join("outside resource twin.txt")).unwrap();
    rejected_inspect(&scratch, &project, &report, None);
    assert_eq!(fs::read(path).unwrap(), before);
    scratch.sentinel();
}

#[cfg(windows)]
#[test]
fn actual_pdf_cache_junction_is_rejected_and_its_target_stays_unchanged() {
    let scratch = Scratch::new();
    let (project, _) = scratch.project("junction protection");
    let report = success(&scratch.import(&project, &fixture("single-page.pdf"), None, false, true));
    let path = project.join(".noetome-pdf-parses");
    let target = scratch.root.join("outside managed cache");
    fs::rename(&path, &target).unwrap();
    let target_before = snapshot(&target);
    let dos_path = |path: &Path| {
        let text = path.to_string_lossy();
        text.strip_prefix(r"\\?\").unwrap_or(&text).to_owned()
    };
    let linked = Command::new("cmd")
        .args(["/c", "mklink", "/J"])
        .arg(dos_path(&path))
        .arg(dos_path(&target))
        .output()
        .unwrap();
    assert!(
        linked.status.success(),
        "create owned test junction: {}",
        String::from_utf8_lossy(&linked.stderr)
    );
    let config_before = fs::read(project.join("project.json")).unwrap();
    let database_before = fs::read(project.join("project.sqlite")).unwrap();
    let output = scratch.command(
        &[
            "source".into(),
            "inspect".into(),
            project.as_os_str().to_owned(),
            report["import_id"].as_str().unwrap().into(),
        ],
        false,
    );
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(
        fs::read(project.join("project.json")).unwrap(),
        config_before
    );
    assert_eq!(
        fs::read(project.join("project.sqlite")).unwrap(),
        database_before
    );
    assert_eq!(snapshot(&target), target_before);
    // Remove only the confirmed junction object, never recurse through it.
    assert!(is_link(&fs::symlink_metadata(&path).unwrap()));
    assert_eq!(
        fs::canonicalize(&path).unwrap(),
        fs::canonicalize(&target).unwrap()
    );
    fs::remove_dir(&path).unwrap();
    assert_eq!(snapshot(&target), target_before);
    scratch.sentinel();
}
