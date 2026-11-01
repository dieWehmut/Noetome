use std::cell::RefCell;
use std::collections::BTreeMap;
#[cfg(windows)]
use std::fs::OpenOptions;
use std::fs::{self, Metadata};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::SystemTime;

use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

struct Sandbox {
    root: PathBuf,
    junctions: RefCell<Vec<PathBuf>>,
}

impl Sandbox {
    fn new() -> Self {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/fixture-cli-tests");
        fs::create_dir_all(&parent).unwrap();
        let root = parent
            .canonicalize()
            .unwrap()
            .join(Uuid::new_v4().to_string());
        fs::create_dir(&root).unwrap();
        for name in ["cwd 中文 with spaces", "home", "appdata", "localappdata"] {
            fs::create_dir(root.join(name)).unwrap();
        }
        fs::write(root.join("外部人工哨兵.md"), "人工内容只读。\n").unwrap();
        Self {
            root,
            junctions: RefCell::new(Vec::new()),
        }
    }

    fn copy_fixture(&self, name: &str) -> PathBuf {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/text-compare-v1");
        let destination = self.root.join(name);
        copy_tree(&fixture, &destination);
        destination
    }

    fn verify(&self, fixture: &Path) -> Output {
        Command::new(env!("CARGO_BIN_EXE_noetome"))
            .args(["case", "verify"])
            .arg(fixture)
            .current_dir(self.root.join("cwd 中文 with spaces"))
            .env("HOME", self.root.join("home"))
            .env("USERPROFILE", self.root.join("home"))
            .env("APPDATA", self.root.join("appdata"))
            .env("LOCALAPPDATA", self.root.join("localappdata"))
            .env("TMP", &self.root)
            .env("TEMP", &self.root)
            .env("TMPDIR", &self.root)
            .output()
            .expect("execute compiled case verifier")
    }

    fn verify_preserving(&self, fixture: &Path) -> Output {
        let before = snapshot(&self.root);
        let output = self.verify(fixture);
        assert_eq!(
            snapshot(&self.root),
            before,
            "case verify changed bytes, mtimes, directories, or external sentinel"
        );
        output
    }

    fn mutated(&self, name: &str, mutate: impl FnOnce(&Path)) -> PathBuf {
        let fixture = self.copy_fixture(name);
        let expected_before = fs::read(fixture.join("expected.json")).unwrap();
        mutate(&fixture);
        assert_eq!(
            fs::read(fixture.join("expected.json")).unwrap(),
            expected_before,
            "input mutation must never rewrite independent expected"
        );
        fixture
    }

    #[cfg(windows)]
    fn junction(&self, link: &Path, target: &Path) {
        let dos = |path: &Path| {
            path.to_string_lossy()
                .trim_start_matches(r"\\?\")
                .to_owned()
        };
        let output = Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(dos(link))
            .arg(dos(target))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "create test-owned junction: {}",
            String::from_utf8_lossy(&output.stderr)
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
            .join("../../target/fixture-cli-tests")
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

#[derive(Debug, PartialEq, Eq)]
struct Entry {
    bytes: Option<Vec<u8>>,
    modified: Option<SystemTime>,
    readonly: bool,
    link: bool,
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
    fn visit(root: &Path, path: &Path, entries: &mut BTreeMap<PathBuf, Entry>) {
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
                modified: metadata.modified().ok(),
                readonly: metadata.permissions().readonly(),
                link,
            },
        );
        if metadata.is_dir() && !link {
            for entry in fs::read_dir(path).unwrap() {
                visit(root, &entry.unwrap().path(), entries);
            }
        }
    }
    let mut result = BTreeMap::new();
    visit(root, root, &mut result);
    result
}

fn success_json(output: Output) -> Value {
    assert_eq!(
        output.status.code(),
        Some(0),
        "case verify failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    serde_json::from_slice(&output.stdout).expect("stdout must be pure JSON")
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn edit_json(fixture: &Path, file: &str, edit: impl FnOnce(&mut Value)) {
    let path = fixture.join(file);
    let mut value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    edit(&mut value);
    let mut bytes = serde_json::to_vec_pretty(&value).unwrap();
    bytes.push(b'\n');
    fs::write(path, bytes).unwrap();
}

fn edit_ir(fixture: &Path, edit: impl FnOnce(&mut Value)) {
    edit_json(fixture, "ir.json", edit);
    // Refresh only the NotePlan binding to isolate the semantic rule being tested.
    // The independent expected is never derived from mutated IR or validator output.
    let new_sha = sha256(&fs::read(fixture.join("ir.json")).unwrap());
    edit_json(fixture, "note-plan.json", |plan| {
        plan["knowledge_sha256"] = new_sha.into()
    });
}

fn failed_check(output: Output, check: &str, name: &str) -> Value {
    assert_eq!(
        output.status.code(),
        Some(3),
        "{name}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|_| {
        panic!(
            "{name} must have failed JSON: {}",
            String::from_utf8_lossy(&output.stdout)
        )
    });
    assert_eq!(report["passed"], false, "{name}: {report}");
    assert_eq!(
        report["checks"][check]["status"], "failed",
        "{name}: {report}"
    );
    let issues = report["issues"].as_array().unwrap();
    assert!(!issues.is_empty(), "{name} needs actionable issues");
    for issue in issues {
        assert!(!issue["rule_id"].as_str().unwrap().is_empty());
        assert!(!issue["message"].as_str().unwrap().is_empty());
    }
    for check in ["rendering", "scientific_quality"] {
        assert_eq!(report["checks"][check]["status"], "not_run");
    }
    report
}

fn input_rejection(output: Output, name: &str) {
    assert!(
        matches!(output.status.code(), Some(2 | 3)),
        "{name}: unexpected exit {:?}",
        output.status.code()
    );
    assert!(
        output.stdout.is_empty(),
        "{name}: malformed or unreadable input must not fabricate report"
    );
    let diagnostic = String::from_utf8(output.stderr).unwrap();
    assert!(
        diagnostic.contains("CASE_") || diagnostic.contains("PROJECT_"),
        "{name}: no stable input diagnostic: {diagnostic}"
    );
    assert!(
        !diagnostic.contains("CLI_INVALID_ARGUMENT"),
        "{name}: case verify must dispatch before input validation"
    );
}

fn input_rejection_with_codes(output: Output, name: &str, codes: &[&str]) {
    let diagnostic = String::from_utf8_lossy(&output.stderr);
    assert!(
        codes.iter().any(|code| diagnostic.contains(code)),
        "{name}: expected one of {codes:?}, got {diagnostic}"
    );
    input_rejection(output, name);
}

fn atom<'a>(ir: &'a mut Value, id: &str) -> &'a mut Value {
    ir["atoms"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|a| a["atom_id"] == id)
        .unwrap()
}

fn coverage<'a>(ir: &'a mut Value, id: &str) -> &'a mut Value {
    ir["coverage"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|c| c["span_id"] == id)
        .unwrap()
}

#[test]
fn valid_fixture_reports_only_contract_success_and_preserves_all_inputs() {
    // Removing the real CLI case-verify dispatch must fail this process assertion.
    let sandbox = Sandbox::new();
    let fixture = sandbox.copy_fixture("有效 中文比较包");
    let report = success_json(sandbox.verify_preserving(&fixture));
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["producer"], "fixture");
    assert_eq!(report["case_id"], "C7k");
    assert_eq!(report["validator_version"], "fixture-contracts-v1");
    assert_eq!(report["passed"], true);
    assert_eq!(
        report["counts"],
        serde_json::json!({
            "sources": 2, "spans": 12, "atoms": 4, "required_atoms": 4, "notes": 2, "nodes": 4
        })
    );
    for check in ["sources", "coverage", "knowledge", "note_plan", "expected"] {
        assert_eq!(
            report["checks"][check]["status"], "passed",
            "{check}: {report}"
        );
    }
    for check in ["rendering", "scientific_quality"] {
        assert_eq!(report["checks"][check]["status"], "not_run");
    }
    assert_eq!(report["issues"].as_array().unwrap().len(), 0);
    assert_eq!(report["resolved_targets"].as_array().unwrap().len(), 2);
    let targets: BTreeMap<_, _> = report["resolved_targets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|target| (target["concept_id"].as_str().unwrap(), target.clone()))
        .collect();
    assert_eq!(
        targets["C2f"],
        serde_json::json!({
            "concept_id": "C2f", "note_id": "N4u", "section_id": "H2j",
            "output_path": "笔记/甲容器.md", "heading": "定义与比较"
        })
    );
    assert_eq!(
        targets["C6h"],
        serde_json::json!({
            "concept_id": "C6h", "note_id": "N8v", "section_id": "H6n",
            "output_path": "笔记/乙容器.md", "heading": "定义与条件"
        })
    );
    assert_eq!(
        report["ir_sha256"],
        sha256(&fs::read(fixture.join("ir.json")).unwrap())
    );
    assert_eq!(
        report["plan_sha256"],
        sha256(&fs::read(fixture.join("note-plan.json")).unwrap())
    );
    assert_eq!(
        report["expected_sha256"],
        sha256(&fs::read(fixture.join("expected.json")).unwrap())
    );
    assert_eq!(
        report["case_sha256"],
        sha256(&fs::read(fixture.join("case.json")).unwrap())
    );
    assert_eq!(
        success_json(sandbox.verify_preserving(&fixture)),
        report,
        "repeated read-only validation must preserve deterministic contract facts"
    );
}

#[test]
fn missing_atom_and_evidence_and_refutes_only_cannot_validate_required_knowledge() {
    // Counting IDs or accepting refutes as supports would make a named case falsely pass.
    let sandbox = Sandbox::new();
    for name in [
        "missing-required-atom",
        "missing-evidence",
        "refutes-only",
        "unknown-required-value",
        "unverified-required-atom",
        "empty-atom-content",
    ] {
        let fixture = sandbox.mutated(name, |root| {
            edit_ir(root, |ir| match name {
                "missing-required-atom" => ir["atoms"]
                    .as_array_mut()
                    .unwrap()
                    .retain(|a| a["atom_id"] != "A5p"),
                "missing-evidence" => ir["evidence"]
                    .as_array_mut()
                    .unwrap()
                    .retain(|e| e["evidence_id"] != "E5a"),
                "refutes-only" => {
                    atom(ir, "A5p")["evidence_links"][0]["relation"] = "refutes".into()
                }
                "unknown-required-value" => atom(ir, "A5p")["value_status"] = "unknown".into(),
                "unverified-required-atom" => atom(ir, "A5p")["verification"] = "unverified".into(),
                "empty-atom-content" => atom(ir, "A5p")["content"] = "".into(),
                _ => unreachable!(),
            })
        });
        // The independent list detects removal of an atom absent from IR;
        // knowledge validates the remaining IR entries and their evidence.
        let check = if name == "missing-required-atom" {
            "expected"
        } else {
            "knowledge"
        };
        failed_check(sandbox.verify_preserving(&fixture), check, name);
    }
}

#[test]
fn independent_expected_catches_required_false_and_content_or_qualifier_drift() {
    let sandbox = Sandbox::new();
    for name in [
        "required-false",
        "changed-content",
        "deleted-unit",
        "deleted-condition",
        "deleted-exception",
    ] {
        let fixture = sandbox.mutated(name, |root| {
            edit_ir(root, |ir| {
                let value = atom(ir, "A5p");
                match name {
                    "required-false" => value["required"] = false.into(),
                    "changed-content" => value["content"] = "甲容器的虚构刻度为50 cm。".into(),
                    "deleted-unit" => value["content"] = "甲容器的虚构刻度为5。".into(),
                    "deleted-condition" => {
                        value["qualifiers"].as_array_mut().unwrap().remove(0);
                    }
                    "deleted-exception" => {
                        value["qualifiers"].as_array_mut().unwrap().pop();
                    }
                    _ => unreachable!(),
                }
            })
        });
        failed_check(sandbox.verify_preserving(&fixture), "expected", name);
    }
}

#[test]
fn complete_coverage_rejects_missing_duplicate_self_cycle_and_fake_exclusions() {
    let sandbox = Sandbox::new();
    for name in [
        "missing-coverage",
        "duplicate-coverage",
        "self-duplicate",
        "duplicate-cycle",
        "missing-duplicate-position-support",
        "duplicate-mapping-drift",
        "fake-exclusion",
        "deterministic-exclusion-of-content",
        "pending-coverage",
        "failed-coverage",
        "premature-rendered-stage",
    ] {
        let fixture = sandbox.mutated(name, |root| {
            edit_ir(root, |ir| match name {
                "missing-coverage" => ir["coverage"]
                    .as_array_mut()
                    .unwrap()
                    .retain(|c| c["span_id"] != "P5a"),
                "duplicate-coverage" => {
                    let duplicate = coverage(ir, "P5a").clone();
                    ir["coverage"].as_array_mut().unwrap().push(duplicate);
                }
                "self-duplicate" => coverage(ir, "P7b")["duplicate_of_span_id"] = "P7b".into(),
                "duplicate-cycle" => {
                    coverage(ir, "P3a")["disposition"] = "duplicate".into();
                    coverage(ir, "P3a")["duplicate_of_span_id"] = "P7b".into();
                }
                "missing-duplicate-position-support" => atom(ir, "A3d")["evidence_links"]
                    .as_array_mut()
                    .unwrap()
                    .retain(|e| e["evidence_id"] != "E7b"),
                "duplicate-mapping-drift" => {
                    coverage(ir, "P7b")["atom_ids"] = serde_json::json!(["A7e"]);
                    coverage(ir, "P7b")["required_atom_ids"] = serde_json::json!(["A7e"]);
                }
                "fake-exclusion" | "deterministic-exclusion-of-content" => {
                    let record = coverage(ir, "P5a");
                    record["disposition"] = "excluded".into();
                    record["atom_ids"] = serde_json::json!([]);
                    record["required_atom_ids"] = serde_json::json!([]);
                    record["exclusion_reason"] = "随意理由不能替代独立预期。".into();
                    if name == "deterministic-exclusion-of-content" {
                        record["decision_source"] = "deterministic_format".into();
                    }
                }
                "pending-coverage" => coverage(ir, "P5a")["disposition"] = "pending".into(),
                "failed-coverage" => coverage(ir, "P5a")["disposition"] = "failed".into(),
                "premature-rendered-stage" => coverage(ir, "P5a")["stage"] = "rendered".into(),
                _ => unreachable!(),
            })
        });
        let check = if name == "fake-exclusion" {
            "expected"
        } else {
            "coverage"
        };
        failed_check(sandbox.verify_preserving(&fixture), check, name);
    }
}

#[test]
fn complete_raw_ir_binding_detects_whitespace_change_even_with_same_json_fields() {
    let sandbox = Sandbox::new();
    let fixture = sandbox.mutated("ir-whitespace-stale-sha", |root| {
        let path = root.join("ir.json");
        let before = fs::read(&path).unwrap();
        let mut changed = before.clone();
        changed.push(b'\n');
        assert_eq!(
            serde_json::from_slice::<Value>(&before).unwrap(),
            serde_json::from_slice::<Value>(&changed).unwrap()
        );
        fs::write(path, changed).unwrap();
        // Deliberately do not refresh NotePlan knowledge_sha256 here.
    });
    failed_check(
        sandbox.verify_preserving(&fixture),
        "note_plan",
        "ir-whitespace-stale-sha",
    );
}

#[test]
fn note_plan_requires_content_in_each_mapped_paragraph_list_item_and_table_cell() {
    let sandbox = Sandbox::new();
    for name in [
        "paragraph-id-only",
        "paragraph-empty",
        "list-item-unit-missing",
        "table-cell-unit-missing",
        "table-cell-exception-missing",
        "empty-table",
        "misplaced-cell",
        "unit-borrowed-from-label",
        "table-atom-turned-label",
    ] {
        let fixture = sandbox.mutated(name, |root| edit_json(root, "note-plan.json", |plan| {
            match name {
                "paragraph-id-only" => plan["notes"][0]["sections"][0]["ordered_nodes"][0]["text"] = "只留下原子编号A3d。".into(),
                "paragraph-empty" => plan["notes"][0]["sections"][0]["ordered_nodes"][0]["text"] = "".into(),
                "list-item-unit-missing" => plan["notes"][1]["sections"][0]["ordered_nodes"][1]["items"][0]["text"] = "乙容器的虚构刻度为8。仅用于室温演示；空盒时不读数；浸水后不适用。".into(),
                _ => {
                    let table = &mut plan["notes"][0]["sections"][0]["ordered_nodes"][1];
                    match name {
                        "table-cell-unit-missing" => table["rows"][0]["cells"][1]["text"] = "甲容器的虚构刻度为5。仅用于室温演示；装满后停止读数；破损时不适用。".into(),
                        "table-cell-exception-missing" => table["rows"][0]["cells"][1]["text"] = "甲容器的虚构刻度为5 cm。仅用于室温演示；装满后停止读数。".into(),
                        "empty-table" => table["rows"] = serde_json::json!([]),
                        "misplaced-cell" => table["rows"][0]["cells"].as_array_mut().unwrap().swap(1, 2),
                        "unit-borrowed-from-label" => {
                            table["rows"][0]["cells"][0]["text"] = "虚构刻度 cm".into();
                            table["rows"][0]["cells"][1]["text"] = "甲容器的虚构刻度为5。仅用于室温演示；装满后停止读数；破损时不适用。".into();
                        },
                        "table-atom-turned-label" => {
                            table["rows"][0]["cells"][1]["kind"] = "label".into();
                            table["rows"][0]["cells"][1]["atom_ids"] = serde_json::json!([]);
                            table["atom_ids"] = serde_json::json!(["A9w"]);
                        },
                        _ => unreachable!(),
                    }
                },
            }
        }));
        let check = if matches!(name, "misplaced-cell" | "table-atom-turned-label") {
            "expected"
        } else {
            "note_plan"
        };
        failed_check(sandbox.verify_preserving(&fixture), check, name);
    }
}

#[test]
fn independent_expected_fixes_table_labels_missing_values_and_cell_kind() {
    let sandbox = Sandbox::new();
    for name in [
        "column-label-drift",
        "row-label-drift",
        "missing-value-drift",
        "missing-turned-label",
        "row-order-drift",
    ] {
        let fixture = sandbox.mutated(name, |root| {
            edit_json(root, "note-plan.json", |plan| {
                let table = &mut plan["notes"][0]["sections"][0]["ordered_nodes"][1];
                match name {
                    "column-label-drift" => table["columns"][1]["label"] = "乙容器".into(),
                    "row-label-drift" => table["rows"][0]["cells"][0]["text"] = "单价".into(),
                    "missing-value-drift" => table["rows"][1]["cells"][1]["text"] = "不适用".into(),
                    "missing-turned-label" => table["rows"][1]["cells"][1]["kind"] = "label".into(),
                    "row-order-drift" => table["rows"].as_array_mut().unwrap().swap(0, 1),
                    _ => unreachable!(),
                }
            })
        });
        failed_check(sandbox.verify_preserving(&fixture), "expected", name);
    }
}

#[test]
fn canonical_targets_and_title_paths_must_be_unique_and_unambiguous() {
    let sandbox = Sandbox::new();
    for name in [
        "duplicate-canonical-placement",
        "missing-canonical-placement",
        "unknown-link-target",
        "changed-link-display",
        "case-insensitive-output-path-collision",
        "heading-ambiguity",
        "wiki-heading-control",
        "wiki-title-control",
        "unknown-node-atom",
    ] {
        let fixture = sandbox.mutated(name, |root| edit_json(root, "note-plan.json", |plan| match name {
            "duplicate-canonical-placement" => {
                let duplicate = plan["placements"][0].clone();
                plan["placements"].as_array_mut().unwrap().push(duplicate);
            },
            "missing-canonical-placement" => { plan["placements"].as_array_mut().unwrap().remove(0); },
            "unknown-link-target" => plan["notes"][0]["sections"][0]["ordered_nodes"][0]["links"][0]["target_concept_id"] = "C0unknown".into(),
            "changed-link-display" => plan["notes"][0]["sections"][0]["ordered_nodes"][0]["links"][0]["display_text"] = "错误别名".into(),
            "case-insensitive-output-path-collision" => {
                plan["notes"][0]["output_path"] = "Notes/Shared.md".into();
                plan["notes"][1]["output_path"] = "notes/shared.md".into();
            },
            "heading-ambiguity" => {
                let mut section = plan["notes"][0]["sections"][0].clone();
                section["section_id"] = "H0extra".into();
                section["heading"] = "  定义与比较  ".into();
                section["ordered_nodes"] = serde_json::json!([{"node_id":"Q0extra","representation_origin":"fixture","atom_ids":["A3d"],"kind":"paragraph","text":"甲容器是虚构的蓝砂收纳盒。","links":[]}]);
                plan["notes"][0]["sections"].as_array_mut().unwrap().push(section);
            },
            "wiki-heading-control" => plan["notes"][0]["sections"][0]["heading"] = "定义#隐藏".into(),
            "wiki-title-control" => plan["notes"][0]["title"] = "甲[容器]".into(),
            "unknown-node-atom" => plan["notes"][0]["sections"][0]["ordered_nodes"][0]["atom_ids"] = serde_json::json!(["A0unknown"]),
            _ => unreachable!(),
        }));
        let check = if name == "changed-link-display" {
            "expected"
        } else {
            "note_plan"
        };
        failed_check(sandbox.verify_preserving(&fixture), check, name);
    }
}

#[test]
fn source_mismatch_stops_later_checks_and_does_not_slice_declared_bad_boundaries() {
    let sandbox = Sandbox::new();
    for name in [
        "changed-source-bytes",
        "source-sha-drift",
        "span-gap",
        "span-utf8-boundary",
        "span-kind-drift",
        "span-line-drift",
        "span-needs-review",
    ] {
        let fixture = sandbox.mutated(name, |root| {
            if name == "changed-source-bytes" {
                let path = root.join("sources/材料甲.md");
                let mut bytes = fs::read(&path).unwrap();
                bytes.extend_from_slice("额外内容\n".as_bytes());
                fs::write(path, bytes).unwrap();
            } else {
                edit_json(root, "case.json", |case| match name {
                    "source-sha-drift" => {
                        case["sources"][0]["content_sha256"] = "0".repeat(64).into()
                    }
                    "span-gap" => case["sources"][0]["spans"][2]["start_byte"] = 28.into(),
                    "span-utf8-boundary" => {
                        case["sources"][0]["spans"][2]["start_byte"] = 26.into()
                    }
                    "span-kind-drift" => case["sources"][0]["spans"][2]["kind"] = "heading".into(),
                    "span-line-drift" => case["sources"][0]["spans"][2]["start_line"] = 9.into(),
                    "span-needs-review" => {
                        case["sources"][0]["spans"][2]["parse_status"] = "needs_review".into()
                    }
                    _ => unreachable!(),
                });
            }
        });
        let report = failed_check(sandbox.verify_preserving(&fixture), "sources", name);
        for check in ["coverage", "knowledge", "note_plan", "expected"] {
            assert_eq!(
                report["checks"][check]["status"], "not_run",
                "{name}: {report}"
            );
        }
    }
}

#[test]
fn evidence_ranges_require_actual_utf8_boundaries_and_slice_sha() {
    let sandbox = Sandbox::new();
    for name in [
        "excerpt-utf8-boundary",
        "excerpt-out-of-span",
        "excerpt-empty",
        "excerpt-sha-drift",
        "unknown-evidence-span",
    ] {
        let fixture = sandbox.mutated(name, |root| {
            edit_ir(root, |ir| {
                let evidence = &mut ir["evidence"][0];
                match name {
                    "excerpt-utf8-boundary" => evidence["excerpt_start_byte"] = 1.into(),
                    "excerpt-out-of-span" => evidence["excerpt_end_byte"] = 400.into(),
                    "excerpt-empty" => evidence["excerpt_end_byte"] = 0.into(),
                    "excerpt-sha-drift" => evidence["excerpt_sha256"] = "0".repeat(64).into(),
                    "unknown-evidence-span" => evidence["source_span_id"] = "P0unknown".into(),
                    _ => unreachable!(),
                }
            })
        });
        failed_check(sandbox.verify_preserving(&fixture), "knowledge", name);
    }
}

#[test]
fn malformed_contract_versions_unknown_fields_and_missing_fields_are_explicit_input_errors() {
    let sandbox = Sandbox::new();
    for file in ["case.json", "ir.json", "note-plan.json"] {
        for mutation in [
            "future-schema",
            "unknown-field",
            "missing-field",
            "wrong-producer",
        ] {
            let name = format!("{file}-{mutation}");
            let fixture = sandbox.mutated(&name, |root| {
                edit_json(root, file, |value| match mutation {
                    "future-schema" => value["schema_version"] = 2.into(),
                    "unknown-field" => value["schema_verison"] = 1.into(),
                    "missing-field" => {
                        value.as_object_mut().unwrap().remove("producer");
                    }
                    "wrong-producer" => value["producer"] = "model".into(),
                    _ => unreachable!(),
                })
            });
            let code = if mutation == "future-schema" {
                "CASE_UNSUPPORTED_SCHEMA"
            } else {
                "CASE_INVALID_JSON"
            };
            input_rejection_with_codes(sandbox.verify_preserving(&fixture), &name, &[code]);
        }
    }
    for name in [
        "unknown-node-field",
        "unknown-cell-field",
        "unknown-enum",
        "invalid-json",
        "invalid-json-utf8",
        "invalid-source-utf8",
    ] {
        let fixture = sandbox.mutated(name, |root| match name {
            "unknown-node-field" => edit_json(root, "note-plan.json", |plan| plan["notes"][0]["sections"][0]["ordered_nodes"][0]["text_typo"] = "隐藏字段".into()),
            "unknown-cell-field" => edit_json(root, "note-plan.json", |plan| plan["notes"][0]["sections"][0]["ordered_nodes"][1]["rows"][0]["cells"][0]["value_typo"] = 0.into()),
            "unknown-enum" => edit_json(root, "ir.json", |ir| atom(ir, "A5p")["verification"] = "trusted".into()),
            "invalid-json" => fs::write(root.join("ir.json"), b"{").unwrap(),
            "invalid-json-utf8" => fs::write(root.join("ir.json"), [0xff]).unwrap(),
            "invalid-source-utf8" => fs::write(root.join("sources/材料甲.md"), [0xff]).unwrap(),
            _ => unreachable!(),
        });
        let code = if name == "invalid-source-utf8" {
            "CASE_INVALID_UTF8"
        } else {
            "CASE_INVALID_JSON"
        };
        input_rejection_with_codes(sandbox.verify_preserving(&fixture), name, &[code]);
    }
}

#[test]
fn selected_input_paths_reject_traversal_absolute_ads_collisions_and_reserved_names() {
    let sandbox = Sandbox::new();
    for path in [
        "../外部人工哨兵.md",
        "/outside.md",
        "D:/outside.md",
        "sources/材料甲.md:stream",
        "sources/../sources/材料甲.md",
        "sources\\材料甲.md",
        "sources/CON.md",
        "sources/tail.md ",
        "sources/tail.md.",
    ] {
        let name = format!("bad-source-path-{}", sha256(path.as_bytes()));
        let fixture = sandbox.mutated(&name, |root| {
            edit_json(root, "case.json", |case| {
                case["sources"][0]["relative_path"] = path.into()
            })
        });
        input_rejection_with_codes(
            sandbox.verify_preserving(&fixture),
            path,
            &["CASE_INVALID_PATH", "CASE_UNSUPPORTED_SOURCE"],
        );
    }
    for name in [
        "case-insensitive-input-collision",
        "duplicate-declared-file",
        "manifest-ir-parent-path",
        "manifest-expected-absolute-path",
        "media-extension-mismatch",
        "missing-selected-file",
    ] {
        let fixture = sandbox.mutated(name, |root| {
            edit_json(root, "case.json", |case| match name {
                "case-insensitive-input-collision" => {
                    // Both selected spellings exist on Windows; missing-file rejection
                    // must not hide a missing case-insensitive collision check.
                    case["sources"][1]["relative_path"] = "Sources/材料甲.md".into();
                    #[cfg(not(windows))]
                    {
                        fs::create_dir(root.join("Sources")).unwrap();
                        fs::copy(
                            root.join("sources/材料乙.md"),
                            root.join("Sources/材料甲.md"),
                        )
                        .unwrap();
                    }
                }
                "duplicate-declared-file" => {
                    case["sources"][1]["relative_path"] =
                        case["sources"][0]["relative_path"].clone()
                }
                "manifest-ir-parent-path" => case["ir_file"] = "../ir.json".into(),
                "manifest-expected-absolute-path" => {
                    case["expected_file"] = "D:/expected.json".into()
                }
                "media-extension-mismatch" => {
                    case["sources"][0]["media_type"] = "text/plain".into()
                }
                "missing-selected-file" => {
                    case["sources"][0]["relative_path"] = "sources/missing.md".into()
                }
                _ => unreachable!(),
            })
        });
        let code = match name {
            "media-extension-mismatch" => "CASE_UNSUPPORTED_SOURCE",
            "missing-selected-file" => "PROJECT_IO_ERROR",
            _ => "CASE_INVALID_PATH",
        };
        input_rejection_with_codes(sandbox.verify_preserving(&fixture), name, &[code]);
    }
}

#[test]
fn note_output_paths_reject_traversal_reserved_names_and_windows_ambiguity() {
    let sandbox = Sandbox::new();
    for path in [
        "../escape.md",
        "D:/escape.md",
        "/escape.md",
        "note.md:stream",
        "a/../escape.md",
        "a\\escape.md",
        "CON.md",
        "a/NUL.md",
        "note.md.",
        "note.md ",
        "note.txt",
    ] {
        let name = format!("bad-output-path-{}", sha256(path.as_bytes()));
        let fixture = sandbox.mutated(&name, |root| {
            edit_json(root, "note-plan.json", |plan| {
                plan["notes"][0]["output_path"] = path.into()
            })
        });
        failed_check(sandbox.verify_preserving(&fixture), "note_plan", path);
    }
}

#[cfg(windows)]
#[test]
fn real_windows_exclusive_source_lock_fails_without_any_written_bytes() {
    use std::os::windows::fs::OpenOptionsExt;
    let sandbox = Sandbox::new();
    let fixture = sandbox.copy_fixture("真实Windows源锁");
    let before = snapshot(&sandbox.root);
    let guard = OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(fixture.join("sources/材料甲.md"))
        .unwrap();
    let output = sandbox.verify(&fixture);
    drop(guard);
    assert_eq!(
        snapshot(&sandbox.root),
        before,
        "locked-input verification mutated selected or external files"
    );
    input_rejection_with_codes(
        output,
        "real-exclusive-source-lock",
        &["PROJECT_DATABASE_BUSY"],
    );
}

#[cfg(windows)]
#[test]
fn real_windows_root_and_source_junctions_never_widen_selected_inputs() {
    let sandbox = Sandbox::new();
    let fixture = sandbox.copy_fixture("junction-original");
    let link = sandbox.root.join("junction-case-root");
    sandbox.junction(&link, &fixture);
    input_rejection_with_codes(
        sandbox.verify_preserving(&link),
        "root-junction",
        &["PROJECT_UNSAFE_PATH"],
    );
    let external = sandbox.root.join("外部资料目录");
    fs::create_dir(&external).unwrap();
    fs::copy(
        fixture.join("sources/材料甲.md"),
        external.join("材料甲.md"),
    )
    .unwrap();
    let source_link = fixture.join("linked-sources");
    sandbox.junction(&source_link, &external);
    edit_json(&fixture, "case.json", |case| {
        case["sources"][0]["relative_path"] = "linked-sources/材料甲.md".into()
    });
    input_rejection_with_codes(
        sandbox.verify_preserving(&fixture),
        "source-junction",
        &["PROJECT_UNSAFE_PATH"],
    );
}

fn reported_rule(report: &Value, rule: &str, name: &str) {
    assert!(
        report["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|issue| issue["rule_id"] == rule),
        "{name}: missing precise rule {rule}: {report}"
    );
}

#[test]
fn review_regression_full_reviewed_evidence_cannot_shrink_to_one_utf8_character() {
    let sandbox = Sandbox::new();
    let fixture = sandbox.mutated("review-evidence-shrunk-to-one-character", |root| {
        edit_ir(root, |ir| {
            let evidence = ir["evidence"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|entry| entry["evidence_id"] == "E5a")
                .unwrap();
            evidence["excerpt_end_byte"] = 3.into();
            evidence["excerpt_sha256"] = sha256("仅".as_bytes()).into();
        });
    });
    let report = failed_check(
        sandbox.verify_preserving(&fixture),
        "expected",
        "review-evidence-shrunk-to-one-character",
    );
    // This is a valid UTF-8 source slice with its real SHA; only the independently
    // reviewed range detects substituting a nonknowledge prefix for the full evidence.
    assert_eq!(report["checks"]["knowledge"]["status"], "passed");
    reported_rule(
        &report,
        "EXPECTED_EVIDENCE",
        "review-evidence-shrunk-to-one-character",
    );
}

#[test]
fn review_regression_required_atoms_inside_html_or_hidden_comments_are_not_displayed() {
    let sandbox = Sandbox::new();
    for scope in ["paragraph", "list-item", "table-cell"] {
        for wrapper in ["html-hidden", "html-comment", "obsidian-comment"] {
            let name = format!("review-hidden-{scope}-{wrapper}");
            let fixture =
                sandbox.mutated(&name, |root| {
                    edit_json(root, "note-plan.json", |plan| {
                        let text = match scope {
                            "paragraph" => {
                                &mut plan["notes"][0]["sections"][0]["ordered_nodes"][0]["text"]
                            }
                            "list-item" => {
                                &mut plan["notes"][1]["sections"][0]["ordered_nodes"][1]["items"][0]
                                    ["text"]
                            }
                            "table-cell" => {
                                &mut plan["notes"][0]["sections"][0]["ordered_nodes"][1]["rows"][0]
                                    ["cells"][1]["text"]
                            }
                            _ => unreachable!(),
                        };
                        let original = text.as_str().unwrap();
                        *text = match wrapper {
                            "html-hidden" => format!("<div hidden>{original}</div>"),
                            "html-comment" => format!("<!--{original}-->"),
                            "obsidian-comment" => format!("%%{original}%%"),
                            _ => unreachable!(),
                        }
                        .into();
                    })
                });
            let report = failed_check(sandbox.verify_preserving(&fixture), "note_plan", &name);
            reported_rule(&report, "PLAN_TEXT_SYNTAX", &name);
        }
    }
}

#[test]
fn review_regression_raw_wiki_and_html_in_all_display_fields_and_aliases_are_rejected() {
    let sandbox = Sandbox::new();
    let ordinary = sandbox.mutated("review-ordinary-math-remains-plaintext", |root| {
        edit_json(root, "note-plan.json", |plan| {
            let text = &mut plan["notes"][0]["sections"][0]["ordered_nodes"][0]["text"];
            *text = format!("{} 数学明文 x < 2 且 y > 1。", text.as_str().unwrap()).into();
        });
    });
    assert_eq!(
        success_json(sandbox.verify_preserving(&ordinary))["passed"],
        true,
        "ordinary mathematical comparisons are visible plaintext, not HTML"
    );
    for scope in [
        "paragraph",
        "list-item",
        "table-cell",
        "row-label",
        "column-label",
        "missing-cell",
        "semantic-alias",
        "concept-alias",
    ] {
        for suffix in ["[[ghost]]", "<span hidden>ghost</span>"] {
            let name = format!("review-raw-{scope}-{}", sha256(suffix.as_bytes()));
            let fixture = sandbox.mutated(&name, |root| {
                if scope == "concept-alias" {
                    edit_ir(root, |ir| {
                        let alias = &mut ir["concepts"][1]["aliases"][0];
                        *alias = format!("{} {suffix}", alias.as_str().unwrap()).into();
                    });
                } else {
                    edit_json(root, "note-plan.json", |plan| {
                        let text = match scope {
                            "paragraph" => {
                                &mut plan["notes"][0]["sections"][0]["ordered_nodes"][0]["text"]
                            }
                            "list-item" => {
                                &mut plan["notes"][1]["sections"][0]["ordered_nodes"][1]["items"][0]
                                    ["text"]
                            }
                            "table-cell" => {
                                &mut plan["notes"][0]["sections"][0]["ordered_nodes"][1]["rows"][0]
                                    ["cells"][1]["text"]
                            }
                            "row-label" => {
                                &mut plan["notes"][0]["sections"][0]["ordered_nodes"][1]["rows"][0]
                                    ["cells"][0]["text"]
                            }
                            "column-label" => {
                                &mut plan["notes"][0]["sections"][0]["ordered_nodes"][1]["columns"]
                                    [0]["label"]
                            }
                            "missing-cell" => {
                                &mut plan["notes"][0]["sections"][0]["ordered_nodes"][1]["rows"][1]
                                    ["cells"][1]["text"]
                            }
                            "semantic-alias" => {
                                &mut plan["notes"][0]["sections"][0]["ordered_nodes"][0]["links"][0]
                                    ["display_text"]
                            }
                            _ => unreachable!(),
                        };
                        *text = format!("{} {suffix}", text.as_str().unwrap()).into();
                    });
                }
            });
            let check = if scope == "concept-alias" {
                "knowledge"
            } else {
                "note_plan"
            };
            let report = failed_check(sandbox.verify_preserving(&fixture), check, &name);
            if scope != "concept-alias" {
                reported_rule(&report, "PLAN_TEXT_SYNTAX", &name);
            }
        }
    }
}

#[test]
fn review_regression_definition_moved_away_cannot_leave_a_canonical_value_only_section() {
    let sandbox = Sandbox::new();
    let fixture = sandbox.mutated("review-canonical-definition-moved-to-other-note", |root| {
        edit_json(root, "note-plan.json", |plan| {
            let definition = plan["notes"][1]["sections"][0]["ordered_nodes"]
                .as_array_mut()
                .unwrap()
                .remove(0);
            assert_eq!(definition["node_id"], "Q6z");
            plan["notes"][0]["sections"][0]["ordered_nodes"]
                .as_array_mut()
                .unwrap()
                .push(definition);
            assert_eq!(
                plan["notes"][1]["sections"][0]["ordered_nodes"][0]["node_id"],
                "L9x"
            );
        });
    });
    let report = failed_check(
        sandbox.verify_preserving(&fixture),
        "note_plan",
        "review-canonical-definition-moved-to-other-note",
    );
    reported_rule(
        &report,
        "PLAN_CANONICAL",
        "review-canonical-definition-moved-to-other-note",
    );
    reported_rule(
        &report,
        "EXPECTED_CANONICAL",
        "review-canonical-definition-moved-to-other-note",
    );
    assert!(
        report["resolved_targets"].as_array().unwrap().is_empty(),
        "invalid canonical plan must not report successful targets"
    );
}

#[test]
fn review_regression_unplanned_optional_atoms_still_require_plain_text() {
    let sandbox = Sandbox::new();
    for (name, content, qualifiers, invalid) in [
        (
            "optional-html-content",
            "<script>alert(1)</script>",
            vec![],
            true,
        ),
        (
            "optional-hidden-condition",
            "optional text",
            vec!["<!--隐藏条件-->"],
            true,
        ),
        (
            "optional-math-text",
            "x < 2 且 y > 1。",
            vec!["区间 [1,2]"],
            false,
        ),
    ] {
        let fixture = sandbox.mutated(name, |root| {
            edit_ir(root, |ir| {
                let mut atom = ir["atoms"][0].clone();
                atom["atom_id"] = "Optional1".into();
                atom["required"] = false.into();
                atom["verification"] = "unverified".into();
                atom["value_status"] = "unknown".into();
                atom["content"] = content.into();
                atom["qualifiers"] = serde_json::json!(qualifiers);
                ir["atoms"].as_array_mut().unwrap().push(atom);
            });
        });
        let output = sandbox.verify_preserving(&fixture);
        if invalid {
            let report = failed_check(output, "knowledge", name);
            reported_rule(&report, "KNOWLEDGE_ATOM_CONTENT", name);
        } else {
            assert_eq!(
                output.status.code(),
                Some(0),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let report: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(report["passed"], true);
        }
    }
}
