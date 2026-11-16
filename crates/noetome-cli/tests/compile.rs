use std::cell::RefCell;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::ffi::OsString;
#[cfg(windows)]
use std::fs::OpenOptions;
use std::fs::{self, Metadata};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::SystemTime;

use rusqlite::{Connection, OpenFlags};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

#[path = "support/compile_expectations.rs"]
mod author;

struct Sandbox {
    root: PathBuf,
    junctions: RefCell<Vec<PathBuf>>,
}

impl Sandbox {
    fn new() -> Self {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/compile-cli-tests");
        fs::create_dir_all(&parent).unwrap();
        let root = parent
            .canonicalize()
            .unwrap()
            .join(Uuid::new_v4().to_string());
        fs::create_dir(&root).unwrap();
        for name in ["cwd 中文 with spaces", "home", "appdata", "localappdata"] {
            fs::create_dir(root.join(name)).unwrap();
        }
        fs::write(
            root.join("外部人工哨兵.md"),
            "人工内容只读，不能成为发布目标。\n",
        )
        .unwrap();
        Self {
            root,
            junctions: RefCell::new(Vec::new()),
        }
    }

    fn fixture(&self, name: &str) -> PathBuf {
        let original = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/text-compare-v1");
        let destination = self.root.join(name);
        copy_tree(&original, &destination);
        destination
    }

    fn compile(&self, fixture: &Path, output: &Path) -> Output {
        self.run(&[
            "case".into(),
            "compile".into(),
            fixture.as_os_str().to_owned(),
            output.as_os_str().to_owned(),
        ])
    }

    fn run(&self, arguments: &[OsString]) -> Output {
        self.command(arguments)
            .output()
            .expect("execute compiled case compiler")
    }

    fn command(&self, arguments: &[OsString]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_noetome"));
        command
            .args(arguments)
            .current_dir(self.root.join("cwd 中文 with spaces"))
            .env("HOME", self.root.join("home"))
            .env("USERPROFILE", self.root.join("home"))
            .env("APPDATA", self.root.join("appdata"))
            .env("LOCALAPPDATA", self.root.join("localappdata"))
            .env("TMP", &self.root)
            .env("TEMP", &self.root)
            .env("TMPDIR", &self.root);
        command
    }

    fn compile_preserving(&self, fixture: &Path, output: &Path, may_create_output: bool) -> Output {
        let allowed = may_create_output.then_some(output);
        let before = snapshot(&self.root, allowed);
        let result = self.compile(fixture, output);
        let after = snapshot(&self.root, allowed);
        let changed: Vec<_> = before
            .keys()
            .chain(after.keys())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .filter(|path| before.get(*path) != after.get(*path))
            .collect();
        assert!(
            changed.is_empty(),
            "compile changed selected input, expected, metadata, directory inventory or external sentinel; paths: {changed:?}"
        );
        result
    }

    #[cfg(windows)]
    fn junction(&self, link: &Path, target: &Path) {
        let dos = |path: &Path| {
            path.to_string_lossy()
                .trim_start_matches(r"\\?\")
                .to_owned()
        };
        let result = Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(dos(link))
            .arg(dos(target))
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "create owned junction: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        self.junctions.borrow_mut().push(link.to_owned());
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        for link in self.junctions.borrow().iter().rev() {
            fs::remove_dir(link).unwrap();
        }
        let parent = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/compile-cli-tests")
            .canonicalize()
            .unwrap();
        assert!(self.root.canonicalize().unwrap().starts_with(parent));
        fs::remove_dir_all(&self.root).unwrap();
    }
}

fn copy_tree(source: &Path, destination: &Path) {
    fs::create_dir(destination).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let target = destination.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

#[derive(PartialEq, Eq)]
struct Entry {
    bytes: Option<Vec<u8>>,
    modified: Option<SystemTime>,
    readonly: bool,
    link: bool,
}

impl std::fmt::Debug for Entry {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Entry")
            .field(
                "file",
                &self
                    .bytes
                    .as_ref()
                    .map(|bytes| (bytes.len(), sha256(bytes))),
            )
            .field("modified", &self.modified)
            .field("readonly", &self.readonly)
            .field("link", &self.link)
            .finish()
    }
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

fn snapshot(root: &Path, allowed: Option<&Path>) -> BTreeMap<PathBuf, Entry> {
    fn visit(
        root: &Path,
        path: &Path,
        allowed: Option<&Path>,
        entries: &mut BTreeMap<PathBuf, Entry>,
    ) {
        if allowed.is_some_and(|output| path.starts_with(output)) {
            return;
        }
        let metadata = fs::symlink_metadata(path).unwrap();
        let link = is_link(&metadata);
        entries.insert(
            path.strip_prefix(root).unwrap().to_owned(),
            Entry {
                bytes: if metadata.is_file() && !link {
                    Some(fs::read(path).unwrap())
                } else {
                    None
                },
                // Creating the selected output necessarily changes its ancestors' directory mtimes.
                modified: if allowed.is_some_and(|output| output.starts_with(path)) {
                    None
                } else {
                    metadata.modified().ok()
                },
                readonly: metadata.permissions().readonly(),
                link,
            },
        );
        if metadata.is_dir() && !link {
            for entry in fs::read_dir(path).unwrap() {
                visit(root, &entry.unwrap().path(), allowed, entries);
            }
        }
    }
    let mut entries = BTreeMap::new();
    visit(root, root, allowed, &mut entries);
    entries
}

fn success_json(output: Output) -> Value {
    assert_eq!(
        output.status.code(),
        Some(0),
        "case compile failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "successful compile emitted diagnostics"
    );
    serde_json::from_slice(&output.stdout).expect("compile stdout must be pure JSON")
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn edit_json(root: &Path, file: &str, edit: impl FnOnce(&mut Value)) {
    let path = root.join(file);
    let mut value = read_json(&path);
    edit(&mut value);
    let mut bytes = serde_json::to_vec_pretty(&value).unwrap();
    bytes.push(b'\n');
    fs::write(path, bytes).unwrap();
}

fn edit_ir(root: &Path, edit: impl FnOnce(&mut Value)) {
    edit_json(root, "ir.json", edit);
    let hash = sha256(&fs::read(root.join("ir.json")).unwrap());
    edit_json(root, "note-plan.json", |plan| {
        plan["knowledge_sha256"] = hash.into()
    });
}

fn valid_uuid(value: &Value) {
    let text = value.as_str().unwrap();
    let id = Uuid::parse_str(text).unwrap();
    assert_eq!(id.get_version_num(), 4);
    assert_eq!(id.to_string(), text);
}

fn audit_time(value: &Value, before: SystemTime, after: SystemTime) -> OffsetDateTime {
    let text = value.as_str().unwrap();
    let parsed = OffsetDateTime::parse(text, &Rfc3339).unwrap();
    assert_eq!(parsed.offset(), time::UtcOffset::UTC);
    let before: OffsetDateTime = before.into();
    let after: OffsetDateTime = after.into();
    assert!(
        parsed >= before && parsed <= after,
        "audit timestamp {text} is not from this invocation"
    );
    parsed
}

fn assert_report_facts(
    root: &Path,
    fixture: &Path,
    outcome: &Value,
    before: SystemTime,
    after: SystemTime,
) {
    let report = &outcome["report"];
    assert_eq!(
        report,
        &read_json(&root.join("reports/validation.json")),
        "stdout must contain the exact persisted facts"
    );
    assert_eq!(report["result"], "validated");
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["producer"], "fixture");
    assert_eq!(report["case_id"], "C7k");
    assert_eq!(report["run_id"], outcome["run_id"]);
    assert_eq!(report["release_id"], outcome["release_id"]);
    valid_uuid(&outcome["run_id"]);
    valid_uuid(&outcome["release_id"]);
    assert_ne!(outcome["run_id"], outcome["release_id"]);
    let started = audit_time(&report["started_at"], before, after);
    let finished = audit_time(&report["finished_at"], before, after);
    assert!(started <= finished);
    assert!(report["issues"].as_array().unwrap().is_empty());
    assert_eq!(report["versions"]["renderer"], "fixture-markdown-v1");
    for key in [
        "sources",
        "coverage",
        "knowledge",
        "note_plan",
        "expected",
        "rendering",
        "staging_output",
        "vault_output",
    ] {
        assert_eq!(
            report["checks"][key]["status"], "passed",
            "required {key}: {report}"
        );
    }
    assert_eq!(report["checks"]["scientific_quality"]["status"], "not_run");
    for key in [
        "source_coverage",
        "knowledge_compilation",
        "quality_validation",
        "optional_research",
    ] {
        let dimension = &report["dimensions"][key];
        assert_eq!(
            dimension["state"],
            if key == "optional_research" {
                "not_applicable"
            } else {
                "complete"
            }
        );
        assert!(!dimension["reason"].as_str().unwrap().is_empty());
    }
    for (key, count) in [
        ("sources", 2),
        ("spans", 12),
        ("atoms", 4),
        ("required_atoms", 4),
        ("notes", 2),
        ("nodes", 4),
    ] {
        assert_eq!(report["counts"][key], count, "actual fixture count {key}");
    }
    for (key, file) in [
        ("case_sha256", "case.json"),
        ("ir_sha256", "ir.json"),
        ("plan_sha256", "note-plan.json"),
        ("expected_sha256", "expected.json"),
    ] {
        assert_eq!(
            report["input_hashes"][key],
            sha256(&fs::read(fixture.join(file)).unwrap())
        );
    }
    assert_eq!(
        report["publication"],
        json!({
            "database":"state/compile.sqlite", "commit_table":"publication_commits", "release_id":outcome["release_id"]
        })
    );
    assert_eq!(outcome["paths"]["vault"], "vault");
    assert_eq!(outcome["paths"]["database"], "state/compile.sqlite");
    assert_eq!(outcome["paths"]["json_report"], "reports/validation.json");
    assert_eq!(outcome["paths"]["markdown_report"], "reports/validation.md");
    let reported_root = Path::new(outcome["paths"]["root"].as_str().unwrap());
    assert!(reported_root.is_absolute());
    assert_eq!(
        reported_root.canonicalize().unwrap(),
        root.canonicalize().unwrap()
    );
    assert_markdown_report(root, report);
}

fn unescape_cell(value: &str) -> String {
    let mut out = String::new();
    let mut chars = value.trim().chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' && chars.peek().is_some_and(|next| next.is_ascii_punctuation()) {
            out.push(chars.next().unwrap());
        } else {
            out.push(ch);
        }
    }
    out.replace("<br>", "\n")
        .replace("&#9;", "\t")
        .replace("&#13;", "\r")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn markdown_cells(line: &str) -> Vec<String> {
    let text = line.strip_prefix('|').unwrap().strip_suffix('|').unwrap();
    let mut fields = Vec::new();
    let mut start = 0;
    let mut backslashes = 0;
    for (offset, ch) in text.char_indices() {
        if ch == '|' && backslashes % 2 == 0 {
            fields.push(unescape_cell(&text[start..offset]));
            start = offset + 1;
        }
        backslashes = if ch == '\\' { backslashes + 1 } else { 0 };
    }
    fields.push(unescape_cell(&text[start..]));
    fields
}

fn assert_markdown_report(root: &Path, report: &Value) {
    // Parse the emitted report independently; do not call the production report formatter.
    let text = fs::read_to_string(root.join("reports/validation.md")).unwrap();
    assert!(text.starts_with("# Noetome fixture 验证报告\n"));
    let mut section = String::new();
    let mut bullets: BTreeMap<(String, String), String> = BTreeMap::new();
    let mut tables: BTreeMap<String, Vec<Vec<String>>> = BTreeMap::new();
    for line in text.lines() {
        if let Some(title) = line.strip_prefix("## ") {
            section = title.to_owned();
        } else if let Some(value) = line.strip_prefix("- ") {
            if let Some((key, value)) = value.split_once(": ") {
                assert!(
                    bullets
                        .insert((section.clone(), unescape_cell(key)), unescape_cell(value))
                        .is_none()
                );
            }
        } else if line.starts_with('|') && line.ends_with('|') {
            let row = markdown_cells(line);
            if row.iter().all(|cell| {
                !cell.is_empty() && cell.chars().all(|ch| matches!(ch, '-' | ':' | ' '))
            }) {
                continue;
            }
            tables.entry(section.clone()).or_default().push(row);
        }
    }
    for key in [
        "schema_version",
        "producer",
        "case_id",
        "run_id",
        "release_id",
        "started_at",
        "finished_at",
        "result",
    ] {
        let expected = report[key]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| report[key].to_string());
        assert_eq!(
            bullets.get(&(String::new(), key.into())).unwrap(),
            &expected,
            "MD fact {key}"
        );
    }
    for key in ["database", "commit_table", "release_id"] {
        assert_eq!(
            bullets.get(&("发布依据".into(), key.into())).unwrap(),
            report["publication"][key].as_str().unwrap()
        );
    }
    let expected_dimensions: Vec<_> = report["dimensions"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(key, value)| {
            vec![
                key.clone(),
                value["state"].as_str().unwrap().into(),
                value["reason"].as_str().unwrap().into(),
            ]
        })
        .collect();
    assert_table(
        &tables,
        "四维状态",
        &["dimension", "state", "reason"],
        expected_dimensions,
    );
    let mut expected_counts = Vec::new();
    for (key, value) in report["counts"].as_object().unwrap() {
        expected_counts.push(vec!["overall".into(), key.clone(), value.to_string()]);
    }
    for (scope, dimension) in report["dimensions"].as_object().unwrap() {
        for (key, value) in dimension["counts"].as_object().unwrap() {
            expected_counts.push(vec![scope.clone(), key.clone(), value.to_string()]);
        }
    }
    assert_table(&tables, "计数", &["scope", "key", "value"], expected_counts);
    let expected_checks: Vec<_> = report["checks"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(key, value)| {
            vec![
                key.clone(),
                value["status"].as_str().unwrap().into(),
                value["issue_count"].to_string(),
            ]
        })
        .collect();
    assert_table(
        &tables,
        "检查",
        &["check", "status", "issue_count"],
        expected_checks,
    );
    let expected_files: Vec<_> = report["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|file| {
            vec![
                file["relative_path"].as_str().unwrap().into(),
                file["note_id"].as_str().unwrap().into(),
                file["plan_revision"].to_string(),
                file["byte_length"].to_string(),
                file["content_sha256"].as_str().unwrap().into(),
            ]
        })
        .collect();
    assert_table(
        &tables,
        "文件",
        &[
            "relative_path",
            "note_id",
            "plan_revision",
            "byte_length",
            "content_sha256",
        ],
        expected_files,
    );
    let mut expected_mappings = Vec::new();
    let mut expected_links = Vec::new();
    for file in report["files"].as_array().unwrap() {
        for mapping in file["content_mappings"].as_array().unwrap() {
            let scope = &mapping["scope"];
            let locator = format!(
                "{}:{}:{}:{}",
                scope["kind"].as_str().unwrap(),
                scope["item_id"].as_str().unwrap_or(""),
                scope["row_id"].as_str().unwrap_or(""),
                scope["column_id"].as_str().unwrap_or("")
            );
            let atoms = mapping["atom_ids"]
                .as_array()
                .unwrap()
                .iter()
                .map(|id| id.as_str().unwrap())
                .collect::<Vec<_>>()
                .join(",");
            expected_mappings.push(vec![
                file["relative_path"].as_str().unwrap().into(),
                scope["section_id"].as_str().unwrap().into(),
                scope["node_id"].as_str().unwrap().into(),
                locator,
                atoms,
                mapping["start_byte"].to_string(),
                mapping["end_byte"].to_string(),
            ]);
        }
        for link in file["links"].as_array().unwrap() {
            let scope = &link["scope"];
            let locator = format!(
                "{}:{}:{}:{}",
                scope["kind"].as_str().unwrap(),
                scope["item_id"].as_str().unwrap_or(""),
                scope["row_id"].as_str().unwrap_or(""),
                scope["column_id"].as_str().unwrap_or("")
            );
            expected_links.push(vec![
                file["relative_path"].as_str().unwrap().into(),
                scope["section_id"].as_str().unwrap().into(),
                scope["node_id"].as_str().unwrap().into(),
                locator,
                link["target_concept_id"].as_str().unwrap().into(),
                link["target_path"].as_str().unwrap().into(),
                link["target_heading"].as_str().unwrap().into(),
                link["display_text"].as_str().unwrap().into(),
                link["start_byte"].to_string(),
                link["end_byte"].to_string(),
            ]);
        }
    }
    assert_table(
        &tables,
        "内容映射",
        &[
            "relative_path",
            "section_id",
            "node_id",
            "scope",
            "atom_ids",
            "start_byte",
            "end_byte",
        ],
        expected_mappings,
    );
    assert_table(
        &tables,
        "语义引用",
        &[
            "relative_path",
            "section_id",
            "node_id",
            "scope",
            "target_concept_id",
            "target_path",
            "target_heading",
            "display_text",
            "start_byte",
            "end_byte",
        ],
        expected_links,
    );
    let mut expected_rules = Vec::new();
    for scope in ["input_hashes", "versions"] {
        for (key, value) in report[scope].as_object().unwrap() {
            expected_rules.push(vec![
                scope.into(),
                key.clone(),
                value.as_str().unwrap().into(),
            ]);
        }
    }
    assert_table(
        &tables,
        "输入与规则",
        &["scope", "key", "value"],
        expected_rules,
    );
    assert_table(
        &tables,
        "问题",
        &["rule_id", "object_refs", "message"],
        vec![],
    );
}

fn assert_table(
    tables: &BTreeMap<String, Vec<Vec<String>>>,
    section: &str,
    header: &[&str],
    rows: Vec<Vec<String>>,
) {
    let actual = tables
        .get(section)
        .unwrap_or_else(|| panic!("missing Markdown report section {section}"));
    assert_eq!(
        actual.first().unwrap(),
        &header
            .iter()
            .map(|value| (*value).to_owned())
            .collect::<Vec<_>>()
    );
    let actual_rows = &actual[1..];
    assert_eq!(actual_rows.len(), rows.len(), "{section} row counts differ");
    assert_eq!(
        actual_rows.iter().cloned().collect::<BTreeSet<_>>(),
        rows.into_iter().collect::<BTreeSet<_>>(),
        "{section} facts differ"
    );
}

fn assert_inventory(root: &Path) {
    fn visit(
        root: &Path,
        path: &Path,
        files: &mut BTreeSet<String>,
        directories: &mut BTreeSet<String>,
    ) {
        let metadata = fs::symlink_metadata(path).unwrap();
        assert!(!is_link(&metadata), "published inventory has a link");
        let relative = path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if metadata.is_dir() {
            directories.insert(relative);
            for entry in fs::read_dir(path).unwrap() {
                visit(root, &entry.unwrap().path(), files, directories);
            }
        } else {
            assert!(metadata.is_file());
            files.insert(relative);
        }
    }
    let mut files = BTreeSet::new();
    let mut directories = BTreeSet::new();
    visit(root, root, &mut files, &mut directories);
    assert_eq!(
        files,
        [
            "state/compile.sqlite",
            "reports/validation.json",
            "reports/validation.md",
            "staging/vault/笔记/甲容器.md",
            "staging/vault/笔记/乙容器.md",
            "vault/笔记/甲容器.md",
            "vault/笔记/乙容器.md"
        ]
        .map(str::to_owned)
        .into_iter()
        .collect()
    );
    assert_eq!(
        directories,
        [
            "",
            "state",
            "reports",
            "staging",
            "staging/vault",
            "staging/vault/笔记",
            "vault",
            "vault/笔记"
        ]
        .map(str::to_owned)
        .into_iter()
        .collect()
    );
}

fn assert_notes_and_mappings(root: &Path, fixture: &Path, report: &Value, table_alias: bool) {
    let notes = BTreeMap::from([
        (
            author::ALPHA_PATH,
            if table_alias {
                author::ALPHA_WITH_TABLE_ALIAS
            } else {
                author::ALPHA
            },
        ),
        (author::BETA_PATH, author::BETA),
    ]);
    let files = report["files"].as_array().unwrap();
    assert_eq!(files.len(), 2);
    let mut mapped = BTreeSet::new();
    let atoms = read_json(&fixture.join("expected.json"))["required_atoms"]
        .as_array()
        .unwrap()
        .clone();
    let mut mapping_count = 0;
    let mut link_count = 0;
    for file in files {
        let relative = file["relative_path"].as_str().unwrap();
        let expected = notes.get(relative).expect("unexpected published note");
        let bytes = fs::read(root.join("vault").join(relative)).unwrap();
        assert_eq!(
            &bytes,
            expected.as_bytes(),
            "author-fixed Markdown differed for {relative}"
        );
        assert_eq!(
            fs::read(root.join("staging/vault").join(relative)).unwrap(),
            bytes
        );
        assert_eq!(file["byte_length"], bytes.len());
        assert_eq!(file["content_sha256"], sha256(&bytes));
        assert_eq!(file["plan_revision"], 1);
        assert_eq!(
            file["note_id"],
            if relative == author::ALPHA_PATH {
                "N4u"
            } else {
                "N8v"
            }
        );
        let text = std::str::from_utf8(&bytes).unwrap();
        for mapping in file["content_mappings"].as_array().unwrap() {
            mapping_count += 1;
            let start = mapping["start_byte"].as_u64().unwrap() as usize;
            let end = mapping["end_byte"].as_u64().unwrap() as usize;
            assert!(start < end);
            let body = text
                .get(start..end)
                .expect("mapping must use actual UTF8 character boundaries");
            assert!(
                !body.contains("[["),
                "content mapping borrowed a semantic alias"
            );
            let scope = &mapping["scope"];
            let node = scope["node_id"].as_str().unwrap();
            let (section, kind, item, row, column, id, literal) =
                match (node, scope["row_id"].as_str(), scope["column_id"].as_str()) {
                    ("Q2z", _, _) => (
                        "H2j",
                        "paragraph",
                        None,
                        None,
                        None,
                        Some("A3d"),
                        "甲容器是虚构的蓝砂收纳盒。参见乙容器。",
                    ),
                    ("Q6z", _, _) => (
                        "H6n",
                        "paragraph",
                        None,
                        None,
                        None,
                        Some("A7e"),
                        "乙容器是虚构的红砂收纳盒。参见甲容器。",
                    ),
                    ("L9x", _, _) => (
                        "H6n",
                        "list_item",
                        Some("I4b"),
                        None,
                        None,
                        Some("A9w"),
                        "乙容器的虚构刻度为8 cm。仅用于室温演示；空盒时不读数；浸水后不适用。",
                    ),
                    ("T5x", Some("W3g"), Some("D4c")) => (
                        "H2j",
                        "table_cell",
                        None,
                        Some("W3g"),
                        Some("D4c"),
                        Some("A5p"),
                        "甲容器的虚构刻度为5 cm。仅用于室温演示；装满后停止读数；破损时不适用。",
                    ),
                    ("T5x", Some("W3g"), Some("D6c")) => (
                        "H2j",
                        "table_cell",
                        None,
                        Some("W3g"),
                        Some("D6c"),
                        Some("A9w"),
                        "乙容器的虚构刻度为8 cm。仅用于室温演示；空盒时不读数；浸水后不适用。",
                    ),
                    ("T5x", Some("W3g"), Some("D2c")) => (
                        "H2j",
                        "table_cell",
                        None,
                        Some("W3g"),
                        Some("D2c"),
                        None,
                        "虚构刻度",
                    ),
                    ("T5x", Some("W7g"), Some("D2c")) => (
                        "H2j",
                        "table_cell",
                        None,
                        Some("W7g"),
                        Some("D2c"),
                        None,
                        "单价",
                    ),
                    ("T5x", Some("W7g"), Some("D4c")) => (
                        "H2j",
                        "table_cell",
                        None,
                        Some("W7g"),
                        Some("D4c"),
                        None,
                        "未给出",
                    ),
                    ("T5x", Some("W7g"), Some("D6c")) => (
                        "H2j",
                        "table_cell",
                        None,
                        Some("W7g"),
                        Some("D6c"),
                        None,
                        "未给出",
                    ),
                    _ => panic!("unexpected content scope {scope}"),
                };
            assert_eq!(
                scope,
                &json!({"section_id":section,"node_id":node,"kind":kind,"item_id":item,"row_id":row,"column_id":column})
            );
            assert_eq!(
                mapping["atom_ids"],
                json!(id.into_iter().collect::<Vec<_>>())
            );
            assert_eq!(body, literal, "scope body must match its actual plan text");
            if let Some(id) = id {
                let atom = atoms.iter().find(|atom| atom["atom_id"] == id).unwrap();
                assert!(body.contains(atom["content"].as_str().unwrap()));
                for qualifier in atom["qualifiers"].as_array().unwrap() {
                    assert!(body.contains(qualifier.as_str().unwrap()));
                }
            }
            assert!(
                mapped.insert((relative.to_owned(), node.to_owned(), row, column)),
                "duplicate scope mapping"
            );
        }
        let actual_tokens: BTreeSet<_> = text
            .match_indices("[[")
            .map(|(start, _)| {
                let end = start + text[start..].find("]]").unwrap() + 2;
                (start, end)
            })
            .collect();
        let mut declared_tokens = BTreeSet::new();
        for link in file["links"].as_array().unwrap() {
            link_count += 1;
            let start = link["start_byte"].as_u64().unwrap() as usize;
            let end = link["end_byte"].as_u64().unwrap() as usize;
            let token = text.get(start..end).expect("link UTF8 range");
            let (target, heading) = match link["target_concept_id"].as_str().unwrap() {
                "C2f" => (author::ALPHA_PATH, "定义与比较"),
                "C6h" => (author::BETA_PATH, "定义与条件"),
                _ => panic!("ghost link target"),
            };
            assert_eq!(link["target_path"], target);
            assert_eq!(link["target_heading"], heading);
            let stem = target.strip_suffix(".md").unwrap();
            let separator = if link["scope"]["kind"] == "table_cell" {
                "\\|"
            } else {
                "|"
            };
            assert_eq!(
                token,
                format!(
                    "[[{stem}#{heading}{separator}{}]]",
                    link["display_text"].as_str().unwrap()
                )
            );
            assert!(declared_tokens.insert((start, end)), "duplicate link range");
            assert_eq!(
                notes[target]
                    .lines()
                    .filter(|line| *line == format!("## {heading}"))
                    .count(),
                1
            );
        }
        assert_eq!(
            actual_tokens, declared_tokens,
            "all emitted wiki tokens must have actual link metadata"
        );
    }
    assert_eq!(mapping_count, 9);
    assert_eq!(link_count, if table_alias { 3 } else { 2 });
}

fn assert_sqlite(
    root: &Path,
    fixture: &Path,
    report: &Value,
    before: SystemTime,
    after: SystemTime,
) {
    let protected = snapshot(root, None);
    let database = root.join("state/compile.sqlite");
    for suffix in ["-journal", "-wal", "-shm"] {
        assert!(
            !PathBuf::from(format!("{}{}", database.display(), suffix)).exists(),
            "unexpected SQLite recovery side file"
        );
    }
    let db = Connection::open_with_flags(&database, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    db.pragma_update(None, "foreign_keys", true).unwrap();
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        db.query_row("PRAGMA application_id", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        0x4e434d50
    );
    assert_eq!(
        db.query_row("PRAGMA journal_mode", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "delete"
    );
    let names: BTreeSet<String> = db
        .prepare("SELECT name FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%'")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(
        names,
        [
            "compile_metadata",
            "runs",
            "releases",
            "coverage_records",
            "rendered_files",
            "publication_commits"
        ]
        .map(str::to_owned)
        .into_iter()
        .collect()
    );
    assert!(
        db.prepare("PRAGMA foreign_key_check")
            .unwrap()
            .query([])
            .unwrap()
            .next()
            .unwrap()
            .is_none()
    );
    for (table, count) in [
        ("compile_metadata", 1),
        ("runs", 1),
        ("releases", 1),
        ("coverage_records", 12),
        ("rendered_files", 2),
        ("publication_commits", 1),
    ] {
        let query = format!("SELECT COUNT(*) FROM {table}");
        assert_eq!(
            db.query_row(&query, [], |row| row.get::<_, i64>(0))
                .unwrap(),
            count,
            "actual {table} count"
        );
    }
    let metadata = db.query_row("SELECT schema_version,producer,case_id,fixture_scope_id,input_hashes_json,versions_json FROM compile_metadata WHERE singleton=1", [], |row|
        Ok((row.get::<_, i64>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,String>(3)?,row.get::<_,String>(4)?,row.get::<_,String>(5)?))).unwrap();
    assert_eq!(
        (
            metadata.0,
            metadata.1.as_str(),
            metadata.2.as_str(),
            metadata.3.as_str()
        ),
        (1, "fixture", "C7k", "R8s")
    );
    assert_eq!(
        serde_json::from_str::<Value>(&metadata.4).unwrap(),
        report["input_hashes"]
    );
    assert_eq!(
        serde_json::from_str::<Value>(&metadata.5).unwrap(),
        report["versions"]
    );
    let run = db.query_row("SELECT run_id,started_at,finished_at,stage,state,report_json_path,report_md_path,checks_json,counts_json,issues_json FROM runs", [], |row|
        Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,String>(3)?,row.get::<_,String>(4)?,row.get::<_,String>(5)?,row.get::<_,String>(6)?,row.get::<_,String>(7)?,row.get::<_,String>(8)?,row.get::<_,String>(9)?))).unwrap();
    assert_eq!(run.0, report["run_id"].as_str().unwrap());
    assert_eq!(run.1, report["started_at"].as_str().unwrap());
    assert_eq!(run.2, report["finished_at"].as_str().unwrap());
    assert_eq!(
        (
            run.3.as_str(),
            run.4.as_str(),
            run.5.as_str(),
            run.6.as_str()
        ),
        (
            "finished",
            "succeeded",
            "reports/validation.json",
            "reports/validation.md"
        )
    );
    for (stored, value) in [
        (&run.7, &report["checks"]),
        (&run.8, &report["counts"]),
        (&run.9, &report["issues"]),
    ] {
        assert_eq!(&serde_json::from_str::<Value>(stored).unwrap(), value);
    }
    let release = db.query_row("SELECT release_id,run_id,status,ir_revision,plan_revision,renderer_version,manifest_json FROM releases", [], |row|
        Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,i64>(3)?,row.get::<_,i64>(4)?,row.get::<_,String>(5)?,row.get::<_,String>(6)?))).unwrap();
    assert_eq!(release.0, report["release_id"].as_str().unwrap());
    assert_eq!(release.1, run.0);
    assert_eq!(
        (release.2.as_str(), release.3, release.4, release.5.as_str()),
        ("published", 1, 1, "fixture-markdown-v1")
    );
    assert_eq!(
        serde_json::from_str::<Value>(&release.6).unwrap(),
        report["files"]
    );
    let expected_coverage = read_json(&fixture.join("expected.json"))["coverage"]
        .as_array()
        .unwrap()
        .clone();
    let mut actual_coverage = Vec::new();
    let mut stmt = db.prepare("SELECT run_id,span_id,disposition,atom_ids_json,required_atom_ids_json,duplicate_of_span_id,exclusion_reason,decision_source,policy_version,stage FROM coverage_records ORDER BY span_id").unwrap();
    let records = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
            ))
        })
        .unwrap();
    for record in records {
        let value = record.unwrap();
        assert_eq!(value.0, run.0);
        assert_ne!(value.0, "R8s");
        assert_eq!(
            (value.8.as_str(), value.9.as_str()),
            ("fixture-policy-v1", "validated")
        );
        actual_coverage.push(json!({"span_id":value.1,"disposition":value.2,"atom_ids":serde_json::from_str::<Value>(&value.3).unwrap(),"required_atom_ids":serde_json::from_str::<Value>(&value.4).unwrap(),"duplicate_of_span_id":value.5,"exclusion_reason":value.6,"decision_source":value.7}));
    }
    let by_span = |values: Vec<Value>| {
        values
            .into_iter()
            .map(|v| (v["span_id"].as_str().unwrap().to_owned(), v))
            .collect::<BTreeMap<_, _>>()
    };
    assert_eq!(by_span(actual_coverage.clone()), by_span(expected_coverage));
    for (disposition, count) in [("included", 4), ("excluded", 7), ("duplicate", 1)] {
        assert_eq!(
            actual_coverage
                .iter()
                .filter(|value| value["disposition"] == disposition)
                .count(),
            count
        );
    }
    let stored_files: Vec<_> = db.prepare("SELECT release_id,relative_path,note_id,plan_revision,content_sha256,byte_length,mappings_json,links_json FROM rendered_files ORDER BY relative_path").unwrap()
        .query_map([], |row| Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,i64>(3)?,row.get::<_,String>(4)?,row.get::<_,i64>(5)?,row.get::<_,String>(6)?,row.get::<_,String>(7)?))).unwrap().map(Result::unwrap).collect();
    for file in stored_files {
        assert_eq!(file.0, release.0);
        let declared = report["files"]
            .as_array()
            .unwrap()
            .iter()
            .find(|value| value["relative_path"] == file.1)
            .unwrap();
        assert_eq!(file.2, declared["note_id"].as_str().unwrap());
        assert_eq!(file.3, 1);
        assert_eq!(file.4, declared["content_sha256"].as_str().unwrap());
        assert_eq!(file.5, declared["byte_length"].as_i64().unwrap());
        assert_eq!(
            serde_json::from_str::<Value>(&file.6).unwrap(),
            declared["content_mappings"]
        );
        assert_eq!(
            serde_json::from_str::<Value>(&file.7).unwrap(),
            declared["links"]
        );
    }
    let commit=db.query_row("SELECT release_id,committed_at,report_json_path,report_json_sha256,report_md_path,report_md_sha256,manifest_sha256 FROM publication_commits", [], |row|
        Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,String>(3)?,row.get::<_,String>(4)?,row.get::<_,String>(5)?,row.get::<_,String>(6)?))).unwrap();
    assert_eq!(commit.0, release.0);
    audit_time(&json!(commit.1), before, after);
    assert_eq!(
        (commit.2.as_str(), commit.4.as_str()),
        ("reports/validation.json", "reports/validation.md")
    );
    assert_eq!(commit.3, sha256(&fs::read(root.join(&commit.2)).unwrap()));
    assert_eq!(commit.5, sha256(&fs::read(root.join(&commit.4)).unwrap()));
    // Hash the stored compact serialization itself, rather than reordering JSON object keys.
    assert_eq!(commit.6, sha256(release.6.as_bytes()));
    drop(stmt);
    drop(db);
    assert_eq!(
        snapshot(root, None),
        protected,
        "independent SQLite inspection wrote bytes, metadata, or side files"
    );
}

#[test]
fn valid_case_compile_creates_published_output_only_in_selected_new_root() {
    // Removing real case-compile dispatch must fail this process assertion.
    let sandbox = Sandbox::new();
    let fixture = sandbox.fixture("自造fixture 中文");
    let output_root = sandbox.root.join("新测试输出 根");
    let before = SystemTime::now();
    let outcome = success_json(sandbox.compile_preserving(&fixture, &output_root, true));
    let after = SystemTime::now();
    assert_eq!(outcome["schema_version"], 1);
    assert_eq!(outcome["producer"], "fixture");
    assert_eq!(outcome["operation"], "case_compile");
    assert_eq!(outcome["case_id"], "C7k");
    assert_eq!(outcome["release_status"], "published");
    assert_eq!(outcome["output_created"], true);
    assert_report_facts(&output_root, &fixture, &outcome, before, after);
    assert_notes_and_mappings(&output_root, &fixture, &outcome["report"], false);
    assert_inventory(&output_root);
    assert_sqlite(&output_root, &fixture, &outcome["report"], before, after);
}

fn input_rejection(output: Output, codes: &[&str], name: &str) {
    assert_eq!(codes.len(), 1, "each rejection locks one stable diagnostic");
    let expected_exit = match codes[0] {
        "COMPILE_OUTPUT_EXISTS"
        | "COMPILE_INPUT_OUTPUT_OVERLAP"
        | "PROJECT_IO_ERROR"
        | "PROJECT_DATABASE_BUSY" => 3,
        "CASE_INVALID_JSON" | "CASE_INVALID_UTF8" | "PROJECT_UNSAFE_PATH" => 2,
        code => panic!("unreviewed preflight exit contract for {code}"),
    };
    assert_eq!(
        output.status.code(),
        Some(expected_exit),
        "{name}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stdout.is_empty(),
        "{name}: input rejection fabricated output facts"
    );
    let diagnostic = String::from_utf8(output.stderr).unwrap();
    assert!(
        codes.iter().any(|code| diagnostic.contains(code)),
        "{name}: expected {codes:?}, got {diagnostic}"
    );
    assert!(
        !diagnostic.contains("CLI_INVALID_ARGUMENT"),
        "{name}: compile never dispatched"
    );
}

#[test]
fn two_new_roots_keep_note_bytes_and_identities_stable_with_independent_runs() {
    let sandbox = Sandbox::new();
    let fixture = sandbox.fixture("stable-case");
    let mut outcomes = Vec::new();
    for name in ["第一次输出", "第二次输出"] {
        let root = sandbox.root.join(name);
        let before = SystemTime::now();
        let outcome = success_json(sandbox.compile_preserving(&fixture, &root, true));
        let after = SystemTime::now();
        assert_report_facts(&root, &fixture, &outcome, before, after);
        assert_notes_and_mappings(&root, &fixture, &outcome["report"], false);
        assert_sqlite(&root, &fixture, &outcome["report"], before, after);
        assert_inventory(&root);
        outcomes.push(outcome);
    }
    assert_ne!(outcomes[0]["run_id"], outcomes[1]["run_id"]);
    assert_ne!(outcomes[0]["release_id"], outcomes[1]["release_id"]);
    for key in ["files", "counts", "input_hashes", "versions"] {
        assert_eq!(
            outcomes[0]["report"][key], outcomes[1]["report"][key],
            "stable {key}"
        );
    }
}

#[test]
fn author_reviewed_table_alias_variant_emits_escaped_real_canonical_link() {
    // This intentional author recipe was specified before rendering. The same literal
    // change is independently declared in expected, never copied from actual output.
    let sandbox = Sandbox::new();
    let fixture = sandbox.fixture("reviewed-table-alias-case");
    let original_expected = read_json(&fixture.join("expected.json"));
    edit_json(&fixture, "note-plan.json", |plan| {
        plan["notes"][0]["sections"][0]["ordered_nodes"][1]["rows"][0]["cells"][1]["links"] =
            json!([{"target_concept_id":"C6h","display_text":"乙容器入口"}]);
    });
    edit_json(&fixture, "expected.json", |expected| {
        expected["tables"][0]["rows"][0]["cells"][1]["links"] =
            json!([{"target_concept_id":"C6h","display_text":"乙容器入口"}]);
        expected["required_links"].as_array_mut().unwrap().push(
            json!({"from_node_id":"T5x","target_concept_id":"C6h","display_text":"乙容器入口"}),
        );
    });
    let mut check = read_json(&fixture.join("expected.json"));
    check["tables"][0]["rows"][0]["cells"][1]["links"] = json!([]);
    check["required_links"].as_array_mut().unwrap().pop();
    assert_eq!(
        check, original_expected,
        "review must change only the intentionally declared link"
    );
    let root = sandbox.root.join("表格别名输出");
    let before = SystemTime::now();
    let outcome = success_json(sandbox.compile_preserving(&fixture, &root, true));
    let after = SystemTime::now();
    assert_report_facts(&root, &fixture, &outcome, before, after);
    assert_notes_and_mappings(&root, &fixture, &outcome["report"], true);
    assert_inventory(&root);
    assert_sqlite(&root, &fixture, &outcome["report"], before, after);
    assert!(
        fs::read_to_string(root.join("vault").join(author::ALPHA_PATH))
            .unwrap()
            .contains(author::TABLE_ALIAS_LITERAL)
    );
}

#[test]
fn literal_ascii_punctuation_is_escaped_without_changing_required_content() {
    let sandbox = Sandbox::new();
    let fixture = sandbox.fixture("author-math-text-case");
    let protected = fs::read(fixture.join("expected.json")).unwrap();
    edit_json(&fixture, "note-plan.json", |plan| {
        plan["notes"][0]["sections"][0]["ordered_nodes"][0]["text"] =
            "甲容器是虚构的蓝砂收纳盒。参见乙容器。 普通明文 x < 2 和区间 [1,2]。".into();
    });
    assert_eq!(fs::read(fixture.join("expected.json")).unwrap(), protected);
    let root = sandbox.root.join("明文输出");
    let before = SystemTime::now();
    let outcome = success_json(sandbox.compile_preserving(&fixture, &root, true));
    let after = SystemTime::now();
    assert_report_facts(&root, &fixture, &outcome, before, after);
    assert_inventory(&root);
    assert_eq!(
        fs::read(root.join("vault").join(author::ALPHA_PATH)).unwrap(),
        author::ALPHA_WITH_ESCAPED_TEXT.as_bytes()
    );
    assert_eq!(
        fs::read(root.join("vault").join(author::BETA_PATH)).unwrap(),
        author::BETA.as_bytes()
    );
    let file = &outcome["report"]["files"][0];
    let text = fs::read_to_string(root.join("vault").join(author::ALPHA_PATH)).unwrap();
    let mapping = &file["content_mappings"][0];
    let body = text
        .get(
            mapping["start_byte"].as_u64().unwrap() as usize
                ..mapping["end_byte"].as_u64().unwrap() as usize,
        )
        .unwrap();
    assert_eq!(
        unescape_cell(body),
        "甲容器是虚构的蓝砂收纳盒。参见乙容器。 普通明文 x < 2 和区间 [1,2]。"
    );
    assert_sqlite(&root, &fixture, &outcome["report"], before, after);
}

#[test]
fn invalid_fixture_preflight_never_creates_output_or_false_publication() {
    let sandbox = Sandbox::new();
    for name in [
        "source-sha-drift",
        "missing-required-atom",
        "missing-coverage",
    ] {
        let fixture = sandbox.fixture(name);
        let expected = fs::read(fixture.join("expected.json")).unwrap();
        match name {
            "source-sha-drift" => edit_json(&fixture, "case.json", |case| {
                case["sources"][0]["content_sha256"] = "0".repeat(64).into()
            }),
            "missing-required-atom" => edit_ir(&fixture, |ir| {
                ir["atoms"]
                    .as_array_mut()
                    .unwrap()
                    .retain(|a| a["atom_id"] != "A5p")
            }),
            "missing-coverage" => edit_ir(&fixture, |ir| {
                ir["coverage"]
                    .as_array_mut()
                    .unwrap()
                    .retain(|c| c["span_id"] != "P5a")
            }),
            _ => unreachable!(),
        }
        assert_eq!(fs::read(fixture.join("expected.json")).unwrap(), expected);
        let root = sandbox.root.join(format!("out-{name}"));
        let output = sandbox.compile_preserving(&fixture, &root, false);
        assert_eq!(
            output.status.code(),
            Some(3),
            "{name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report: Value = serde_json::from_slice(&output.stdout)
            .expect("semantic preflight must report real issues");
        assert_eq!(report["schema_version"], 1);
        assert_eq!(report["producer"], "fixture");
        assert_eq!(report["operation"], "case_compile");
        assert_eq!(report["case_id"], "C7k");
        assert_eq!(report["output_created"], false);
        assert!(report["run_id"].is_null());
        assert!(report["release_id"].is_null());
        assert_eq!(report["release_status"], "not_started");
        assert!(report["report"].is_null());
        assert_eq!(report["paths"]["root"], root.to_string_lossy().as_ref());
        for name in ["vault", "database", "json_report", "markdown_report"] {
            assert!(
                report["paths"][name].is_null(),
                "preflight allocated {name}"
            );
        }
        assert!(!report["issues"].as_array().unwrap().is_empty());
        let failed_check = match name {
            "source-sha-drift" => "sources",
            "missing-required-atom" => "expected",
            "missing-coverage" => "coverage",
            _ => unreachable!(),
        };
        assert_eq!(
            report["checks"][failed_check]["status"], "failed",
            "{name}: {}",
            report["checks"]
        );
        assert!(
            report["checks"][failed_check]["issue_count"]
                .as_u64()
                .unwrap()
                > 0
        );
        let expected_rule = match name {
            "source-sha-drift" => "SOURCE_CONTRACT",
            "missing-required-atom" => "EXPECTED_ATOM",
            "missing-coverage" => "COVERAGE_RECORD_COUNT",
            _ => unreachable!(),
        };
        assert!(
            report["issues"]
                .as_array()
                .unwrap()
                .iter()
                .any(|issue| issue["rule_id"] == expected_rule),
            "{name} lacks {expected_rule}: {}",
            report["issues"]
        );
        for check in [
            "rendering",
            "staging_output",
            "vault_output",
            "scientific_quality",
        ] {
            assert_eq!(report["checks"][check]["status"], "not_run");
        }
        assert!(!root.exists());
    }
}

#[test]
fn malformed_fixture_and_invalid_utf8_fail_before_output_allocation() {
    let sandbox = Sandbox::new();
    for name in ["malformed-ir", "invalid-source-utf8"] {
        let fixture = sandbox.fixture(name);
        if name == "malformed-ir" {
            fs::write(fixture.join("ir.json"), b"{").unwrap();
        } else {
            fs::write(fixture.join("sources/材料甲.md"), [0xff]).unwrap();
        }
        let root = sandbox.root.join(format!("out-{name}"));
        input_rejection(
            sandbox.compile_preserving(&fixture, &root, false),
            &[if name == "malformed-ir" {
                "CASE_INVALID_JSON"
            } else {
                "CASE_INVALID_UTF8"
            }],
            name,
        );
        assert!(!root.exists());
    }
}

#[test]
fn existing_targets_and_human_edited_published_vault_are_never_overwritten() {
    let sandbox = Sandbox::new();
    let fixture = sandbox.fixture("existing-target-case");
    for name in [
        "existing-empty-dir",
        "existing-file",
        "existing-human-vault",
    ] {
        let target = sandbox.root.join(name);
        if name == "existing-file" {
            fs::write(&target, "现有文件\n").unwrap();
        } else {
            fs::create_dir(&target).unwrap();
            if name == "existing-human-vault" {
                fs::write(target.join("人工笔记.md"), "手写内容保留\n").unwrap();
            }
        }
        input_rejection(
            sandbox.compile_preserving(&fixture, &target, false),
            &["COMPILE_OUTPUT_EXISTS"],
            name,
        );
    }
    let target = sandbox.root.join("已发布测试根");
    success_json(sandbox.compile_preserving(&fixture, &target, true));
    fs::write(
        target.join("vault").join(author::ALPHA_PATH),
        "人工改写并非fixture产物。\n",
    )
    .unwrap();
    input_rejection(
        sandbox.compile_preserving(&fixture, &target, false),
        &["COMPILE_OUTPUT_EXISTS"],
        "human-edited-published-vault",
    );
}

#[test]
fn overlapping_input_output_roots_are_rejected_but_component_siblings_are_valid() {
    let sandbox = Sandbox::new();
    let fixture = sandbox.fixture("case");
    for (name, target) in [
        ("same-root", fixture.clone()),
        ("output-inside-input", fixture.join("new-output")),
        ("input-inside-output", sandbox.root.clone()),
    ] {
        input_rejection(
            sandbox.compile_preserving(&fixture, &target, false),
            &["COMPILE_INPUT_OUTPUT_OVERLAP"],
            name,
        );
    }
    let root = sandbox.root.join("case-results");
    let outcome = success_json(sandbox.compile_preserving(&fixture, &root, true));
    assert_eq!(outcome["release_status"], "published");
    assert_inventory(&root);
}

#[cfg(windows)]
#[test]
fn windows_path_controls_are_rejected_before_any_selected_root_is_created() {
    let sandbox = Sandbox::new();
    let fixture = sandbox.fixture("windows-path-case");
    fs::create_dir(sandbox.root.join("ordinary")).unwrap();
    for name in [
        "ordinary/../escaped-target",
        "CON",
        "NUL",
        "target:stream",
        "tail.",
        "tail ",
    ] {
        // A Windows verbatim PathBuf::join normalizes ParentDir before spawning.
        // Preserve the author's raw argument so the CLI itself sees the unsafe component.
        let mut raw_argument = sandbox.root.as_os_str().to_owned();
        raw_argument.push("\\");
        raw_argument.push(name.replace('/', "\\"));
        let target = PathBuf::from(raw_argument);
        if name.contains("/../") {
            assert!(target.as_os_str().to_string_lossy().contains("\\..\\"));
        }
        input_rejection(
            sandbox.compile_preserving(&fixture, &target, false),
            &["PROJECT_UNSAFE_PATH"],
            name,
        );
        assert!(!target.exists());
    }
    let target = sandbox.root.join("missing-parent/output");
    input_rejection(
        sandbox.compile_preserving(&fixture, &target, false),
        &["PROJECT_IO_ERROR"],
        "missing-output-parent",
    );
    assert!(!sandbox.root.join("missing-parent").exists());
}

#[test]
fn parent_components_and_missing_parents_are_rejected_without_creating_outputs() {
    let sandbox = Sandbox::new();
    let fixture = sandbox.fixture("parent-path-case");
    fs::create_dir(sandbox.root.join("ordinary")).unwrap();
    let mut raw = sandbox.root.as_os_str().to_owned();
    raw.push(std::path::MAIN_SEPARATOR_STR);
    raw.push("ordinary/../escaped-target".replace('/', std::path::MAIN_SEPARATOR_STR));
    let target = PathBuf::from(raw);
    input_rejection(
        sandbox.compile_preserving(&fixture, &target, false),
        &["PROJECT_UNSAFE_PATH"],
        "raw-parent-component",
    );
    assert!(!sandbox.root.join("escaped-target").exists());
    let target = sandbox.root.join("missing-parent/output");
    input_rejection(
        sandbox.compile_preserving(&fixture, &target, false),
        &["PROJECT_IO_ERROR"],
        "missing-output-parent",
    );
    assert!(!sandbox.root.join("missing-parent").exists());
}

#[cfg(windows)]
#[test]
fn real_windows_source_lock_and_junctions_do_not_write_external_targets() {
    use std::os::windows::fs::OpenOptionsExt;
    let sandbox = Sandbox::new();
    let fixture = sandbox.fixture("lock-junction-case");
    let output = sandbox.root.join("locked-output");
    let before = snapshot(&sandbox.root, None);
    let guard = OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(fixture.join("sources/材料甲.md"))
        .unwrap();
    let result = sandbox.compile(&fixture, &output);
    drop(guard);
    assert_eq!(snapshot(&sandbox.root, None), before);
    input_rejection(result, &["PROJECT_DATABASE_BUSY"], "real-source-lock");
    assert!(!output.exists());
    let external = sandbox.root.join("外部人工资料");
    fs::create_dir(&external).unwrap();
    fs::write(external.join("哨兵.md"), "外部目录不能被接管\n").unwrap();
    let parent_link = sandbox.root.join("linked-output-parent");
    sandbox.junction(&parent_link, &external);
    input_rejection(
        sandbox.compile_preserving(&fixture, &parent_link.join("new-root"), false),
        &["PROJECT_UNSAFE_PATH"],
        "output-parent-junction",
    );
    let root_link = sandbox.root.join("linked-existing-root");
    sandbox.junction(&root_link, &external);
    input_rejection(
        sandbox.compile_preserving(&fixture, &root_link, false),
        &["PROJECT_UNSAFE_PATH"],
        "output-root-junction",
    );
    let source_link = sandbox.root.join("linked-case-root");
    sandbox.junction(&source_link, &fixture);
    let target = sandbox.root.join("junction-input-output");
    input_rejection(
        sandbox.compile_preserving(&source_link, &target, false),
        &["PROJECT_UNSAFE_PATH"],
        "input-root-junction",
    );
    assert!(!target.exists());
}

#[test]
fn parameter_separator_supports_dash_paths_and_extra_arguments_remain_read_only() {
    let sandbox = Sandbox::new();
    let fixture = sandbox.fixture("cwd 中文 with spaces/-case");
    let root = sandbox.root.join("cwd 中文 with spaces/-output");
    let before = snapshot(&sandbox.root, Some(&root));
    let outcome = success_json(sandbox.run(&[
        "case".into(),
        "compile".into(),
        "--".into(),
        "-case".into(),
        "-output".into(),
    ]));
    assert_eq!(snapshot(&sandbox.root, Some(&root)), before);
    assert_eq!(outcome["release_status"], "published");
    assert_inventory(&root);
    let untouched = snapshot(&sandbox.root, None);
    for args in [
        vec!["case".into(), "compile".into()],
        vec![
            "case".into(),
            "compile".into(),
            fixture.as_os_str().to_owned(),
        ],
        vec![
            "case".into(),
            "compile".into(),
            fixture.as_os_str().to_owned(),
            "unused-root".into(),
            "extra".into(),
        ],
    ] {
        let output = sandbox.run(&args);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("CLI_INVALID_ARGUMENT"));
        assert_eq!(snapshot(&sandbox.root, None), untouched);
    }
}

#[test]
fn closed_stdout_after_real_compile_keeps_committed_publication_authority() {
    let sandbox = Sandbox::new();
    let fixture = sandbox.fixture("stdout-delivery-case");
    let root = sandbox.root.join("stdout关闭仍已提交");
    let protected = snapshot(&sandbox.root, Some(&root));
    let before = SystemTime::now();
    let mut child = sandbox
        .command(&[
            "case".into(),
            "compile".into(),
            fixture.as_os_str().to_owned(),
            root.as_os_str().to_owned(),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdout.take().unwrap());
    let output = child.wait_with_output().unwrap();
    let after = SystemTime::now();
    assert_eq!(
        output.status.code(),
        Some(1),
        "closed stdout must report delivery error: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("CLI_OUTPUT_ERROR"));
    assert_eq!(snapshot(&sandbox.root, Some(&root)), protected);
    let report = read_json(&root.join("reports/validation.json"));
    assert_eq!(report["result"], "validated");
    assert_markdown_report(&root, &report);
    assert_notes_and_mappings(&root, &fixture, &report, false);
    assert_inventory(&root);
    // The sole publication authority is already committed. Delivery failure must
    // not rewrite Run=succeeded, Release=published, or Coverage=validated to failed.
    assert_sqlite(&root, &fixture, &report, before, after);
}
