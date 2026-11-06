//! Ordinary selected-image CLI behavior, strict inputs, and source protection.
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
fn read(path: impl AsRef<Path>) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn write(path: impl AsRef<Path>, value: &Value) {
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn snapshot(path: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut out = BTreeMap::new();
    for entry in fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        assert!(!path.is_symlink());
        if path.is_dir() {
            out.extend(snapshot(&path));
        } else {
            let bytes = fs::read(&path).unwrap();
            out.insert(path, bytes);
        }
    }
    out
}
fn unchanged(before: &BTreeMap<PathBuf, Vec<u8>>) {
    for (path, bytes) in before {
        assert_eq!(&fs::read(path).unwrap(), bytes, "{}", path.display());
    }
}
struct Case {
    root: PathBuf,
    case: PathBuf,
    profile: PathBuf,
    output: PathBuf,
    expected: Value,
}
impl Case {
    fn new(name: &str) -> Self {
        let fixtures = repo().join("fixtures/image-fidelity");
        let authors = read(fixtures.join("authors.json"));
        let author = authors["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["name"] == name)
            .unwrap();
        let parent = repo().join("target/compiler-temp");
        fs::create_dir_all(&parent).unwrap();
        let root = parent.join(format!("image-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let case = root.join("明确选图 source");
        fs::create_dir(&case).unwrap();
        for entry in
            fs::read_dir(fixtures.join(author["case_directory"].as_str().unwrap())).unwrap()
        {
            let entry = entry.unwrap();
            fs::copy(entry.path(), case.join(entry.file_name())).unwrap();
        }
        let profile = root.join("profile.json");
        fs::copy(
            fixtures.join(author["profile_file"].as_str().unwrap()),
            &profile,
        )
        .unwrap();
        fs::write(root.join("external-sentinel.txt"), b"human content remains").unwrap();
        Self {
            output: root.join("全新测试 Vault"),
            expected: read(fixtures.join(author["expected_file"].as_str().unwrap())),
            root,
            case,
            profile,
        }
    }
    fn run(&self) -> Output {
        Command::new(env!("CARGO_BIN_EXE_noetome"))
            .args(["knowledge", "image"])
            .arg(&self.case)
            .arg(&self.profile)
            .arg(&self.output)
            .output()
            .unwrap()
    }
    fn pin(&self, key: &str, file: &str) {
        let path = self.case.join("case.json");
        let mut value = read(&path);
        value[key]["sha256"] = json!(hash(&fs::read(self.case.join(file)).unwrap()));
        write(path, &value);
    }
    fn reject_before_output(&self) {
        let before = snapshot(&self.root);
        let result = self.run();
        assert!(
            !result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stdout)
        );
        assert!(
            !self.output.exists(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(snapshot(&self.root), before);
    }
    fn blocked(&self) {
        let before = snapshot(&self.root);
        let result = self.run();
        assert_eq!(
            result.status.code(),
            Some(3),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(!self.output.join("plan.json").exists());
        assert!(!self.output.join("图像预览.md").exists());
        assert!(!self.output.join("redrawn.svg").exists());
        assert_eq!(
            fs::read(self.case.join("source.png")).unwrap(),
            fs::read(self.output.join("original.png")).unwrap()
        );
        unchanged(&before);
    }
}

#[test]
fn frozen_author_cases_run_ordinary_cli_with_originals_and_exact_decisions() {
    let fixtures = repo().join("fixtures/image-fidelity");
    let frozen = read(fixtures.join("frozen-sha256.json"));
    for (path, pin) in frozen["files"].as_object().unwrap() {
        let bytes = fs::read(fixtures.join(path)).unwrap();
        assert_eq!(hash(&bytes), pin["sha256"]);
        assert_eq!(bytes.len() as u64, pin["byte_length"].as_u64().unwrap());
    }
    for author in read(fixtures.join("authors.json"))["cases"]
        .as_array()
        .unwrap()
    {
        let name = author["name"].as_str().unwrap();
        let case = Case::new(name);
        let before = snapshot(&case.root);
        let result = case.run();
        let ready = !case.expected["representation"].is_null();
        assert_eq!(
            result.status.code(),
            Some(if ready { 0 } else { 3 }),
            "{name}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
        let report = read(case.output.join("image.json"));
        for key in ["representation", "issue_codes", "fallback_reasons"] {
            assert_eq!(report["decision"][key], case.expected[key], "{name}/{key}");
        }
        for (key, expected) in [
            ("published", json!(false)),
            ("knowledge_status", json!("pending")),
            ("scientific_quality", json!("not_run")),
            ("network_calls", json!(0)),
            ("paid_calls", json!(0)),
        ] {
            assert_eq!(report[key], expected, "{name}/{key}");
        }
        assert_eq!(case.output.join("plan.json").exists(), ready);
        assert_eq!(case.output.join("图像预览.md").exists(), ready);
        assert_eq!(
            case.output.join("redrawn.svg").exists(),
            case.expected["representation"] == "svg_redraw"
        );
        for (source, copy) in [
            ("source.png", "original.png"),
            ("case.json", "source-case.json"),
            ("semantics.json", "source-semantics.json"),
        ] {
            assert_eq!(
                fs::read(case.case.join(source)).unwrap(),
                fs::read(case.output.join(copy)).unwrap(),
                "{name}/{copy}"
            );
        }
        assert_eq!(
            fs::read(&case.profile).unwrap(),
            fs::read(case.output.join("profile.json")).unwrap()
        );
        unchanged(&before);
    }
}

#[test]
fn missing_nullable_duplicate_unknown_and_wrongly_typed_fields_are_rejected() {
    for (file, mode) in [
        ("case.json", "extra"),
        ("case.json", "missing"),
        ("case.json", "duplicate"),
        ("semantics.json", "extra"),
        ("semantics.json", "missing-nullable"),
        ("semantics.json", "duplicate"),
        ("profile.json", "extra"),
        ("profile.json", "missing"),
        ("profile.json", "wrong-type"),
    ] {
        let case = Case::new("signed_arrows");
        let path = if file == "profile.json" {
            case.profile.clone()
        } else {
            case.case.join(file)
        };
        let mut value = read(&path);
        match mode {
            "extra" => {
                value["unexpected"] = json!(true);
                write(&path, &value);
            }
            "missing" => {
                value
                    .as_object_mut()
                    .unwrap()
                    .remove(if file == "profile.json" {
                        "verified"
                    } else {
                        "dimensions"
                    });
                write(&path, &value);
            }
            "missing-nullable" => {
                value["elements"][0].as_object_mut().unwrap().remove("unit");
                write(&path, &value);
            }
            "duplicate" => {
                let text = fs::read_to_string(&path).unwrap();
                fs::write(&path, text.replacen('{', "{\"schema_version\":1,", 1)).unwrap();
            }
            _ => {
                value["local_images"] = json!("true");
                write(&path, &value);
            }
        }
        if file == "semantics.json" {
            case.pin("semantics", file);
        }
        case.reject_before_output();
    }
}

#[test]
fn full_png_decode_exact_selected_inventory_and_geometry_bounds_are_required() {
    for mode in [
        "pin",
        "truncated",
        "trailing",
        "crc",
        "dimensions",
        "bounds",
        "traversal",
        "collision",
        "extra-file",
        "missing-reference",
        "duplicate-id",
    ] {
        let case = Case::new("signed_arrows");
        let manifest = case.case.join("case.json");
        let mut value = read(&manifest);
        match mode {
            "pin" => fs::write(case.case.join("source.png"), b"not a PNG").unwrap(),
            "truncated" | "trailing" | "crc" => {
                let image = case.case.join("source.png");
                let mut bytes = fs::read(&image).unwrap();
                match mode {
                    "truncated" => bytes.truncate(33),
                    "trailing" => bytes.extend(b"trailing hidden payload"),
                    _ => bytes[29] ^= 1,
                };
                fs::write(image, bytes).unwrap();
                case.pin("image", "source.png");
            }
            "dimensions" => {
                value["dimensions"][0] = json!(959);
                write(manifest, &value);
            }
            "traversal" => {
                value["image"]["file"] = json!("../external-sentinel.txt");
                write(manifest, &value);
            }
            "collision" => {
                value["image"] = value["semantics"].clone();
                write(manifest, &value);
            }
            "extra-file" => {
                fs::write(case.case.join("unselected-private.txt"), b"do not read me").unwrap()
            }
            _ => {
                let path = case.case.join("semantics.json");
                let mut semantics = read(&path);
                match mode {
                    "bounds" => semantics["elements"][0]["bbox"][0] = json!(9999),
                    "missing-reference" => semantics["relations"][0]["from"] = json!("missing"),
                    _ => semantics["elements"][1]["id"] = semantics["elements"][0]["id"].clone(),
                }
                write(path, &semantics);
                case.pin("semantics", "semantics.json");
            }
        }
        case.reject_before_output();
    }
}

#[test]
fn contradictory_known_geometry_and_critical_unknowns_block_instead_of_fallback() {
    for mode in [
        "left",
        "endpoint",
        "incomplete",
        "legend-meaning",
        "effect",
        "label",
    ] {
        let case = Case::new("signed_arrows");
        let path = case.case.join("semantics.json");
        let mut semantics = read(&path);
        match mode {
            "left" => {
                semantics["spatial_relations"][0]["a"] = json!("response");
                semantics["spatial_relations"][0]["b"] = json!("feed");
            }
            "endpoint" => semantics["relations"][0]["end"] = json!([480, 210]),
            "incomplete" => semantics["complete"] = json!(false),
            "legend-meaning" => semantics["legend"][0]["meaning"] = Value::Null,
            "effect" => semantics["relations"][0]["effect"] = json!("unknown"),
            _ => semantics["elements"][0]["label"] = Value::Null,
        }
        write(path, &semantics);
        case.pin("semantics", "semantics.json");
        case.blocked();
    }
}

#[test]
fn observation_and_declared_data_availability_cannot_enable_synthetic_curve_redraw() {
    for kind in ["experimental_curve", "observation"] {
        let case = Case::new("curve_no_data");
        let path = case.case.join("case.json");
        let mut manifest = read(&path);
        manifest["source_kind"] = json!(kind);
        write(path, &manifest);
        let path = case.case.join("semantics.json");
        let mut semantics = read(&path);
        semantics["data_available"] = json!(true);
        write(path, &semantics);
        case.pin("semantics", "semantics.json");
        let result = case.run();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(
            read(case.output.join("image.json"))["decision"]["representation"],
            "original_image"
        );
        assert!(!case.output.join("redrawn.svg").exists());
        assert_eq!(
            fs::read(case.case.join("source.png")).unwrap(),
            fs::read(case.output.join("original.png")).unwrap()
        );
    }
}

#[test]
fn self_reported_unmeasured_targets_use_original_image() {
    for (field, value) in [("app", "OtherApp"), ("app_version", "99.99.99")] {
        let case = Case::new("signed_arrows");
        let mut profile = read(&case.profile);
        profile[field] = json!(value);
        write(&case.profile, &profile);
        let result = case.run();
        assert!(result.status.success());
        assert_eq!(
            read(case.output.join("image.json"))["decision"]["representation"],
            "original_image"
        );
        assert!(!case.output.join("redrawn.svg").exists());
    }
}

#[test]
fn existing_output_is_never_adopted_or_overwritten() {
    let case = Case::new("signed_arrows");
    fs::create_dir(&case.output).unwrap();
    fs::write(case.output.join("human.md"), b"edited human content").unwrap();
    let before = snapshot(&case.root);
    assert!(!case.run().status.success());
    assert_eq!(snapshot(&case.root), before);
}

#[cfg(unix)]
#[test]
fn input_symlinks_hardlinks_and_output_ancestor_links_are_rejected() {
    use std::os::unix::fs::symlink;
    for mode in [
        "image-link",
        "image-hardlink",
        "case-link",
        "profile-link",
        "output-link",
    ] {
        let mut case = Case::new("signed_arrows");
        match mode {
            "image-link" => {
                fs::rename(case.case.join("source.png"), case.root.join("held.png")).unwrap();
                symlink(case.root.join("held.png"), case.case.join("source.png")).unwrap();
            }
            "image-hardlink" => {
                fs::hard_link(case.case.join("source.png"), case.root.join("held.png")).unwrap();
            }
            "case-link" => {
                symlink(&case.case, case.root.join("linked-source")).unwrap();
                case.case = case.root.join("linked-source");
            }
            "profile-link" => {
                fs::rename(&case.profile, case.root.join("held-profile.json")).unwrap();
                symlink(case.root.join("held-profile.json"), &case.profile).unwrap();
            }
            _ => {
                fs::create_dir(case.root.join("actual-parent")).unwrap();
                symlink(
                    case.root.join("actual-parent"),
                    case.root.join("linked-parent"),
                )
                .unwrap();
                case.output = case.root.join("linked-parent/new-Vault");
            }
        }
        let result = case.run();
        assert!(!result.status.success());
        assert!(!case.output.exists());
        assert_eq!(
            fs::read(case.root.join("external-sentinel.txt")).unwrap(),
            b"human content remains"
        );
    }
}
