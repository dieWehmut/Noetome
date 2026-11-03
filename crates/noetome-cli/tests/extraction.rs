//! Real CLI behavior against author gold; no production validator is the oracle.
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

struct Example {
    root: PathBuf,
    project: PathBuf,
    case: PathBuf,
    output: PathBuf,
    import_id: String,
    imported: Value,
    database: Vec<u8>,
    config: Vec<u8>,
}

impl Example {
    fn new(change: impl FnOnce(&mut Value, &mut Value)) -> Self {
        let parent = repo().join("target/compiler-temp");
        fs::create_dir_all(&parent).unwrap();
        let root = parent.join(format!("extract-test-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let project = root.join("project 中文");
        let run = command(&["project", "init", project.to_str().unwrap()]);
        assert!(
            run.status.success(),
            "{}",
            String::from_utf8_lossy(&run.stderr)
        );
        let source = repo().join("fixtures/extraction-gateway-v1/source.txt");
        let run = command(&[
            "source",
            "import",
            project.to_str().unwrap(),
            source.to_str().unwrap(),
        ]);
        assert!(
            run.status.success(),
            "{}",
            String::from_utf8_lossy(&run.stderr)
        );
        let imported: Value = serde_json::from_slice(&run.stdout).unwrap();
        let case = root.join("fake input");
        fs::create_dir(&case).unwrap();
        let mut manifest: Value = serde_json::from_slice(
            &fs::read(repo().join("fixtures/extraction-gateway-v1/fake.json")).unwrap(),
        )
        .unwrap();
        let mut payload: Value = serde_json::from_slice(
            &fs::read(repo().join("fixtures/extraction-gateway-v1/response.json")).unwrap(),
        )
        .unwrap();
        change(&mut manifest, &mut payload);
        fs::write(
            case.join("fake.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        fs::write(
            case.join("response.json"),
            serde_json::to_vec(&payload).unwrap(),
        )
        .unwrap();
        let database = fs::read(project.join("project.sqlite")).unwrap();
        let config = fs::read(project.join("project.json")).unwrap();
        Self {
            output: root.join("new output"),
            import_id: imported["import_id"].as_str().unwrap().into(),
            root,
            project,
            case,
            imported,
            database,
            config,
        }
    }

    fn args(&self) -> Vec<String> {
        vec![
            "extract".into(),
            "fake".into(),
            self.project.to_str().unwrap().into(),
            self.import_id.clone(),
            self.case.to_str().unwrap().into(),
            self.output.to_str().unwrap().into(),
        ]
    }

    fn run(&self) -> Output {
        Command::new(env!("CARGO_BIN_EXE_noetome"))
            .args(self.args())
            .output()
            .unwrap()
    }

    fn unchanged(&self) {
        assert_eq!(
            fs::read(self.project.join("project.sqlite")).unwrap(),
            self.database
        );
        assert_eq!(
            fs::read(self.project.join("project.json")).unwrap(),
            self.config
        );
        let version = &self.imported["version"];
        let bytes = fs::read(
            self.project
                .join(version["normalized_snapshot"].as_str().unwrap()),
        )
        .unwrap();
        assert_eq!(
            digest(&bytes),
            version["normalized_sha256"].as_str().unwrap()
        );
        for suffix in [
            "project.sqlite-wal",
            "project.sqlite-shm",
            ".noetome-writing",
        ] {
            assert!(
                !self.project.join(suffix).exists(),
                "unexpected writer artifact {suffix}"
            );
        }
    }
}

fn command(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_noetome"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn ordinary_fake_uses_real_source_and_preserves_project() {
    let example = Example::new(|_, _| {});
    let run = example.run();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let request: Value =
        serde_json::from_slice(&fs::read(example.output.join("request.json")).unwrap()).unwrap();
    let result: Value =
        serde_json::from_slice(&fs::read(example.output.join("candidates.json")).unwrap()).unwrap();
    let report: Value =
        serde_json::from_slice(&fs::read(example.output.join("validation.json")).unwrap()).unwrap();
    let gold: Value = serde_json::from_slice(
        &fs::read(repo().join("fixtures/extraction-gateway-v1/expected.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        request["binding"]["version_id"],
        example.imported["version_id"]
    );
    assert_eq!(
        request["units"][0]["locator"]["span_id"],
        example.imported["spans"][0]["span_id"]
    );
    assert_eq!(
        result["payload"]["atoms"][0]["atom"]["content"],
        gold["atom_content"]
    );
    assert_eq!(
        result["payload"]["atoms"][0]["measurement"]["conditions"],
        gold["conditions"]
    );
    assert_eq!(result["payload"]["evidence"][0]["quote"], gold["quote"]);
    assert_eq!(report["published"], false);
    assert_eq!(report["knowledge_status"], "pending");
    assert_eq!(report["usage"]["price_usd"], Value::Null);
    example.unchanged();
}

#[test]
fn ordinary_fake_rejects_chinese_interior_byte_without_candidates() {
    let example =
        Example::new(|_, payload| payload["evidence"][0]["excerpt_start_byte"] = json!(1));
    let run = example.run();
    assert_eq!(
        run.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(!example.output.join("candidates.json").exists());
    let diagnostics =
        String::from_utf8(run.stdout).unwrap() + &String::from_utf8(run.stderr).unwrap();
    assert!(
        diagnostics.contains("EXTRACT_EVIDENCE_RANGE"),
        "{diagnostics}"
    );
    example.unchanged();
}

#[test]
fn ordinary_fake_runtime_cancel_does_not_publish() {
    let example = Example::new(|manifest, _| {
        manifest["delay_ms"] = json!(1000);
        manifest["limits"]["timeout_ms"] = json!(5000);
    });
    let signal = example.root.join("cancel.signal");
    let mut args = example.args();
    args.extend(["--cancel-file".into(), signal.to_str().unwrap().into()]);
    let mut child = Command::new(env!("CARGO_BIN_EXE_noetome"))
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let started = Instant::now();
    while !example.output.join("request.json").exists()
        && started.elapsed() < Duration::from_secs(5)
    {
        if let Some(status) = child.try_wait().unwrap() {
            let run = child.wait_with_output().unwrap();
            panic!(
                "gateway ended before request: {status}; {}",
                String::from_utf8_lossy(&run.stderr)
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        example.output.join("request.json").exists(),
        "bounded request-start wait expired"
    );
    fs::write(&signal, b"stop\n").unwrap();
    let run = child.wait_with_output().unwrap();
    assert_eq!(run.status.code(), Some(3));
    assert!(
        String::from_utf8_lossy(&run.stdout).contains("EXTRACT_CANCELLED")
            || String::from_utf8_lossy(&run.stderr).contains("EXTRACT_CANCELLED")
    );
    assert!(!example.output.join("candidates.json").exists());
    example.unchanged();
}
