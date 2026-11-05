//! Real CLI project/import setup from frozen independent process authors.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};
pub(crate) fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}
pub(crate) fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub(crate) fn read(path: impl AsRef<Path>) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
pub(crate) fn write(path: impl AsRef<Path>, value: &Value) {
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}
pub(crate) fn snapshot(path: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut result = BTreeMap::new();
    fn gather(p: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        assert!(!p.is_symlink());
        if p.is_file() {
            out.insert(p.to_owned(), fs::read(p).unwrap());
        } else {
            for entry in fs::read_dir(p).unwrap() {
                gather(&entry.unwrap().path(), out);
            }
        }
    }
    gather(path, &mut result);
    result
}
pub(crate) fn assert_snapshot(saved: &BTreeMap<PathBuf, Vec<u8>>) {
    for (path, bytes) in saved {
        assert_eq!(
            &fs::read(path).unwrap(),
            bytes,
            "protected {}",
            path.display()
        );
    }
}
pub(crate) struct ProcessCase {
    pub(crate) root: PathBuf,
    pub(crate) project: PathBuf,
    pub(crate) case: PathBuf,
    pub(crate) review: PathBuf,
    pub(crate) profile: PathBuf,
    pub(crate) output: PathBuf,
    pub(crate) expected: Value,
}
impl ProcessCase {
    pub(crate) fn prepare(name: &str) -> Self {
        let fixture = repo().join("fixtures/process-flow-v1");
        let frozen = read(fixture.join("frozen-sha256.json"));
        for (path, pin) in frozen["files"].as_object().unwrap() {
            let bytes = fs::read(fixture.join(path)).unwrap();
            assert_eq!(hash(&bytes), pin["sha256"].as_str().unwrap());
            assert_eq!(bytes.len() as u64, pin["byte_length"].as_u64().unwrap());
        }
        let authors = read(fixture.join("authors.json"));
        let author = authors["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == name)
            .unwrap();
        let parent = repo().join("target/compiler-temp");
        fs::create_dir_all(&parent).unwrap();
        let root = parent.join(format!("process-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let value = Self {
            project: root.join("project"),
            case: root.join("case"),
            review: root.join("review"),
            profile: root.join("profile.json"),
            output: root.join("test-Vault"),
            expected: read(fixture.join(author["expected_file"].as_str().unwrap())),
            root,
        };
        println!("process_case={} author={name}", value.root.display());
        fs::create_dir(&value.case).unwrap();
        fs::copy(
            fixture.join(author["source_file"].as_str().unwrap()),
            value.root.join("source.txt"),
        )
        .unwrap();
        fs::copy(
            fixture.join(author["gold_file"].as_str().unwrap()),
            value.case.join("gold.json"),
        )
        .unwrap();
        fs::copy(
            fixture.join(author["profile_file"].as_str().unwrap()),
            &value.profile,
        )
        .unwrap();
        write(value.root.join("expected.json"), &value.expected);
        write(value.root.join("author-pins.json"), &frozen);
        let init = value.command(
            "init",
            &["project", "init", value.project.to_str().unwrap()],
        );
        assert!(
            init.status.success(),
            "{}",
            String::from_utf8_lossy(&init.stderr)
        );
        let imported = value.command(
            "import",
            &[
                "source",
                "import",
                value.project.to_str().unwrap(),
                value.root.join("source.txt").to_str().unwrap(),
            ],
        );
        assert!(
            imported.status.success(),
            "{}",
            String::from_utf8_lossy(&imported.stderr)
        );
        let imported: Value = serde_json::from_slice(&imported.stdout).unwrap();
        write(
            value.case.join("process-review.json"),
            &json!({"schema_version":1,"purpose":"process_source_review","project_id":read(value.project.join("project.json"))["project_id"],"import_id":imported["import_id"],"selection":[{"kind":"text","ordinal":0}],"gold_file":"gold.json","gold_sha256":hash(&fs::read(value.case.join("gold.json")).unwrap())}),
        );
        value
    }
    pub(crate) fn command(&self, name: &str, args: &[&str]) -> Output {
        let mut label = name.to_owned();
        let mut n = 1;
        while self.root.join(format!("{label}-stdout.txt")).exists() {
            n += 1;
            label = format!("{name}-{n}");
        }
        let cli = option_env!("CARGO_BIN_EXE_noetome")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                repo().join(if cfg!(windows) {
                    "target/debug/noetome.exe"
                } else {
                    "target/debug/noetome"
                })
            });
        let result = Command::new(cli).args(args).output().unwrap();
        write(self.root.join(format!("{label}-argv.json")), &json!(args));
        fs::write(
            self.root.join(format!("{label}-stdout.txt")),
            &result.stdout,
        )
        .unwrap();
        fs::write(
            self.root.join(format!("{label}-stderr.txt")),
            &result.stderr,
        )
        .unwrap();
        fs::write(
            self.root.join(format!("{label}-exit.txt")),
            result.status.code().unwrap().to_string(),
        )
        .unwrap();
        result
    }
    pub(crate) fn review(&self) -> Output {
        self.command(
            "review",
            &[
                "knowledge",
                "process-review",
                self.project.to_str().unwrap(),
                self.case.to_str().unwrap(),
                self.review.to_str().unwrap(),
            ],
        )
    }
    pub(crate) fn flow(&self) -> Output {
        self.flow_to(&self.output)
    }
    pub(crate) fn flow_to(&self, path: &Path) -> Output {
        self.command(
            "flow",
            &[
                "knowledge",
                "flow",
                self.project.to_str().unwrap(),
                self.case.to_str().unwrap(),
                self.review.to_str().unwrap(),
                self.profile.to_str().unwrap(),
                path.to_str().unwrap(),
            ],
        )
    }
    pub(crate) fn ready(name: &str) -> Self {
        let value = Self::prepare(name);
        let result = value.review();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        value
    }
    pub(crate) fn protected(&self) -> BTreeMap<PathBuf, Vec<u8>> {
        let mut files = snapshot(&self.project);
        files.extend(snapshot(&self.case));
        files.insert(self.profile.clone(), fs::read(&self.profile).unwrap());
        if self.review.exists() {
            files.extend(snapshot(&self.review));
        }
        files
    }
}
