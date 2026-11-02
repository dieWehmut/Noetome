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
        Self {
            root,
            parent,
            owner,
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
        assert_eq!(row[3], "confirmed");
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
                rows(&db, &format!("SELECT * FROM {table} ORDER BY rowid")),
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
        let now = rows(&db, &format!("SELECT * FROM {table} ORDER BY rowid"));
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
    for (index, report) in [&repeat, &rename].into_iter().enumerate() {
        for key in ["source_id", "version_id"] {
            assert_eq!(report[key], first[key]);
        }
        assert_ne!(report["import_id"], first["import_id"]);
        assert_eq!(report["reused"], true);
        assert_eq!(report["pdf"]["parse_id"], first["pdf"]["parse_id"]);
        assert_eq!(report["pdf"]["reused"], true);
        assert_eq!(report["version"], first["version"]);
        scratch.inspect(&project, report, 0);
        assert!(index < 2);
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
    let valid = success(&scratch.import(&project, &selected, None, false, true));
    assert_eq!(valid["version_id"], failed["version_id"]);
    assert_ne!(valid["pdf"]["parse_id"], failed["pdf"]["parse_id"]);
    assert_eq!(valid["reused"], true);
    assert_eq!(valid["pdf"]["reused"], false);
    check_authoring(&project, &valid, &authoring_case("single-page.pdf"));
    fs::rename(&selected, scratch.root.join("original moved away.pdf")).unwrap();
    scratch.inspect(&project, &failed, 3);
    scratch.inspect(&project, &valid, 0);
    assert_eq!(
        snapshot(&managed(&project, &failed["pdf"]["snapshot_path"])),
        failed_snapshot
    );
    assert_eq!(counts(&project)["source_versions"], 1);
    assert_eq!(counts(&project)["pdf_parses"], 2);
    scratch.sentinel();
}
