//! Held, exact case inventories and independent source-review reconstruction.
use crate::{
    extraction_model::ExtractionLimits,
    extraction_review,
    extraction_review_model::*,
    extraction_source::SourceSession,
    extraction_verify,
    knowledge_coordination_model::*,
    project::{self, ProjectError},
    sidecar_model::Fingerprint,
    sidecar_process::Control,
    sidecar_workspace::{Directory, GuardedFile},
    text::sha256,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
    time::SystemTime,
};
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
pub(crate) fn err(code: &'static str, exit: u8, msg: impl ToString) -> ProjectError {
    ProjectError::new(code, exit, msg.to_string())
}
pub(crate) fn parse<T: DeserializeOwned>(b: &[u8]) -> Result<T, ProjectError> {
    extraction_verify::recover_unambiguous_json(b).map_err(|e| err("COORD_SCHEMA", 2, e))?;
    serde_json::from_slice(b).map_err(|e| err("COORD_SCHEMA", 2, e))
}
pub(crate) fn encoded<T: Serialize>(v: &T) -> Result<Vec<u8>, ProjectError> {
    serde_json::to_vec_pretty(v).map_err(|e| err("COORD_SCHEMA", 2, e))
}
pub(crate) fn fingerprint(b: &[u8]) -> Fingerprint {
    Fingerprint {
        sha256: sha256(b),
        byte_length: b.len() as u64,
    }
}
pub(crate) fn check(c: &Control) -> Result<(), ProjectError> {
    c.check().map_err(|e| err("COORD_TIMEOUT", 3, e))
}
pub(crate) fn plain(s: &str) -> bool {
    !s.trim().is_empty() && s.len() <= 1024 && crate::fixture_plan::plain_text(s)
}
pub(crate) fn uuid(s: &str) -> bool {
    uuid::Uuid::parse_str(s).is_ok_and(|u| u.to_string() == s)
}
fn filename(s: &str) -> bool {
    crate::fixture_input::valid_relative_path(s) && !s.contains(['/', '\\'])
}
fn input_id(s: &str) -> bool {
    (1..=32).contains(&s.len())
        && s.bytes().next().is_some_and(|b| b.is_ascii_lowercase())
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}
struct HeldFile {
    guard: GuardedFile,
    bytes: Vec<u8>,
    modified: SystemTime,
    path: PathBuf,
}
struct Held {
    root: PathBuf,
    parent: Directory,
    directory: Directory,
    files: BTreeMap<String, HeldFile>,
    inventory: BTreeSet<std::ffi::OsString>,
}
impl Held {
    fn open(path: &Path) -> Result<Self, ProjectError> {
        let root = project::check_components(path).map_err(|e| err("COORD_INPUT_PATH", 2, e))?;
        let parent = Directory::capture(
            root.parent()
                .ok_or_else(|| err("COORD_INPUT_PATH", 2, "missing parent"))?,
            "COORD_INPUT_CHANGED",
        )
        .map_err(|e| err("COORD_INPUT_PATH", 2, e))?;
        let directory = Directory::capture(&root, "COORD_INPUT_CHANGED")
            .map_err(|e| err("COORD_INPUT_PATH", 2, e))?;
        let inventory = Self::inventory(&root)?;
        Ok(Self {
            root,
            parent,
            directory,
            files: BTreeMap::new(),
            inventory,
        })
    }
    fn inventory(p: &Path) -> Result<BTreeSet<std::ffi::OsString>, ProjectError> {
        fs::read_dir(p)
            .map_err(|e| err("COORD_INPUT_PATH", 2, e))?
            .map(|e| e.map(|e| e.file_name()))
            .collect::<Result<_, _>>()
            .map_err(|e| err("COORD_INPUT_PATH", 2, e))
    }
    fn add(&mut self, name: &str, max: u64, c: &Control) -> Result<(), ProjectError> {
        check(c)?;
        if !filename(name) || self.files.contains_key(name) {
            return Err(err(
                "COORD_INPUT_PATH",
                2,
                "distinct ordinary filenames required",
            ));
        }
        self.verify(c)?;
        let path = self.root.join(name);
        let modified = fs::metadata(&path)
            .and_then(|m| m.modified())
            .map_err(|e| err("COORD_INPUT_PATH", 2, e))?;
        let (guard, bytes) =
            GuardedFile::open(&path, max, c, "COORD_INPUT_CHANGED").map_err(|e| {
                err(
                    if e.code == "SIDECAR_LIMIT" {
                        "COORD_INPUT_LIMIT"
                    } else {
                        "COORD_INPUT_PATH"
                    },
                    2,
                    e,
                )
            })?;
        for f in self.files.values() {
            if same_file::is_same_file(&path, &f.path).map_err(|e| err("COORD_INPUT_PATH", 2, e))? {
                return Err(err("COORD_INPUT_PATH", 2, "file alias"));
            }
        }
        self.files.insert(
            name.into(),
            HeldFile {
                guard,
                bytes,
                modified,
                path,
            },
        );
        self.verify(c)
    }
    fn verify(&self, c: &Control) -> Result<(), ProjectError> {
        check(c)?;
        self.parent
            .verify("COORD_INPUT_CHANGED")
            .and_then(|_| self.directory.verify("COORD_INPUT_CHANGED"))
            .map_err(|e| err("COORD_INPUT_CHANGED", 3, e))?;
        if Self::inventory(&self.root)? != self.inventory {
            return Err(err("COORD_INPUT_CHANGED", 3, "directory inventory changed"));
        }
        for f in self.files.values() {
            f.guard
                .verify(c, "COORD_INPUT_CHANGED")
                .map_err(|e| err("COORD_INPUT_CHANGED", 3, e))?;
            if fs::metadata(&f.path)
                .and_then(|m| m.modified())
                .map_err(|e| err("COORD_INPUT_CHANGED", 3, e))?
                != f.modified
            {
                return Err(err("COORD_INPUT_CHANGED", 3, "timestamp changed"));
            }
        }
        Ok(())
    }
    fn bytes(&self, name: &str) -> Result<&[u8], ProjectError> {
        self.files
            .get(name)
            .map(|f| f.bytes.as_slice())
            .ok_or_else(|| err("COORD_SCHEMA", 2, "missing held file"))
    }
    fn exact(&self, names: &[String]) -> Result<(), ProjectError> {
        let expected = names.iter().map(std::ffi::OsString::from).collect();
        if self.inventory != expected {
            return Err(err("COORD_SCHEMA", 2, "unexpected case/bundle inventory"));
        }
        Ok(())
    }
}
pub(crate) struct CaseInputs {
    pub(crate) config: CoordinationSelection,
    pub(crate) map: IdentityMap,
    pub(crate) version: CaseVersion,
    held: Held,
}
// A private, non-wire view obtained only after the corresponding strict version DTO parses.
pub(crate) struct CoordinationSelection {
    pub(crate) project_id: String,
    pub(crate) identity_file: String,
    pub(crate) inputs: Vec<InputSpec>,
}
pub(crate) enum CaseVersion {
    V1,
    V2(Vec<crate::knowledge_coordination_v2_model::AtomContext>),
    V3(Vec<crate::knowledge_coordination_v3_model::AtomContext>),
}
impl CaseInputs {
    pub(crate) fn open(path: &Path, c: &Control) -> Result<Self, ProjectError> {
        let mut held = Held::open(path)?;
        held.add("coordination.json", 65536, c)?;
        let original = held.bytes("coordination.json")?;
        let dispatch: serde_json::Value = parse(original)?;
        let schema = dispatch["schema_version"]
            .as_u64()
            .ok_or_else(|| err("COORD_SCHEMA", 2, "missing schema version"))?;
        let config: CoordinationSelection = match schema {
            1 => {
                let v: CoordinationCase = parse(original)?;
                CoordinationSelection {
                    project_id: v.project_id,
                    identity_file: v.identity_file,
                    inputs: v.inputs,
                }
            }
            2 => {
                let v: crate::knowledge_coordination_v2_model::CoordinationCase = parse(original)?;
                CoordinationSelection {
                    project_id: v.project_id,
                    identity_file: v.identity_file,
                    inputs: v.inputs,
                }
            }
            3 => {
                let v: crate::knowledge_coordination_v3_model::CoordinationCase = parse(original)?;
                CoordinationSelection {
                    project_id: v.project_id,
                    identity_file: v.identity_file,
                    inputs: v.inputs,
                }
            }
            _ => return Err(err("COORD_SCHEMA", 2, "unknown schema version")),
        };
        if !uuid(&config.project_id)
            || !(2..=3).contains(&config.inputs.len())
            || !filename(&config.identity_file)
            || config.identity_file == "coordination.json"
        {
            return Err(err(
                "COORD_SCHEMA",
                2,
                "project, count or identity filename",
            ));
        }
        held.add(&config.identity_file, 65536, c)?;
        let (map, version) = match schema {
            1 => (
                parse::<IdentityMap>(held.bytes(&config.identity_file)?)?,
                CaseVersion::V1,
            ),
            2 => {
                let v: crate::knowledge_coordination_v2_model::IdentityMap =
                    parse(held.bytes(&config.identity_file)?)?;
                // The shared identity builder consumes only these already parsed identity fields.
                // This private v1-shaped projection is never written over the original v2 map.
                (
                    IdentityMap {
                        schema_version: 1,
                        author: v.author,
                        concepts: v.concepts,
                        concept_mappings: v.concept_mappings,
                        atom_mappings: v.atom_mappings,
                    },
                    CaseVersion::V2(v.atom_contexts),
                )
            }
            3 => {
                let v: crate::knowledge_coordination_v3_model::IdentityMap =
                    parse(held.bytes(&config.identity_file)?)?;
                (
                    IdentityMap {
                        schema_version: 1,
                        author: v.author,
                        concepts: v.concepts,
                        concept_mappings: v.concept_mappings,
                        atom_mappings: v.atom_mappings,
                    },
                    CaseVersion::V3(v.atom_contexts),
                )
            }
            _ => unreachable!(),
        };
        let mut names = vec!["coordination.json".into(), config.identity_file.clone()];
        let mut ids = BTreeSet::new();
        let mut directories = vec![];
        for i in &config.inputs {
            if !input_id(&i.input_id)
                || !ids.insert(&i.input_id)
                || !filename(&i.review_bundle)
                || names.contains(&i.review_bundle)
            {
                return Err(err("COORD_SCHEMA", 2, "input ID or bundle"));
            }
            if i.binding.project_id != config.project_id {
                return Err(err(
                    "COORD_SOURCE_REVIEW",
                    3,
                    format!("{}: project mismatch", i.input_id),
                ));
            }
            if i.artifacts
                .keys()
                .map(String::as_str)
                .collect::<BTreeSet<_>>()
                != FILES.into_iter().collect()
            {
                return Err(err("COORD_SCHEMA", 2, "nine fingerprints required"));
            }
            let p = project::check_components(&held.root.join(&i.review_bundle))
                .map_err(|e| err("COORD_INPUT_PATH", 2, e))?;
            for old in &directories {
                if same_file::is_same_file(&p, old).map_err(|e| err("COORD_INPUT_PATH", 2, e))? {
                    return Err(err("COORD_INPUT_PATH", 2, "directory alias"));
                }
            }
            directories.push(p);
            names.push(i.review_bundle.clone());
        }
        held.exact(&names)?;
        if !plain(&map.author)
            || map.concepts.len() > 1024
            || map.concept_mappings.len() > 1024
            || map.atom_mappings.len() > 1024
        {
            return Err(err("COORD_INPUT_LIMIT", 2, "identity map limits"));
        }
        Ok(Self {
            config,
            map,
            version,
            held,
        })
    }
    pub(crate) fn root(&self) -> &Path {
        &self.held.root
    }
    pub(crate) fn original_bytes(&self) -> (&[u8], &[u8]) {
        (
            &self.held.files["coordination.json"].bytes,
            &self.held.files[&self.config.identity_file].bytes,
        )
    }
    pub(crate) fn verify(&self, c: &Control) -> Result<(), ProjectError> {
        self.held.verify(c)
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Owner {
    schema_version: u32,
    producer: String,
    request_id: String,
    purpose: String,
}
pub(crate) struct VerifiedReview {
    pub(crate) input: InputSpec,
    pub(crate) source: SourceSession,
    pub(crate) review: Review,
    pub(crate) gold: Gold,
    pub(crate) candidate: SavedCandidate,
    pub(crate) checks: Vec<Check>,
    pub(crate) files: BTreeMap<String, Vec<u8>>,
    held: Held,
}
impl VerifiedReview {
    pub(crate) fn open(
        project: &Path,
        input: &InputSpec,
        case: &Path,
        c: &Control,
        cancel: &Arc<AtomicBool>,
    ) -> Result<Self, ProjectError> {
        Self::open_inner(project, input, case, c, cancel).map_err(|e| {
            if matches!(
                e.code(),
                "COORD_SCHEMA"
                    | "COORD_INPUT_PATH"
                    | "COORD_INPUT_LIMIT"
                    | "COORD_FINGERPRINT"
                    | "COORD_INPUT_CHANGED"
                    | "COORD_TIMEOUT"
            ) {
                e
            } else {
                err(
                    "COORD_SOURCE_REVIEW",
                    3,
                    format!("{}: {}", input.input_id, e),
                )
            }
        })
    }
    fn open_inner(
        project: &Path,
        input: &InputSpec,
        case: &Path,
        c: &Control,
        cancel: &Arc<AtomicBool>,
    ) -> Result<Self, ProjectError> {
        check(c)?;
        let mut held = Held::open(&case.join(&input.review_bundle))?;
        held.exact(&FILES.iter().map(|s| s.to_string()).collect::<Vec<_>>())?;
        for name in FILES {
            let max = match name {
                ".owner.json" => 4096,
                "review-original.json" | "gold-original.json" | "decision-original.json" => 65536,
                "candidates-original.json" => 1048576,
                "source-snapshot.json" => 4194304,
                _ => 8388608,
            };
            held.add(name, max, c)?;
            let fp = fingerprint(held.bytes(name)?);
            let declared = &input.artifacts[name];
            if !crate::fixture_model::is_sha(&declared.sha256)
                || fp.sha256 != declared.sha256
                || fp.byte_length != declared.byte_length
            {
                return Err(err(
                    "COORD_FINGERPRINT",
                    2,
                    format!("{}: {name}", input.input_id),
                ));
            }
        }
        let owner: Owner = parse(held.bytes(".owner.json")?)?;
        if owner.schema_version != 1 {
            return Err(err(
                "COORD_SCHEMA",
                2,
                "unsupported saved source owner schema",
            ));
        }
        let review: Review = parse(held.bytes("review-original.json")?)?;
        let candidate: SavedCandidate = parse(held.bytes("candidates-original.json")?)?;
        let gold_input: GoldInput = parse(held.bytes("gold-original.json")?)?;
        let author = match &gold_input {
            GoldInput::V1(gold) => &gold.author,
            GoldInput::V2(gold) => &gold.author,
        };
        if author.len() > 1024 {
            return Err(err("COORD_INPUT_LIMIT", 2, "source gold author text limit"));
        }
        if !plain(author) {
            return Err(err(
                "COORD_SCHEMA",
                2,
                "source gold author must be nonempty plain text",
            ));
        }
        let gold: Gold = gold_input.into();
        let decision: Decision = parse(held.bytes("decision-original.json")?)?;
        let roles = [
            "review.json",
            &review.candidate_file,
            &review.gold_file,
            &review.decision_file,
        ];
        let role_set: BTreeSet<_> = roles.iter().copied().collect();
        if role_set.len() != 4
            || roles.iter().any(|s| !filename(s))
            || review.selection.len() != 1
            || review.gold_sha256 != sha256(held.bytes("gold-original.json")?)
            || owner.producer != "source_review"
            || owner.purpose != "unpublished_source_review"
            || owner.request_id != candidate.request_id
            || !plain(&candidate.request_id)
            || candidate.request_id != candidate.payload.request_id
            || candidate.binding != input.binding
            || candidate.payload.binding != input.binding
            || candidate.payload.atoms.len() != 1
            || candidate.payload.concepts.len() > 1024
            || candidate.payload.evidence.len() > 1024
            || decision.disposition != Disposition::Included
            || decision.atom_ids != vec![candidate.payload.atoms[0].atom.atom_id.clone()]
            || decision.unit_id != "unit-0"
            || gold.unknown
        {
            return Err(err(
                "COORD_SOURCE_REVIEW",
                3,
                "owner, roles, single included atom or binding differs",
            ));
        }
        if (review.interpretation_timing == Timing::BeforeSyntheticResponse)
            != (candidate.transport_mode == Transport::LocalHttpTest)
        {
            return Err(err(
                "COORD_SOURCE_REVIEW",
                3,
                "interpretation timing differs",
            ));
        }
        // Enforce bounded nonempty entity text without touching original bytes or deriving identities from names.
        fn strings(v: &serde_json::Value) -> bool {
            match v {
                serde_json::Value::String(s) => {
                    s.len() <= 1024 && !s.chars().any(|c| c.is_control() && !c.is_whitespace())
                }
                serde_json::Value::Array(a) => a.iter().all(strings),
                serde_json::Value::Object(o) => o.values().all(strings),
                _ => true,
            }
        }
        if !strings(&serde_json::to_value(&candidate).map_err(|e| err("COORD_SCHEMA", 2, e))?) {
            return Err(err("COORD_INPUT_LIMIT", 2, "candidate text limit"));
        }
        check(c)?;
        let source = SourceSession::open(
            project,
            &input.binding.import_id,
            &review.selection,
            &ExtractionLimits {
                max_units: 1,
                max_input_bytes: 1048576,
                max_response_bytes: 1048576,
                max_output_tokens: 1,
                timeout_ms: 30000,
            },
        )?;
        check(c)?;
        if source.binding != input.binding {
            return Err(err("COORD_SOURCE_REVIEW", 3, "actual binding differs"));
        }
        extraction_review::validate_gold(&review, &gold, &source.units[0])?;
        crate::extraction::validate_candidates(
            &source.units,
            &candidate.payload.concepts,
            &candidate.payload.evidence,
            &candidate.payload.atoms,
            c,
            cancel,
            None,
        )?;
        let assessment = extraction_review::assess(
            &review,
            &gold,
            &decision,
            Some(&candidate),
            &source.units[0],
            &source.binding,
            None,
            None,
        );
        if !assessment.passed {
            return Err(err("COORD_SOURCE_REVIEW", 3, "single passed atom required"));
        }
        let snapshot = Snapshot {
            schema_version: 1,
            binding: source.binding.clone(),
            units: source.units.clone(),
        };
        let saved_snapshot: Snapshot = parse(held.bytes("source-snapshot.json")?)?;
        let ir = ReviewIr {
            schema_version: 1,
            producer: "source_review".into(),
            revision: 1,
            request_id: candidate.request_id.clone(),
            binding: source.binding.clone(),
            source_match: "passed".into(),
            concepts: candidate.payload.concepts.clone(),
            atoms: candidate.payload.atoms.clone(),
            evidence: candidate.payload.evidence.clone(),
            checks: assessment.checks.clone(),
        };
        let saved_ir: ReviewIr = parse(held.bytes("knowledge-ir.json")?)?;
        if encoded(&snapshot)? != encoded(&saved_snapshot)? || encoded(&ir)? != encoded(&saved_ir)?
        {
            return Err(err("COORD_SOURCE_REVIEW", 3, "snapshot or IR differs"));
        }
        let role_fingerprints = roles
            .iter()
            .zip([
                "review-original.json",
                "candidates-original.json",
                "gold-original.json",
                "decision-original.json",
            ])
            .map(|(r, f)| Ok((r.to_string(), fingerprint(held.bytes(f)?))))
            .collect::<Result<BTreeMap<_, _>, ProjectError>>()?;
        let artifact_fingerprints = FILES
            .iter()
            .filter(|n| **n != "verification.json" && **n != "verification.md")
            .map(|n| Ok((n.to_string(), fingerprint(held.bytes(n)?))))
            .collect::<Result<_, ProjectError>>()?;
        let expected = extraction_verify::report_from_fingerprints(
            &review,
            &source,
            &assessment,
            Some(&candidate),
            role_fingerprints,
            artifact_fingerprints,
        )?;
        let saved: ReviewReport = parse(held.bytes("verification.json")?)?;
        if encoded(&expected)? != encoded(&saved)?
            || extraction_verify::review_markdown(&expected)?.as_bytes()
                != held.bytes("verification.md")?
        {
            return Err(err("COORD_SOURCE_REVIEW", 3, "saved source review differs"));
        }
        let files = held
            .files
            .iter()
            .map(|(n, f)| (n.clone(), f.bytes.clone()))
            .collect();
        let value = Self {
            input: input.clone(),
            source,
            review,
            gold,
            candidate,
            checks: assessment.checks,
            files,
            held,
        };
        value.verify(c)?;
        Ok(value)
    }
    pub(crate) fn verify(&self, c: &Control) -> Result<(), ProjectError> {
        check(c)?;
        self.held.verify(c)?;
        self.source.verify()?;
        check(c)
    }
}
