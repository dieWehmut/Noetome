//! Offline review orchestration, with held source and four selected input capabilities.
use crate::{
    extraction_model::ExtractionLimits,
    extraction_output::{self, OutputRoot},
    extraction_review::{self, Assessment},
    extraction_review_model::*,
    extraction_source::SourceSession,
    project::{self, ProjectError},
    sidecar_process::Control,
    sidecar_workspace::{Directory, GuardedFile},
    text::sha256,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
    time::SystemTime,
};
const INPUT: &str = "VERIFY_INPUT_CHANGED";
fn err(code: &'static str, exit: u8, msg: impl ToString) -> ProjectError {
    ProjectError::new(code, exit, msg.to_string())
}
fn encoded<T: Serialize>(v: &T) -> Result<Vec<u8>, ProjectError> {
    serde_json::to_vec_pretty(v).map_err(|e| err("VERIFY_ENCODING", 3, e))
}
fn parse<T: DeserializeOwned>(b: &[u8]) -> Result<T, ProjectError> {
    serde_json::from_slice(b).map_err(|e| err("VERIFY_SCHEMA", 2, e))
}
// Count recovery is permitted only after every object key is unambiguous,
// including keys in schema-invalid/unknown nested metadata. Never trust Value's
// last-key-wins representation to establish which candidate array was supplied.
struct UniqueKeys;
impl<'de> Deserialize<'de> for UniqueKeys {
    fn deserialize<D: serde::Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        struct Syntax;
        impl<'de> serde::de::Visitor<'de> for Syntax {
            type Value = UniqueKeys;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("JSON with unique object keys")
            }
            fn visit_bool<E: serde::de::Error>(self, _: bool) -> Result<UniqueKeys, E> {
                Ok(UniqueKeys)
            }
            fn visit_i64<E: serde::de::Error>(self, _: i64) -> Result<UniqueKeys, E> {
                Ok(UniqueKeys)
            }
            fn visit_u64<E: serde::de::Error>(self, _: u64) -> Result<UniqueKeys, E> {
                Ok(UniqueKeys)
            }
            fn visit_f64<E: serde::de::Error>(self, _: f64) -> Result<UniqueKeys, E> {
                Ok(UniqueKeys)
            }
            fn visit_str<E: serde::de::Error>(self, _: &str) -> Result<UniqueKeys, E> {
                Ok(UniqueKeys)
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<UniqueKeys, E> {
                Ok(UniqueKeys)
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut access: A,
            ) -> Result<UniqueKeys, A::Error> {
                while access.next_element::<UniqueKeys>()?.is_some() {}
                Ok(UniqueKeys)
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut access: A,
            ) -> Result<UniqueKeys, A::Error> {
                let mut keys = std::collections::BTreeSet::new();
                while let Some(key) = access.next_key::<String>()? {
                    if !keys.insert(key) {
                        return Err(serde::de::Error::custom("duplicate JSON key"));
                    }
                    access.next_value::<UniqueKeys>()?;
                }
                Ok(UniqueKeys)
            }
        }
        de.deserialize_any(Syntax)
    }
}
pub(crate) fn recover_unambiguous_json(bytes: &[u8]) -> Result<Value, serde_json::Error> {
    serde_json::from_slice::<UniqueKeys>(bytes)?;
    serde_json::from_slice(bytes)
}
struct Input {
    path: PathBuf,
    guard: GuardedFile,
    bytes: Vec<u8>,
    modified: SystemTime,
}
struct Inputs {
    root: PathBuf,
    parent: Directory,
    directory: Directory,
    files: Vec<Input>,
}
impl Inputs {
    fn open(path: &Path, control: &Control) -> Result<Self, ProjectError> {
        let root = project::check_components(path)?;
        let parent = Directory::capture(
            root.parent()
                .ok_or_else(|| err(INPUT, 2, "missing parent"))?,
            INPUT,
        )
        .map_err(|e| err(INPUT, 2, e))?;
        let directory = Directory::capture(&root, INPUT).map_err(|e| err(INPUT, 2, e))?;
        let mut value = Self {
            root,
            parent,
            directory,
            files: vec![],
        };
        value.add("review.json", 65536, control)?;
        Ok(value)
    }
    fn add(&mut self, name: &str, max: u64, control: &Control) -> Result<(), ProjectError> {
        if !crate::fixture_input::valid_relative_path(name)
            || name.contains('/')
            || name.contains('\\')
            || self
                .files
                .iter()
                .any(|f| f.path.file_name().is_some_and(|n| n == name))
        {
            return Err(err(
                "VERIFY_INPUT_PATH",
                2,
                "four distinct ordinary single filenames required",
            ));
        }
        self.verify(control)?;
        let path = self.root.join(name);
        let before = fs::metadata(&path)
            .and_then(|m| m.modified())
            .map_err(|e| err(INPUT, 2, e))?;
        let (guard, bytes) = GuardedFile::open(&path, max, control, INPUT).map_err(|e| {
            err(
                if e.code == "SIDECAR_LIMIT" {
                    "VERIFY_INPUT_LIMIT"
                } else {
                    INPUT
                },
                2,
                e,
            )
        })?;
        let modified = fs::metadata(&path)
            .and_then(|m| m.modified())
            .map_err(|e| err(INPUT, 2, e))?;
        if before != modified {
            return Err(err(INPUT, 3, "input timestamp changed during open"));
        }
        for old in &self.files {
            if same_file::is_same_file(&old.path, &path).map_err(|e| err(INPUT, 2, e))? {
                return Err(err("VERIFY_INPUT_PATH", 2, "duplicate file identity"));
            }
        }
        self.files.push(Input {
            path,
            guard,
            bytes,
            modified,
        });
        self.verify(control)
    }
    fn verify(&self, control: &Control) -> Result<(), ProjectError> {
        self.parent
            .verify(INPUT)
            .and_then(|_| self.directory.verify(INPUT))
            .map_err(|e| err(INPUT, 3, e))?;
        for f in &self.files {
            f.guard
                .verify(control, INPUT)
                .map_err(|e| err(INPUT, 3, e))?;
            if fs::metadata(&f.path)
                .and_then(|m| m.modified())
                .map_err(|e| err(INPUT, 3, e))?
                != f.modified
            {
                return Err(err(INPUT, 3, "input timestamp changed"));
            }
        }
        Ok(())
    }
    fn bytes(&self, index: usize) -> &[u8] {
        &self.files[index].bytes
    }
}
pub struct VerificationOutput {
    pub json: String,
    pub failed: bool,
}
pub fn run(
    project: &Path,
    import_id: &str,
    case: &Path,
    output: &Path,
) -> Result<VerificationOutput, ProjectError> {
    run_inner(project, import_id, case, output)
}
#[cfg(test)]
thread_local! {static TEST_HOOK: std::cell::RefCell<Option<Box<dyn FnMut(&str)>>> = std::cell::RefCell::new(None);}
#[cfg(test)]
fn stage(name: &str) {
    TEST_HOOK.with(|hook| {
        if let Some(hook) = hook.borrow_mut().as_mut() {
            hook(name);
        }
    });
}
fn run_inner(
    project: &Path,
    import_id: &str,
    case: &Path,
    destination: &Path,
) -> Result<VerificationOutput, ProjectError> {
    let cancel = Arc::new(AtomicBool::new(false));
    let control = Control::new(30000, cancel.clone());
    let mut inputs = Inputs::open(case, &control)?;
    let review: Review = parse(inputs.bytes(0))?;
    if review.selection.len() != 1 || !crate::fixture_model::is_sha(&review.gold_sha256) {
        return Err(err(
            "VERIFY_REVIEW",
            2,
            "selection exactly one and exact gold SHA required",
        ));
    }
    for (name, max) in [
        (&review.candidate_file, 1048576),
        (&review.gold_file, 65536),
        (&review.decision_file, 65536),
    ] {
        inputs.add(name, max, &control)?;
    }
    if sha256(inputs.bytes(2)) != review.gold_sha256 {
        return Err(err("VERIFY_GOLD_SHA", 2, "original gold byte SHA differs"));
    }
    let gold: Gold = parse::<GoldInput>(inputs.bytes(2))?.into();
    let decision: Decision = parse(inputs.bytes(3))?;
    if decision.atom_ids.len() > 1024
        || decision.disposition == Disposition::Excluded
            && !decision.exclusion_reason.as_deref().is_some_and(|s| {
                !s.trim().is_empty() && s.len() <= 1024 && crate::fixture_plan::plain_text(s)
            })
    {
        return Err(err(
            "VERIFY_DECISION",
            2,
            "excluded requires a bounded plain reason",
        ));
    }
    let source = SourceSession::open(
        project,
        import_id,
        &review.selection,
        &ExtractionLimits {
            max_units: 1,
            max_input_bytes: 1048576,
            max_response_bytes: 1048576,
            max_output_tokens: 1,
            timeout_ms: 30000,
        },
    )?;
    let path = project::check_components(destination)?;
    if extraction_output::inside(&path, source.root())?
        || extraction_output::inside(&path, &inputs.root)?
        || path == inputs.root
        || path == source.root()
    {
        return Err(err(
            "EXTRACT_OUTPUT_OVERLAP",
            2,
            "output overlaps selected source or input directory",
        ));
    }
    extraction_review::validate_gold(&review, &gold, &source.units[0])?;
    let decoded = serde_json::from_slice::<SavedCandidate>(inputs.bytes(1));
    let (candidate, schema_error) = match decoded {
        Ok(c) => (Some(c), None),
        Err(e) => (None, Some(e.to_string())),
    };
    if review.interpretation_timing == Timing::BeforeSyntheticResponse
        && candidate
            .as_ref()
            .is_some_and(|c| c.transport_mode != Transport::LocalHttpTest)
    {
        return Err(err(
            "VERIFY_TIMING",
            2,
            "before_synthetic_response only accepts explicit local_http_test",
        ));
    }
    let strict = if let Some(c) = &candidate {
        if c.payload.concepts.len() > 1024
            || c.payload.evidence.len() > 1024
            || c.payload.atoms.len() > 1024
        {
            return Err(err(
                "VERIFY_CANDIDATE_LIMIT",
                2,
                "maximum 1024 entities of each kind",
            ));
        }
        if c.binding != source.binding
            || c.payload.binding != source.binding
            || c.request_id != c.payload.request_id
            || c.request_id.trim().is_empty()
        {
            Some(err("VERIFY_BINDING", 3, "actual source differs"))
        } else if c.payload.atoms.is_empty() {
            None
        } else {
            crate::extraction::validate_candidates(
                &source.units,
                &c.payload.concepts,
                &c.payload.evidence,
                &c.payload.atoms,
                &control,
                &cancel,
                None,
            )
            .err()
        }
    } else {
        None
    };
    let mut assessment = extraction_review::assess(
        &review,
        &gold,
        &decision,
        candidate.as_ref(),
        &source.units[0],
        &source.binding,
        schema_error.as_deref(),
        strict.as_ref().map(|e| e.code()),
    );
    if candidate.is_none() {
        assessment.totals["candidate_atoms"] = Value::Null;
        assessment.totals["unexpected_atoms"] = Value::Null;
        assessment.totals["duplicate_atoms"] = Value::Null;
        // Syntax-invalid candidates are unprocessed objects; raw counts remain explicit when syntactically recoverable.
        if let Ok(raw) = recover_unambiguous_json(inputs.bytes(1)) {
            if let Some(atoms) = raw
                .get("payload")
                .and_then(|p| p.get("atoms"))
                .and_then(Value::as_array)
            {
                if atoms.len() > 1024 {
                    return Err(err(
                        "VERIFY_CANDIDATE_LIMIT",
                        2,
                        "maximum 1024 raw candidate atoms",
                    ));
                }
                assessment.totals["candidate_atoms"] = json!(atoms.len());
                assessment.totals["unexpected_atoms"] = json!(atoms.len().saturating_sub(1));
                assessment.candidates = atoms
                    .iter()
                    .enumerate()
                    .map(|(i, a)| ObjectResult {
                        object_id: a
                            .get("atom")
                            .and_then(|a| a.get("atom_id"))
                            .and_then(Value::as_str)
                            .map(str::to_owned)
                            .unwrap_or_else(|| format!("candidate-{i}")),
                        unit_id: source.units[0].unit_id.clone(),
                        start_byte: source.units[0].start_byte,
                        end_byte: source.units[0].end_byte,
                        status: "unprocessed".into(),
                        issue_codes: vec!["VERIFY_SCHEMA".into()],
                    })
                    .collect();
            }
        }
    }
    #[cfg(test)]
    stage("inputs_held");
    inputs.verify(&control)?;
    source.verify()?;
    let request_id = candidate
        .as_ref()
        .map(|c| c.request_id.as_str())
        .unwrap_or("invalid-candidate");
    let mut output = OutputRoot::create_review(&path, request_id)?;
    let result = (|| {
        for (index, name) in [
            (0, "review-original.json"),
            (1, "candidates-original.json"),
            (2, "gold-original.json"),
            (3, "decision-original.json"),
        ] {
            output.add(name, inputs.bytes(index))?;
        }
        let snapshot = Snapshot {
            schema_version: 1,
            binding: source.binding.clone(),
            units: source.units.clone(),
        };
        let snapshot_bytes = encoded(&snapshot)?;
        output.add("source-snapshot.json", &snapshot_bytes)?;
        let saved: Snapshot = parse(&output.read("source-snapshot.json")?)?;
        if encoded(&saved)? != snapshot_bytes {
            return Err(err("VERIFY_SNAPSHOT", 3, "saved source differs"));
        }
        if assessment.passed {
            let c = candidate
                .as_ref()
                .ok_or_else(|| err("VERIFY_SCHEMA", 3, "missing candidate"))?;
            let ir = ReviewIr {
                schema_version: 1,
                producer: "source_review".into(),
                revision: 1,
                request_id: c.request_id.clone(),
                binding: source.binding.clone(),
                source_match: "passed".into(),
                concepts: c.payload.concepts.clone(),
                atoms: c.payload.atoms.clone(),
                evidence: c.payload.evidence.clone(),
                checks: assessment.checks.clone(),
            };
            let bytes = encoded(&ir)?;
            output.add("knowledge-ir.json", &bytes)?;
            let saved: ReviewIr = parse(&output.read("knowledge-ir.json")?)?;
            if encoded(&saved)? != bytes {
                return Err(err("VERIFY_IR", 3, "saved IR differs"));
            }
        }
        let report = report(
            &review,
            &source,
            &inputs,
            &assessment,
            candidate.as_ref(),
            output.fingerprints(),
        )?;
        let bytes = encoded(&report)?;
        let facts = String::from_utf8(bytes.clone()).map_err(|e| err("VERIFY_ENCODING", 3, e))?;
        let markdown = review_markdown(&report)?;
        output.add("verification.json", &bytes)?;
        output.add("verification.md", markdown.as_bytes())?;
        let saved: ReviewReport = serde_json::from_slice(&output.read("verification.json")?)
            .map_err(|e| err("VERIFY_REPORT", 3, e))?;
        if encoded(&saved)? != bytes || output.read("verification.md")? != markdown.as_bytes() {
            return Err(err("VERIFY_REPORT", 3, "saved dual facts differ"));
        }
        #[cfg(test)]
        stage("results_saved");
        control.check().map_err(|e| err("VERIFY_TIMEOUT", 3, e))?;
        inputs.verify(&control)?;
        source.verify()?;
        output.verify()?;
        Ok(VerificationOutput {
            json: format!("{facts}\n"),
            failed: !assessment.passed,
        })
    })();
    if result.is_err() {
        output.discard_review_result()?;
    }
    result
}
fn report(
    r: &Review,
    s: &SourceSession,
    i: &Inputs,
    a: &Assessment,
    c: Option<&SavedCandidate>,
    artifacts: std::collections::BTreeMap<String, crate::sidecar_model::Fingerprint>,
) -> Result<ReviewReport, ProjectError> {
    let fingerprints = i
        .files
        .iter()
        .map(|f| {
            (
                f.path.file_name().unwrap().to_string_lossy().into_owned(),
                crate::sidecar_model::Fingerprint {
                    sha256: sha256(&f.bytes),
                    byte_length: f.bytes.len() as u64,
                },
            )
        })
        .collect();
    report_from_fingerprints(r, s, a, c, fingerprints, artifacts)
}
pub(crate) fn report_from_fingerprints(
    r: &Review,
    s: &SourceSession,
    a: &Assessment,
    c: Option<&SavedCandidate>,
    fingerprints: std::collections::BTreeMap<String, crate::sidecar_model::Fingerprint>,
    artifacts: std::collections::BTreeMap<String, crate::sidecar_model::Fingerprint>,
) -> Result<ReviewReport, ProjectError> {
    serde_json::from_value(json!({"schema_version":1,"report_kind":"source_knowledge_verification","producer":"source_review","request_id":c.map(|c|&c.request_id),"status":a.expected[0].status,"knowledge_status":"pending","published":false,"scientific_quality":"not_run","research_status":"not_requested","network_calls":0,"paid_calls":0,"scope":{"purpose":r.purpose,"selection":r.selection,"binding":s.binding,"units":s.units,"expected_atoms":1},"binding":s.binding,"origin":{"producer":c.map(|c|&c.producer),"transport_mode":c.map(|c|&c.transport_mode),"reviewer":r.reviewer,"interpretation_timing":r.interpretation_timing,"input_fingerprints":fingerprints,"gold_sha256":r.gold_sha256},"rules":{"grammar":"controlled_measurement_zh_v1","subject":r.subject,"property":r.property,"allowed_contexts":r.allowed_contexts,"reviewer":r.reviewer,"interpretation_timing":r.interpretation_timing},"expected_results":a.expected,"candidate_results":a.candidates,"checks":a.checks,"totals":a.totals,"quality":a.quality,"artifacts":artifacts})).map_err(|e|err("VERIFY_REPORT",3,e))
}

#[cfg(test)]
mod guard_tests {
    use super::*;
    struct Fixture {
        root: PathBuf,
        project: PathBuf,
        case: PathBuf,
        output: PathBuf,
        id: String,
        database: Vec<u8>,
        resource: PathBuf,
        source_bytes: Vec<u8>,
    }
    impl Fixture {
        fn new() -> Self {
            let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/compiler-temp");
            fs::create_dir_all(&parent).unwrap();
            let root = parent.join(format!("review-guard-{}", uuid::Uuid::new_v4()));
            fs::create_dir(&root).unwrap();
            let root = root.canonicalize().unwrap();
            let project = root.join("project");
            project::initialize(&project).unwrap();
            let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../fixtures/extraction-review-v1")
                .canonicalize()
                .unwrap();
            let run = crate::source::import(&project, &fixture.join("source.txt"), None).unwrap();
            let imported: Value = serde_json::from_str(&run.json).unwrap();
            let id = imported["import_id"].as_str().unwrap().to_owned();
            let limits = ExtractionLimits {
                max_units: 1,
                max_input_bytes: 1048576,
                max_response_bytes: 1048576,
                max_output_tokens: 1,
                timeout_ms: 30000,
            };
            let source = SourceSession::open(
                &project,
                &id,
                &[crate::extraction_model::Selection::Text { ordinal: 0 }],
                &limits,
            )
            .unwrap();
            let binding = source.binding.clone();
            drop(source);
            let case = root.join("case");
            fs::create_dir(&case).unwrap();
            for name in ["gold.json", "review.json", "decision.json"] {
                fs::copy(fixture.join(name), case.join(name)).unwrap();
            }
            let mut candidate: Value =
                serde_json::from_slice(&fs::read(fixture.join("candidates.json")).unwrap())
                    .unwrap();
            candidate["binding"] = json!(binding);
            candidate["payload"]["binding"] = json!(binding);
            fs::write(case.join("candidates.json"), encoded(&candidate).unwrap()).unwrap();
            let database = fs::read(project.join("project.sqlite")).unwrap();
            let resource =
                project.join(imported["version"]["normalized_snapshot"].as_str().unwrap());
            let source_bytes = fs::read(&resource).unwrap();
            Self {
                output: root.join("output"),
                root,
                project,
                case,
                id,
                database,
                resource,
                source_bytes,
            }
        }
        fn run_hook(
            &self,
            hook: impl FnMut(&str) + 'static,
        ) -> Result<VerificationOutput, ProjectError> {
            TEST_HOOK.with(|h| *h.borrow_mut() = Some(Box::new(hook)));
            let result = run(&self.project, &self.id, &self.case, &self.output);
            TEST_HOOK.with(|h| *h.borrow_mut() = None);
            result
        }
        fn unchanged(&self) {
            assert_eq!(
                fs::read(self.project.join("project.sqlite")).unwrap(),
                self.database
            );
            assert_eq!(fs::read(&self.resource).unwrap(), self.source_bytes);
        }
        fn no_results(&self) {
            for name in ["knowledge-ir.json", "verification.json", "verification.md"] {
                assert!(!self.output.join(name).exists(), "residual {name}");
            }
            for name in [
                "candidates-original.json",
                "gold-original.json",
                "review-original.json",
                "decision-original.json",
                "source-snapshot.json",
            ] {
                assert!(self.output.join(name).is_file());
            }
        }
    }
    #[test]
    fn source_added_during_final_guard_removes_only_captured_results() {
        let f = Fixture::new();
        let project = f.project.clone();
        let output = f.output.clone();
        let result = f.run_hook(move |stage| {
            if stage == "results_saved" {
                let project = project.clone();
                let output = output.clone();
                std::thread::spawn(move || {
                    fs::write(project.join("human-source-addition.txt"), b"human source").unwrap();
                    fs::write(output.join("human-output-addition.txt"), b"human output").unwrap();
                })
                .join()
                .unwrap();
            }
        });
        assert_eq!(result.err().unwrap().code(), "EXTRACT_SOURCE_CHANGED");
        f.unchanged();
        f.no_results();
        assert_eq!(
            fs::read(f.output.join("human-output-addition.txt")).unwrap(),
            b"human output"
        );
        assert!(f.project.join("human-source-addition.txt").exists());
    }
    #[test]
    fn output_added_during_final_guard_retains_human_file() {
        let f = Fixture::new();
        let output = f.output.clone();
        let result = f.run_hook(move |stage| {
            if stage == "results_saved" {
                let path = output.join("human-result.txt");
                std::thread::spawn(move || fs::write(path, b"kept").unwrap())
                    .join()
                    .unwrap();
            }
        });
        assert_eq!(result.err().unwrap().code(), "EXTRACT_OUTPUT_CHANGED");
        f.unchanged();
        f.no_results();
        assert_eq!(
            fs::read(f.output.join("human-result.txt")).unwrap(),
            b"kept"
        );
    }
    #[test]
    fn result_hardlink_is_denied_or_invalidates_and_cleans_only_owned_results() {
        let f = Fixture::new();
        let output = f.output.clone();
        let alias = f.root.join("human-ir-alias.json");
        let recorded = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let saved = recorded.clone();
        let alias_in = alias.clone();
        let result = f.run_hook(move |stage| {
            if stage == "results_saved" {
                let path = output.join("knowledge-ir.json");
                let alias = alias_in.clone();
                let created = std::thread::spawn(move || fs::hard_link(path, alias).is_ok())
                    .join()
                    .unwrap();
                saved.store(created, std::sync::atomic::Ordering::SeqCst);
            }
        });
        f.unchanged();
        if recorded.load(std::sync::atomic::Ordering::SeqCst) {
            assert!(result.is_err());
            f.no_results();
            assert!(alias.is_file());
        } else {
            assert!(!result.unwrap().failed);
            assert!(!alias.exists());
        }
    }
    #[test]
    fn selected_case_ignores_private_sibling_content() {
        let f = Fixture::new();
        fs::write(f.case.join("private-other.json"), [0xff, 0xfe, 0xfd]).unwrap();
        fs::create_dir(f.case.join("private-folder")).unwrap();
        fs::write(f.case.join("private-folder/ignored.json"), b"not json").unwrap();
        let run = run(&f.project, &f.id, &f.case, &f.output).unwrap();
        assert!(!run.failed);
        f.unchanged();
        assert_eq!(
            fs::read(f.case.join("private-other.json")).unwrap(),
            [0xff, 0xfe, 0xfd]
        );
    }
    #[cfg(windows)]
    #[test]
    fn held_source_inputs_and_results_deny_actual_concurrent_write_and_delete() {
        let f = Fixture::new();
        let candidate = f.case.join("candidates.json");
        let database = f.project.join("project.sqlite");
        let resource = f.resource.clone();
        let output = f.output.clone();
        let result = f
            .run_hook(move |stage| {
                if stage == "inputs_held" {
                    for path in [&candidate, &database, &resource] {
                        let path = path.clone();
                        std::thread::spawn(move || {
                            assert!(fs::write(&path, b"changed").is_err());
                            assert!(fs::remove_file(&path).is_err());
                        })
                        .join()
                        .unwrap();
                    }
                }
                if stage == "results_saved" {
                    let path = output.join("knowledge-ir.json");
                    std::thread::spawn(move || {
                        assert!(fs::write(&path, b"changed").is_err());
                        assert!(fs::remove_file(&path).is_err());
                    })
                    .join()
                    .unwrap();
                }
            })
            .unwrap();
        assert!(!result.failed);
        f.unchanged();
    }
    #[test]
    fn actual_hardlink_and_linked_directory_inputs_fail_before_output() {
        let f = Fixture::new();
        fs::hard_link(
            f.case.join("candidates.json"),
            f.root.join("actual-alias.json"),
        )
        .unwrap();
        assert!(run(&f.project, &f.id, &f.case, &f.output).is_err());
        assert!(!f.output.exists());
        f.unchanged();
        let f = Fixture::new();
        let original = f.case.join("candidates.json");
        let outside = f.root.join("outside.json");
        fs::rename(&original, &outside).unwrap();
        #[cfg(windows)]
        {
            // This host does not grant file-symlink privilege. A real Windows
            // junction exercises the same component rejection without it.
            fs::rename(&outside, &original).unwrap();
            let junction = f.root.join("linked-case");
            let result = std::process::Command::new("cmd")
                .args(["/c", "mklink", "/J"])
                .arg(&junction)
                .arg(&f.case)
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            assert!(project::is_link(&fs::symlink_metadata(&junction).unwrap()));
            assert!(run(&f.project, &f.id, &junction, &f.output).is_err());
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outside, &original).unwrap();
        }
        #[cfg(unix)]
        assert!(run(&f.project, &f.id, &f.case, &f.output).is_err());
        assert!(!f.output.exists());
        f.unchanged();
    }
}

pub(crate) fn review_markdown(report: &ReviewReport) -> Result<String, ProjectError> {
    let facts = String::from_utf8(encoded(report)?).map_err(|e| err("VERIFY_ENCODING", 3, e))?;
    Ok(format!(
        "# 有界来源知识核对\n\n状态：{}；知识状态：pending；发布：false。\n\n科学质量：not_run；研究：not_requested；网络与付费调用：0。\n\n规则：controlled_measurement_zh_v1；复核主体：automated_source_review；解释时间：{}。\n\n来源范围、对象、问题码、逐类别检查与分母见以下完整事实快照，与 verification.json 一致。\n\n```json\n{}\n```\n",
        report.status,
        json!(report.rules.interpretation_timing),
        facts
    ))
}
