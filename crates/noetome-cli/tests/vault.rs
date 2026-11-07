//! B07-T03 端到端集成测试：托管测试集合的人工审批与可恢复发布。
//!
//! 只通过编译出的 CLI 二进制操作真实文件系统；每个用例在独立临时根内，
//! 断言的是磁盘字节与退出码，不锁死人类可读措辞。

use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_CASE: AtomicU64 = AtomicU64::new(0);

fn fixture() -> PathBuf {
    let raw = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/managed-vault-update-v1");
    fs::canonicalize(&raw).expect("resolve the frozen author pack")
}

fn sandbox(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "noetome-vault-{label}-{}-{}",
        std::process::id(),
        NEXT_CASE.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("create exclusive test root");
    root
}

fn arg(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn cli(args: &[String]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_noetome"))
        .args(args)
        .output()
        .expect("execute compiled CLI")
}

fn text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// 递归读出目录树里每个普通文件的字节，路径相对 root。不跟随符号链接。
fn tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, current: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(current).expect("read test-owned directory") {
            let entry = entry.expect("read directory entry");
            let path = entry.path();
            let kind = entry.file_type().expect("read entry type");
            if kind.is_dir() {
                walk(root, &path, out);
            } else if kind.is_file() {
                let relative = path
                    .strip_prefix(root)
                    .expect("entry below root")
                    .to_string_lossy()
                    .replace('\\', "/");
                out.insert(relative, fs::read(&path).expect("read test-owned file"));
            }
        }
    }
    let mut files = BTreeMap::new();
    if root.is_dir() {
        walk(root, root, &mut files);
    }
    files
}

/// 候选目录里的 `operations.json` 是计划输入，不属于托管集合，绝不能进 vault。
fn expected_vault(case: &str) -> BTreeMap<String, Vec<u8>> {
    let mut expected = tree(&fixture().join("baseline"));
    let mut candidate = tree(&fixture().join("candidates").join(case));
    candidate.remove("operations.json");
    expected.extend(candidate);
    expected
}

fn adopt(label: &str) -> PathBuf {
    let root = sandbox(label).join("托管根");
    let output = cli(&[
        "vault".into(),
        "adopt".into(),
        arg(&fixture().join("author.json")),
        arg(&root),
    ]);
    assert!(output.status.success(), "adopt failed: {}", text(&output));
    assert!(
        root.join("control/owner.json").is_file(),
        "owner.json missing"
    );
    root
}

fn plan(adopted: &Path, case: &str) -> (PathBuf, Output) {
    let plan_path = adopted
        .parent()
        .expect("sandbox parent")
        .join(format!("{case}.plan.json"));
    let output = cli(&[
        "vault".into(),
        "plan".into(),
        arg(adopted),
        arg(&fixture().join("candidates").join(case)),
        arg(&plan_path),
    ]);
    (plan_path, output)
}

fn approve(adopted: &Path, plan_path: &Path) -> (PathBuf, Output) {
    let approval = plan_path.with_extension("approval.json");
    let output = cli(&[
        "vault".into(),
        "approve".into(),
        arg(adopted),
        arg(plan_path),
        arg(&approval),
    ]);
    (approval, output)
}

#[test]
fn adopt_freezes_baseline_and_publishes_initial_vault_copy() {
    let root = adopt("adopt");
    let baseline = tree(&fixture().join("baseline"));
    assert!(!baseline.is_empty(), "frozen baseline must not be empty");
    assert_eq!(
        tree(&root.join("baseline")),
        baseline,
        "baseline must equal the frozen author pack"
    );
    assert_eq!(
        tree(&root.join("vault")),
        baseline,
        "initial vault must equal the frozen baseline"
    );
    assert!(root.join("control/vault.sqlite").is_file());
}

#[test]
fn adopt_refuses_to_create_a_root_with_missing_parent_or_existing_root() {
    let base = sandbox("adopt-refuse");
    let missing_parent = base.join("不存在/更深/根");
    let output = cli(&[
        "vault".into(),
        "adopt".into(),
        arg(&fixture().join("author.json")),
        arg(&missing_parent),
    ]);
    assert!(!output.status.success(), "missing parent must be refused");
    assert!(
        !base.join("不存在").exists(),
        "refused adopt must not create a root"
    );

    let existing = sandbox("adopt-existing");
    let output = cli(&[
        "vault".into(),
        "adopt".into(),
        arg(&fixture().join("author.json")),
        arg(&existing),
    ]);
    assert!(
        !output.status.success(),
        "existing directory must be refused: {}",
        text(&output)
    );
    assert_eq!(
        tree(&existing),
        BTreeMap::new(),
        "refused adopt must not write into an existing root"
    );
}

#[test]
fn edit_body_plan_approve_publish_end_to_end() {
    let root = adopt("edit-body");
    let foreign_before = tree(&fixture().join("foreign"));
    let vault_before = tree(&root.join("vault"));

    let (plan_path, output) = plan(&root, "edit-body");
    assert!(output.status.success(), "plan failed: {}", text(&output));
    assert!(plan_path.is_file(), "plan must be written");

    let (approval, output) = approve(&root, &plan_path);
    assert!(output.status.success(), "approve failed: {}", text(&output));
    assert!(approval.is_file());

    let output = cli(&["vault".into(), "publish".into(), arg(&root), arg(&approval)]);
    assert!(output.status.success(), "publish failed: {}", text(&output));
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("release_id="),
        "publish must print a machine-readable release id: {}",
        text(&output)
    );

    let expected = expected_vault("edit-body");
    assert_eq!(
        tree(&root.join("vault")),
        expected,
        "published vault must equal baseline plus the candidate bytes"
    );
    assert_ne!(
        tree(&root.join("vault")),
        vault_before,
        "the edit must actually change the vault"
    );
    assert!(
        !root.join("vault/operations.json").exists(),
        "the plan input must never be published into the vault"
    );
    assert_eq!(
        tree(&fixture().join("foreign")),
        foreign_before,
        "fixture foreign content is untouched"
    );
}

#[test]
fn rename_dangling_is_refused_and_writes_nothing() {
    let root = adopt("rename-dangling");
    let vault_before = tree(&root.join("vault"));
    let (plan_path, output) = plan(&root, "rename-dangling");
    assert!(!output.status.success(), "dangling rename must be refused");
    assert!(
        text(&output).contains("link-would-break"),
        "refusal code missing: {}",
        text(&output)
    );
    assert!(!plan_path.exists(), "refused plan must not be written");
    assert_eq!(tree(&root.join("vault")), vault_before);
}

#[test]
fn foreign_touch_is_refused_with_path_outside_managed_set() {
    let root = adopt("foreign-touch");
    let vault_before = tree(&root.join("vault"));
    let (plan_path, output) = plan(&root, "foreign-touch");
    assert!(!output.status.success(), "foreign path must be refused");
    assert!(
        text(&output).contains("path-outside-managed-set"),
        "refusal code missing: {}",
        text(&output)
    );
    assert!(!plan_path.exists(), "refused plan must not be written");
    assert_eq!(tree(&root.join("vault")), vault_before);
}

#[test]
fn delete_without_explicit_flag_is_refused() {
    let root = adopt("delete-note");
    let vault_before = tree(&root.join("vault"));
    let (plan_path, output) = plan(&root, "delete-note");
    assert!(
        !output.status.success(),
        "delete must need an explicit flag"
    );
    assert!(
        text(&output).contains("delete-requires-explicit-flag"),
        "refusal code missing: {}",
        text(&output)
    );
    assert!(!plan_path.exists());
    assert_eq!(tree(&root.join("vault")), vault_before);
}

#[test]
fn edit_after_approval_invalidates_the_approval() {
    let root = adopt("edit-after-approval");
    let (plan_path, output) = plan(&root, "edit-after-approval");
    assert!(output.status.success(), "plan failed: {}", text(&output));
    let (approval, output) = approve(&root, &plan_path);
    assert!(output.status.success(), "approve failed: {}", text(&output));

    // 批准之后人工再次改写同一笔记：批准必须失效。
    fs::write(root.join("vault/笔记/力学.md"), "人工改写\n").expect("manual edit");
    let vault_before = tree(&root.join("vault"));
    let output = cli(&["vault".into(), "publish".into(), arg(&root), arg(&approval)]);
    assert!(
        !output.status.success(),
        "stale approval must be refused, not published"
    );
    assert!(
        text(&output).contains("approval-stale"),
        "refusal code missing: {}",
        text(&output)
    );
    assert_eq!(
        tree(&root.join("vault")),
        vault_before,
        "stale approval must not overwrite manual bytes"
    );
}

#[test]
fn interrupted_publish_is_recoverable() {
    let root = adopt("interrupt");
    let (plan_path, output) = plan(&root, "edit-body");
    assert!(output.status.success(), "plan failed: {}", text(&output));
    let (approval, output) = approve(&root, &plan_path);
    assert!(output.status.success(), "approve failed: {}", text(&output));

    let mut child = Command::new(env!("CARGO_BIN_EXE_noetome"))
        .args(["vault", "publish", &arg(&root), &arg(&approval)])
        .env("NOETOME_VAULT_TEST_PAUSE_AFTER_PENDING", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn publish for interruption");
    {
        let stdout = child.stdout.take().expect("piped stdout");
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        loop {
            line.clear();
            let read = reader.read_line(&mut line).expect("read publish stdout");
            assert_ne!(
                read, 0,
                "publish ended before reaching the pending boundary"
            );
            if line.trim() == "NOETOME_VAULT_TEST_READY" {
                break;
            }
        }
    }
    child.kill().expect("kill publish at the pending boundary");
    let _ = child.wait();

    let status = cli(&["vault".into(), "status".into(), arg(&root)]);
    assert!(
        text(&status).contains("needs_recovery"),
        "status must report the interrupted release: {}",
        text(&status)
    );
    let expected = expected_vault("edit-body");
    assert_ne!(
        tree(&root.join("vault")),
        expected,
        "the kill must land mid-commit"
    );

    let output = cli(&["vault".into(), "publish".into(), arg(&root), arg(&approval)]);
    assert!(
        output.status.success(),
        "recovery publish failed: {}",
        text(&output)
    );
    assert_eq!(
        tree(&root.join("vault")),
        expected,
        "recovered release must equal baseline plus the approved candidate bytes"
    );
    let status = cli(&["vault".into(), "status".into(), arg(&root)]);
    assert!(
        !text(&status).contains("needs_recovery"),
        "recovery must clear needs_recovery"
    );
}
