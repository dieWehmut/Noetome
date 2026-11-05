//! Held process inputs and reconstruction from the actual selected project source.
use crate::{
    extraction_model::{ExtractionLimits, Selection},
    extraction_source::SourceSession,
    extraction_verify,
    knowledge_coordinate_input::{Held, fingerprint, plain, uuid},
    process_model::*,
    process_parser,
    project::{self, ProjectError},
    sidecar_process::Control,
    sidecar_workspace::{Directory, GuardedFile},
};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::SystemTime,
};

pub(crate) fn error(code: &'static str, exit: u8, msg: impl ToString) -> ProjectError {
    ProjectError::new(code, exit, msg.to_string())
}
pub(crate) fn check(control: &Control) -> Result<(), ProjectError> {
    control.check().map_err(|e| error("PROCESS_TIMEOUT", 3, e))
}
pub(crate) fn parse<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, ProjectError> {
    extraction_verify::recover_unambiguous_json(bytes)
        .map_err(|e| error("PROCESS_SCHEMA", 2, e))?;
    serde_json::from_slice(bytes).map_err(|e| error("PROCESS_SCHEMA", 2, e))
}
pub(crate) fn encoded<T: Serialize>(value: &T) -> Result<Vec<u8>, ProjectError> {
    serde_json::to_vec_pretty(value).map_err(|e| error("PROCESS_SCHEMA", 2, e))
}
fn held_error(e: ProjectError) -> ProjectError {
    error(
        if e.exit_code() == 2 {
            "PROCESS_INPUT"
        } else {
            "PROCESS_INPUT_CHANGED"
        },
        e.exit_code(),
        e,
    )
}

pub(crate) struct ProcessInputs {
    pub(crate) case: Held,
    pub(crate) config: ProcessCase,
    pub(crate) gold: ProcessDocument,
    pub(crate) source: SourceSession,
}
impl ProcessInputs {
    pub(crate) fn open(
        project: &Path,
        case: &Path,
        control: &Control,
    ) -> Result<Self, ProjectError> {
        check(control)?;
        let mut held = Held::open(case).map_err(held_error)?;
        held.add("process-review.json", 16 * 1024, control)
            .map_err(held_error)?;
        let config: ProcessCase = parse(held.bytes("process-review.json").map_err(held_error)?)?;
        if !uuid(&config.project_id)
            || !uuid(&config.import_id)
            || config.selection.len() != 1
            || !matches!(config.selection[0], Selection::Text { .. })
            || config.gold_file == "process-review.json"
            || config.gold_file.starts_with('.')
            || !crate::fixture_input::valid_relative_path(&config.gold_file)
            || config.gold_file.contains(['/', '\\'])
            || config.gold_sha256.len() != 64
            || !config
                .gold_sha256
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        {
            return Err(error(
                "PROCESS_SCHEMA",
                2,
                "invalid identity, selection, or gold reference",
            ));
        }
        held.exact(&["process-review.json".into(), config.gold_file.clone()])
            .map_err(held_error)?;
        held.add(&config.gold_file, 1024 * 1024, control)
            .map_err(held_error)?;
        let gold_bytes = held.bytes(&config.gold_file).map_err(held_error)?;
        if fingerprint(gold_bytes).sha256 != config.gold_sha256 {
            return Err(error(
                "PROCESS_GOLD_PIN",
                2,
                "gold bytes differ from the selected fingerprint",
            ));
        }
        let gold: ProcessGold = parse(gold_bytes)?;
        if !plain(&gold.author) {
            return Err(error("PROCESS_SCHEMA", 2, "invalid gold author"));
        }
        let gold = process_parser::normalize(gold.document())
            .map_err(|e| error("PROCESS_SCHEMA", 2, e))?;
        let source = SourceSession::open(
            project,
            &config.import_id,
            &config.selection,
            &ExtractionLimits {
                max_units: 1,
                max_input_bytes: 65536,
                max_response_bytes: 1024 * 1024,
                max_output_tokens: 65536,
                timeout_ms: 30000,
            },
        )?;
        if source.binding.project_id != config.project_id || source.units.len() != 1 {
            return Err(error(
                "PROCESS_SOURCE_BINDING",
                2,
                "selected project/unit mismatch",
            ));
        }
        let value = Self {
            case: held,
            config,
            gold,
            source,
        };
        value.verify(control)?;
        Ok(value)
    }
    pub(crate) fn verify(&self, control: &Control) -> Result<(), ProjectError> {
        check(control)?;
        self.case.verify(control).map_err(held_error)?;
        self.source.verify()?;
        check(control)
    }
    pub(crate) fn originals(&self) -> Result<BTreeMap<String, Vec<u8>>, ProjectError> {
        Ok(BTreeMap::from([
            (
                ".case.json".into(),
                self.case
                    .bytes("process-review.json")
                    .map_err(held_error)?
                    .to_vec(),
            ),
            (
                ".gold.json".into(),
                self.case
                    .bytes(&self.config.gold_file)
                    .map_err(held_error)?
                    .to_vec(),
            ),
            (
                ".source.json".into(),
                encoded(&ProcessSnapshot {
                    schema_version: 1,
                    binding: self.source.binding.clone(),
                    units: self.source.units.clone(),
                })?,
            ),
        ]))
    }
    pub(crate) fn evaluate(&self) -> Evaluation {
        let parsed = process_parser::parse(&self.source.units[0].text);
        let mut issue_codes = parsed.issues;
        let source_ok = parsed.document.is_some() && issue_codes.is_empty();
        let gold_ok = source_ok && parsed.document.as_ref() == Some(&self.gold);
        if source_ok && !gold_ok {
            issue_codes.push("PROCESS_GOLD_MISMATCH".into());
        }
        Evaluation {
            document: parsed.document,
            source_ok,
            gold_ok,
            unparsed_statements: parsed.unparsed_statements,
            issue_codes,
            observed_totals: parsed.observed_totals,
        }
    }
}
pub(crate) struct Evaluation {
    pub(crate) document: Option<ProcessDocument>,
    pub(crate) source_ok: bool,
    pub(crate) gold_ok: bool,
    pub(crate) unparsed_statements: usize,
    pub(crate) issue_codes: Vec<String>,
    pub(crate) observed_totals: BTreeMap<String, usize>,
}
impl Evaluation {
    pub(crate) fn ready(&self) -> bool {
        self.source_ok && self.gold_ok && self.issue_codes.is_empty()
    }
}

/// One selected ordinary file; sibling creation does not change its parent identity.
pub(crate) struct SingleInput {
    pub(crate) path: PathBuf,
    pub(crate) bytes: Vec<u8>,
    parent: Directory,
    file: GuardedFile,
    modified: SystemTime,
}
impl SingleInput {
    pub(crate) fn open(path: &Path, control: &Control) -> Result<Self, ProjectError> {
        let path = project::check_components(path)?;
        let parent = Directory::capture(
            path.parent()
                .ok_or_else(|| error("PROCESS_PROFILE_PATH", 2, "missing profile parent"))?,
            "PROCESS_INPUT_CHANGED",
        )
        .map_err(|e| error("PROCESS_PROFILE_PATH", 2, e))?;
        let modified = std::fs::metadata(&path)
            .and_then(|m| m.modified())
            .map_err(|e| error("PROCESS_PROFILE_PATH", 2, e))?;
        let (file, bytes) = GuardedFile::open(&path, 8192, control, "PROCESS_INPUT_CHANGED")
            .map_err(|e| error("PROCESS_PROFILE_PATH", 2, e))?;
        let value = Self {
            path,
            bytes,
            parent,
            file,
            modified,
        };
        value.verify(control)?;
        Ok(value)
    }
    pub(crate) fn verify(&self, control: &Control) -> Result<(), ProjectError> {
        self.parent
            .verify("PROCESS_INPUT_CHANGED")
            .map_err(|e| error("PROCESS_INPUT_CHANGED", 3, e))?;
        self.file
            .verify(control, "PROCESS_INPUT_CHANGED")
            .map_err(|e| error("PROCESS_INPUT_CHANGED", 3, e))?;
        if std::fs::metadata(&self.path)
            .and_then(|m| m.modified())
            .map_err(|e| error("PROCESS_INPUT_CHANGED", 3, e))?
            != self.modified
        {
            return Err(error(
                "PROCESS_INPUT_CHANGED",
                3,
                "profile timestamp changed",
            ));
        }
        check(control)
    }
}

pub(crate) struct SavedReview {
    pub(crate) held: Held,
    pub(crate) files: BTreeMap<String, Vec<u8>>,
    pub(crate) report: ProcessReviewReport,
}
impl SavedReview {
    pub(crate) fn open(
        path: &Path,
        inputs: &ProcessInputs,
        control: &Control,
    ) -> Result<Self, ProjectError> {
        let mut held = Held::open(path).map_err(held_error)?;
        held.add(".owner.json", 4096, control).map_err(held_error)?;
        let owner: ProcessOwner = parse(held.bytes(".owner.json").map_err(held_error)?)?;
        if owner.producer != "process_source_review"
            || owner.purpose != "unpublished_process_source_review"
            || !uuid(&owner.request_id)
        {
            return Err(error(
                "PROCESS_REVIEW_INVALID",
                2,
                "saved review owner mismatch",
            ));
        }
        let rebuilt = crate::process_review::build(inputs, &owner.request_id)?;
        held.exact(&rebuilt.files.keys().cloned().collect::<Vec<_>>())
            .map_err(held_error)?;
        for (name, expected) in &rebuilt.files {
            check(control)?;
            if name != ".owner.json" {
                held.add(name, 4 * 1024 * 1024, control)
                    .map_err(held_error)?;
            }
            if held.bytes(name).map_err(held_error)? != expected {
                return Err(error(
                    "PROCESS_REVIEW_CHANGED",
                    2,
                    format!("saved review {name} differs from actual source reconstruction"),
                ));
            }
        }
        inputs.verify(control)?;
        held.verify(control).map_err(held_error)?;
        Ok(Self {
            held,
            files: rebuilt.files,
            report: rebuilt.report,
        })
    }
    pub(crate) fn verify(&self, control: &Control) -> Result<(), ProjectError> {
        self.held.verify(control).map_err(held_error)
    }
}
