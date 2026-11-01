//! Compile a reviewed fixture to a new, protected test workspace.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::json;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use uuid::Uuid;

use crate::compile_model::{
    CheckFact, CompileContext, CompileReport, Dimension, PublicationRef, ReportPaths,
};
use crate::compile_report;
use crate::compile_store::Store;
use crate::fixture::CommandOutput;
use crate::fixture_input::{LoadedCase, load};
use crate::fixture_knowledge::{validate_coverage, validate_knowledge};
use crate::fixture_model::{CoverageDisposition, Issue, ResolvedTarget};
use crate::fixture_plan::{validate_expected, validate_plan};
use crate::fixture_render::{self, ReadBackFile, RenderedFile, RenderedNote};
use crate::project::ProjectError;

pub(crate) trait CompileHooks {
    fn after_render(&mut self, _notes: &mut Vec<RenderedNote>) -> Result<(), ProjectError> {
        Ok(())
    }
    fn checkpoint(&mut self, _stage: &str, _root: &Path) -> Result<(), ProjectError> {
        Ok(())
    }
}

struct Noop;
impl CompileHooks for Noop {}

fn now() -> Result<String, ProjectError> {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|error| ProjectError::new("COMPILE_TIME_ERROR", 1, error.to_string()))
}

fn issue(error: &ProjectError) -> Issue {
    Issue::new(error.code(), Vec::new(), &error.to_string())
}

fn fact(issues: &[Issue], ran: bool) -> CheckFact {
    CheckFact {
        status: if !ran {
            "not_run"
        } else if issues.is_empty() {
            "passed"
        } else {
            "failed"
        }
        .into(),
        issue_count: issues.len() as u64,
    }
}

fn contracts(case: &LoadedCase) -> (BTreeMap<String, CheckFact>, Vec<Issue>, Vec<ResolvedTarget>) {
    let sources = case.source_issues.clone();
    let ran = sources.is_empty();
    let coverage = if ran {
        validate_coverage(case)
    } else {
        Vec::new()
    };
    let knowledge = if ran {
        validate_knowledge(case)
    } else {
        Vec::new()
    };
    let (plan, mut targets) = if ran {
        validate_plan(case)
    } else {
        (Vec::new(), Vec::new())
    };
    let expected = if ran {
        validate_expected(case)
    } else {
        Vec::new()
    };
    if !plan.is_empty() {
        targets.clear();
    }
    let checks = BTreeMap::from([
        ("sources".into(), fact(&sources, true)),
        ("coverage".into(), fact(&coverage, ran)),
        ("knowledge".into(), fact(&knowledge, ran)),
        ("note_plan".into(), fact(&plan, ran)),
        ("expected".into(), fact(&expected, ran)),
    ]);
    let issues = [&sources, &coverage, &knowledge, &plan, &expected]
        .into_iter()
        .flat_map(|items| items.iter().cloned())
        .collect();
    (checks, issues, targets)
}

fn context(case: &LoadedCase) -> Result<CompileContext, ProjectError> {
    let counts = BTreeMap::from([
        ("sources".into(), case.manifest.sources.len() as u64),
        ("spans".into(), case.spans.len() as u64),
        ("atoms".into(), case.ir.atoms.len() as u64),
        (
            "required_atoms".into(),
            case.ir.atoms.iter().filter(|atom| atom.required).count() as u64,
        ),
        ("notes".into(), case.plan.notes.len() as u64),
        (
            "nodes".into(),
            case.plan
                .notes
                .iter()
                .flat_map(|note| &note.sections)
                .map(|section| section.ordered_nodes.len() as u64)
                .sum(),
        ),
        (
            "included".into(),
            case.ir
                .coverage
                .iter()
                .filter(|entry| entry.disposition == CoverageDisposition::Included)
                .count() as u64,
        ),
        (
            "excluded".into(),
            case.ir
                .coverage
                .iter()
                .filter(|entry| entry.disposition == CoverageDisposition::Excluded)
                .count() as u64,
        ),
        (
            "duplicates".into(),
            case.ir
                .coverage
                .iter()
                .filter(|entry| entry.disposition == CoverageDisposition::Duplicate)
                .count() as u64,
        ),
    ]);
    let mut versions = BTreeMap::from([
        ("renderer".into(), "fixture-markdown-v1".into()),
        ("validator".into(), "fixture-contracts-v1".into()),
        ("report".into(), "fixture-compile-report-v1".into()),
        ("ir_schema".into(), case.ir.schema_version.to_string()),
        ("plan_schema".into(), case.plan.schema_version.to_string()),
    ]);
    for source in &case.manifest.sources {
        versions.insert(
            format!("normalization:{}", source.source_id),
            source.normalization_version.clone(),
        );
        versions.insert(
            format!("parser:{}", source.source_id),
            source.parser_version.clone(),
        );
    }
    for coverage in &case.ir.coverage {
        versions.insert(
            format!("coverage_policy:{}", coverage.span_id),
            coverage.policy_version.clone(),
        );
    }
    Ok(CompileContext {
        case_id: case.manifest.case_id.clone(),
        run_id: Uuid::new_v4().to_string(),
        release_id: Uuid::new_v4().to_string(),
        started_at: now()?,
        fixture_scope_id: case.ir.coverage_run_id.clone(),
        ir_revision: case.ir.revision,
        plan_revision: case.plan.revision,
        input_hashes: BTreeMap::from([
            ("case_sha256".into(), case.case_sha256.clone()),
            ("ir_sha256".into(), case.ir_sha256.clone()),
            ("plan_sha256".into(), case.plan_sha256.clone()),
            ("expected_sha256".into(), case.expected_sha256.clone()),
        ]),
        versions,
        counts,
    })
}

fn passed(checks: &BTreeMap<String, CheckFact>, name: &str) -> bool {
    checks
        .get(name)
        .is_some_and(|check| check.status == "passed" && check.issue_count == 0)
}

fn ensure_input(
    case: &LoadedCase,
    checks: &mut BTreeMap<String, CheckFact>,
) -> Result<(), ProjectError> {
    case.check_unchanged().inspect_err(|error| {
        checks.insert("sources".into(), fact(&[issue(error)], true));
    })
}

fn report(
    ctx: &CompileContext,
    checks: &BTreeMap<String, CheckFact>,
    files: &[RenderedFile],
    issues: Vec<Issue>,
    result: &str,
) -> Result<CompileReport, ProjectError> {
    let source_ok = ["sources", "coverage"]
        .iter()
        .all(|name| passed(checks, name));
    let knowledge_ok = [
        "knowledge",
        "note_plan",
        "expected",
        "staging_output",
        "vault_output",
    ]
    .iter()
    .all(|name| passed(checks, name));
    let quality_ok = [
        "sources",
        "coverage",
        "knowledge",
        "note_plan",
        "expected",
        "rendering",
        "staging_output",
        "vault_output",
    ]
    .iter()
    .all(|name| passed(checks, name));
    let mut counts = ctx.counts.clone();
    counts.insert("rendered_files".into(), files.len() as u64);
    counts.insert(
        "mappings".into(),
        files
            .iter()
            .map(|file| file.content_mappings.len() as u64)
            .sum(),
    );
    counts.insert(
        "links".into(),
        files.iter().map(|file| file.links.len() as u64).sum(),
    );
    counts.insert("issues".into(), issues.len() as u64);
    let dimension = |done: bool, yes: &str, no: &str, values: BTreeMap<String, u64>| Dimension {
        state: if done { "complete" } else { "incomplete" }.into(),
        reason: if done { yes } else { no }.into(),
        counts: values,
    };
    let dimensions = BTreeMap::from([
        (
            "source_coverage".into(),
            dimension(
                source_ok,
                "自造fixture的完整来源及覆盖处置已双向对账。",
                "来源或完整覆盖未通过，不能算完成。",
                BTreeMap::from([
                    ("spans".into(), ctx.counts["spans"]),
                    ("included".into(), ctx.counts["included"]),
                    ("excluded".into(), ctx.counts["excluded"]),
                    ("duplicates".into(), ctx.counts["duplicates"]),
                ]),
            ),
        ),
        (
            "knowledge_compilation".into(),
            dimension(
                knowledge_ok,
                "自造fixture的必要原子、条件和证据映射已在实际文件中验证。",
                "fixture知识或实际输出尚未完整验证。",
                BTreeMap::from([
                    ("required_atoms".into(), ctx.counts["required_atoms"]),
                    ("files".into(), files.len() as u64),
                ]),
            ),
        ),
        (
            "quality_validation".into(),
            dimension(
                quality_ok,
                "仅完成本轮fixture契约、实际内容、映射、清单与本地引用的确定性检查；科学质量未运行。",
                "必需的契约或实际输出检查失败或未运行。",
                BTreeMap::from([("issues".into(), issues.len() as u64)]),
            ),
        ),
        (
            "optional_research".into(),
            Dimension {
                state: "not_applicable".into(),
                reason: "本轮未启用研究、模型或网络抽取。".into(),
                counts: BTreeMap::new(),
            },
        ),
    ]);
    Ok(CompileReport {
        schema_version: 1,
        producer: "fixture".into(),
        case_id: ctx.case_id.clone(),
        run_id: ctx.run_id.clone(),
        release_id: ctx.release_id.clone(),
        started_at: ctx.started_at.clone(),
        finished_at: now()?,
        result: result.into(),
        dimensions,
        checks: checks.clone(),
        counts,
        issues,
        files: files.to_vec(),
        input_hashes: ctx.input_hashes.clone(),
        versions: ctx.versions.clone(),
        publication: PublicationRef {
            database: "state/compile.sqlite".into(),
            commit_table: "publication_commits".into(),
            release_id: ctx.release_id.clone(),
        },
    })
}

fn encode(value: &serde_json::Value) -> Result<String, ProjectError> {
    serde_json::to_string_pretty(value)
        .map(|text| format!("{text}\n"))
        .map_err(|error| ProjectError::new("COMPILE_REPORT_ERROR", 1, error.to_string()))
}

fn outcome(
    ctx: &CompileContext,
    root: &Path,
    status: &str,
    vault_created: bool,
    paths: Option<&ReportPaths>,
    facts: Option<&CompileReport>,
    issues: &[Issue],
) -> Result<CommandOutput, ProjectError> {
    let json = encode(&json!({
        "schema_version": 1, "producer":"fixture", "operation":"case_compile", "case_id":ctx.case_id,
        "run_id":ctx.run_id, "release_id":ctx.release_id, "release_status":status, "output_created":true,
        "paths":{"root":root, "vault":if vault_created { Some("vault") } else { None }, "database":"state/compile.sqlite",
            "json_report":paths.map(|value| value.json.as_str()), "markdown_report":paths.map(|value| value.markdown.as_str())},
        "report":facts, "issues":issues,
    }))?;
    Ok(CommandOutput {
        json,
        failed: status != "published",
    })
}

fn write_reports(
    store: &mut Store,
    facts: &CompileReport,
    paths: &ReportPaths,
    any_written: &mut bool,
) -> Result<(), ProjectError> {
    let json = serde_json::to_string_pretty(facts)
        .map(|text| format!("{text}\n"))
        .map_err(|error| ProjectError::new("COMPILE_REPORT_ERROR", 1, error.to_string()))?;
    let markdown = compile_report::markdown(facts);
    store.write_owned(&paths.json, json.as_bytes())?;
    *any_written = true;
    store.write_owned(&paths.markdown, markdown.as_bytes())?;
    if store.read_owned(&paths.json)? != json.as_bytes()
        || store.read_owned(&paths.markdown)? != markdown.as_bytes()
    {
        return Err(ProjectError::new(
            "COMPILE_REPORT_CHANGED",
            3,
            "双报告重读与本轮事实不一致。",
        ));
    }
    Ok(())
}

fn read_files(
    store: &Store,
    prefix: &str,
    manifest: &[RenderedFile],
) -> Result<Vec<ReadBackFile>, ProjectError> {
    store.ensure_integrity()?;
    manifest
        .iter()
        .map(|file| {
            Ok(ReadBackFile {
                relative_path: file.relative_path.clone(),
                bytes: store.read_owned(&format!("{prefix}/{}", file.relative_path))?,
            })
        })
        .collect()
}

fn output_facts(checks: &mut BTreeMap<String, CheckFact>, stage: &str, issues: &[Issue]) {
    checks.insert(stage.into(), fact(issues, true));
    for (key, rule) in [
        ("output_manifest", "OUTPUT_MANIFEST"),
        ("output_mapping", "OUTPUT_MAPPING"),
        ("output_content", "OUTPUT_CONTENT"),
        ("output_link", "OUTPUT_LINK"),
    ] {
        let selected: Vec<_> = issues
            .iter()
            .filter(|issue| issue.rule_id == rule)
            .cloned()
            .collect();
        checks.insert(key.into(), fact(&selected, true));
    }
}

pub fn compile(case_root: &Path, output_root: &Path) -> Result<CommandOutput, ProjectError> {
    compile_with_hooks(case_root, output_root, &mut Noop)
}

pub(crate) fn compile_with_hooks(
    case_root: &Path,
    output_root: &Path,
    hooks: &mut impl CompileHooks,
) -> Result<CommandOutput, ProjectError> {
    let case = load(case_root)?;
    let root = Store::preflight(case.root(), output_root)?;
    let (mut checks, mut input_issues, targets) = contracts(&case);
    for name in [
        "rendering",
        "staging_output",
        "vault_output",
        "output_manifest",
        "output_mapping",
        "output_content",
        "output_link",
        "scientific_quality",
    ] {
        checks.insert(name.into(), fact(&[], false));
    }
    if input_issues.is_empty() {
        let renderable = fixture_render::validate_renderable(&case, &targets);
        if !renderable.is_empty() {
            checks.insert("rendering".into(), fact(&renderable, true));
            input_issues.extend(renderable);
        }
        if case.ir.revision > i64::MAX as u64 || case.plan.revision > i64::MAX as u64 {
            input_issues.push(Issue::new(
                "COMPILE_REVISION_RANGE",
                Vec::new(),
                "本轮SQLite修订号超过可保存的正整数范围。",
            ));
        }
    }
    if !input_issues.is_empty() {
        return Ok(CommandOutput {
            failed: true,
            json: encode(
                &json!({"schema_version":1,"producer":"fixture","operation":"case_compile","case_id":case.manifest.case_id,
            "run_id":null,"release_id":null,"release_status":"not_started","output_created":false,
            "paths":{"root":root,"vault":null,"database":null,"json_report":null,"markdown_report":null},
            "report":null,"checks":checks,"issues":input_issues}),
            )?,
        });
    }
    case.check_unchanged()?;
    let ctx = context(&case)?;
    let mut store = Store::create(&root, &ctx, &case)?;
    let mut manifest = Vec::new();
    let mut actual_issues = Vec::new();
    let mut vault_created = false;
    let mut any_report_written = false;
    let mut stage = "rendering";
    let success_paths = ReportPaths {
        json: "reports/validation.json".into(),
        markdown: "reports/validation.md".into(),
    };
    let run = (|| -> Result<CommandOutput, ProjectError> {
        hooks.checkpoint("after_root", store.root())?;
        let mut notes = match fixture_render::render(&case, &targets) {
            Ok(notes) => notes,
            Err(issues) => {
                checks.insert("rendering".into(), fact(&issues, true));
                actual_issues.extend(issues);
                return Err(ProjectError::new(
                    "COMPILE_RENDER_FAILED",
                    3,
                    "fixture渲染未完成。",
                ));
            }
        };
        hooks.after_render(&mut notes)?;
        checks.insert("rendering".into(), fact(&[], true));
        for (index, note) in notes.iter().enumerate() {
            store.write_owned(
                &format!("staging/vault/{}", note.file.relative_path),
                &note.bytes,
            )?;
            manifest.push(note.file.clone());
            if index == 0 {
                hooks.checkpoint("after_first_staged_file", store.root())?;
            }
        }
        store.record_staged(&manifest)?;
        hooks.checkpoint("after_staged", store.root())?;
        stage = "staging_output";
        let staged = read_files(&store, "staging/vault", &manifest)?;
        let issues = fixture_render::validate_rendered(&case, &targets, &manifest, &staged);
        output_facts(&mut checks, "staging_output", &issues);
        if !issues.is_empty() {
            actual_issues.extend(issues);
            return Err(ProjectError::new(
                "COMPILE_OUTPUT_INVALID",
                3,
                "实际暂存输出未通过独立内容、映射或引用校验。",
            ));
        }
        hooks.checkpoint("after_staging_validation", store.root())?;
        stage = "vault_output";
        for file in &staged {
            store.write_owned(&format!("vault/{}", file.relative_path), &file.bytes)?;
            vault_created = true;
        }
        let vault = read_files(&store, "vault", &manifest)?;
        let issues = fixture_render::validate_rendered(&case, &targets, &manifest, &vault);
        output_facts(&mut checks, "vault_output", &issues);
        if !issues.is_empty() {
            actual_issues.extend(issues);
            return Err(ProjectError::new(
                "COMPILE_OUTPUT_INVALID",
                3,
                "实际Vault重读未通过完整验证。",
            ));
        }
        store.record_verified(&manifest)?;
        hooks.checkpoint("after_vault", store.root())?;
        stage = "reports";
        ensure_input(&case, &mut checks)?;
        hooks.checkpoint("before_reports", store.root())?;
        let facts = report(&ctx, &checks, &manifest, Vec::new(), "validated")?;
        write_reports(&mut store, &facts, &success_paths, &mut any_report_written)?;
        stage = "publication";
        hooks.checkpoint("before_commit", store.root())?;
        ensure_input(&case, &mut checks)?;
        store.ensure_integrity()?;
        // Prepare all output before the last authoritative transaction. An output
        // transport error afterwards must leave the committed release untouched.
        let result = outcome(
            &ctx,
            store.root(),
            "published",
            vault_created,
            Some(&success_paths),
            Some(&facts),
            &[],
        )?;
        store.commit_publication(&facts, &manifest, &success_paths)?;
        Ok(result)
    })();
    match run {
        Ok(result) => Ok(result),
        Err(error) => {
            if let Some(check) = checks.get_mut(stage) {
                if check.status == "not_run" {
                    *check = fact(&[issue(&error)], true);
                }
            }
            actual_issues.push(issue(&error));
            let status = if vault_created
                || matches!(
                    stage,
                    "staging_output" | "vault_output" | "reports" | "publication"
                ) {
                "recovery_required"
            } else {
                "staged"
            };
            let facts = report(&ctx, &checks, &manifest, actual_issues.clone(), "failed")?;
            let failure_paths = if any_report_written {
                ReportPaths {
                    json: "reports/failure.json".into(),
                    markdown: "reports/failure.md".into(),
                }
            } else {
                success_paths
            };
            match write_reports(&mut store, &facts, &failure_paths, &mut any_report_written) {
                Ok(()) => {
                    if let Err(record_error) = store.record_failure(&facts, status, &failure_paths)
                    {
                        if let Err(recovery_error) = store.recover(
                            &facts,
                            "recovery_required",
                            &failure_paths,
                            &record_error,
                        ) {
                            actual_issues.push(issue(&recovery_error));
                        }
                        actual_issues.push(issue(&record_error));
                        return outcome(
                            &ctx,
                            store.root(),
                            "recovery_required",
                            vault_created,
                            Some(&failure_paths),
                            Some(&facts),
                            &actual_issues,
                        );
                    }
                    outcome(
                        &ctx,
                        store.root(),
                        status,
                        vault_created,
                        Some(&failure_paths),
                        Some(&facts),
                        &actual_issues,
                    )
                }
                Err(report_error) => {
                    actual_issues.push(issue(&report_error));
                    if let Err(recovery_error) =
                        store.recover(&facts, "recovery_required", &failure_paths, &report_error)
                    {
                        actual_issues.push(issue(&recovery_error));
                    }
                    outcome(
                        &ctx,
                        store.root(),
                        "recovery_required",
                        vault_created,
                        None,
                        None,
                        &actual_issues,
                    )
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    struct Sandbox(PathBuf);
    impl Sandbox {
        fn new() -> Self {
            let parent =
                Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/compile-workflow-tests");
            fs::create_dir_all(&parent).unwrap();
            let root = parent
                .canonicalize()
                .unwrap()
                .join(Uuid::new_v4().to_string());
            fs::create_dir(&root).unwrap();
            Self(root)
        }
        fn output(&self) -> PathBuf {
            self.0.join("output")
        }
        fn fixture(&self) -> PathBuf {
            let source =
                Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/text-compare-v1");
            let target = self.0.join("case");
            copy_tree(&source, &target);
            target
        }
    }
    impl Drop for Sandbox {
        fn drop(&mut self) {
            let parent = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/compile-workflow-tests")
                .canonicalize()
                .unwrap();
            let root = self.0.canonicalize().unwrap();
            assert!(root.starts_with(&parent) && root != parent);
            fs::remove_dir_all(&root).unwrap();
        }
    }
    fn copy_tree(source: &Path, target: &Path) {
        fs::create_dir(target).unwrap();
        for entry in fs::read_dir(source).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                copy_tree(&entry.path(), &target.join(entry.file_name()));
            } else {
                fs::copy(entry.path(), target.join(entry.file_name())).unwrap();
            }
        }
    }
    fn connection(root: &Path) -> rusqlite::Connection {
        rusqlite::Connection::open_with_flags(
            root.join("state/compile.sqlite"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap()
    }
    fn state(root: &Path) -> (String, String, String, i64) {
        let db = connection(root);
        let (run, stage): (String, String) = db
            .query_row("SELECT state,stage FROM runs", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .unwrap();
        let release: String = db
            .query_row("SELECT status FROM releases", [], |row| row.get(0))
            .unwrap();
        let commits: i64 = db
            .query_row("SELECT count(*) FROM publication_commits", [], |row| {
                row.get(0)
            })
            .unwrap();
        (run, stage, release, commits)
    }
    fn failed(result: CommandOutput, root: &Path) -> serde_json::Value {
        assert!(result.failed);
        let value: serde_json::Value = serde_json::from_str(&result.json).unwrap();
        assert_ne!(value["release_status"], "published");
        assert_ne!(state(root).0, "succeeded");
        assert_ne!(state(root).2, "published");
        assert_eq!(state(root).3, 0);
        assert!(!value["issues"].as_array().unwrap().is_empty());
        value
    }
    fn reports(value: &serde_json::Value, root: &Path) -> serde_json::Value {
        let facts: serde_json::Value = serde_json::from_slice(
            &fs::read(root.join(value["paths"]["json_report"].as_str().unwrap())).unwrap(),
        )
        .unwrap();
        assert_eq!(&facts, &value["report"]);
        assert_eq!(facts["result"], "failed");
        let markdown =
            fs::read_to_string(root.join(value["paths"]["markdown_report"].as_str().unwrap()))
                .unwrap()
                .replace("\\_", "_");
        for (name, dimension) in facts["dimensions"].as_object().unwrap() {
            assert!(markdown.contains(&format!(
                "| {name} | {} |",
                dimension["state"].as_str().unwrap()
            )));
            for (key, count) in dimension["counts"].as_object().unwrap() {
                assert!(markdown.contains(&format!("| {name} | {key} | {count} |")));
            }
        }
        for (key, count) in facts["counts"].as_object().unwrap() {
            assert!(markdown.contains(&format!("| overall | {key} | {count} |")));
        }
        facts
    }
    struct StopAt(&'static str);
    impl CompileHooks for StopAt {
        fn checkpoint(&mut self, stage: &str, _root: &Path) -> Result<(), ProjectError> {
            if stage == self.0 {
                Err(ProjectError::new("TEST_CHECKPOINT_FAILURE", 3, stage))
            } else {
                Ok(())
            }
        }
    }
    #[test]
    fn first_staged_file_failure_keeps_partial_scene_and_truthful_reports() {
        let sandbox = Sandbox::new();
        let fixture = sandbox.fixture();
        let root = sandbox.output();
        let value = failed(
            compile_with_hooks(&fixture, &root, &mut StopAt("after_first_staged_file")).unwrap(),
            &root,
        );
        assert_eq!(value["release_status"], "staged");
        let facts = reports(&value, &root);
        assert_eq!(facts["files"].as_array().unwrap().len(), 1);
        assert_eq!(facts["checks"]["staging_output"]["status"], "not_run");
        assert_eq!(facts["checks"]["vault_output"]["status"], "not_run");
        assert_eq!(fs::read_dir(root.join("vault")).unwrap().count(), 0);
        assert_eq!(state(&root).0, "failed");
        assert!(
            facts["issues"]
                .as_array()
                .unwrap()
                .iter()
                .any(|issue| issue["rule_id"] == "TEST_CHECKPOINT_FAILURE")
        );
    }
    struct DropUnit;
    impl CompileHooks for DropUnit {
        fn after_render(&mut self, notes: &mut Vec<RenderedNote>) -> Result<(), ProjectError> {
            let note = notes
                .iter_mut()
                .find(|note| String::from_utf8_lossy(&note.bytes).contains("5 cm"))
                .unwrap();
            let start = String::from_utf8_lossy(&note.bytes).find("5 cm").unwrap() + 1;
            note.bytes.drain(start..start + 3);
            let shift = |offset: &mut u64| {
                if *offset >= (start + 3) as u64 {
                    *offset -= 3;
                }
            };
            for mapping in &mut note.file.content_mappings {
                shift(&mut mapping.start_byte);
                shift(&mut mapping.end_byte);
            }
            for link in &mut note.file.links {
                shift(&mut link.start_byte);
                shift(&mut link.end_byte);
            }
            note.file.content_sha256 = crate::text::sha256(&note.bytes);
            note.file.byte_length = note.bytes.len() as u64;
            Ok(())
        }
    }
    #[test]
    fn actual_unit_loss_with_refreshed_hash_and_offsets_is_blocked() {
        let sandbox = Sandbox::new();
        let fixture = sandbox.fixture();
        let root = sandbox.output();
        let value = failed(
            compile_with_hooks(&fixture, &root, &mut DropUnit).unwrap(),
            &root,
        );
        let facts = reports(&value, &root);
        assert_eq!(facts["checks"]["output_manifest"]["status"], "passed");
        assert_eq!(value["release_status"], "recovery_required");
        assert_eq!(state(&root).2, "recovery_required");
        assert_eq!(facts["checks"]["output_content"]["status"], "failed");
        assert_eq!(facts["checks"]["staging_output"]["status"], "failed");
        assert!(
            facts["issues"]
                .as_array()
                .unwrap()
                .iter()
                .any(|issue| issue["rule_id"] == "OUTPUT_CONTENT")
        );
        assert_eq!(fs::read_dir(root.join("vault")).unwrap().count(), 0);
    }
    struct Intrude {
        stage: &'static str,
        path: &'static str,
    }
    impl CompileHooks for Intrude {
        fn checkpoint(&mut self, stage: &str, root: &Path) -> Result<(), ProjectError> {
            if stage == self.stage {
                fs::write(root.join(self.path), "人工内容不得覆盖\n").unwrap();
            }
            Ok(())
        }
    }
    #[test]
    fn unknown_staged_file_is_preserved_without_publication() {
        let sandbox = Sandbox::new();
        let fixture = sandbox.fixture();
        let root = sandbox.output();
        let value = failed(
            compile_with_hooks(
                &fixture,
                &root,
                &mut Intrude {
                    stage: "after_staged",
                    path: "staging/vault/manual.md",
                },
            )
            .unwrap(),
            &root,
        );
        assert_eq!(
            fs::read_to_string(root.join("staging/vault/manual.md")).unwrap(),
            "人工内容不得覆盖\n"
        );
        assert_eq!(value["release_status"], "recovery_required");
        assert!(value["report"].is_null());
        assert_eq!(fs::read_dir(root.join("vault")).unwrap().count(), 0);
        assert_eq!(state(&root).2, "staged");
    }
    #[test]
    fn occupied_report_path_is_preserved_and_does_not_claim_a_report_pair() {
        let sandbox = Sandbox::new();
        let fixture = sandbox.fixture();
        let root = sandbox.output();
        let value = failed(
            compile_with_hooks(
                &fixture,
                &root,
                &mut Intrude {
                    stage: "before_reports",
                    path: "reports/validation.md",
                },
            )
            .unwrap(),
            &root,
        );
        assert_eq!(
            fs::read_to_string(root.join("reports/validation.md")).unwrap(),
            "人工内容不得覆盖\n"
        );
        assert!(value["paths"]["json_report"].is_null());
        assert!(value["paths"]["markdown_report"].is_null());
        assert!(value["report"].is_null());
        assert_eq!(state(&root).2, "verified");
    }
    struct HoldReader {
        reader: Option<rusqlite::Connection>,
    }
    impl CompileHooks for HoldReader {
        fn checkpoint(&mut self, stage: &str, root: &Path) -> Result<(), ProjectError> {
            if stage == "before_commit" {
                let reader = connection(root);
                reader.execute_batch("BEGIN; SELECT * FROM runs;").unwrap();
                self.reader = Some(reader);
            }
            Ok(())
        }
    }
    #[test]
    fn real_sqlite_commit_busy_rolls_back_all_success_records() {
        let sandbox = Sandbox::new();
        let fixture = sandbox.fixture();
        let root = sandbox.output();
        let mut hook = HoldReader { reader: None };
        let value = failed(
            compile_with_hooks(&fixture, &root, &mut hook).unwrap(),
            &root,
        );
        let facts = reports(&value, &root);
        assert!(
            value["issues"]
                .as_array()
                .unwrap()
                .iter()
                .any(|issue| issue["rule_id"] == "COMPILE_DATABASE_ERROR")
        );
        assert_eq!(facts["result"], "failed");
        assert_eq!(value["release_status"], "recovery_required");
        assert!(root.join("state/recovery.json").is_file());
        assert_eq!(state(&root).2, "verified");
        hook.reader
            .take()
            .unwrap()
            .execute_batch("ROLLBACK")
            .unwrap();
        assert_eq!(state(&root).3, 0);
        let coverage: i64 = connection(&root)
            .query_row(
                "SELECT count(*) FROM coverage_records WHERE stage='validated'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(coverage, 0);
        assert_eq!(
            compile(&fixture, &root).err().unwrap().code(),
            "COMPILE_OUTPUT_EXISTS"
        );
    }
    struct ChangeInput(PathBuf);
    impl CompileHooks for ChangeInput {
        fn checkpoint(&mut self, stage: &str, _root: &Path) -> Result<(), ProjectError> {
            if stage == "before_commit" {
                let path = self.0.join("sources/材料甲.md");
                let mut bytes = fs::read(&path).unwrap();
                bytes.extend_from_slice(b"\nchanged\n");
                #[cfg(not(windows))]
                fs::write(path, bytes).unwrap();
                #[cfg(windows)]
                assert!(
                    fs::write(path, bytes).is_err(),
                    "selected input must remain guarded against an active Windows writer"
                );
            }
            Ok(())
        }
    }
    #[test]
    #[cfg(not(windows))]
    fn late_selected_input_change_marks_source_check_failed() {
        let sandbox = Sandbox::new();
        let fixture = sandbox.fixture();
        let root = sandbox.output();
        let value = failed(
            compile_with_hooks(&fixture, &root, &mut ChangeInput(fixture.clone())).unwrap(),
            &root,
        );
        let facts = reports(&value, &root);
        assert_eq!(facts["checks"]["sources"]["status"], "failed");
        assert_eq!(
            facts["dimensions"]["source_coverage"]["state"],
            "incomplete"
        );
        assert_eq!(
            facts["dimensions"]["quality_validation"]["state"],
            "incomplete"
        );
    }
    #[test]
    #[cfg(windows)]
    fn active_windows_input_writer_is_blocked_and_original_bytes_publish() {
        let sandbox = Sandbox::new();
        let fixture = sandbox.fixture();
        let root = sandbox.output();
        let source = fixture.join("sources/材料甲.md");
        let original = fs::read(&source).unwrap();
        let result =
            compile_with_hooks(&fixture, &root, &mut ChangeInput(fixture.clone())).unwrap();
        assert!(!result.failed);
        assert_eq!(fs::read(source).unwrap(), original);
        assert_eq!(
            state(&root),
            ("succeeded".into(), "finished".into(), "published".into(), 1)
        );
    }
    struct ChildCheckpoint(PathBuf);
    impl CompileHooks for ChildCheckpoint {
        fn checkpoint(&mut self, stage: &str, _root: &Path) -> Result<(), ProjectError> {
            if stage == "after_staged" {
                fs::write(&self.0, b"persisted staged\n").unwrap();
                let mut line = String::new();
                std::io::stdin().read_line(&mut line).unwrap();
            }
            Ok(())
        }
    }
    #[test]
    fn checkpoint_child() {
        let Some(case) = std::env::var_os("NOETOME_TEST_CHECKPOINT_CASE") else {
            return;
        };
        let output = std::env::var_os("NOETOME_TEST_CHECKPOINT_OUTPUT").unwrap();
        let marker = std::env::var_os("NOETOME_TEST_CHECKPOINT_MARKER").unwrap();
        compile_with_hooks(
            Path::new(&case),
            Path::new(&output),
            &mut ChildCheckpoint(marker.into()),
        )
        .unwrap();
    }
    #[test]
    fn actual_process_kill_after_persisted_staging_retains_unpublished_state() {
        let sandbox = Sandbox::new();
        let fixture = sandbox.fixture();
        let root = sandbox.output();
        let marker = sandbox.0.join("staged-ready");
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "fixture_compile::tests::checkpoint_child",
                "--nocapture",
            ])
            .env("NOETOME_TEST_CHECKPOINT_CASE", &fixture)
            .env("NOETOME_TEST_CHECKPOINT_OUTPUT", &root)
            .env("NOETOME_TEST_CHECKPOINT_MARKER", &marker)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        while !marker.is_file() {
            if let Some(status) = child.try_wait().unwrap() {
                panic!("checkpoint child stopped before staging: {status}");
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("checkpoint child did not persist staging");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(
            state(&root),
            ("running".into(), "staged".into(), "staged".into(), 0)
        );
        assert!(child.try_wait().unwrap().is_none());
        child.kill().unwrap();
        assert!(!child.wait().unwrap().success());
        assert_eq!(
            state(&root),
            ("running".into(), "staged".into(), "staged".into(), 0)
        );
        let db = connection(&root);
        let files: i64 = db
            .query_row("SELECT count(*) FROM rendered_files", [], |row| row.get(0))
            .unwrap();
        assert_eq!(files, 2);
        assert_eq!(fs::read_dir(root.join("vault")).unwrap().count(), 0);
        assert!(!root.join("reports/validation.json").exists());
        assert_eq!(
            compile(&fixture, &root).err().unwrap().code(),
            "COMPILE_OUTPUT_EXISTS"
        );
    }
}
