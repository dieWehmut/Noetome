//! Ordinary CLI input/namespace protections. Mathematical OCR is separately verified with the pinned runtime.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};
fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}
fn read(p: impl AsRef<Path>) -> Value {
    serde_json::from_slice(&fs::read(p).unwrap()).unwrap()
}
fn write(p: impl AsRef<Path>, v: &Value) {
    fs::write(p, serde_json::to_vec_pretty(v).unwrap()).unwrap();
}
fn hash(b: &[u8]) -> String {
    format!("{:x}", Sha256::digest(b))
}
fn snapshot(p: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut s = BTreeMap::new();
    for e in fs::read_dir(p).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            s.extend(snapshot(&p));
        } else {
            s.insert(p.clone(), fs::read(p).unwrap());
        }
    }
    s
}
fn unchanged(before: &BTreeMap<PathBuf, Vec<u8>>) {
    for (p, b) in before {
        assert_eq!(&fs::read(p).unwrap(), b, "{}", p.display());
    }
}
struct Case {
    root: PathBuf,
    case: PathBuf,
    profile: PathBuf,
    output: PathBuf,
}
impl Case {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("noetome-scan-cli-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let case = root.join("选定扫描区域");
        fs::create_dir(&case).unwrap();
        for e in fs::read_dir(repo().join("fixtures/scan-formula/cases/clear")).unwrap() {
            let p = e.unwrap().path();
            fs::copy(&p, case.join(p.file_name().unwrap())).unwrap();
        }
        let profile = root.join("profile.json");
        write(
            &profile,
            &json!({"schema_version":1,"app":"Obsidian","app_version":"1.6.7","mathjax_version":"3.2.2","native_math":true,"compatibility_dfrac":true,"local_images":true,"verified":true}),
        );
        fs::write(
            root.join("outside-sentinel.txt"),
            b"human-selected-boundary",
        )
        .unwrap();
        Self {
            output: root.join("new-Vault"),
            root,
            case,
            profile,
        }
    }
    fn run(&self) -> Output {
        Command::new(env!("CARGO_BIN_EXE_noetome"))
            .args(["knowledge", "scan-formula"])
            .arg(&self.case)
            .arg(&self.profile)
            .arg(&self.output)
            .env_remove("NOETOME_SCAN_PYTHON")
            .env_remove("NOETOME_SCAN_MODEL_DIR")
            .output()
            .unwrap()
    }
    fn repin(&self) {
        let mut v = read(self.case.join("case.json"));
        for field in ["pdf", "page", "region", "semantics"] {
            let p = self.case.join(v[field]["file"].as_str().unwrap());
            v[field]["sha256"] = json!(hash(&fs::read(p).unwrap()));
        }
        write(self.case.join("case.json"), &v);
    }
}
impl Drop for Case {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
#[test]
fn help_and_strict_three_path_namespace() {
    let help = Command::new(env!("CARGO_BIN_EXE_noetome"))
        .args(["knowledge", "scan-formula", "--help"])
        .output()
        .unwrap();
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("NOETOME_SCAN_PYTHON"));
    for args in [
        vec!["knowledge", "scan-formula"],
        vec!["knowledge", "scan-formula", "a", "b"],
        vec!["knowledge", "scan-formula", "a", "b", "c", "d"],
        vec!["knowledge", "scan-formula", "--fake", "b", "c"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_noetome"))
            .args(args)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&output.stderr).contains("CLI_INVALID_ARGUMENT"));
    }
}
#[test]
fn missing_selected_runtime_does_not_install_download_or_change_sources() {
    let c = Case::new();
    let before = snapshot(&c.root);
    let output = c.run();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("SCAN_RUNTIME_CONFIG"));
    assert!(!c.output.exists());
    unchanged(&before);
}
#[test]
fn crop_replacement_with_matching_file_sha_still_rejects_actual_page_pixels() {
    let c = Case::new();
    fs::copy(
        repo().join("fixtures/scan-formula/cases/unreadable_sign/region.png"),
        c.case.join("region.png"),
    )
    .unwrap();
    c.repin();
    let before = snapshot(&c.root);
    let output = c.run();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("SCAN_REGION_MISMATCH"));
    assert!(!c.output.exists());
    unchanged(&before);
}
#[test]
fn bounds_inventory_unknown_fields_and_nullable_omission_reject_before_runtime() {
    for mode in 0..4 {
        let c = Case::new();
        match mode {
            0 => {
                let mut v = read(c.case.join("case.json"));
                v["region"]["bounds"] = json!([700, 100, 560, 200]);
                write(c.case.join("case.json"), &v);
            }
            1 => {
                fs::write(c.case.join("manual.txt"), b"human").unwrap();
            }
            2 => {
                let mut v = read(c.case.join("case.json"));
                v["invented"] = json!(true);
                write(c.case.join("case.json"), &v);
            }
            _ => {
                let mut v = read(c.case.join("semantics.json"));
                v["symbols"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("definition");
                write(c.case.join("semantics.json"), &v);
                c.repin();
            }
        }
        let before = snapshot(&c.root);
        let output = c.run();
        assert_eq!(output.status.code(), Some(2));
        assert!(!String::from_utf8_lossy(&output.stderr).contains("SCAN_RUNTIME_CONFIG"));
        assert!(!c.output.exists());
        unchanged(&before);
    }
}
#[test]
fn actual_hardlinked_selected_png_is_rejected_before_runtime() {
    let c = Case::new();
    let selected = c.case.join("region.png");
    fs::hard_link(&selected, c.root.join("outside-linked.png")).unwrap();
    let before = snapshot(&c.root);
    let output = c.run();
    assert_eq!(output.status.code(), Some(2));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("SCAN_RUNTIME_CONFIG"));
    assert!(!c.output.exists());
    unchanged(&before);
}
