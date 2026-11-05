//! Ordinary CLI behavior and independent frozen formula-source protection.
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
            out.insert(path.clone(), fs::read(path).unwrap());
        }
    }
    out
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
    expected: Value,
}
impl Case {
    fn new(name: &str) -> Self {
        let fixture = repo().join("fixtures/formula-fidelity");
        let authors = read(fixture.join("authors.json"));
        let a = authors["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["name"] == name)
            .unwrap();
        let parent = repo().join("target/compiler-temp");
        fs::create_dir_all(&parent).unwrap();
        let root = parent.join(format!("formula-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let case = root.join("选择公式 source");
        fs::create_dir(&case).unwrap();
        for e in fs::read_dir(fixture.join(a["case_directory"].as_str().unwrap())).unwrap() {
            let e = e.unwrap();
            fs::copy(e.path(), case.join(e.file_name())).unwrap();
        }
        let profile = root.join("profile.json");
        fs::copy(fixture.join(a["profile_file"].as_str().unwrap()), &profile).unwrap();
        fs::write(root.join("external-sentinel.txt"), b"human content remains").unwrap();
        Self {
            output: root.join("全新测试 Vault"),
            expected: read(fixture.join(a["expected_file"].as_str().unwrap())),
            root,
            case,
            profile,
        }
    }
    fn run(&self) -> Output {
        Command::new(env!("CARGO_BIN_EXE_noetome"))
            .args(["knowledge", "formula"])
            .arg(&self.case)
            .arg(&self.profile)
            .arg(&self.output)
            .output()
            .unwrap()
    }
    fn pin(&self, key: &str, file: &str) {
        let p = self.case.join("case.json");
        let mut v = read(&p);
        v[key]["sha256"] = json!(hash(&fs::read(self.case.join(file)).unwrap()));
        write(p, &v);
    }
    fn reject_before_output(&self) {
        let before = snapshot(&self.root);
        let r = self.run();
        assert!(
            !r.status.success(),
            "{}",
            String::from_utf8_lossy(&r.stdout)
        );
        assert!(
            !self.output.exists(),
            "invalid input created output: {}",
            String::from_utf8_lossy(&r.stderr)
        );
        unchanged(&before);
    }
}

#[test]
fn all_frozen_author_cases_run_through_ordinary_cli_and_preserve_source_bytes() {
    let fixture = repo().join("fixtures/formula-fidelity");
    let frozen = read(fixture.join("frozen-sha256.json"));
    for (p, pin) in frozen["files"].as_object().unwrap() {
        let b = fs::read(fixture.join(p)).unwrap();
        assert_eq!(hash(&b), pin["sha256"]);
        assert_eq!(b.len() as u64, pin["byte_length"].as_u64().unwrap());
    }
    for a in read(fixture.join("authors.json"))["cases"]
        .as_array()
        .unwrap()
    {
        let name = a["name"].as_str().unwrap();
        let c = Case::new(name);
        let before = snapshot(&c.root);
        let r = c.run();
        let ready = !c.expected["representation"].is_null();
        assert_eq!(
            r.status.code(),
            Some(if ready { 0 } else { 3 }),
            "{name}: {}",
            String::from_utf8_lossy(&r.stderr)
        );
        let report = read(c.output.join("formula.json"));
        assert_eq!(
            report["decision"]["representation"], c.expected["representation"],
            "{name}"
        );
        assert_eq!(
            report["decision"]["issue_codes"], c.expected["issue_codes"],
            "{name}"
        );
        for (key, expected) in [
            ("published", json!(false)),
            ("knowledge_status", json!("pending")),
            ("scientific_quality", json!("not_run")),
            ("network_calls", json!(0)),
            ("paid_calls", json!(0)),
        ] {
            assert_eq!(report[key], expected, "{name}/{key}");
        }
        assert_eq!(c.output.join("plan.json").exists(), ready);
        assert_eq!(c.output.join("公式预览.md").exists(), ready);
        let case = read(c.case.join("case.json"));
        for (key, out) in [
            ("tex", "original.tex"),
            ("region", "original.png"),
            ("semantics", "source-semantics.json"),
        ] {
            if !case[key].is_null() {
                let source = case[key]["file"].as_str().unwrap();
                assert_eq!(
                    fs::read(c.case.join(source)).unwrap(),
                    fs::read(c.output.join(out)).unwrap(),
                    "{name}/{key}"
                );
            }
        }
        unchanged(&before);
    }
}

#[test]
fn strict_manifest_semantics_and_profile_reject_missing_null_duplicate_and_unknown_fields() {
    for (file, mode) in [
        ("case.json", "extra"),
        ("case.json", "missing"),
        ("case.json", "duplicate"),
        ("semantics.json", "extra"),
        ("semantics.json", "missing"),
        ("profile.json", "extra"),
        ("profile.json", "missing"),
        ("profile.json", "wrong-type"),
    ] {
        let c = Case::new("native");
        let p = if file == "profile.json" {
            c.profile.clone()
        } else {
            c.case.join(file)
        };
        let mut v = read(&p);
        match mode {
            "extra" => {
                v["unknown"] = json!(true);
                write(&p, &v);
            }
            "missing" => {
                if file == "semantics.json" {
                    v["symbols"][1].as_object_mut().unwrap().remove("unit");
                } else {
                    v.as_object_mut()
                        .unwrap()
                        .remove(if file == "profile.json" {
                            "verified"
                        } else {
                            "tex"
                        });
                }
                write(&p, &v);
            }
            "duplicate" => {
                let text = String::from_utf8(fs::read(&p).unwrap()).unwrap();
                fs::write(&p, text.replacen('{', "{\"schema_version\":1,", 1)).unwrap();
            }
            _ => {
                v["native_math"] = json!("true");
                write(&p, &v);
            }
        }
        if file == "semantics.json" {
            c.pin("semantics", file);
        }
        c.reject_before_output();
    }
}

#[test]
fn attachment_fingerprints_png_contents_and_region_bounds_are_not_trusted_from_headers() {
    for mode in [
        "tex-pin",
        "image-pin",
        "truncated",
        "trailing",
        "crc",
        "bounds",
        "dimensions",
        "traversal",
        "duplicate-file",
        "extra-file",
    ] {
        let c = Case::new("native");
        let p = c.case.join("case.json");
        let mut v = read(&p);
        match mode {
            "tex-pin" => fs::write(c.case.join("original.tex"), b"x=1").unwrap(),
            "image-pin" => fs::write(c.case.join("region.png"), b"not an image").unwrap(),
            "truncated" | "trailing" | "crc" => {
                let image = c.case.join("region.png");
                let mut b = fs::read(&image).unwrap();
                match mode {
                    "truncated" => b.truncate(33),
                    "trailing" => b.extend(b"untrusted trailing payload"),
                    _ => b[29] ^= 1,
                };
                fs::write(image, b).unwrap();
                c.pin("region", "region.png");
            }
            "bounds" => {
                v["region"]["bounds"][0] = json!(999);
                write(p, &v);
            }
            "dimensions" => {
                v["region"]["bounds"][2] = json!(799);
                write(p, &v);
            }
            "traversal" => {
                v["region"]["file"] = json!("../external-sentinel.txt");
                write(p, &v);
            }
            "duplicate-file" => {
                v["tex"] = v["semantics"].clone();
                write(p, &v);
            }
            _ => fs::write(c.case.join("unselected-private.txt"), b"do not read me").unwrap(),
        }
        c.reject_before_output();
    }
}

#[test]
fn malformed_source_and_transcription_cannot_succeed_even_when_synchronized() {
    for expression in [
        "x_",
        "x=",
        "x+",
        "x^{}",
        "x_{i",
        "x__i",
        "\\frac{}{b}",
        "\\frac{a}",
        "\\href{https://example.com}{x}",
    ] {
        let c = Case::new("native");
        fs::write(c.case.join("original.tex"), expression).unwrap();
        let p = c.case.join("semantics.json");
        let mut s = read(&p);
        s["expression"] = json!(expression);
        write(p, &s);
        c.pin("tex", "original.tex");
        c.pin("semantics", "semantics.json");
        let before = snapshot(&c.root);
        let r = c.run();
        assert!(!r.status.success(), "{expression}");
        assert!(!c.output.join("plan.json").exists());
        assert!(!c.output.join("公式预览.md").exists());
        unchanged(&before);
    }
}

#[test]
fn existing_output_collision_preserves_every_existing_file() {
    let c = Case::new("native");
    fs::create_dir(&c.output).unwrap();
    fs::write(c.output.join("human.md"), b"edited user note").unwrap();
    let before = snapshot(&c.root);
    assert!(!c.run().status.success());
    unchanged(&before);
    assert_eq!(snapshot(&c.root), before);
}

#[test]
fn claimed_but_unmeasured_target_versions_cannot_enable_native_math() {
    for (field, value) in [
        ("app_version", "99.99.99"),
        ("mathjax_version", "99.99.99"),
        ("app", "OtherApp"),
    ] {
        let c = Case::new("native");
        let mut v = read(&c.profile);
        v[field] = json!(value);
        write(&c.profile, &v);
        let r = c.run();
        assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
        assert_eq!(
            read(c.output.join("formula.json"))["decision"]["representation"],
            "formula_image"
        );
        assert!(!c.output.join("compiled.tex").exists());
    }
}

#[test]
fn fraction_compatibility_is_decided_from_original_source_not_transcription() {
    for name in ["native", "compatibility"] {
        let c = Case::new(name);
        let p = c.case.join("semantics.json");
        let mut s = read(&p);
        let expression = s["expression"].as_str().unwrap();
        s["expression"] = json!(if name == "native" {
            expression.replace("\\frac", "\\dfrac")
        } else {
            expression.replace("\\dfrac", "\\frac")
        });
        write(p, &s);
        c.pin("semantics", "semantics.json");
        let before = snapshot(&c.root);
        let r = c.run();
        assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
        let report = read(c.output.join("formula.json"));
        assert_eq!(report["decision"]["representation"], "native_math");
        assert_eq!(
            report["decision"]["compatibility_rewrites"],
            if name == "native" {
                json!([])
            } else {
                json!(["FORMULA_DFRAC_COMPATIBILITY"])
            }
        );
        assert!(
            !fs::read_to_string(c.output.join("compiled.tex"))
                .unwrap()
                .contains("\\dfrac")
        );
        unchanged(&before);
    }
}

#[cfg(unix)]
#[test]
fn selected_file_root_and_output_ancestor_symlinks_and_hardlinks_are_rejected() {
    use std::os::unix::fs::symlink;
    for mode in [
        "tex-link",
        "png-hardlink",
        "root-link",
        "output-link",
        "profile-link",
    ] {
        let mut c = Case::new("native");
        match mode {
            "tex-link" => {
                fs::remove_file(c.case.join("original.tex")).unwrap();
                symlink(
                    c.root.join("external-sentinel.txt"),
                    c.case.join("original.tex"),
                )
                .unwrap();
            }
            "png-hardlink" => {
                fs::hard_link(c.case.join("region.png"), c.root.join("outside-image.png")).unwrap();
            }
            "root-link" => {
                let link = c.root.join("case-link");
                symlink(&c.case, &link).unwrap();
                c.case = link;
            }
            "output-link" => {
                let outside = c.root.join("outside");
                fs::create_dir(&outside).unwrap();
                fs::write(outside.join("sentinel.txt"), b"external user file").unwrap();
                let link = c.root.join("output-parent-link");
                symlink(outside, &link).unwrap();
                c.output = link.join("Vault");
            }
            _ => {
                let link = c.root.join("profile-link.json");
                symlink(&c.profile, &link).unwrap();
                c.profile = link;
            }
        }
        let sentinel = fs::read(c.root.join("external-sentinel.txt")).unwrap();
        let r = c.run();
        assert!(!r.status.success(), "{mode}");
        assert!(!c.output.exists(), "{mode}");
        assert_eq!(
            fs::read(c.root.join("external-sentinel.txt")).unwrap(),
            sentinel
        );
    }
}
