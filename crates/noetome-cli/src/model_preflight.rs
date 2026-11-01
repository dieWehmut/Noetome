//! Read-only, complete short-text preparation before choosing a model provider.

use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use same_file::Handle;
use serde::{Deserialize, Serialize};

use crate::knowledge_extract::ModelError;
use crate::model_candidate::{SourceSlice, SourceUnit, SourceUnitKind};
use crate::{project, sidecar, source, source_model, text};

const MAX_TEXT_BYTES: u64 = 8192;
const MAX_RAW_BYTES: u64 = 64 * 1024 * 1024;

fn require(condition: bool, code: &'static str) -> Result<(), ModelError> {
    if condition { Ok(()) } else { Err(code.into()) }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FullTextSummary {
    pub schema_version: u32,
    pub project_id: String,
    pub source_id: String,
    pub version_id: String,
    pub revision: u64,
    pub raw_sha256: String,
    pub raw_byte_length: u64,
    pub normalized_sha256: String,
    pub normalized_byte_length: u64,
    pub normalization_version: String,
    pub parser_version: String,
    pub span_count: u64,
    pub evidence_span_count: u64,
}

impl<'de> Deserialize<'de> for FullTextSummary {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            schema_version: u32,
            project_id: String,
            source_id: String,
            version_id: String,
            revision: u64,
            raw_sha256: String,
            raw_byte_length: u64,
            normalized_sha256: String,
            normalized_byte_length: u64,
            normalization_version: String,
            parser_version: String,
            span_count: u64,
            evidence_span_count: u64,
        }
        let fields: Fields = crate::pdf_model::deserialize_object(deserializer)?;
        let summary = Self {
            schema_version: fields.schema_version,
            project_id: fields.project_id,
            source_id: fields.source_id,
            version_id: fields.version_id,
            revision: fields.revision,
            raw_sha256: fields.raw_sha256,
            raw_byte_length: fields.raw_byte_length,
            normalized_sha256: fields.normalized_sha256,
            normalized_byte_length: fields.normalized_byte_length,
            normalization_version: fields.normalization_version,
            parser_version: fields.parser_version,
            span_count: fields.span_count,
            evidence_span_count: fields.evidence_span_count,
        };
        summary.validate().map_err(serde::de::Error::custom)?;
        Ok(summary)
    }
}

impl FullTextSummary {
    fn validate(&self) -> Result<(), &'static str> {
        if self.schema_version != 1
            || !source_model::valid_uuid(&self.project_id)
            || !source_model::valid_uuid(&self.source_id)
            || !source_model::valid_uuid(&self.version_id)
            || self.revision == 0
            || !source_model::valid_hash(&self.raw_sha256)
            || !source_model::valid_hash(&self.normalized_sha256)
            || self.raw_byte_length == 0
            || self.raw_byte_length > MAX_RAW_BYTES
            || self.normalized_byte_length == 0
            || self.normalized_byte_length > MAX_TEXT_BYTES
            || self.normalized_byte_length > self.raw_byte_length
            || self.normalization_version != text::NORMALIZATION_VERSION
            || self.parser_version != text::PARSER_VERSION
            || self.span_count == 0
            || self.span_count > self.normalized_byte_length
            || self.evidence_span_count == 0
            || self.evidence_span_count > self.span_count
        {
            return Err("MODEL_FULL_TEXT_SUMMARY_INVALID");
        }
        Ok(())
    }
}

struct FileIdentity {
    path: PathBuf,
    identity: Handle,
    length: u64,
    modified: Option<SystemTime>,
}

/// A capability over the complete registered text, not over the original file.
/// Snapshot and ancestor guards remain alive until the preparation is dropped.
pub struct PreparedText {
    root: PathBuf,
    summary: FullTextSummary,
    text: String,
    units: Vec<SourceUnit>,
    evidence: SourceSlice,
    dirs: Vec<(PathBuf, Handle)>,
    identities: Vec<FileIdentity>,
    files: Vec<File>,
}

impl std::fmt::Debug for PreparedText {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PreparedText")
            .field("summary", &self.summary)
            .finish()
    }
}

impl PreparedText {
    pub fn summary(&self) -> &FullTextSummary {
        &self.summary
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn units(&self) -> &[SourceUnit] {
        &self.units
    }
    pub fn evidence_source(&self) -> SourceSlice {
        self.evidence.clone()
    }
    /// Revalidate the same registered version and complete byte range.
    pub fn verify(&self) -> Result<(), ModelError> {
        self.verify_guards()?;
        let again = acquire(&self.root, &self.summary.version_id)?;
        require(
            again.summary == self.summary
                && again.text == self.text
                && again.units == self.units
                && again.evidence == self.evidence,
            "MODEL_SOURCE_CHANGED",
        )?;
        again.verify_guards()?;
        self.verify_guards()
    }

    fn verify_guards(&self) -> Result<(), ModelError> {
        for (path, handle) in &self.dirs {
            sidecar::ensure_identity(path, handle, "MODEL_SOURCE_CHANGED")?;
        }
        for (identity, file) in self.identities.iter().zip(&self.files) {
            sidecar::ensure_identity(&identity.path, &identity.identity, "MODEL_SOURCE_CHANGED")?;
            let metadata = file
                .metadata()
                .map_err(|_| ModelError::from("MODEL_SOURCE_CHANGED"))?;
            require(
                metadata.len() == identity.length && metadata.modified().ok() == identity.modified,
                "MODEL_SOURCE_CHANGED",
            )?;
        }
        require(
            self.identities.len() == self.files.len(),
            "MODEL_SOURCE_CHANGED",
        )
    }
}

/// Prepare one complete bounded MD/TXT without credentials, provider I/O or writes.
pub fn prepare_whole_text(project: &Path, version_id: &str) -> Result<PreparedText, ModelError> {
    let prepared = acquire(project, version_id)?;
    prepared.verify()?;
    Ok(prepared)
}

fn acquire(project_root: &Path, version_id: &str) -> Result<PreparedText, ModelError> {
    let root = project::check_components(project_root)?;
    let mut dirs = sidecar::guard_chain(&root)?;
    let mut files = Vec::new();
    let mut identities = Vec::new();
    let mut hold = |path: PathBuf| -> Result<(), ModelError> {
        let file = project::read_guard(&path)?;
        let metadata = file
            .metadata()
            .map_err(|_| ModelError::from("MODEL_SOURCE_CHANGED"))?;
        let identity = Handle::from_file(
            file.try_clone()
                .map_err(|_| ModelError::from("MODEL_SOURCE_CHANGED"))?,
        )
        .map_err(|_| ModelError::from("MODEL_SOURCE_CHANGED"))?;
        sidecar::ensure_identity(&path, &identity, "MODEL_SOURCE_CHANGED")?;
        identities.push(FileIdentity {
            path,
            identity,
            length: metadata.len(),
            modified: metadata.modified().ok(),
        });
        files.push(file);
        Ok(())
    };
    hold(root.join(project::CONFIG))?;
    hold(root.join(project::DATABASE))?;
    let info = project::inspect(&root)?;
    let (store, _, _) =
        source::database_state(&root.join(project::DATABASE), &root, &info, None, None)?;
    project::reject_sidecars(&root)?;
    let version = store
        .versions
        .get(version_id)
        .ok_or(ModelError::from("MODEL_SOURCE_VERSION_UNKNOWN"))?;
    require(
        matches!(version.media_type.as_str(), "text/markdown" | "text/plain"),
        "MODEL_SOURCE_CAPABILITY_INSUFFICIENT",
    )?;
    if version.ingest_status != "ready" {
        return Err(if version
            .error
            .as_ref()
            .is_some_and(|error| error.code == "SOURCE_INVALID_UTF8")
        {
            "MODEL_SOURCE_INVALID_UTF8"
        } else {
            "MODEL_SOURCE_NOT_READY"
        }
        .into());
    }
    require(
        version.byte_length <= MAX_RAW_BYTES,
        "MODEL_SOURCE_TOO_LARGE",
    )?;
    let raw_path = root.join(&version.raw_snapshot);
    let normalized_path = root.join(
        version
            .normalized_snapshot
            .as_ref()
            .ok_or(ModelError::from("MODEL_SOURCE_NOT_READY"))?,
    );
    dirs.extend(sidecar::guard_chain(
        normalized_path
            .parent()
            .ok_or(ModelError::from("MODEL_SOURCE_CHANGED"))?,
    )?);
    hold(raw_path.clone())?;
    hold(normalized_path.clone())?;
    let (again, _, _) =
        source::database_state(&root.join(project::DATABASE), &root, &info, None, None)?;
    require(again == store, "MODEL_SOURCE_CHANGED")?;
    require(
        project::checked_file(&normalized_path)?.len() <= MAX_TEXT_BYTES,
        "MODEL_SOURCE_TOO_LARGE",
    )?;
    let raw = source_model::read_snapshot(&raw_path, MAX_RAW_BYTES, Some(version.byte_length))?;
    require(
        sidecar::digest(&raw) == version.content_sha256,
        "MODEL_SOURCE_CHANGED",
    )?;
    let analyzed =
        text::analyze(&raw).map_err(|_| ModelError::from("MODEL_SOURCE_INVALID_UTF8"))?;
    require(
        analyzed.text.len() as u64 <= MAX_TEXT_BYTES,
        "MODEL_SOURCE_TOO_LARGE",
    )?;
    let normalized = source_model::read_snapshot(
        &normalized_path,
        MAX_TEXT_BYTES,
        Some(analyzed.text.len() as u64),
    )?;
    require(
        normalized == analyzed.text.as_bytes()
            && version.normalized_sha256.as_deref() == Some(analyzed.normalized_sha256.as_str())
            && sidecar::digest(&normalized) == analyzed.normalized_sha256
            && version.normalization_version.as_deref() == Some(text::NORMALIZATION_VERSION),
        "MODEL_SOURCE_CHANGED",
    )?;
    require(!analyzed.text.trim().is_empty(), "MODEL_SOURCE_EMPTY")?;
    let spans = store
        .spans
        .get(version_id)
        .ok_or(ModelError::from("MODEL_SOURCE_NOT_READY"))?;
    let mut units = Vec::new();
    let mut evidence_units = Vec::new();
    let mut cursor = 0;
    let mut reconstructed = String::new();
    for (ordinal, span) in spans.iter().enumerate() {
        require(
            span.parse_status == text::ParseStatus::Parsed,
            "MODEL_SOURCE_NOT_READY",
        )?;
        require(
            span.source_version_id == version.version_id
                && span.ordinal == ordinal
                && span.parser_version == text::PARSER_VERSION
                && span.start_byte == cursor
                && span.end_byte > span.start_byte,
            "MODEL_SOURCE_CHANGED",
        )?;
        let content = analyzed
            .text
            .get(span.start_byte..span.end_byte)
            .ok_or(ModelError::from("MODEL_SOURCE_CHANGED"))?;
        require(
            sidecar::digest(content.as_bytes()) == span.text_sha256,
            "MODEL_SOURCE_CHANGED",
        )?;
        let unit = SourceUnit {
            kind: SourceUnitKind::TextSpan,
            span_id: span.span_id.clone(),
            ordinal: span.ordinal as u64,
            start_byte: span.start_byte as u64,
            end_byte: span.end_byte as u64,
            text_sha256: span.text_sha256.clone(),
            text: content.into(),
        };
        if span.kind != text::SpanKind::Formatting {
            require(!content.trim().is_empty(), "MODEL_SOURCE_NOT_READY")?;
            evidence_units.push(unit.clone());
        }
        units.push(unit);
        reconstructed.push_str(content);
        cursor = span.end_byte;
    }
    require(
        cursor == analyzed.text.len() && reconstructed == analyzed.text,
        "MODEL_SOURCE_CHANGED",
    )?;
    require(!evidence_units.is_empty(), "MODEL_SOURCE_EMPTY")?;
    let evidence = SourceSlice {
        project_id: info.project_id.clone(),
        source_id: version.source_id.clone(),
        version_id: version.version_id.clone(),
        revision: version.revision,
        raw_sha256: version.content_sha256.clone(),
        normalized_sha256: analyzed.normalized_sha256.clone(),
        normalization_version: text::NORMALIZATION_VERSION.into(),
        parser_version: text::PARSER_VERSION.into(),
        units: evidence_units,
    };
    let summary = FullTextSummary {
        schema_version: 1,
        project_id: info.project_id,
        source_id: version.source_id.clone(),
        version_id: version.version_id.clone(),
        revision: version.revision,
        raw_sha256: version.content_sha256.clone(),
        raw_byte_length: raw.len() as u64,
        normalized_sha256: analyzed.normalized_sha256,
        normalized_byte_length: normalized.len() as u64,
        normalization_version: text::NORMALIZATION_VERSION.into(),
        parser_version: text::PARSER_VERSION.into(),
        span_count: units.len() as u64,
        evidence_span_count: evidence.units.len() as u64,
    };
    summary.validate()?;
    let prepared = PreparedText {
        root,
        summary,
        text: analyzed.text,
        units,
        evidence,
        dirs,
        identities,
        files,
    };
    prepared.verify_guards()?;
    Ok(prepared)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::{Path, PathBuf};

    use serde_json::{Value, json};
    use uuid::Uuid;

    struct Sandbox {
        root: PathBuf,
        project: PathBuf,
        selected: PathBuf,
        imported: Value,
        before: BTreeMap<String, Vec<u8>>,
    }

    impl Sandbox {
        fn new(name: &str, raw: &[u8]) -> Self {
            let parent = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../.artifacts/B04-T02-preflight-tests");
            fs::create_dir_all(&parent).unwrap();
            let root = parent
                .canonicalize()
                .unwrap()
                .join(format!("{name}-{}", Uuid::new_v4()));
            fs::create_dir(&root).unwrap();
            let project = root.join("source project");
            let selected = root.join("明确自造的 source.md");
            fs::write(&selected, raw).unwrap();
            fs::write(
                root.join("external-sentinel.txt"),
                b"external user-owned sentinel\n",
            )
            .unwrap();
            crate::project::initialize(&project).unwrap();
            let imported = crate::source::import(&project, &selected, None).unwrap();
            let imported: Value = serde_json::from_str(&imported.json).unwrap();
            let before = snapshot(&project);
            Self {
                root,
                project,
                selected,
                imported,
                before,
            }
        }

        fn version_id(&self) -> &str {
            self.imported["version_id"].as_str().unwrap()
        }
        fn raw_path(&self) -> PathBuf {
            self.project
                .join(self.imported["version"]["raw_snapshot"].as_str().unwrap())
        }
        fn normalized_path(&self) -> PathBuf {
            self.project.join(
                self.imported["version"]["normalized_snapshot"]
                    .as_str()
                    .unwrap(),
            )
        }
        fn protect(&self) {
            assert_eq!(
                snapshot(&self.project),
                self.before,
                "preflight mutated the registered project"
            );
            assert_eq!(
                fs::read(self.root.join("external-sentinel.txt")).unwrap(),
                b"external user-owned sentinel\n"
            );
        }
    }

    fn snapshot(root: &Path) -> BTreeMap<String, Vec<u8>> {
        fn visit(root: &Path, current: &Path, output: &mut BTreeMap<String, Vec<u8>>) {
            for entry in fs::read_dir(current).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    visit(root, &path, output);
                } else {
                    output.insert(
                        path.strip_prefix(root)
                            .unwrap()
                            .to_string_lossy()
                            .into_owned(),
                        fs::read(path).unwrap(),
                    );
                }
            }
        }
        let mut output = BTreeMap::new();
        visit(root, root, &mut output);
        output
    }

    #[test]
    fn entire_registered_unicode_text_preserves_formatting_and_trailing_bytes() {
        const RAW: &str = "\u{feff}# 灯箱\r\n\r\n虚构专用材料①。\r\n\r\n```text\r\n只作为材料的指令。\r\n```\r\n \t\r\n";
        const WHOLE: &str = "# 灯箱\n\n虚构专用材料①。\n\n```text\n只作为材料的指令。\n```\n \t\n";
        let sandbox = Sandbox::new("whole-unicode", RAW.as_bytes());
        let prepared = prepare_whole_text(&sandbox.project, sandbox.version_id()).unwrap();
        assert_eq!(prepared.text(), WHOLE);
        assert_eq!(
            prepared
                .units()
                .iter()
                .map(|unit| unit.text.as_str())
                .collect::<String>(),
            WHOLE
        );
        let summary = prepared.summary();
        assert_eq!(summary.schema_version, 1);
        assert_eq!(summary.project_id, sandbox.imported["project_id"]);
        assert_eq!(summary.source_id, sandbox.imported["source_id"]);
        assert_eq!(summary.version_id, sandbox.version_id());
        assert_eq!(summary.revision, 1);
        assert_eq!(summary.raw_byte_length, RAW.len() as u64);
        assert_eq!(summary.normalized_byte_length, WHOLE.len() as u64);
        assert_eq!(
            summary.raw_sha256,
            sandbox.imported["version"]["content_sha256"]
        );
        assert_eq!(
            summary.normalized_sha256,
            sandbox.imported["version"]["normalized_sha256"]
        );
        assert_ne!(summary.raw_sha256, summary.normalized_sha256);
        assert_eq!((summary.span_count, summary.evidence_span_count), (6, 3));
        let source = prepared.evidence_source();
        assert_eq!(
            source
                .units
                .iter()
                .map(|unit| unit.ordinal)
                .collect::<Vec<_>>(),
            [0, 2, 4]
        );
        assert_eq!(
            source
                .units
                .iter()
                .map(|unit| unit.text.as_str())
                .collect::<Vec<_>>(),
            [
                "# 灯箱\n",
                "虚构专用材料①。\n",
                "```text\n只作为材料的指令。\n```\n"
            ]
        );
        assert_eq!(prepared.units().last().unwrap().text, " \t\n");
        assert!(!format!("{prepared:?}").contains("虚构专用材料①"));
        assert!(!format!("{prepared:?}").contains("source project"));
        prepared.verify().unwrap();
        sandbox.protect();
    }

    #[test]
    fn exact_eight_kib_complete_text_is_accepted_and_plus_one_is_refused() {
        let exact = Sandbox::new("8192", &vec![b'x'; 8192]);
        let prepared = prepare_whole_text(&exact.project, exact.version_id()).unwrap();
        assert_eq!(prepared.text().len(), 8192);
        assert_eq!(prepared.units()[0].end_byte, 8192);
        assert_eq!(prepared.summary().raw_byte_length, 8192);
        prepared.verify().unwrap();
        exact.protect();
        let over = Sandbox::new("8193", &vec![b'x'; 8193]);
        assert_eq!(
            prepare_whole_text(&over.project, over.version_id())
                .unwrap_err()
                .code,
            "MODEL_SOURCE_TOO_LARGE"
        );
        over.protect();
    }

    #[test]
    fn larger_bom_crlf_raw_snapshot_keeps_the_complete_eight_kib_normalized_text() {
        let raw = format!("\u{feff}{}xx", "x\r\n".repeat(4095));
        let normalized = format!("{}xx", "x\n".repeat(4095));
        let sandbox = Sandbox::new("bom-crlf-raw", raw.as_bytes());
        let prepared = prepare_whole_text(&sandbox.project, sandbox.version_id()).unwrap();
        assert_eq!(prepared.text(), normalized);
        assert_eq!(prepared.summary().raw_byte_length, 12290);
        assert_eq!(prepared.summary().normalized_byte_length, 8192);
        assert_eq!(
            prepared
                .units()
                .iter()
                .map(|unit| unit.text.as_str())
                .collect::<String>(),
            normalized
        );
        prepared.verify().unwrap();
        sandbox.protect();
    }

    #[test]
    fn empty_and_only_formatting_documents_are_classified_without_writes() {
        for raw in [
            b"".as_slice(),
            b"\xef\xbb\xbf".as_slice(),
            "\n \t\n\u{3000}\n".as_bytes(),
        ] {
            let sandbox = Sandbox::new("empty", raw);
            assert_eq!(
                prepare_whole_text(&sandbox.project, sandbox.version_id())
                    .unwrap_err()
                    .code,
                "MODEL_SOURCE_EMPTY"
            );
            sandbox.protect();
        }
    }

    #[test]
    fn invalid_utf8_and_unparsed_span_are_classified_without_writes() {
        let invalid = Sandbox::new("invalid-utf8", b"a\xff");
        assert_eq!(invalid.imported["status"], "failed");
        assert_eq!(
            prepare_whole_text(&invalid.project, invalid.version_id())
                .unwrap_err()
                .code,
            "MODEL_SOURCE_INVALID_UTF8"
        );
        invalid.protect();
        let unparsed = Sandbox::new("unclosed-code", b"# header\n```text\nnot closed\n");
        assert_eq!(
            prepare_whole_text(&unparsed.project, unparsed.version_id())
                .unwrap_err()
                .code,
            "MODEL_SOURCE_NOT_READY"
        );
        unparsed.protect();
    }

    #[test]
    fn unknown_or_malformed_version_never_falls_back_to_another_registered_source() {
        let sandbox = Sandbox::new("unknown-version", b"explicit source\n");
        for version in ["", "../../raw.bin", "11111111-1111-4111-8111-111111111111"] {
            assert_eq!(
                prepare_whole_text(&sandbox.project, version)
                    .unwrap_err()
                    .code,
                "MODEL_SOURCE_VERSION_UNKNOWN"
            );
        }
        sandbox.protect();
    }

    #[test]
    fn registered_pdf_is_rejected_as_a_capability_mismatch_without_starting_parser() {
        let sandbox = Sandbox::new("pdf-capability", b"explicit text source\n");
        let selected_pdf = sandbox.root.join("明确选定的 N01.pdf");
        let authored_pdf = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/pdf-text-v1/single-column.pdf");
        let raw = fs::read(&authored_pdf).unwrap();
        assert_eq!(raw.len(), 931);
        assert_eq!(
            crate::text::sha256(&raw),
            "faebbe43a9cd81450304873e6ad5cdce3f9a90c6bac1f7b13076e0552fc15e4e"
        );
        fs::copy(&authored_pdf, &selected_pdf).unwrap();
        let imported = crate::pdf_source::import(
            &sandbox.project,
            &selected_pdf,
            None,
            &crate::pdf_parse::PdfParserSpec {
                python: sandbox.root.join("missing-self-authorized-python.exe"),
                adapter: sandbox.root.join("missing-self-authorized-adapter.py"),
            },
            crate::pdf_parse::PdfLimits::default(),
        )
        .unwrap();
        let imported: Value = serde_json::from_str(&imported.json).unwrap();
        let before = snapshot(&sandbox.project);
        let version = imported["version_id"].as_str().unwrap();
        assert_eq!(
            prepare_whole_text(&sandbox.project, version)
                .unwrap_err()
                .code,
            "MODEL_SOURCE_CAPABILITY_INSUFFICIENT"
        );
        assert_eq!(snapshot(&sandbox.project), before);
        assert_eq!(fs::read(&selected_pdf).unwrap(), raw);
        assert_eq!(fs::read(&authored_pdf).unwrap(), raw);
    }

    #[test]
    fn original_file_can_move_while_the_registered_snapshots_remain_readable() {
        let sandbox = Sandbox::new("original-move", b"selected source contents\n\n");
        let prepared = prepare_whole_text(&sandbox.project, sandbox.version_id()).unwrap();
        fs::rename(&sandbox.selected, sandbox.root.join("moved-original.md")).unwrap();
        assert_eq!(prepared.text(), "selected source contents\n\n");
        prepared.verify().unwrap();
        let again = prepare_whole_text(&sandbox.project, sandbox.version_id()).unwrap();
        assert_eq!(again.summary(), prepared.summary());
        assert_eq!(again.text(), "selected source contents\n\n");
        sandbox.protect();
    }

    #[test]
    fn tampered_registered_normalized_snapshot_is_refused() {
        let sandbox = Sandbox::new("tampered-snapshot", b"author bound\n\n");
        fs::write(sandbox.normalized_path(), b"attacker!!!\n\n").unwrap();
        assert_eq!(
            prepare_whole_text(&sandbox.project, sandbox.version_id())
                .unwrap_err()
                .code,
            "SOURCE_INVALID_SNAPSHOT"
        );
        assert_eq!(
            fs::read(sandbox.normalized_path()).unwrap(),
            b"attacker!!!\n\n"
        );
        assert_eq!(fs::read(sandbox.selected).unwrap(), b"author bound\n\n");
    }

    #[cfg(windows)]
    #[test]
    fn live_preparation_denies_write_delete_and_directory_replacement_until_drop() {
        let sandbox = Sandbox::new("live-guards", b"protected source\n\n");
        let prepared = prepare_whole_text(&sandbox.project, sandbox.version_id()).unwrap();
        for path in [
            sandbox.project.join(crate::project::CONFIG),
            sandbox.project.join(crate::project::DATABASE),
            sandbox.raw_path(),
            sandbox.normalized_path(),
        ] {
            assert!(
                fs::OpenOptions::new().write(true).open(&path).is_err(),
                "write guard missing: {}",
                path.display()
            );
            assert!(
                fs::rename(&path, path.with_extension("replacement")).is_err(),
                "delete guard missing: {}",
                path.display()
            );
        }
        let raw_parent = sandbox.raw_path().parent().unwrap().to_owned();
        assert!(fs::rename(&raw_parent, raw_parent.with_extension("moved")).is_err());
        assert!(fs::rename(&sandbox.project, sandbox.root.join("moved-project")).is_err());
        prepared.verify().unwrap();
        sandbox.protect();
        drop(prepared);
        let raw = sandbox.raw_path();
        let moved = raw.with_extension("moved");
        fs::rename(&raw, &moved).unwrap();
        fs::rename(&moved, &raw).unwrap();
        sandbox.protect();
    }

    #[test]
    fn summary_uses_named_required_fields_and_contains_no_source_text() {
        let sandbox = Sandbox::new("summary-codec", b"PRIVATE_SELF_AUTHORED_SOURCE_TEXT\n");
        let prepared = prepare_whole_text(&sandbox.project, sandbox.version_id()).unwrap();
        let bytes = serde_json::to_vec(prepared.summary()).unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("PRIVATE_SELF_AUTHORED_SOURCE_TEXT"));
        assert_eq!(
            serde_json::from_slice::<FullTextSummary>(&bytes).unwrap(),
            *prepared.summary()
        );
        let value: Value = serde_json::from_slice(&bytes).unwrap();
        for field in value.as_object().unwrap().keys() {
            let mut missing = value.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(
                serde_json::from_value::<FullTextSummary>(missing).is_err(),
                "missing {field}"
            );
        }
        let mut unknown = value.clone();
        unknown["source_text"] = json!("unexpected");
        assert!(serde_json::from_value::<FullTextSummary>(unknown).is_err());
        let duplicate = format!(
            "{{\"schema_version\":1,{}",
            String::from_utf8(bytes).unwrap().strip_prefix('{').unwrap()
        );
        assert!(serde_json::from_str::<FullTextSummary>(&duplicate).is_err());
        assert!(serde_json::from_value::<FullTextSummary>(json!([1, "bad"])).is_err());
        for (field, invalid) in [
            ("schema_version", json!(2)),
            ("revision", json!(0)),
            ("project_id", json!("not-a-uuid")),
            ("raw_sha256", json!("bad")),
            ("raw_byte_length", json!(67108865)),
            ("normalized_byte_length", json!(0)),
            ("span_count", json!(0)),
            ("evidence_span_count", json!(0)),
            ("normalization_version", json!("future")),
            ("parser_version", json!("future")),
        ] {
            let mut bad = value.clone();
            bad[field] = invalid;
            assert!(
                serde_json::from_value::<FullTextSummary>(bad).is_err(),
                "invalid {field}"
            );
        }
        sandbox.protect();
    }
}
