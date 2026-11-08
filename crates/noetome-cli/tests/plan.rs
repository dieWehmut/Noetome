//! Integration tests for `noetome plan`.
//!
//! Every test drives the real binary and reads the disk. Nothing here imports
//! the planning logic, so a passing test means the observable plan changed, not
//! that an internal helper still agrees with itself.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_CASE: AtomicU64 = AtomicU64::new(1);

fn fixture() -> PathBuf {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/adaptive-plan-v1");
    fs::canonicalize(&path).expect("fixture package")
}

fn sandbox(label: &str) -> PathBuf {
    let serial = NEXT_CASE.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!(
        "noetome-plan-{label}-{}-{serial}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("sandbox root");
    fs::canonicalize(&path).expect("canonical sandbox")
}

fn arg(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn cli(args: &[String]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_noetome"))
        .args(args)
        .output()
        .expect("run noetome")
}

fn text(output: &Output) -> String {
    let mut combined = String::from_utf8_lossy(&output.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    combined
}

fn tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut found = BTreeMap::new();
    let mut queue = vec![root.to_path_buf()];
    while let Some(current) = queue.pop() {
        let Ok(entries) = fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(metadata) = fs::symlink_metadata(&path) else {
                continue;
            };
            if metadata.is_dir() {
                queue.push(path);
            } else if metadata.is_file() {
                let relative = path
                    .strip_prefix(root)
                    .expect("inside root")
                    .to_string_lossy()
                    .replace('\\', "/");
                found.insert(relative, fs::read(&path).expect("read file"));
            }
        }
    }
    found
}

fn adopt_file(project: &str, root: &Path) -> Output {
    cli(&[
        "plan".into(),
        "adopt".into(),
        arg(&fixture().join(project)),
        arg(root),
    ])
}

fn adopt(root: &Path) -> Output {
    adopt_file("project.json", root)
}

fn build(root: &Path, output: &Path) -> Output {
    cli(&["plan".into(), "build".into(), arg(root), arg(output)])
}

fn json(path: &Path) -> serde_json::Value {
    serde_json::from_slice(&fs::read(path).expect("read json")).expect("parse json")
}

fn prior() -> serde_json::Value {
    json(&fixture().join("expected.json"))
}

fn names(path: &Path) -> Vec<String> {
    let mut found: Vec<String> = fs::read_dir(path)
        .expect("list directory")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    found.sort();
    found
}

#[test]
fn adopt_freezes_the_declaration_and_only_writes_control() {
    let root = sandbox("adopt").join("root");
    let output = adopt(&root);
    assert!(output.status.success(), "{}", text(&output));
    assert_eq!(names(&root), vec!["control".to_string()]);
    assert_eq!(
        names(&root.join("control")),
        vec![
            "inputs.json".to_string(),
            "manifest.json".to_string(),
            "project.json".to_string()
        ]
    );
    let manifest = json(&root.join("control").join("manifest.json"));
    assert_eq!(manifest["policy"], "adaptive-plan-v1");
    assert_eq!(manifest["producer"], "noetome-plan");
    // Freezing keeps the declaration byte for byte: a planner must plan exactly
    // what the user declared, not a normalised rewrite of it.
    assert_eq!(
        fs::read(root.join("control").join("project.json")).expect("read copy"),
        fs::read(fixture().join("project.json")).expect("read source")
    );
}

#[test]
fn adopt_refuses_an_existing_root_without_touching_it() {
    let sandbox = sandbox("adopt-existing");
    let root = sandbox.join("root");
    fs::create_dir_all(&root).expect("pre-create root");
    fs::write(root.join("keep.txt"), "keep").expect("seed file");
    let before = tree(&root);
    let output = adopt(&root);
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(2));
    assert!(text(&output).contains("PLAN_ARGUMENT"), "{}", text(&output));
    assert_eq!(tree(&root), before);
}

#[test]
fn adopt_refuses_a_missing_parent_and_leaves_nothing_behind() {
    let sandbox = sandbox("adopt-parent");
    let root = sandbox.join("no-parent").join("root");
    let output = adopt(&root);
    assert!(!output.status.success());
    assert!(!root.exists(), "a refused adopt must not create a root");
}

#[test]
fn a_prerequisite_cycle_is_refused_before_the_root_exists() {
    let root = sandbox("cycle").join("root");
    let output = adopt_file("project-cycle.json", &root);
    assert_eq!(output.status.code(), Some(3));
    assert!(
        text(&output).contains("PLAN_PREREQUISITE_CYCLE"),
        "{}",
        text(&output)
    );
    assert!(!root.exists());
}

#[test]
fn a_chain_split_without_a_reason_is_refused() {
    let root = sandbox("split").join("root");
    let output = adopt_file("project-split.json", &root);
    assert_eq!(output.status.code(), Some(3));
    let message = text(&output);
    assert!(message.contains("PLAN_CHAIN_SPLIT"), "{message}");
    assert!(message.contains("01-goal-glycolysis-main.md"), "{message}");
    assert!(
        message.contains("02-goal-glycolysis-energy.md"),
        "{message}"
    );
    assert!(!root.exists());
}

#[test]
fn a_concept_without_an_atom_in_the_primary_goal_is_refused() {
    let root = sandbox("unplaced").join("root");
    let output = adopt_file("project-unplaced.json", &root);
    assert_eq!(output.status.code(), Some(3));
    assert!(text(&output).contains("PLAN_UNPLACED"), "{}", text(&output));
    assert!(!root.exists());
}

#[test]
fn an_atom_cannot_be_moved_into_another_topic() {
    let root = sandbox("argument").join("root");
    let output = adopt_file("project-argument.json", &root);
    assert_eq!(output.status.code(), Some(2));
    let message = text(&output);
    assert!(message.contains("PLAN_ARGUMENT"), "{message}");
    assert!(message.contains("跨主题只能建立链接"), "{message}");
    assert!(!root.exists());
}

#[test]
fn build_is_deterministic_and_matches_the_frozen_expectation() {
    let sandbox = sandbox("build");
    let root = sandbox.join("root");
    assert!(adopt(&root).status.success());
    let first = sandbox.join("out-1");
    let second = sandbox.join("out-2");
    let output = build(&root, &first);
    assert!(output.status.success(), "{}", text(&output));
    assert!(build(&root, &second).status.success());
    assert_eq!(
        tree(&first),
        tree(&second),
        "two builds must be byte identical"
    );

    let expected = prior();
    let plan = json(&first.join("plan.json"));
    let report = json(&first.join("report.json"));
    assert_eq!(report["counts"], expected["counts"]);
    assert_eq!(plan["teaching_order"], expected["teaching_order"]);
    assert_eq!(plan["folders"], expected["folders"]);
    assert_eq!(plan["notes"], expected["notes"]);
    assert_eq!(plan["atoms"], expected["atoms"]);
    assert_eq!(report["chains"], expected["chains"]);
    assert_eq!(report["model_calls"], 0);
    assert_eq!(report["network_requests"], 0);

    // The output manifest must not carry a timestamp: byte-identical rebuilds
    // are the promise, and a timestamp breaks it.
    let manifest = json(&first.join("manifest.json"));
    assert!(manifest.get("created_at").is_none());
    assert_eq!(manifest["producer"], "adaptive_plan_output");
    assert_eq!(
        names(&first),
        vec![
            "manifest.json".to_string(),
            "plan.json".to_string(),
            "report.json".to_string(),
            "report.md".to_string()
        ]
    );
}

#[test]
fn every_atom_has_exactly_one_position() {
    let sandbox = sandbox("atoms");
    let root = sandbox.join("root");
    assert!(adopt(&root).status.success());
    let out = sandbox.join("out");
    assert!(build(&root, &out).status.success());
    let plan = json(&out.join("plan.json"));
    let atoms = plan["atoms"].as_array().expect("atoms array");
    let expected = prior();
    assert_eq!(
        atoms.len(),
        expected["counts"]["atoms"].as_u64().unwrap() as usize
    );
    let mut seen = BTreeMap::new();
    for atom in atoms {
        let id = atom["atom_id"].as_str().expect("atom id").to_string();
        assert!(
            seen.insert(id.clone(), atom["note_id"].clone()).is_none(),
            "{id} appears twice"
        );
    }
    // Positions must be 1..=n inside every note, in reading order.
    let notes = plan["notes"].as_array().expect("notes array");
    for note in notes {
        let note_id = note["note_id"].as_str().expect("note id");
        let ids: Vec<&str> = note["atom_ids"]
            .as_array()
            .expect("atom_ids")
            .iter()
            .map(|value| value.as_str().expect("atom id"))
            .collect();
        for (index, atom_id) in ids.iter().enumerate() {
            let found = atoms
                .iter()
                .find(|atom| atom["atom_id"] == *atom_id)
                .expect("planned atom");
            assert_eq!(found["note_id"], note_id);
            assert_eq!(found["position"], (index + 1) as u64);
        }
    }
}

#[test]
fn the_secondary_goals_link_to_the_primary_note_instead_of_copying_atoms() {
    let sandbox = sandbox("links");
    let root = sandbox.join("root");
    assert!(adopt(&root).status.success());
    let out = sandbox.join("out");
    assert!(build(&root, &out).status.success());
    let plan = json(&out.join("plan.json"));
    let notes = plan["notes"].as_array().expect("notes array");
    let energy = notes
        .iter()
        .find(|note| note["goal_id"] == "goal-glycolysis-energy")
        .expect("energy note");
    let links = energy["links"].as_array().expect("links");
    assert_eq!(links.len(), 1);
    assert_eq!(
        links[0]["target_note"],
        "代谢/糖酵解/01-goal-glycolysis-main.md"
    );
    assert_eq!(links[0]["display_text"], "糖酵解");
    // The chain atom lives in the energy note; the definition stays in main.
    let main = notes
        .iter()
        .find(|note| note["goal_id"] == "goal-glycolysis-main")
        .expect("main note");
    assert!(
        main["atom_ids"]
            .as_array()
            .expect("atom ids")
            .iter()
            .any(|value| value == "atom-glycolysis-definition")
    );
}

#[test]
fn the_plan_records_the_teaching_order_it_used() {
    let sandbox = sandbox("order");
    let root = sandbox.join("root");
    assert!(adopt(&root).status.success());
    let out = sandbox.join("out");
    assert!(build(&root, &out).status.success());
    let plan = json(&out.join("plan.json"));
    let order: Vec<&str> = plan["teaching_order"]
        .as_array()
        .expect("order array")
        .iter()
        .map(|entry| entry["concept_id"].as_str().expect("concept id"))
        .collect();
    assert_eq!(
        order,
        vec![
            "concept-glycolysis",
            "concept-pfk",
            "concept-pyruvate",
            "concept-gluconeogenesis"
        ]
    );
}

#[test]
fn the_markdown_shows_the_order_the_chains_and_the_boundaries() {
    let sandbox = sandbox("markdown");
    let root = sandbox.join("root");
    assert!(adopt(&root).status.success());
    let out = sandbox.join("out");
    assert!(build(&root, &out).status.success());
    let markdown = fs::read_to_string(out.join("report.md")).expect("read markdown");
    for needle in [
        "# 教材式自适应规划（plan-metabolism-v1）",
        "## 教学顺序",
        "## 阅读目标与位置",
        "## 反应链",
        "## 提醒",
        "## 边界",
        "chain_too_long",
        "goal_without_atoms",
        "模型调用与网络请求",
    ] {
        assert!(markdown.contains(needle), "missing {needle}");
    }
}

#[test]
fn build_refuses_an_existing_output_root() {
    let sandbox = sandbox("output-existing");
    let root = sandbox.join("root");
    assert!(adopt(&root).status.success());
    let out = sandbox.join("out");
    fs::create_dir_all(&out).expect("pre-create output");
    let output = build(&root, &out);
    assert_eq!(output.status.code(), Some(2));
    assert!(text(&output).contains("PLAN_ARGUMENT"), "{}", text(&output));
    assert!(names(&out).is_empty());
}

#[test]
fn build_refuses_an_output_inside_the_frozen_root() {
    let sandbox = sandbox("output-inside");
    let root = sandbox.join("root");
    assert!(adopt(&root).status.success());
    let before = tree(&root);
    let output = build(&root, &root.join("out"));
    assert!(!output.status.success());
    assert!(text(&output).contains("PLAN_ARGUMENT"), "{}", text(&output));
    assert_eq!(tree(&root), before);
}

#[test]
fn a_changed_declaration_is_detected_before_any_report_is_written() {
    let sandbox = sandbox("tamper");
    let root = sandbox.join("root");
    assert!(adopt(&root).status.success());
    let declaration = root.join("control").join("project.json");
    let mut bytes = fs::read(&declaration).expect("read declaration");
    bytes.push(b'\n');
    fs::write(&declaration, bytes).expect("tamper");
    let out = sandbox.join("out");
    let output = build(&root, &out);
    assert_eq!(output.status.code(), Some(3));
    assert!(
        text(&output).contains("PLAN_INTEGRITY"),
        "{}",
        text(&output)
    );
    assert!(!out.exists());
}

#[test]
fn an_extra_entry_in_the_control_directory_is_refused() {
    let sandbox = sandbox("extra");
    let root = sandbox.join("root");
    assert!(adopt(&root).status.success());
    fs::write(root.join("control").join("note.txt"), "extra").expect("seed extra entry");
    let output = cli(&["plan".into(), "status".into(), arg(&root)]);
    assert_eq!(output.status.code(), Some(3));
    assert!(
        text(&output).contains("PLAN_INTEGRITY"),
        "{}",
        text(&output)
    );
}

#[test]
fn an_unknown_root_is_reported_as_such_and_left_alone() {
    let sandbox = sandbox("unknown");
    let root = sandbox.join("foreign");
    fs::create_dir_all(&root).expect("create foreign root");
    let before = tree(&root);
    let output = cli(&["plan".into(), "status".into(), arg(&root)]);
    assert_eq!(output.status.code(), Some(3));
    assert!(
        text(&output).contains("PLAN_UNKNOWN_ROOT"),
        "{}",
        text(&output)
    );
    assert_eq!(tree(&root), before);
}

#[test]
fn status_and_list_report_the_same_plan_without_writing() {
    let sandbox = sandbox("readonly");
    let root = sandbox.join("root");
    assert!(adopt(&root).status.success());
    let before = tree(&root);
    let status = cli(&["plan".into(), "status".into(), arg(&root)]);
    assert!(status.status.success(), "{}", text(&status));
    let status_text = text(&status);
    for needle in [
        "已建立",
        "6 份笔记、2 个文件夹",
        "已安置原子 8/8",
        "教学顺序：糖酵解 → 磷酸果糖激酶 → 丙酮酸 → 糖异生",
    ] {
        assert!(status_text.contains(needle), "missing {needle}");
    }
    let listing = cli(&["plan".into(), "list".into(), arg(&root)]);
    assert!(listing.status.success(), "{}", text(&listing));
    let parsed: serde_json::Value = serde_json::from_slice(&listing.stdout).expect("parse listing");
    assert_eq!(parsed["counts"], prior()["counts"]);
    assert_eq!(tree(&root), before, "reading a plan must not write it");
}

#[test]
fn help_and_invalid_arguments_are_reported() {
    let help = cli(&["plan".into(), "--help".into()]);
    assert!(help.status.success());
    let text_value = String::from_utf8_lossy(&help.stdout).into_owned();
    assert!(text_value.contains("教材式自适应规划"));
    assert!(text_value.contains("拆分理由"));

    let empty = cli(&["plan".into()]);
    assert!(empty.status.success());

    let bad = cli(&["plan".into(), "adopt".into(), "a".into()]);
    assert_eq!(bad.status.code(), Some(2));
    assert!(
        text(&bad).contains("CLI_INVALID_ARGUMENT"),
        "{}",
        text(&bad)
    );

    let unknown = cli(&["plan".into(), "explode".into(), "a".into()]);
    assert_eq!(unknown.status.code(), Some(2));
}
