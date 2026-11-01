//! Guarded, bounded and explicitly declared fixture input.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{File, Metadata};
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::fixture_model::*;
use crate::project::{self, ProjectError};
use crate::text;

const MAX_TOTAL: u64 = 64 * 1024 * 1024;
const MAX_JSON: u64 = 4 * 1024 * 1024;

struct GuardedInput {
    path: PathBuf,
    file: File,
    identity: same_file::Handle,
    before: Metadata,
}

impl GuardedInput {
    fn check(&self) -> Result<(), ProjectError> {
        project::check_components(&self.path)?;
        project::checked_file(&self.path)?;
        let metadata = self
            .file
            .metadata()
            .map_err(|error| ProjectError::io(&self.path, error))?;
        let current = same_file::Handle::from_path(&self.path)
            .map_err(|error| ProjectError::io(&self.path, error))?;
        if current != self.identity
            || metadata.len() != self.before.len()
            || metadata.modified().ok() != self.before.modified().ok()
        {
            return Err(ProjectError::new(
                "CASE_INPUT_CHANGED",
                3,
                "fixture文件在只读校验期间发生变化。",
            ));
        }
        Ok(())
    }
}

struct Reader {
    root: PathBuf,
    root_identity: same_file::Handle,
    guards: Vec<GuardedInput>,
    paths: BTreeSet<String>,
    total: u64,
}

impl Reader {
    fn read(&mut self, relative: &str, limit: u64) -> Result<Vec<u8>, ProjectError> {
        self.check_root()?;
        if !valid_relative_path(relative) || !self.paths.insert(relative.to_lowercase()) {
            return Err(ProjectError::new(
                "CASE_INVALID_PATH",
                2,
                "fixture只接受互不碰撞的普通相对文件路径。",
            ));
        }
        let path = project::check_components(&self.root.join(relative))?;
        project::checked_file(&path)?;
        let mut file = project::read_guard(&path)?;
        let before = file
            .metadata()
            .map_err(|error| ProjectError::io(&path, error))?;
        let total = self
            .total
            .checked_add(before.len())
            .ok_or_else(input_too_large)?;
        if before.len() > limit || total > MAX_TOTAL {
            return Err(input_too_large());
        }
        let identity = same_file::Handle::from_file(
            file.try_clone()
                .map_err(|error| ProjectError::io(&path, error))?,
        )
        .map_err(|error| ProjectError::io(&path, error))?;
        let mut bytes = Vec::new();
        (&mut file)
            .take(limit + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| ProjectError::io(&path, error))?;
        if bytes.len() as u64 != before.len() {
            return Err(ProjectError::new(
                "CASE_INPUT_CHANGED",
                3,
                "fixture文件未能稳定完整读取；没有构造截断输入。",
            ));
        }
        let guard = GuardedInput {
            path,
            file,
            identity,
            before,
        };
        guard.check()?;
        self.total = total;
        self.guards.push(guard);
        Ok(bytes)
    }

    fn check_root(&self) -> Result<(), ProjectError> {
        project::check_components(&self.root)?;
        let current = same_file::Handle::from_path(&self.root)
            .map_err(|error| ProjectError::io(&self.root, error))?;
        if current != self.root_identity {
            return Err(ProjectError::new(
                "CASE_INPUT_CHANGED",
                3,
                "明确选取的fixture根目录身份发生变化。",
            ));
        }
        Ok(())
    }
}

fn input_too_large() -> ProjectError {
    ProjectError::new(
        "CASE_INPUT_TOO_LARGE",
        3,
        "fixture总原始输入最多64 MiB，单份JSON最多4 MiB。",
    )
}

#[derive(Debug, Clone)]
pub(crate) struct ActualSpan {
    pub(crate) source_id: String,
    pub(crate) version_id: String,
    pub(crate) declared: DeclaredSpan,
    pub(crate) text: String,
}

pub(crate) struct LoadedCase {
    pub(crate) manifest: Manifest,
    pub(crate) ir: KnowledgeIr,
    pub(crate) plan: PlanPackage,
    pub(crate) expected: ExpectedPackage,
    pub(crate) spans: BTreeMap<String, ActualSpan>,
    pub(crate) source_issues: Vec<Issue>,
    pub(crate) ir_sha256: String,
    pub(crate) plan_sha256: String,
    pub(crate) expected_sha256: String,
    pub(crate) case_sha256: String,
    guards: Vec<GuardedInput>,
    root: PathBuf,
    root_identity: same_file::Handle,
}

impl LoadedCase {
    pub(crate) fn check_unchanged(&self) -> Result<(), ProjectError> {
        project::check_components(&self.root)?;
        let current = same_file::Handle::from_path(&self.root)
            .map_err(|error| ProjectError::io(&self.root, error))?;
        if current != self.root_identity {
            return Err(ProjectError::new(
                "CASE_INPUT_CHANGED",
                3,
                "fixture根目录发生替换；没有把混合视图算作通过。",
            ));
        }
        for guard in &self.guards {
            guard.check()?;
        }
        Ok(())
    }
}

pub(crate) fn valid_relative_path(value: &str) -> bool {
    if value.is_empty() || value.len() > 4096 {
        return false;
    }
    value.split('/').all(|part| {
        if part.is_empty()
            || part == "."
            || part == ".."
            || part.ends_with([' ', '.'])
            || part
                .chars()
                .any(|ch| ch.is_control() || "<>:\"\\|?*".contains(ch))
        {
            return false;
        }
        let stem = part
            .split('.')
            .next()
            .unwrap_or("")
            .trim_end_matches([' ', '.'])
            .to_uppercase();
        !matches!(
            stem.as_str(),
            "CON" | "PRN" | "AUX" | "NUL" | "CLOCK$" | "CONIN$" | "CONOUT$"
        ) && !["1", "2", "3", "4", "5", "6", "7", "8", "9", "¹", "²", "³"]
            .iter()
            .any(|suffix| stem == format!("COM{suffix}") || stem == format!("LPT{suffix}"))
    })
}

fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8], name: &str) -> Result<T, ProjectError> {
    serde_json::from_slice(bytes).map_err(|error| {
        ProjectError::new(
            "CASE_INVALID_JSON",
            2,
            format!("{name}不符合严格fixture JSON契约：{error}"),
        )
    })
}

fn schema(version: u32) -> Result<(), ProjectError> {
    if version == 1 {
        Ok(())
    } else {
        Err(ProjectError::new(
            "CASE_UNSUPPORTED_SCHEMA",
            2,
            "fixture只支持Schema 1；没有迁移输入。",
        ))
    }
}

fn source_issue(issues: &mut Vec<Issue>, source: &FixtureSource, detail: &str) {
    issues.push(Issue::new(
        "SOURCE_CONTRACT",
        vec![source.source_id.clone(), source.version_id.clone()],
        detail,
    ));
}

pub(crate) fn load(root: &Path) -> Result<LoadedCase, ProjectError> {
    let root = project::check_components(root)?;
    if !root.is_dir() {
        return Err(ProjectError::new(
            "CASE_INVALID_PATH",
            2,
            "case路径必须为已有普通目录。",
        ));
    }
    let root_identity =
        same_file::Handle::from_path(&root).map_err(|error| ProjectError::io(&root, error))?;
    let mut reader = Reader {
        root,
        root_identity,
        guards: Vec::new(),
        paths: BTreeSet::new(),
        total: 0,
    };
    let case_bytes = reader.read("case.json", MAX_JSON)?;
    let manifest: Manifest = decode(&case_bytes, "case.json")?;
    schema(manifest.schema_version)?;
    if !is_id(&manifest.case_id) || manifest.sources.is_empty() || manifest.sources.len() > 32 {
        return Err(ProjectError::new(
            "CASE_INVALID_MANIFEST",
            2,
            "fixture需要有效固定case身份和1至32份明确来源。",
        ));
    }
    for name in [
        &manifest.ir_file,
        &manifest.note_plan_file,
        &manifest.expected_file,
    ] {
        if !name.ends_with(".json") {
            return Err(ProjectError::new(
                "CASE_INVALID_PATH",
                2,
                "IR、计划和独立预期须声明.json普通相对文件。",
            ));
        }
    }
    let ir_bytes = reader.read(&manifest.ir_file, MAX_JSON)?;
    let plan_bytes = reader.read(&manifest.note_plan_file, MAX_JSON)?;
    let expected_bytes = reader.read(&manifest.expected_file, MAX_JSON)?;
    let ir: KnowledgeIr = decode(&ir_bytes, &manifest.ir_file)?;
    let plan: PlanPackage = decode(&plan_bytes, &manifest.note_plan_file)?;
    let expected: ExpectedPackage = decode(&expected_bytes, &manifest.expected_file)?;
    schema(ir.schema_version)?;
    schema(plan.schema_version)?;
    schema(expected.schema_version)?;
    let mut spans = BTreeMap::new();
    let mut source_issues = Vec::new();
    let mut source_ids = BTreeSet::new();
    let mut version_ids = BTreeSet::new();
    let mut span_ids = BTreeSet::new();
    for source in &manifest.sources {
        if !is_id(&source.source_id)
            || !is_id(&source.version_id)
            || !source_ids.insert(source.source_id.clone())
            || !version_ids.insert(source.version_id.clone())
        {
            source_issue(
                &mut source_issues,
                source,
                "来源及版本固定身份必须有效且唯一。",
            );
        }
        let extension = source
            .relative_path
            .rsplit('.')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        if !matches!(
            (source.media_type.as_str(), extension.as_str()),
            ("text/plain", "txt") | ("text/markdown", "md")
        ) {
            return Err(ProjectError::new(
                "CASE_UNSUPPORTED_SOURCE",
                2,
                "fixture仅接受与类型一致的MD/TXT来源。",
            ));
        }
        let raw = reader.read(&source.relative_path, MAX_TOTAL)?;
        let parsed = text::analyze(&raw).map_err(|error| {
            ProjectError::new(
                "CASE_INVALID_UTF8",
                2,
                format!(
                    "来源{}不是完整有效UTF8，原始字节位置{}。",
                    source.source_id,
                    error.valid_up_to()
                ),
            )
        })?;
        if !is_sha(&source.content_sha256)
            || source.content_sha256 != text::sha256(&raw)
            || !is_sha(&source.normalized_sha256)
            || source.normalized_sha256 != parsed.normalized_sha256
            || source.normalization_version != text::NORMALIZATION_VERSION
            || source.parser_version != text::PARSER_VERSION
        {
            source_issue(
                &mut source_issues,
                source,
                "原始/规范SHA或解析规则版本与实际来源不符。",
            );
        }
        if source.spans.len() != parsed.spans.len() {
            source_issue(
                &mut source_issues,
                source,
                "声明Span清单没有完整覆盖实际规范文本。",
            );
        }
        for (ordinal, declared) in source.spans.iter().enumerate() {
            if !is_id(&declared.span_id) || !span_ids.insert(declared.span_id.clone()) {
                source_issue(
                    &mut source_issues,
                    source,
                    "Span固定身份必须全包唯一且有效。",
                );
            }
            let Some(draft) = parsed.spans.get(ordinal) else {
                continue;
            };
            if declared.ordinal != ordinal
                || declared.kind != draft.kind
                || declared.start_byte != draft.start_byte
                || declared.end_byte != draft.end_byte
                || declared.start_line != draft.start_line
                || declared.end_line != draft.end_line
                || declared.text_sha256 != draft.text_sha256
                || declared.parse_status != draft.parse_status
            {
                source_issue(
                    &mut source_issues,
                    source,
                    "Span类型、顺序、位置、行号、指纹或解析状态不符实际分段。",
                );
            }
            let text = parsed
                .text
                .get(draft.start_byte..draft.end_byte)
                .ok_or_else(|| {
                    ProjectError::new(
                        "CASE_INVALID_SPAN",
                        2,
                        "实际解析器产生了非UTF8区间，未创建虚假原文。",
                    )
                })?
                .to_owned();
            spans
                .entry(declared.span_id.clone())
                .or_insert_with(|| ActualSpan {
                    source_id: source.source_id.clone(),
                    version_id: source.version_id.clone(),
                    declared: declared.clone(),
                    text,
                });
        }
    }
    let loaded = LoadedCase {
        manifest,
        ir,
        plan,
        expected,
        spans,
        source_issues,
        ir_sha256: text::sha256(&ir_bytes),
        plan_sha256: text::sha256(&plan_bytes),
        expected_sha256: text::sha256(&expected_bytes),
        case_sha256: text::sha256(&case_bytes),
        guards: reader.guards,
        root: reader.root,
        root_identity: reader.root_identity,
    };
    loaded.check_unchanged()?;
    Ok(loaded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_relative_paths_preserve_unicode_and_spaces() {
        for value in [
            "case.json",
            "sources/材料甲.md",
            "笔记/甲 容器.md",
            "x/y-z_1.txt",
        ] {
            assert!(valid_relative_path(value), "rejected {value}");
        }
    }

    #[test]
    fn real_fixture_loading_rebuilds_twelve_spans_without_writing() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/text-compare-v1")
            .canonicalize()
            .unwrap();
        let before = std::fs::read(root.join("case.json")).unwrap();
        let case = load(&root).expect("the fixture is fully declared and must load");
        assert!(case.source_issues.is_empty(), "{:?}", case.source_issues);
        assert_eq!(case.spans.len(), 12);
        assert_eq!(std::fs::read(root.join("case.json")).unwrap(), before);
    }

    #[test]
    fn unsafe_relative_names_are_rejected_before_filesystem_access() {
        for value in [
            "",
            "/x.md",
            "C:/x.md",
            "../x.md",
            "a/../b.md",
            "a\\b.md",
            "a//b.md",
            "a/./b.md",
            "a/b.md:stream",
            "a/CON.md",
            "a/NUL .md",
            "a/COM¹.md",
            "a/LPT9.txt",
            "a/b.md.",
            "a/b.md ",
            "a/\0b.md",
        ] {
            assert!(!valid_relative_path(value), "accepted {value:?}");
        }
    }

    #[test]
    fn nullable_coverage_fields_are_required_even_when_the_value_is_null() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/text-compare-v1");
        let ir: serde_json::Value =
            serde_json::from_slice(&std::fs::read(root.join("ir.json")).unwrap()).unwrap();
        for name in ["duplicate_of_span_id", "exclusion_reason"] {
            let mut row = ir["coverage"][0].clone();
            row.as_object_mut().unwrap().remove(name);
            let bytes = serde_json::to_vec(&row).unwrap();
            assert!(
                decode::<CoverageRecord>(&bytes, "coverage").is_err(),
                "accepted missing {name}"
            );
        }
        let bytes = serde_json::to_vec(&ir["coverage"][0]).unwrap();
        assert!(decode::<CoverageRecord>(&bytes, "coverage").is_ok());
    }

    struct TestRoot(PathBuf);

    impl TestRoot {
        fn new() -> Self {
            let parent =
                Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/fixture-loader-tests");
            std::fs::create_dir_all(&parent).unwrap();
            let root = parent
                .canonicalize()
                .unwrap()
                .join(uuid::Uuid::new_v4().to_string());
            std::fs::create_dir(&root).unwrap();
            Self(root)
        }
        fn reader(&self) -> Reader {
            Reader {
                root: self.0.clone(),
                root_identity: same_file::Handle::from_path(&self.0).unwrap(),
                guards: Vec::new(),
                paths: BTreeSet::new(),
                total: 0,
            }
        }
    }

    impl Drop for TestRoot {
        fn drop(&mut self) {
            let parent = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/fixture-loader-tests")
                .canonicalize()
                .unwrap();
            assert!(self.0.canonicalize().unwrap().starts_with(parent));
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn json_size_is_rejected_before_decoding_or_reading_the_large_file() {
        let root = TestRoot::new();
        let path = root.0.join("large.json");
        let file = File::create(&path).unwrap();
        file.set_len(MAX_JSON + 1).unwrap();
        drop(file);
        let before = std::fs::metadata(&path).unwrap();
        let mut reader = root.reader();
        let error = reader.read("large.json", MAX_JSON).unwrap_err();
        assert_eq!(error.code(), "CASE_INPUT_TOO_LARGE");
        assert!(reader.guards.is_empty());
        let after = std::fs::metadata(&path).unwrap();
        assert_eq!(before.len(), after.len());
        assert_eq!(before.modified().unwrap(), after.modified().unwrap());
    }

    #[test]
    fn aggregate_size_and_path_reuse_are_checked_before_the_next_read() {
        let root = TestRoot::new();
        std::fs::write(root.0.join("small.json"), "{}").unwrap();
        let file = File::create(root.0.join("large.txt")).unwrap();
        file.set_len(MAX_TOTAL).unwrap();
        drop(file);
        let mut reader = root.reader();
        reader.read("small.json", MAX_JSON).unwrap();
        let error = reader.read("large.txt", MAX_TOTAL).unwrap_err();
        assert_eq!(error.code(), "CASE_INPUT_TOO_LARGE");
        assert_eq!(reader.guards.len(), 1);
        let error = reader.read("SMALL.JSON", MAX_JSON).unwrap_err();
        assert_eq!(error.code(), "CASE_INVALID_PATH");
        assert_eq!(reader.guards.len(), 1);
    }
}
