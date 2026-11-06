//! Recoverable execution of one finite fixture compile graph.
use crate::{
    compile_model::{CheckFact, ReportPaths},
    compile_store::{FileExpectation, Store, StoreIntent},
    fixture::CommandOutput,
    fixture_compile as compiler,
    fixture_input::{LoadedCase, load},
    fixture_model::ResolvedTarget,
    fixture_render,
    project::{self, ProjectError},
    run_model::*,
    run_store::{Journal, error},
};
use serde_json::json;
use std::{collections::BTreeMap, path::Path};

pub(crate) trait RunHooks {
    fn checkpoint(
        &mut self,
        _event: &str,
        _unit: &str,
        _journal: &Journal,
    ) -> Result<(), ProjectError> {
        Ok(())
    }
}
struct Noop;
impl RunHooks for Noop {}

fn validate(
    case: &LoadedCase,
) -> Result<(BTreeMap<String, CheckFact>, Vec<ResolvedTarget>), ProjectError> {
    let (checks, mut issues, targets) = compiler::contracts(case);
    if issues.is_empty() {
        issues.extend(fixture_render::validate_renderable(case, &targets));
    }
    if !issues.is_empty() {
        return Err(error(
            "RUN_CONTRACT",
            format!(
                "fixture契约未通过：{}",
                serde_json::to_string(&issues).unwrap_or_default()
            ),
        ));
    }
    case.check_unchanged()?;
    Ok((checks, targets))
}
pub fn start(case_root: &Path, root: &Path, paused: bool) -> Result<CommandOutput, ProjectError> {
    start_with_hooks(case_root, root, paused, &mut Noop)
}
pub(crate) fn start_with_hooks(
    case_root: &Path,
    root: &Path,
    paused: bool,
    hooks: &mut impl RunHooks,
) -> Result<CommandOutput, ProjectError> {
    let case = load(case_root)?;
    validate(&case)?;
    let context = compiler::context(&case)?;
    let mut journal = Journal::create(&case, root, context, paused)?;
    journal.acquire()?;
    hooks.checkpoint("created", "", &journal)?;
    if !paused {
        execute(&case, &mut journal, hooks)?;
    }
    envelope(&journal, "run_start", None, None)
}
pub fn resume(case_root: &Path, root: &Path) -> Result<CommandOutput, ProjectError> {
    resume_with_hooks(case_root, root, &mut Noop)
}
pub(crate) fn resume_with_hooks(
    case_root: &Path,
    root: &Path,
    hooks: &mut impl RunHooks,
) -> Result<CommandOutput, ProjectError> {
    let mut journal = Journal::open_for_resume(root, case_root)?;
    journal.acquire()?;
    journal.reload()?;
    let case = match load(case_root) {
        Ok(c) => c,
        Err(e) => {
            invalidate(&mut journal, &e)?;
            return Err(e);
        }
    };
    if let Err(e) = journal.verify_inputs(&case) {
        invalidate(&mut journal, &e)?;
        return Err(e);
    }
    if matches!(
        journal.state.status,
        RunStatus::Cancelled | RunStatus::Invalidated
    ) {
        return Err(error(
            "RUN_TERMINAL",
            "运行已取消或版本失效；新版本须新建运行。",
        ));
    }
    validate(&case)?;
    recover_output(&case, &journal)?;
    verify_output(&case, &journal, true)?;
    if journal.state.status != RunStatus::Succeeded {
        let (n, action) = journal.latest_control()?;
        if action.as_deref() == Some("cancel") {
            apply_controls(&mut journal)?;
            return Err(error("RUN_CANCELLED", "已取消的运行不会继续调度。"));
        }
        let mut state = journal.state.clone();
        state.status = RunStatus::Queued;
        state.acknowledged_control = n;
        journal.append("resumed", state)?;
        hooks.checkpoint("resumed", "", &journal)?;
        execute(&case, &mut journal, hooks)?;
    } else {
        verify_output(&case, &journal, false)?;
        acknowledge_completed_controls(&mut journal, hooks)?;
    }
    envelope(&journal, "run_resume", None, None)
}
fn invalidate(journal: &mut Journal, cause: &ProjectError) -> Result<(), ProjectError> {
    if !matches!(
        journal.state.status,
        RunStatus::Cancelled | RunStatus::Invalidated
    ) {
        let mut s = journal.state.clone();
        s.status = RunStatus::Invalidated;
        s.diagnostic = Some(format!("{}: {}", cause.code(), cause));
        journal.append("invalidated", s)?;
    }
    Ok(())
}
pub fn status(root: &Path) -> Result<CommandOutput, ProjectError> {
    let journal = Journal::open(root, false)?;
    let case = load(Path::new(&journal.metadata.owner.case_root))?;
    journal.verify_inputs(&case)?;
    validate(&case)?;
    verify_output(&case, &journal, true)?;
    let release = restore_store(&case, &journal, true, false)?
        .map(|store| store.snapshot().map(|s| s.phase))
        .transpose()?;
    envelope(&journal, "run_status", None, release)
}
pub fn control(root: &Path, action: &str) -> Result<CommandOutput, ProjectError> {
    let mut journal = Journal::open(root, true)?;
    let case = load(Path::new(&journal.metadata.owner.case_root))?;
    journal.verify_inputs(&case)?;
    if matches!(
        journal.state.status,
        RunStatus::Cancelled | RunStatus::Succeeded | RunStatus::Invalidated | RunStatus::Failed
    ) {
        return Err(error("RUN_TERMINAL", "终态运行不接受新控制。"));
    }
    verify_output(&case, &journal, true)?;
    let release = restore_store(&case, &journal, true, false)?
        .map(|store| store.snapshot().map(|s| s.phase))
        .transpose()?;
    let sequence = journal.request(action)?;
    if journal.try_acquire() {
        journal.reload()?;
        apply_controls(&mut journal)?;
    }
    envelope(
        &journal,
        if action == "pause" {
            "run_pause"
        } else {
            "run_cancel"
        },
        Some(sequence),
        release,
    )
}
fn apply_controls(journal: &mut Journal) -> Result<bool, ProjectError> {
    let (n, action) = journal.latest_control()?;
    if n <= journal.state.acknowledged_control {
        return Ok(false);
    }
    let mut state = journal.state.clone();
    state.acknowledged_control = n;
    state.status = if action.as_deref() == Some("cancel") {
        RunStatus::Cancelled
    } else {
        RunStatus::Paused
    };
    journal.append(
        if state.status == RunStatus::Cancelled {
            "cancelled"
        } else {
            "paused"
        },
        state,
    )?;
    Ok(true)
}
fn progress(
    case: &LoadedCase,
    journal: &mut Journal,
    store: &Store,
    state: &mut RunState,
    hooks: &mut impl RunHooks,
    unit: &str,
) -> Result<(), ProjectError> {
    journal.verify_inputs(case)?;
    state.output_snapshot = Some(store.snapshot()?);
    journal.append("unit_progress", state.clone())?;
    hooks.checkpoint("unit_progress", unit, journal)
}
fn acknowledge_completed_controls(
    journal: &mut Journal,
    hooks: &mut impl RunHooks,
) -> Result<(), ProjectError> {
    let (sequence, _) = journal.latest_control()?;
    if sequence <= journal.state.acknowledged_control
        || journal.state.status != RunStatus::Succeeded
    {
        return Ok(());
    }
    let mut state = journal.state.clone();
    state.acknowledged_control = sequence;
    state.diagnostic = Some("completed before control took effect".into());
    journal.append("completed_control", state)?;
    hooks.checkpoint("completed_control", "publication", journal)
}
fn write_once(store: &mut Store, path: &str, bytes: &[u8]) -> Result<bool, ProjectError> {
    if project::exists(&store.root().join(path))? {
        if store.read_owned(path)? != bytes {
            return Err(error("RUN_OUTPUT_CHANGED", "输出与当前单元期望不符。"));
        }
        Ok(false)
    } else {
        store.write_owned(path, bytes)?;
        Ok(true)
    }
}
fn intent_files(
    case: &LoadedCase,
    state: &RunState,
) -> Result<
    (
        String,
        Vec<fixture_render::RenderedFile>,
        BTreeMap<String, Vec<u8>>,
    ),
    ProjectError,
> {
    let index = state.confirmed();
    let mut files = BTreeMap::new();
    let mut manifest = state.manifest.clone();
    let phase = match index {
        0 => "planned",
        1 | 3 => {
            let (_, targets) = validate(case)?;
            let notes = fixture_render::render(case, &targets)
                .map_err(|_| error("RUN_RENDER", "无法重建当前intent的确定性内容。"))?;
            manifest = notes.iter().map(|n| n.file.clone()).collect();
            for note in notes {
                files.insert(
                    format!(
                        "{}/{}",
                        if index == 1 { "staging/vault" } else { "vault" },
                        note.file.relative_path
                    ),
                    note.bytes,
                );
            }
            if index == 1 { "staged" } else { "verified" }
        }
        2 => "staged",
        4 => {
            let facts = state
                .validation_report
                .as_ref()
                .ok_or_else(|| error("RUN_SCHEMA", "报告intent缺少已持久事实。"))?;
            files.insert(
                "reports/validation.json".into(),
                format!(
                    "{}\n",
                    serde_json::to_string_pretty(facts)
                        .map_err(|e| error("RUN_SCHEMA", e.to_string()))?
                )
                .into_bytes(),
            );
            files.insert(
                "reports/validation.md".into(),
                crate::compile_report::markdown(facts).into_bytes(),
            );
            "verified"
        }
        5 => "published",
        _ => return Err(error("RUN_SCHEMA", "终态不允许存在pending intent。")),
    };
    Ok((phase.into(), manifest, files))
}
fn restore_store(
    case: &LoadedCase,
    journal: &Journal,
    reconcile: bool,
    writable: bool,
) -> Result<Option<Store>, ProjectError> {
    restore_parts(
        case,
        journal.root(),
        &journal.metadata.context,
        &journal.state,
        reconcile,
        writable,
    )
}
fn restore_parts(
    case: &LoadedCase,
    run_root: &Path,
    context: &crate::compile_model::CompileContext,
    state: &RunState,
    reconcile: bool,
    writable: bool,
) -> Result<Option<Store>, ProjectError> {
    let root = run_root.join("output");
    if let Some(intent) = &state.pending_intent {
        let (phase, manifest, files) = intent_files(case, state)?;
        let expectations = files
            .iter()
            .map(|(p, b)| {
                (
                    p.clone(),
                    FileExpectation {
                        sha256: crate::text::sha256(b),
                        byte_length: b.len() as u64,
                    },
                )
            })
            .collect();
        if intent.phase != phase || intent.manifest != manifest || intent.files != expectations {
            return Err(error(
                "RUN_INTENT",
                "持久intent与当前来源、单元或已保存报告不符。",
            ));
        }
        if !project::exists(&root)? && state.output_snapshot.is_none() {
            return Ok(None);
        }
        return if writable {
            Store::resume_with_intent(
                &root,
                context,
                case,
                state.output_snapshot.as_ref(),
                intent,
                &files,
            )
        } else {
            Store::resume_intent_readonly(
                &root,
                context,
                case,
                state.output_snapshot.as_ref(),
                intent,
                &files,
            )
        }
        .map(Some);
    }
    if let Some(snapshot) = &state.output_snapshot {
        return if writable {
            Store::resume(
                &root,
                context,
                case,
                snapshot,
                reconcile && state.confirmed() == 5,
            )
        } else {
            Store::resume_readonly(
                &root,
                context,
                case,
                snapshot,
                reconcile && state.confirmed() == 5,
            )
        }
        .map(Some);
    }
    if project::exists(&root)? {
        return Err(error(
            "RUN_RECOVERY_REQUIRED",
            "输出根尚无可信检查点或prepared intent，保留原件。",
        ));
    }
    Ok(None)
}
fn recover_output(case: &LoadedCase, journal: &Journal) -> Result<(), ProjectError> {
    let root = journal.root().join("output");
    if !project::exists(&root.join("state/compile.sqlite-journal"))? {
        return Ok(());
    }
    journal.verify()?;
    journal.verify_inputs(case)?;
    let snapshot = journal.state.output_snapshot.as_ref().ok_or_else(|| {
        error(
            "RUN_RECOVERY_REQUIRED",
            "无已有确认数据库不能接管bootstrap热日志。",
        )
    })?;
    let files = if let Some(intent) = &journal.state.pending_intent {
        let (phase, manifest, files) = intent_files(case, &journal.state)?;
        let expected = files
            .iter()
            .map(|(p, b)| {
                (
                    p.clone(),
                    FileExpectation {
                        sha256: crate::text::sha256(b),
                        byte_length: b.len() as u64,
                    },
                )
            })
            .collect();
        if intent.phase != phase || intent.manifest != manifest || intent.files != expected {
            return Err(error("RUN_INTENT", "恢复前intent与来源派生不符。"));
        }
        files
    } else {
        BTreeMap::new()
    };
    Store::recover_hot_journal(
        &root,
        &journal.metadata.context,
        case,
        snapshot,
        journal.state.pending_intent.as_ref(),
        &files,
    )
}
fn verify_output(
    case: &LoadedCase,
    journal: &Journal,
    reconcile: bool,
) -> Result<(), ProjectError> {
    verify_outputs_parts(
        case,
        journal.root(),
        &journal.metadata.context,
        &journal.state,
        reconcile,
    )
}
pub(crate) fn verify_recovery_outputs(
    case: &LoadedCase,
    root: &Path,
    context: &crate::compile_model::CompileContext,
    state: &RunState,
) -> Result<(), ProjectError> {
    validate(case)?;
    verify_outputs_parts(case, root, context, state, true)
}
fn verify_outputs_parts(
    case: &LoadedCase,
    root: &Path,
    context: &crate::compile_model::CompileContext,
    state: &RunState,
    reconcile: bool,
) -> Result<(), ProjectError> {
    if let Some(store) = restore_parts(case, root, context, state, reconcile, false)? {
        let (_, targets) = validate(case)?;
        if state.confirmed() >= 2 {
            let staged = compiler::read_files(&store, "staging/vault", &state.manifest)?;
            if !fixture_render::validate_rendered(case, &targets, &state.manifest, &staged)
                .is_empty()
            {
                return Err(error(
                    "RUN_OUTPUT_CHANGED",
                    "实际暂存与来源/独立expected失配。",
                ));
            }
        }
        if state.confirmed() >= 4 {
            let vault = compiler::read_files(&store, "vault", &state.manifest)?;
            if !fixture_render::validate_rendered(case, &targets, &state.manifest, &vault)
                .is_empty()
            {
                return Err(error(
                    "RUN_OUTPUT_CHANGED",
                    "实际Vault与来源/独立expected失配。",
                ));
            }
        }
    }
    Ok(())
}
fn execute(
    case: &LoadedCase,
    journal: &mut Journal,
    hooks: &mut impl RunHooks,
) -> Result<(), ProjectError> {
    journal.verify_inputs(case)?;
    verify_output(case, journal, true)?;
    let (_, targets) = validate(case)?;
    let mut store = restore_store(case, journal, true, true)?;
    while journal.state.confirmed() < 6 {
        if apply_controls(journal)? {
            break;
        }
        journal.verify_inputs(case)?;
        journal.verify()?;
        let index = journal.state.confirmed();
        let unit = UNITS[index];
        let mut state = journal.state.clone();
        state.status = RunStatus::Running;
        state.units[index].state = UnitStatus::Running;
        state.units[index].attempts += 1;
        journal.append("unit_started", state.clone())?;
        hooks.checkpoint("unit_started", unit, journal)?;
        if index == 4 && state.validation_report.is_none() {
            state.validation_report = Some(compiler::report(
                &journal.metadata.context,
                &state.checks,
                &state.manifest,
                vec![],
                "validated",
            )?);
        }
        let (phase, manifest, files) = intent_files(case, &state)?;
        if index == 1 {
            state.manifest = manifest.clone();
        }
        state.pending_intent = Some(StoreIntent {
            phase,
            manifest,
            files: files
                .iter()
                .map(|(p, b)| {
                    (
                        p.clone(),
                        FileExpectation {
                            sha256: crate::text::sha256(b),
                            byte_length: b.len() as u64,
                        },
                    )
                })
                .collect(),
        });
        journal.append("unit_prepared", state.clone())?;
        hooks.checkpoint("unit_prepared", unit, journal)?;
        let operation = (|| -> Result<(), ProjectError> {
            match index {
                0 => {
                    state.checks = validate(case)?.0;
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
                        state.checks.insert(name.into(), compiler::fact(&[], false));
                    }
                    if store.is_none() {
                        store = Some(Store::create(
                            &journal.root().join("output"),
                            &journal.metadata.context,
                            case,
                        )?);
                        hooks.checkpoint("output_committed", unit, journal)?;
                    }
                    progress(
                        case,
                        journal,
                        store.as_ref().unwrap(),
                        &mut state,
                        hooks,
                        unit,
                    )?;
                }
                1 => {
                    let s = store
                        .as_mut()
                        .ok_or_else(|| error("RUN_SCHEMA", "缺少已确认来源单元。"))?;
                    let notes = fixture_render::render(case, &targets)
                        .map_err(|_| error("RUN_RENDER", "确定性fixture渲染失败。"))?;
                    state.manifest = notes.iter().map(|n| n.file.clone()).collect();
                    state
                        .checks
                        .insert("rendering".into(), compiler::fact(&[], true));
                    for note in &notes {
                        if write_once(
                            s,
                            &format!("staging/vault/{}", note.file.relative_path),
                            &note.bytes,
                        )? {
                            hooks.checkpoint("output_committed", unit, journal)?;
                        }
                        progress(case, journal, s, &mut state, hooks, unit)?;
                    }
                    if s.snapshot()?.phase == "planned" {
                        s.record_staged(&state.manifest)?;
                        hooks.checkpoint("output_committed", unit, journal)?;
                        progress(case, journal, s, &mut state, hooks, unit)?;
                    }
                }
                2 => {
                    let s = store.as_ref().unwrap();
                    let actual = compiler::read_files(s, "staging/vault", &state.manifest)?;
                    let issues =
                        fixture_render::validate_rendered(case, &targets, &state.manifest, &actual);
                    if !issues.is_empty() {
                        return Err(error(
                            "RUN_STAGE_INVALID",
                            "暂存未通过完整来源、语义、映射、链接校验。",
                        ));
                    }
                    compiler::output_facts(&mut state.checks, "staging_output", &issues);
                    progress(case, journal, s, &mut state, hooks, unit)?;
                }
                3 => {
                    let s = store.as_mut().unwrap();
                    let staged = compiler::read_files(s, "staging/vault", &state.manifest)?;
                    for file in &staged {
                        if write_once(s, &format!("vault/{}", file.relative_path), &file.bytes)? {
                            hooks.checkpoint("output_committed", unit, journal)?;
                        }
                        progress(case, journal, s, &mut state, hooks, unit)?;
                    }
                    let vault = compiler::read_files(s, "vault", &state.manifest)?;
                    let issues =
                        fixture_render::validate_rendered(case, &targets, &state.manifest, &vault);
                    if !issues.is_empty() {
                        return Err(error(
                            "RUN_VAULT_INVALID",
                            "Vault未通过完整来源、语义、映射、链接校验。",
                        ));
                    }
                    compiler::output_facts(&mut state.checks, "vault_output", &issues);
                    if s.snapshot()?.phase == "staged" {
                        s.record_verified(&state.manifest)?;
                        hooks.checkpoint("output_committed", unit, journal)?;
                    }
                    progress(case, journal, s, &mut state, hooks, unit)?;
                }
                4 => {
                    let s = store.as_mut().unwrap();
                    case.check_unchanged()?;
                    if state.validation_report.is_none() {
                        state.validation_report = Some(compiler::report(
                            &journal.metadata.context,
                            &state.checks,
                            &state.manifest,
                            vec![],
                            "validated",
                        )?);
                        progress(case, journal, s, &mut state, hooks, unit)?;
                    }
                    let facts = state.validation_report.clone().unwrap();
                    let raw = format!(
                        "{}\n",
                        serde_json::to_string_pretty(&facts)
                            .map_err(|e| error("RUN_SCHEMA", e.to_string()))?
                    );
                    if write_once(s, "reports/validation.json", raw.as_bytes())? {
                        hooks.checkpoint("output_committed", unit, journal)?;
                    }
                    progress(case, journal, s, &mut state, hooks, unit)?;
                    if write_once(
                        s,
                        "reports/validation.md",
                        crate::compile_report::markdown(&facts).as_bytes(),
                    )? {
                        hooks.checkpoint("output_committed", unit, journal)?;
                    }
                    progress(case, journal, s, &mut state, hooks, unit)?;
                }
                5 => {
                    let s = store.as_mut().unwrap();
                    let paths = ReportPaths {
                        json: "reports/validation.json".into(),
                        markdown: "reports/validation.md".into(),
                    };
                    journal.verify_inputs(case)?;
                    if !s.snapshot()?.published {
                        s.commit_publication(
                            state
                                .validation_report
                                .as_ref()
                                .ok_or_else(|| error("RUN_SCHEMA", "没有真实验证报告。"))?,
                            &state.manifest,
                            &paths,
                        )?;
                        hooks.checkpoint("publication_committed", unit, journal)?;
                    }
                }
                _ => unreachable!(),
            }
            Ok(())
        })();
        if let Err(cause) = operation {
            let mut failed = journal.state.clone();
            failed.status = if matches!(cause.code(), "RUN_INPUT_INVALIDATED" | "RUN_INPUT_CHANGED")
            {
                RunStatus::Invalidated
            } else {
                RunStatus::Failed
            };
            failed.diagnostic = Some(format!("{}: {}", cause.code(), cause));
            journal.append(
                if failed.status == RunStatus::Invalidated {
                    "invalidated"
                } else {
                    "failed"
                },
                failed,
            )?;
            return Err(cause);
        }
        case.check_unchanged()?;
        let s = store.as_ref().unwrap();
        state.output_snapshot = Some(s.snapshot()?);
        state.units[index].state = UnitStatus::Confirmed;
        state.pending_intent = None;
        state.units[index].output_paths = match index {
            1 => state
                .manifest
                .iter()
                .map(|f| format!("output/staging/vault/{}", f.relative_path))
                .collect(),
            3 => state
                .manifest
                .iter()
                .map(|f| format!("output/vault/{}", f.relative_path))
                .collect(),
            4 => vec![
                "output/reports/validation.json".into(),
                "output/reports/validation.md".into(),
            ],
            _ => vec!["output/state/compile.sqlite".into()],
        };
        state.status = if index == 5 {
            RunStatus::Succeeded
        } else {
            RunStatus::Running
        };
        journal.append("unit_confirmed", state)?;
        hooks.checkpoint("unit_confirmed", unit, journal)?;
    }
    acknowledge_completed_controls(journal, hooks)?;
    Ok(())
}
fn envelope(
    journal: &Journal,
    operation: &str,
    request: Option<u64>,
    actual_release: Option<String>,
) -> Result<CommandOutput, ProjectError> {
    let (control_sequence, action) = journal.latest_control()?;
    let s = &journal.state;
    let release_status = actual_release.as_deref().unwrap_or_else(|| {
        s.output_snapshot
            .as_ref()
            .map(|snap| {
                if snap.published {
                    "published"
                } else {
                    snap.phase.as_str()
                }
            })
            .unwrap_or("not_started")
    });
    let raw = json!({"schema_version":1,"producer":"fixture_run","operation":operation,"run_id":journal.metadata.context.run_id,"release_id":journal.metadata.context.release_id,"run_status":s.status,"checkpoint_sequence":journal.sequence,"confirmed_units":s.confirmed(),"work_units":s.units,"release_status":release_status,"controls":{"latest_sequence":control_sequence,"action":action,"acknowledged_sequence":s.acknowledged_control,"request_sequence":request},"input_hashes":journal.metadata.context.input_hashes,"compiler_fingerprint":journal.metadata.owner.compiler_fingerprint,"policy_version":POLICY,"paths":{"root":journal.root(),"journal":"control/run.sqlite","output":"output","vault":"output/vault","publication_database":"output/state/compile.sqlite"},"validation_report":s.validation_report,"diagnostic":s.diagnostic,"scientific_quality":"not_run","real_model_calls":0,"paid_calls":0});
    Ok(CommandOutput {
        json: format!(
            "{}\n",
            serde_json::to_string_pretty(&raw).map_err(|e| error("RUN_SCHEMA", e.to_string()))?
        ),
        failed: matches!(
            s.status,
            RunStatus::Cancelled | RunStatus::Invalidated | RunStatus::Failed
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::BTreeMap,
        env,
        io::{self, BufRead, Write},
        path::PathBuf,
    };

    struct Handshake {
        event: String,
        unit: String,
        occurrence: u64,
        seen: BTreeMap<(String, String), u64>,
    }
    #[test]
    fn control_request_rechecks_terminal_under_sqlite_writer_lock() {
        use std::{fs, sync::mpsc::sync_channel, thread};
        let preserved = env::var_os("NOETOME_RUN_RACE_TEST_ROOT");
        let base = preserved.as_ref().map(PathBuf::from).unwrap_or_else(|| {
            env::temp_dir().join(format!("noetome-run-control-race-{}", uuid::Uuid::new_v4()))
        });
        assert!(!base.exists());
        fs::create_dir(&base).unwrap();
        struct Cleanup {
            path: PathBuf,
            preserve: bool,
        }
        impl Drop for Cleanup {
            fn drop(&mut self) {
                if !self.preserve {
                    let _ = fs::remove_dir_all(&self.path);
                }
            }
        }
        let _cleanup = Cleanup {
            path: base.clone(),
            preserve: preserved.is_some(),
        };
        fn copy(from: &Path, to: &Path) {
            fs::create_dir(to).unwrap();
            for entry in fs::read_dir(from).unwrap() {
                let entry = entry.unwrap();
                if entry.file_type().unwrap().is_dir() {
                    copy(&entry.path(), &to.join(entry.file_name()));
                } else {
                    fs::copy(entry.path(), to.join(entry.file_name())).unwrap();
                }
            }
        }
        let fixture = base.join("fixture");
        let root = base.join("run");
        copy(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/run-checkpoint-v1/fixture"),
            &fixture,
        );
        let (published, at_publication) = sync_channel(0);
        let (finish, finish_worker) = sync_channel(0);
        struct PublicationHold {
            ready: std::sync::mpsc::SyncSender<()>,
            release: std::sync::mpsc::Receiver<()>,
        }
        impl RunHooks for PublicationHold {
            fn checkpoint(
                &mut self,
                event: &str,
                _unit: &str,
                _journal: &Journal,
            ) -> Result<(), ProjectError> {
                if event == "publication_committed" {
                    self.ready.send(()).unwrap();
                    self.release.recv().unwrap();
                }
                Ok(())
            }
        }
        let worker_case = fixture.clone();
        let worker_root = root.clone();
        let worker = thread::spawn(move || {
            start_with_hooks(
                &worker_case,
                &worker_root,
                false,
                &mut PublicationHold {
                    ready: published,
                    release: finish_worker,
                },
            )
        });
        at_publication.recv().unwrap();
        let (checked, controller_checked) = sync_channel(0);
        let (insert, allow_insert) = sync_channel(0);
        let controller_root = root.clone();
        let controller = thread::spawn(move || {
            let mut journal = Journal::open(&controller_root, true).unwrap();
            journal.request_with_before_transaction("pause", || {
                checked.send(()).unwrap();
                allow_insert.recv().unwrap();
            })
        });
        controller_checked.recv().unwrap();
        finish.send(()).unwrap();
        let completed = worker.join().unwrap().unwrap();
        assert!(!completed.failed);
        let before = fs::read(root.join("control/run.sqlite")).unwrap();
        let original = Journal::open(&root, false).unwrap();
        assert_eq!(original.state.status, RunStatus::Succeeded);
        assert_eq!(original.state.confirmed(), 6);
        assert_eq!(original.latest_control().unwrap(), (0, None));
        drop(original);
        insert.send(()).unwrap();
        let result = controller.join().unwrap();
        let after = fs::read(root.join("control/run.sqlite")).unwrap();
        let journal = Journal::open(&root, false).unwrap();
        let control = journal.latest_control().unwrap();
        let receipt = json!({"producer":"actual_control_transaction_race","worker_executed_all_six_units":true,"worker_result":serde_json::from_str::<serde_json::Value>(&completed.json).unwrap(),"controller_checked_before_worker_finished":true,"controller_transaction_after_worker_finished":true,"controller_result":result.as_ref().map(|n|*n).map_err(|e|e.code()),"controls":control,"database_unchanged":before==after,"database_before_sha256":crate::text::sha256(&before),"database_after_sha256":crate::text::sha256(&after),"real_model_calls":0,"paid_calls":0});
        fs::write(
            base.join("actual-race.json"),
            serde_json::to_vec_pretty(&receipt).unwrap(),
        )
        .unwrap();
        fs::write(base.join("before.sqlite"), before).unwrap();
        fs::write(base.join("after.sqlite"), after).unwrap();
        assert_eq!(result.err().map(|e| e.code()), Some("RUN_TERMINAL"));
        assert_eq!(control, (0, None));
        assert!(receipt["database_unchanged"].as_bool().unwrap());
    }
    impl RunHooks for Handshake {
        fn checkpoint(
            &mut self,
            event: &str,
            unit: &str,
            journal: &Journal,
        ) -> Result<(), ProjectError> {
            let count = self.seen.entry((event.into(), unit.into())).or_default();
            *count += 1;
            if self.event != "trace"
                && (event != self.event || unit != self.unit || *count != self.occurrence)
            {
                return Ok(());
            }
            let line = json!({"producer":"run_checkpoint_test_handshake","event":event,"unit":unit,"occurrence":count,"sequence":journal.sequence,"run_id":journal.metadata.context.run_id,"pid":std::process::id(),"compiler_fingerprint":env!("NOETOME_RUN_COMPILER_FINGERPRINT")});
            {
                let mut stdout = io::stdout().lock();
                writeln!(stdout, "{line}")
                    .and_then(|_| stdout.flush())
                    .map_err(|e| error("RUN_TEST_HANDSHAKE", e.to_string()))?;
            }
            let mut release = String::new();
            io::stdin()
                .lock()
                .read_line(&mut release)
                .map_err(|e| error("RUN_TEST_HANDSHAKE", e.to_string()))?;
            if release.trim() != "release" {
                return Err(error("RUN_TEST_HANDSHAKE", "外部审计尚未明确释放边界。"));
            }
            Ok(())
        }
    }
    #[test]
    fn checkpoint_child() {
        let Some(case) = env::var_os("NOETOME_RUN_TEST_CASE") else {
            return;
        };
        let root = PathBuf::from(env::var_os("NOETOME_RUN_TEST_ROOT").expect("explicit test root"));
        let mut hook = Handshake {
            event: env::var("NOETOME_RUN_TEST_EVENT").unwrap(),
            unit: env::var("NOETOME_RUN_TEST_UNIT").unwrap(),
            occurrence: env::var("NOETOME_RUN_TEST_OCCURRENCE")
                .unwrap()
                .parse()
                .unwrap(),
            seen: BTreeMap::new(),
        };
        let result = match env::var("NOETOME_RUN_TEST_OPERATION").unwrap().as_str() {
            "start" => start_with_hooks(Path::new(&case), &root, false, &mut hook),
            "resume" => resume_with_hooks(Path::new(&case), &root, &mut hook),
            _ => panic!("unknown test operation"),
        }
        .expect("real fixture task completed after explicit release");
        let value: serde_json::Value = serde_json::from_str(&result.json).unwrap();
        assert!(matches!(
            value["run_status"].as_str(),
            Some("succeeded" | "paused" | "cancelled")
        ));
    }
}
