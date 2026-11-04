use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};
use uuid::Uuid;
pub(crate) fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}
pub(crate) fn hash(b: &[u8]) -> String {
    format!("{:x}", Sha256::digest(b))
}
pub(crate) const FILES: [&str; 9] = [
    ".owner.json",
    "review-original.json",
    "candidates-original.json",
    "gold-original.json",
    "decision-original.json",
    "source-snapshot.json",
    "knowledge-ir.json",
    "verification.json",
    "verification.md",
];
pub(crate) fn encode(v: &Value) -> Vec<u8> {
    serde_json::to_vec_pretty(v).unwrap()
}
pub(crate) struct ConflictsCase {
    pub(crate) root: PathBuf,
    pub(crate) project: PathBuf,
    pub(crate) case: PathBuf,
    pub(crate) output: PathBuf,
    protected: Vec<(PathBuf, Vec<u8>)>,
}
impl ConflictsCase {
    pub(crate) fn prepare(name: &str) -> Self {
        Self::custom(name, |_, _, _| {})
    }
    pub(crate) fn custom(name: &str, change: impl Fn(&str, &mut String, &mut Value)) -> Self {
        Self::candidate_custom(name, change, |_, _| {})
    }
    pub(crate) fn candidate_custom(
        name: &str,
        change: impl Fn(&str, &mut String, &mut Value),
        change_candidate: impl Fn(&str, &mut Value),
    ) -> Self {
        Self::prepare_variant(name, |_| {}, change, change_candidate)
    }
    pub(crate) fn prepare_variant(
        name: &str,
        change_author: impl Fn(&mut Value),
        change: impl Fn(&str, &mut String, &mut Value),
        change_candidate: impl Fn(&str, &mut Value),
    ) -> Self {
        Self::prepare_with_prefix(name, "", change_author, change, change_candidate)
    }
    pub(crate) fn nonzero_unit(name: &str, change_candidate: impl Fn(&str, &mut Value)) -> Self {
        Self::prepare_with_prefix(
            name,
            "# 独立资料前言 α\n\n",
            |_| {},
            |_, _, _| {},
            change_candidate,
        )
    }
    fn prepare_with_prefix(
        name: &str,
        prefix: &str,
        change_author: impl Fn(&mut Value),
        change: impl Fn(&str, &mut String, &mut Value),
        change_candidate: impl Fn(&str, &mut Value),
    ) -> Self {
        let author_bytes =
            fs::read(repo().join("fixtures/knowledge-coordination-v3/authors.json")).unwrap();
        let authors: Value = serde_json::from_slice(&author_bytes).unwrap();
        let mut authored_case = authors["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == name)
            .unwrap()
            .clone();
        change_author(&mut authored_case);
        let author = &authored_case;
        let same_source = author["same_source"].as_bool().unwrap();
        let parent = repo().join("target/compiler-temp");
        fs::create_dir_all(&parent).unwrap();
        let root = parent.join(format!("conflicts-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let mut f = Self {
            project: root.join("project"),
            case: root.join("case"),
            output: root.join("output"),
            root,
            protected: vec![],
        };
        println!("prepared_conflicts_root={} case={name}", f.root.display());
        let fixture = repo().join("fixtures/knowledge-coordination-v3");
        for entry in fs::read_dir(&fixture).unwrap() {
            let p = entry.unwrap().path();
            f.protected.push((p.clone(), fs::read(p).unwrap()));
        }
        // Author source/gold/expected exist before init/import/candidate calls.
        fs::write(f.root.join("frozen-author.json"), encode(author)).unwrap();
        fs::write(f.root.join("expected.json"), encode(&author["expected"])).unwrap();
        for i in author["inputs"].as_array().unwrap() {
            let id = i["input_id"].as_str().unwrap();
            fs::copy(
                fixture.join(i["source_file"].as_str().unwrap()),
                f.root.join(format!("selected-{id}.txt")),
            )
            .unwrap();
            fs::write(
                f.root.join(format!("frozen-gold-{id}.json")),
                encode(&i["gold"]),
            )
            .unwrap();
        }
        let pins: Value = f
            .protected
            .iter()
            .map(|(p, b)| {
                (
                    p.file_name().unwrap().to_str().unwrap().to_owned(),
                    json!({"sha256":hash(b),"byte_length":b.len()}),
                )
            })
            .collect();
        fs::write(f.root.join("author-pins.json"), encode(&pins)).unwrap();
        assert!(
            f.command("init", &["project", "init", f.project.to_str().unwrap()])
                .status
                .success()
        );
        fs::create_dir(&f.case).unwrap();
        fs::write(f.case.join("identity.json"), encode(&author["identity"])).unwrap();
        let mut inputs: Vec<Value> = vec![];
        for authored in author["inputs"].as_array().unwrap() {
            let id = authored["input_id"].as_str().unwrap();
            let mut source = fs::read_to_string(f.root.join(format!("selected-{id}.txt"))).unwrap();
            let mut gold = authored["gold"].clone();
            change(id, &mut source, &mut gold);
            source = format!("{prefix}{source}");
            // The heading and blank separator occupy ordinals 0 and 1.
            let ordinal = if prefix.is_empty() { 0 } else { 2 };
            let source_path = f.root.join(format!("selected-{id}.txt"));
            fs::write(&source_path, &source).unwrap();
            let gold_bytes = encode(&gold);
            fs::write(f.root.join(format!("frozen-gold-{id}.json")), &gold_bytes).unwrap();
            let seed_id = if !inputs.is_empty() && same_source {
                Some(
                    inputs[0]["binding"]["source_id"]
                        .as_str()
                        .unwrap()
                        .to_owned(),
                )
            } else if !inputs.is_empty() {
                let seed = f.root.join(format!("{id}-unique-seed.txt"));
                fs::write(&seed, format!("独立 {id} 来源登记种子。")).unwrap();
                let r = f.command(
                    &format!("import-{id}-seed"),
                    &[
                        "source",
                        "import",
                        f.project.to_str().unwrap(),
                        seed.to_str().unwrap(),
                    ],
                );
                assert!(r.status.success());
                let v: Value = serde_json::from_slice(&r.stdout).unwrap();
                Some(v["version"]["source_id"].as_str().unwrap().to_owned())
            } else {
                None
            };
            let mut args = vec![
                "source",
                "import",
                f.project.to_str().unwrap(),
                source_path.to_str().unwrap(),
            ];
            if let Some(id) = &seed_id {
                args.extend(["--source-id", id]);
            }
            let run = f.command(&format!("import-{id}"), &args);
            assert!(
                run.status.success(),
                "{}",
                String::from_utf8_lossy(&run.stderr)
            );
            let imported: Value = serde_json::from_slice(&run.stdout).unwrap();
            let project: Value =
                serde_json::from_slice(&fs::read(f.project.join("project.json")).unwrap()).unwrap();
            let version = &imported["version"];
            let binding = json!({"project_id":project["project_id"],"import_id":imported["import_id"],"source_id":version["source_id"],"version_id":version["version_id"],"content_sha256":version["content_sha256"],"normalized_sha256":version["normalized_sha256"],"parse_id":null,"manifest_sha256":null});
            let review_dir = f.root.join(format!("review-{id}"));
            fs::create_dir(&review_dir).unwrap();
            fs::write(review_dir.join("gold.json"), &gold_bytes).unwrap();
            let property = authored["property"].as_str().unwrap();
            let atom_id = authored["original_atom_id"].as_str().unwrap().to_owned();
            let evidence_id = format!("{id}-support");
            let quote = authored["context_quote"].as_str().unwrap();
            let start = source.find(quote).unwrap();
            let mut candidate: Value = serde_json::from_slice(
                &fs::read(repo().join("fixtures/extraction-review-v1/candidates.json")).unwrap(),
            )
            .unwrap();
            candidate["binding"] = binding.clone();
            candidate["payload"]["binding"] = binding.clone();
            let request = json!(format!("synthetic-coordination-{id}"));
            candidate["request_id"] = request.clone();
            candidate["payload"]["request_id"] = request;
            candidate["payload"]["concepts"] = json!([{"concept_id":format!("{id}-subject"),"revision":1,"canonical_name":"样例 A","aliases":[],"needs_full_explanation":true},{"concept_id":format!("{id}-property"),"revision":1,"canonical_name":property,"aliases":[],"needs_full_explanation":true}]);
            candidate["payload"]["evidence"] = json!([{"evidence_id":evidence_id,"unit_id":"unit-0","excerpt_start_byte":start,"excerpt_end_byte":start+quote.len(),"excerpt_sha256":hash(quote.as_bytes()),"quote":quote}]);
            let atom = &mut candidate["payload"]["atoms"][0];
            atom["atom"]["atom_id"] = json!(atom_id);
            atom["atom"]["concept_ids"] =
                json!([format!("{id}-subject"), format!("{id}-property")]);
            atom["atom"]["content"] = gold["atom_content"].clone();
            let mut qualifiers = vec![gold["unit"].clone()];
            qualifiers.extend(gold["conditions"].as_array().unwrap().clone());
            qualifiers.extend(gold["exceptions"].as_array().unwrap().clone());
            atom["atom"]["qualifiers"] = json!(qualifiers);
            atom["atom"]["evidence_links"] =
                json!([{"evidence_id":evidence_id,"relation":"supports"}]);
            atom["measurement"] = json!({"value_status":"known","value":gold["value"],"unit":gold["unit"],"conditions":gold["conditions"]});
            if let Some(counters) = authored["counter_evidence"].as_array() {
                for counter in counters {
                    let quote = counter["quote"].as_str().unwrap();
                    let start = source.find(quote).unwrap();
                    candidate["payload"]["evidence"]
                        .as_array_mut()
                        .unwrap()
                        .push(json!({
                            "evidence_id":counter["evidence_id"],"unit_id":"unit-0",
                            "excerpt_start_byte":start,"excerpt_end_byte":start+quote.len(),
                            "excerpt_sha256":hash(quote.as_bytes()),"quote":quote
                        }));
                    candidate["payload"]["atoms"][0]["atom"]["evidence_links"]
                        .as_array_mut()
                        .unwrap()
                        .push(json!({
                            "evidence_id":counter["evidence_id"],"relation":"refutes"
                        }));
                }
            }
            change_candidate(id, &mut candidate);
            fs::write(review_dir.join("candidates.json"), encode(&candidate)).unwrap();
            fs::write(review_dir.join("decision.json"),encode(&json!({"schema_version":1,"unit_id":"unit-0","disposition":"included","atom_ids":[atom_id],"exclusion_reason":null}))).unwrap();
            fs::write(review_dir.join("review.json"),encode(&json!({"schema_version":1,"purpose":"single_measurement_source_review","selection":[{"kind":"text","ordinal":ordinal}],"candidate_file":"candidates.json","gold_file":"gold.json","gold_sha256":hash(&gold_bytes),"decision_file":"decision.json","subject":"样例 A","property":property,"allowed_contexts":[],"reviewer":"automated_source_review","interpretation_timing":"before_synthetic_response"}))).unwrap();
            let bundle = f.case.join(format!("bundle-{id}"));
            let run = f.command(
                &format!("verify-{id}"),
                &[
                    "extract",
                    "verify",
                    f.project.to_str().unwrap(),
                    imported["import_id"].as_str().unwrap(),
                    review_dir.to_str().unwrap(),
                    bundle.to_str().unwrap(),
                ],
            );
            assert_eq!(
                run.status.code(),
                Some(0),
                "{}",
                String::from_utf8_lossy(&run.stderr)
            );
            let artifacts: Value = FILES
                .iter()
                .map(|n| {
                    let b = fs::read(bundle.join(n)).unwrap();
                    (
                        n.to_string(),
                        json!({"sha256":hash(&b),"byte_length":b.len()}),
                    )
                })
                .collect();
            inputs.push(json!({"input_id":id,"review_bundle":format!("bundle-{id}"),"binding":binding,"artifacts":artifacts}));
        }
        assert_eq!(
            inputs
                .iter()
                .map(|i| i["binding"]["source_id"].as_str().unwrap())
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            if same_source {
                1
            } else {
                author["inputs"].as_array().unwrap().len()
            }
        );
        let project: Value =
            serde_json::from_slice(&fs::read(f.project.join("project.json")).unwrap()).unwrap();
        fs::write(f.case.join("coordination.json"),encode(&json!({"schema_version":3,"purpose":"explicit_identity_conflict_review","project_id":project["project_id"],"identity_file":"identity.json","inputs":inputs}))).unwrap();
        fs::write(f.root.join("external-sentinel.txt"), b"keep human sentinel").unwrap();
        let outside = f.root.parent().unwrap().join(format!(
            "coordination-external-sentinel-{}.txt",
            Uuid::new_v4()
        ));
        fs::write(&outside, b"external protected sentinel").unwrap();
        f.protected
            .push((outside, b"external protected sentinel".to_vec()));
        let cli = option_env!("CARGO_BIN_EXE_noetome")
            .map(PathBuf::from)
            .unwrap_or_else(|| repo().join("target/debug/noetome.exe"));
        fs::write(f.root.join("cli-sha256.txt"), hash(&fs::read(cli).unwrap())).unwrap();
        f.freeze();
        f
    }
    pub(crate) fn freeze(&mut self) {
        fn gather(p: &Path, v: &mut Vec<(PathBuf, Vec<u8>)>) {
            for e in fs::read_dir(p).unwrap() {
                let p = e.unwrap().path();
                if p.is_dir() {
                    gather(&p, v)
                } else {
                    v.push((p.clone(), fs::read(p).unwrap()));
                }
            }
        }
        self.protected
            .retain(|(p, _)| !p.starts_with(&self.case) && !p.starts_with(&self.project));
        gather(&self.case, &mut self.protected);
        gather(&self.project, &mut self.protected);
        for entry in fs::read_dir(&self.root).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy();
            if name.starts_with("selected-")
                || name.starts_with("frozen-")
                || name == "expected.json"
                || name == "author-pins.json"
            {
                self.protected.retain(|(p, _)| p != &path);
                self.protected.push((path.clone(), fs::read(path).unwrap()));
            }
        }
        self.protected.push((
            self.root.join("external-sentinel.txt"),
            b"keep human sentinel".to_vec(),
        ));
        let pins: Value = self
            .protected
            .iter()
            .map(|(p, b)| {
                (
                    p.to_string_lossy().into_owned(),
                    json!({"sha256":hash(b),"byte_length":b.len()}),
                )
            })
            .collect();
        fs::write(self.root.join("protection-before.json"), encode(&pins)).unwrap();
    }
    pub(crate) fn save_cli(&self, name: &str, r: &Output) {
        fs::write(self.root.join(format!("{name}-stdout.txt")), &r.stdout).unwrap();
        fs::write(self.root.join(format!("{name}-stderr.txt")), &r.stderr).unwrap();
        fs::write(
            self.root.join(format!("{name}-exit.txt")),
            r.status.code().unwrap().to_string(),
        )
        .unwrap();
    }
    pub(crate) fn command(&self, name: &str, args: &[&str]) -> Output {
        // Preserve both executions in order/relabeling cases instead of replacing the first receipt.
        let mut label = name.to_owned();
        let mut ordinal = 1;
        while self.root.join(format!("{label}-stdout.txt")).exists() {
            ordinal += 1;
            label = format!("{name}-{ordinal}");
        }
        let r = Command::new(
            option_env!("CARGO_BIN_EXE_noetome")
                .map(PathBuf::from)
                .unwrap_or_else(|| repo().join("target/debug/noetome.exe")),
        )
        .args(args)
        .output()
        .unwrap();
        fs::write(
            self.root.join(format!("{label}-argv.json")),
            encode(&json!(args)),
        )
        .unwrap();
        self.save_cli(&label, &r);
        r
    }
    pub(crate) fn coordinate(&self) -> Output {
        self.command(
            "coordinate",
            &[
                "knowledge",
                "coordinate",
                self.project.to_str().unwrap(),
                self.case.to_str().unwrap(),
                self.output.to_str().unwrap(),
            ],
        )
    }
    pub(crate) fn assert_protected(&self) {
        let mut pins = serde_json::Map::new();
        for (p, b) in &self.protected {
            let actual = fs::read(p).unwrap();
            assert_eq!(actual, *b, "changed {}", p.display());
            pins.insert(
                p.to_string_lossy().into_owned(),
                json!({"sha256":hash(&actual),"byte_length":actual.len()}),
            );
        }
        fs::write(
            self.root.join("protection-after.json"),
            encode(&Value::Object(pins)),
        )
        .unwrap();
    }
}
