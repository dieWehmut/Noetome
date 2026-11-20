#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, WindowEvent};

mod desktop_data;
mod external_path;
mod preferences;
mod project_entry;
mod tasks;
mod workbench;
use preferences::{CloseBehavior, PreferencesState};

const MAIN_TRAY_ID: &str = "noetome-main-tray";
const TRAY_SHOW_ID: &str = "noetome-tray-show";
const TRAY_EXIT_ID: &str = "noetome-tray-exit";

#[derive(Serialize)]
struct RuntimeInfo {
    app_version: &'static str,
    cli_version: &'static str,
    target: &'static str,
    capabilities: &'static [&'static str],
    cli_help: &'static str,
}

#[tauri::command]
fn runtime_info() -> RuntimeInfo {
    RuntimeInfo {
        app_version: env!("CARGO_PKG_VERSION"),
        cli_version: noetome_cli::VERSION,
        target: env!("NOETOME_TARGET"),
        capabilities: &[
            "help",
            "version",
            "project.create",
            "project.open",
            "task.read",
            "source.pick",
            "project.create_in_parent",
            "source.import",
            "source.choices",
            "source.units",
            "source.extract",
            "source.preview_export",
            "model.status",
            "model.credential_store",
            "model.budget_restore",
            "desktop.self_test",
            "source.cancel",
            "source.saved",
            "corpus.pick",
            "corpus.preflight",
            "corpus.import",
            "corpus.list",
            "corpus.status",
            "corpus.pages",
            "corpus.process",
            "corpus.pause",
            "corpus.retry",
            "compile.create",
            "compile.run",
            "compile.list",
            "compile.status",
            "compile.control",
            "compile.query",
            "compile.evidence",
            "compile.export",
        ],
        cli_help: noetome_cli::HELP,
    }
}

#[tauri::command]
fn cli_preview(option: &str) -> Result<String, String> {
    let flag = match option {
        "help" => "--help",
        "version" => "--version",
        _ => return Err(noetome_cli::INVALID_ARGUMENT.to_owned()),
    };
    noetome_cli::render(&[OsString::from(flag)]).map_err(str::to_owned)
}

#[tauri::command]
fn pick_directory() -> Result<Option<String>, String> {
    project_entry::pick_directory()
}

#[tauri::command]
fn project_create(path: String) -> Result<project_entry::ProjectSummary, String> {
    project_entry::create(Path::new(&path))
}

#[tauri::command]
fn project_open(path: String) -> Result<project_entry::ProjectSummary, String> {
    project_entry::open(Path::new(&path))
}

#[tauri::command]
fn task_read(kind: String, path: String) -> Result<tasks::TaskSummary, String> {
    tasks::read(&kind, Path::new(&path))
}

#[tauri::command]
fn task_kinds() -> Vec<tasks::TaskKind> {
    tasks::kinds()
}

async fn workbench_blocking<T: Send + 'static>(
    operation: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(operation)
        .await
        .map_err(|error| format!("DESKTOP_WORKER_FAILED：后台操作失败：{error}"))?
}

#[tauri::command]
async fn compile_create(
    project: String,
    source_ids: Vec<String>,
    scope: Option<noetome_cli::production_compile::CompileScope>,
    state: tauri::State<'_, workbench::WorkbenchState>,
) -> Result<noetome_cli::production_compile::RunSummary, String> {
    let state = state.inner().clone();
    workbench_blocking(move || state.compile_create_scoped(Path::new(&project), &source_ids, scope))
        .await
}
#[tauri::command]
async fn compile_run(
    project: String,
    run_id: String,
    state: tauri::State<'_, workbench::WorkbenchState>,
) -> Result<noetome_cli::production_compile::RunSummary, String> {
    let state = state.inner().clone();
    workbench_blocking(move || state.compile_run(Path::new(&project), &run_id)).await
}
#[tauri::command]
async fn compile_list(
    project: String,
) -> Result<Vec<noetome_cli::production_compile::RunSummary>, String> {
    workbench_blocking(move || {
        noetome_cli::production_compile::list(Path::new(&project))
            .map_err(|error| error.to_string())
    })
    .await
}
#[tauri::command]
async fn compile_status(
    project: String,
    run_id: String,
) -> Result<noetome_cli::production_compile::RunSummary, String> {
    workbench_blocking(move || {
        noetome_cli::production_compile::status(Path::new(&project), &run_id)
            .map_err(|error| error.to_string())
    })
    .await
}
#[tauri::command]
async fn compile_control(
    project: String,
    run_id: String,
    action: String,
) -> Result<noetome_cli::production_compile::RunSummary, String> {
    workbench_blocking(move || {
        noetome_cli::production_compile::control(Path::new(&project), &run_id, &action)
            .map_err(|error| error.to_string())
    })
    .await
}
#[tauri::command]
async fn compile_query(
    project: String,
    run_id: String,
    kind: String,
    offset: u64,
    limit: u32,
    search: String,
) -> Result<serde_json::Value, String> {
    workbench_blocking(move || {
        noetome_cli::production_compile::query(
            Path::new(&project),
            &run_id,
            &kind,
            offset,
            limit,
            &search,
        )
        .map_err(|error| error.to_string())
    })
    .await
}
#[tauri::command]
async fn compile_evidence(
    project: String,
    run_id: String,
    evidence_id: String,
) -> Result<serde_json::Value, String> {
    workbench_blocking(move || {
        noetome_cli::production_compile::evidence(Path::new(&project), &run_id, &evidence_id)
            .map_err(|error| error.to_string())
    })
    .await
}
#[tauri::command]
async fn compile_export(
    project: String,
    run_id: String,
    parent: String,
    name: String,
    state: tauri::State<'_, workbench::WorkbenchState>,
) -> Result<serde_json::Value, String> {
    let state = state.inner().clone();
    workbench_blocking(move || {
        state.compile_export(Path::new(&project), &run_id, Path::new(&parent), &name)
    })
    .await
}

#[tauri::command]
async fn source_pick() -> Result<Option<String>, String> {
    workbench_blocking(workbench::source_pick).await
}
#[tauri::command]
async fn source_saved(project: String) -> Result<serde_json::Value, String> {
    workbench_blocking(move || {
        noetome_cli::extraction_preview::saved_sources(Path::new(&project))
            .map_err(|error| error.to_string())
    })
    .await
}
#[tauri::command]
async fn corpus_pick() -> Result<Option<String>, String> {
    workbench_blocking(workbench::corpus_pick).await
}
#[tauri::command]
async fn corpus_preflight(path: String) -> Result<serde_json::Value, String> {
    workbench_blocking(move || {
        noetome_cli::corpus::preflight(Path::new(&path)).map_err(|error| error.to_string())
    })
    .await
}
#[tauri::command]
async fn corpus_import(
    project: String,
    path: String,
) -> Result<noetome_cli::corpus::SourceSummary, String> {
    workbench_blocking(move || {
        noetome_cli::corpus::import(Path::new(&project), Path::new(&path))
            .map_err(|error| error.to_string())
    })
    .await
}
#[tauri::command]
async fn corpus_list(project: String) -> Result<Vec<noetome_cli::corpus::SourceSummary>, String> {
    workbench_blocking(move || {
        noetome_cli::corpus::list(Path::new(&project)).map_err(|error| error.to_string())
    })
    .await
}
#[tauri::command]
async fn corpus_status(
    project: String,
    source_id: String,
) -> Result<noetome_cli::corpus::SourceSummary, String> {
    workbench_blocking(move || {
        noetome_cli::corpus::status(Path::new(&project), &source_id)
            .map_err(|error| error.to_string())
    })
    .await
}
#[tauri::command]
async fn corpus_pages(
    project: String,
    source_id: String,
    offset: u32,
    limit: u32,
) -> Result<serde_json::Value, String> {
    workbench_blocking(move || {
        noetome_cli::corpus::pages(Path::new(&project), &source_id, offset, limit)
            .map_err(|error| error.to_string())
    })
    .await
}
#[tauri::command]
async fn corpus_process(
    project: String,
    source_id: String,
) -> Result<noetome_cli::corpus::SourceSummary, String> {
    // Whole-book work has no desktop timeout. The Core bounds each worker batch
    // and commits pages durably; the UI polls progress independently.
    workbench_blocking(move || {
        noetome_cli::corpus::process(
            Path::new(&project),
            &source_id,
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
        .map_err(|error| error.to_string())
    })
    .await
}
#[tauri::command]
async fn corpus_pause(
    project: String,
    source_id: String,
) -> Result<noetome_cli::corpus::SourceSummary, String> {
    workbench_blocking(move || {
        noetome_cli::corpus::pause(Path::new(&project), &source_id)
            .map_err(|error| error.to_string())
    })
    .await
}
#[tauri::command]
async fn corpus_retry(
    project: String,
    source_id: String,
) -> Result<noetome_cli::corpus::SourceSummary, String> {
    workbench_blocking(move || {
        noetome_cli::corpus::retry(
            Path::new(&project),
            &source_id,
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
        .map_err(|error| error.to_string())
    })
    .await
}
#[tauri::command]
async fn project_create_in_parent(
    parent: String,
    name: String,
    state: tauri::State<'_, workbench::WorkbenchState>,
) -> Result<project_entry::ProjectSummary, String> {
    let state = state.inner().clone();
    workbench_blocking(move || state.create_project(Path::new(&parent), &name)).await
}
#[tauri::command]
async fn source_import(
    project: String,
    path: String,
    state: tauri::State<'_, workbench::WorkbenchState>,
) -> Result<workbench::ImportResult, String> {
    let state = state.inner().clone();
    workbench_blocking(move || state.import(Path::new(&project), Path::new(&path))).await
}
#[tauri::command]
async fn source_choices(project: String, import_id: String) -> Result<serde_json::Value, String> {
    workbench_blocking(move || {
        noetome_cli::extraction_preview::choices(Path::new(&project), &import_id)
            .map_err(|error| error.to_string())
    })
    .await
}
#[tauri::command]
async fn source_units(
    project: String,
    import_id: String,
    selection: Vec<serde_json::Value>,
) -> Result<serde_json::Value, String> {
    workbench_blocking(move || {
        noetome_cli::extraction_preview::selected_units(
            Path::new(&project),
            &import_id,
            &serde_json::Value::Array(selection),
        )
        .map_err(|error| error.to_string())
    })
    .await
}
#[tauri::command]
async fn source_extract(
    project: String,
    import_id: String,
    selection: Vec<serde_json::Value>,
    confirmed_units: serde_json::Value,
    state: tauri::State<'_, workbench::WorkbenchState>,
) -> Result<workbench::ExtractResult, String> {
    let state = state.inner().clone();
    workbench_blocking(move || {
        state.extract(
            Path::new(&project),
            &import_id,
            serde_json::Value::Array(selection),
            confirmed_units,
        )
    })
    .await
}
#[tauri::command]
async fn source_preview_export(
    project: String,
    import_id: String,
    result_root: String,
    parent: String,
    name: String,
    state: tauri::State<'_, workbench::WorkbenchState>,
) -> Result<serde_json::Value, String> {
    let state = state.inner().clone();
    workbench_blocking(move || {
        state.export(
            Path::new(&project),
            &import_id,
            Path::new(&result_root),
            Path::new(&parent),
            &name,
        )
    })
    .await
}
#[tauri::command]
async fn model_status(
    state: tauri::State<'_, workbench::WorkbenchState>,
) -> Result<workbench::ModelStatus, String> {
    let state = state.inner().clone();
    workbench_blocking(move || state.model_status()).await
}
#[tauri::command]
async fn model_credential_store(
    state: tauri::State<'_, workbench::WorkbenchState>,
) -> Result<workbench::ModelStatus, String> {
    let state = state.inner().clone();
    workbench_blocking(move || state.store_credential()).await
}
#[tauri::command]
async fn model_budget_restore(
    confirmed: bool,
    state: tauri::State<'_, workbench::WorkbenchState>,
) -> Result<workbench::ModelStatus, String> {
    let state = state.inner().clone();
    workbench_blocking(move || state.restore_ledger(confirmed)).await
}
#[tauri::command]
async fn desktop_self_test(
    state: tauri::State<'_, workbench::WorkbenchState>,
) -> Result<workbench::SelfTestResult, String> {
    let state = state.inner().clone();
    workbench_blocking(move || state.self_test()).await
}
#[tauri::command]
fn source_cancel(state: tauri::State<'_, workbench::WorkbenchState>) -> bool {
    state.cancel()
}

#[tauri::command]
fn get_close_behavior(state: tauri::State<'_, PreferencesState>) -> Result<String, String> {
    state.reload()
}

#[tauri::command]
fn set_close_behavior(
    behavior: String,
    state: tauri::State<'_, PreferencesState>,
) -> Result<String, String> {
    state.set(&behavior)
}

fn prepare_profile(path: &Path) -> Result<(), String> {
    let prepare = || -> io::Result<()> {
        fs::create_dir_all(path)?;
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let probe = path.join(format!(
            ".noetome-writecheck-{}-{nonce}.tmp",
            std::process::id()
        ));
        // An existing directory can still be unwritable. Create a new empty
        // file exclusively and remove only that file before starting WebView2.
        drop(
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&probe)?,
        );
        fs::remove_file(probe)
    };
    prepare().map_err(|error| {
        format!(
            "DESKTOP_PROFILE_UNAVAILABLE：Noetome 的应用数据目录无法创建或写入。\n目录：{}\n原因：{error}",
            path.display()
        )
    })
}

fn append_startup_log(path: &Path, message: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut log = OpenOptions::new().create(true).append(true).open(path)?;
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    writeln!(log, "unix_seconds={seconds}\n{message}\n")
}

fn report_startup_error(message: &str, log_dir: Option<PathBuf>) {
    report_desktop_error("Noetome 启动失败", message, log_dir);
}

fn report_desktop_error(title: &str, message: &str, log_dir: Option<PathBuf>) {
    eprintln!("{message}");
    let log_status = match log_dir {
        Some(directory) => {
            let path = directory.join("startup-error.log");
            match append_startup_log(&path, message) {
                Ok(()) => format!("启动日志：{}", path.display()),
                Err(error) => format!("启动日志无法写入：{}\n原因：{error}", path.display()),
            }
        }
        None => "启动日志未保存：应用日志目录不可用。".to_owned(),
    };
    eprintln!("{log_status}");

    #[cfg(target_os = "windows")]
    rfd::MessageDialog::new()
        .set_title(title)
        .set_level(rfd::MessageLevel::Error)
        .set_buttons(rfd::MessageButtons::Ok)
        .set_description(format!(
            "{message}\n\n请从 Windows 开始菜单或资源管理器重新打开 Noetome。若仍失败，请保留此弹窗的完整文字。\n\n{log_status}"
        ))
        .show();
}

fn restore_main_window(app: &tauri::AppHandle) {
    let result = app
        .get_webview_window("main")
        .ok_or_else(|| "主窗口不存在。".to_owned())
        .and_then(|window| {
            window
                .show()
                .and_then(|_| window.unminimize())
                .and_then(|_| window.set_focus())
                .map_err(|error| error.to_string())
        });
    if let Err(error) = result {
        report_desktop_error(
            "Noetome 窗口恢复失败",
            &format!(
                "DESKTOP_WINDOW_RESTORE_FAILED：无法从托盘恢复 Noetome 窗口。\n原因：{error}\n请通过托盘菜单退出后重新打开应用。"
            ),
            app.path().app_log_dir().ok(),
        );
    }
}

fn install_tray(app: &tauri::AppHandle) -> Result<(), String> {
    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or_else(|| "应用图标不可用。".to_owned())?;
    let show = MenuItem::with_id(app, TRAY_SHOW_ID, "显示窗口", true, None::<&str>)
        .map_err(|error| error.to_string())?;
    let exit = MenuItem::with_id(app, TRAY_EXIT_ID, "退出", true, None::<&str>)
        .map_err(|error| error.to_string())?;
    let menu = Menu::with_items(app, &[&show, &exit]).map_err(|error| error.to_string())?;
    TrayIconBuilder::with_id(MAIN_TRAY_ID)
        .icon(icon)
        .tooltip("Noetome — 点击显示窗口")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            TRAY_SHOW_ID => restore_main_window(app),
            TRAY_EXIT_ID => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                restore_main_window(tray.app_handle());
            }
        })
        .build(app)
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn handle_window_event(window: &tauri::Window, event: &WindowEvent) {
    // Standard minimize/resize events remain native Windows taskbar behavior.
    // Only a user-requested close follows the persisted close preference.
    if window.label() != "main" {
        return;
    }
    if let WindowEvent::CloseRequested { api, .. } = event {
        let behavior = window
            .app_handle()
            .try_state::<PreferencesState>()
            .map(|state| {
                // Another running window may have saved a newer preference.
                // Resolve the actual file before deciding what × should do.
                if let Err(error) = state.reload() {
                    report_desktop_error(
                        "Noetome 窗口设置读取失败",
                        &format!("{error}\n本次关闭保留此前已生效的窗口行为。请在“关于 → 窗口行为”中重新读取设置。"),
                        window.app_handle().path().app_log_dir().ok(),
                    );
                }
                state.effective()
            })
            .unwrap_or(CloseBehavior::HideToTray);
        if behavior == CloseBehavior::Exit {
            return;
        }
        api.prevent_close();
        if window.app_handle().tray_by_id(MAIN_TRAY_ID).is_none() {
            report_desktop_error(
                "Noetome 托盘操作失败",
                "DESKTOP_TRAY_UNAVAILABLE：托盘图标不可用，窗口已保留显示；请重新打开应用。",
                window.app_handle().path().app_log_dir().ok(),
            );
            return;
        }
        if let Err(error) = window.hide() {
            report_desktop_error(
                "Noetome 托盘操作失败",
                &format!("DESKTOP_TRAY_HIDE_FAILED：无法将 Noetome 隐藏到托盘。\n原因：{error}"),
                window.app_handle().path().app_log_dir().ok(),
            );
        }
    }
}

fn main() {
    let mut context = tauri::generate_context!();
    let isolated_data = match desktop_data::prepare_from_environment() {
        Ok(selected) => selected,
        Err(error) => {
            report_startup_error(&error, None);
            std::process::exit(1);
        }
    };
    if let Some(selected) = &isolated_data {
        context.config_mut().app.app_directories_override = Some(
            tauri::utils::config::AppDirectoriesOverride::Root(selected.root().to_path_buf()),
        );
    }
    let windows = context
        .config()
        .app
        .windows
        .iter()
        .filter(|window| window.create)
        .cloned()
        .collect::<Vec<_>>();
    // Tauri's automatic window setup panics on failure. Build those same windows
    // ourselves so startup failures can be reported without a hidden panic.
    for window in &mut context.config_mut().app.windows {
        window.create = false;
    }

    let app = match tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            runtime_info,
            cli_preview,
            pick_directory,
            project_create,
            project_open,
            task_read,
            task_kinds,
            get_close_behavior,
            set_close_behavior,
            source_pick, project_create_in_parent, source_import, source_choices, source_units,
            source_extract, source_preview_export, model_status, model_credential_store,
            model_budget_restore, desktop_self_test, source_cancel, source_saved,
            corpus_pick, corpus_preflight, corpus_import, corpus_list, corpus_status,
            corpus_pages, corpus_process, corpus_pause, corpus_retry,
            compile_create, compile_run, compile_list, compile_status, compile_control,
            compile_query, compile_evidence, compile_export
        ])
        .on_window_event(handle_window_event)
        .setup(move |app| {
            app.manage(workbench::WorkbenchState::new(app.path().app_local_data_dir().map_err(|error|error.to_string())));
            let preferences = match app.path().app_local_data_dir() {
                Ok(directory) => PreferencesState::load(directory),
                Err(error) => PreferencesState::unavailable(error.to_string()),
            };
            if let Err(error) = preferences.get() {
                report_desktop_error(
                    "Noetome 窗口设置读取失败",
                    &format!(
                        "{error}\n关闭窗口暂时隐藏到托盘。请在“关于 → 窗口行为”中重新读取设置；设置文件不会被覆盖。"
                    ),
                    app.path().app_log_dir().ok(),
                );
            }
            app.manage(preferences);
            for window in &windows {
                if let Err(error) = tauri::WebviewWindowBuilder::from_config(app, window)
                    .and_then(|builder| builder.build())
                {
                    let data_directory = app
                        .path()
                        .app_local_data_dir()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|error| format!("无法确定目录：{error}"));
                    report_startup_error(
                        &format!(
                            "DESKTOP_WINDOW_UNAVAILABLE：无法打开 Noetome 窗口。\n应用数据目录：{data_directory}\n原因：{error}"
                        ),
                        app.path().app_log_dir().ok(),
                    );
                    app.handle().exit(1);
                    return Ok(());
                }
            }
            if let Err(error) = install_tray(app.handle()) {
                report_startup_error(
                    &format!(
                        "DESKTOP_TRAY_UNAVAILABLE：无法创建 Noetome 托盘图标。\n原因：{error}\n应用已停止启动，窗口不会被隐藏到不可恢复的状态。"
                    ),
                    app.path().app_log_dir().ok(),
                );
                app.handle().exit(1);
            }
            Ok(())
        })
        .build(context)
    {
        Ok(app) => app,
        Err(error) => {
            report_startup_error(
                &format!("DESKTOP_START_FAILED：Noetome 桌面应用启动失败。\n原因：{error}"),
                None,
            );
            std::process::exit(1);
        }
    };

    let profile = app.path().app_local_data_dir().map_err(|error| {
        format!("DESKTOP_PROFILE_UNAVAILABLE：无法确定 Noetome 的应用数据目录。\n原因：{error}")
    });
    if let Err(error) = profile.and_then(|path| prepare_profile(&path)) {
        report_startup_error(&error, app.path().app_log_dir().ok());
        std::process::exit(1);
    }
    std::process::exit(app.run_return(|_, _| {}));
}

#[cfg(test)]
mod tests {
    use super::{append_startup_log, cli_preview, prepare_profile, runtime_info};
    use std::fs;
    use std::path::Path;

    fn with_test_directory(name: &str, test: impl FnOnce(&Path)) {
        let parent = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/desktop-tests");
        fs::create_dir_all(&parent).unwrap();
        let parent = parent.canonicalize().unwrap();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = parent.join(format!("{name}-{}-{nonce}", std::process::id()));
        fs::create_dir(&root).expect("create exclusive test directory");
        test(&root);
        // Exclusively created beneath the canonical ignored test parent.
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn desktop_and_cli_share_versions_and_supported_capabilities() {
        let info = runtime_info();
        assert_eq!(info.app_version, info.cli_version);
        assert_eq!(
            info.capabilities,
            [
                "help",
                "version",
                "project.create",
                "project.open",
                "task.read",
                "source.pick",
                "project.create_in_parent",
                "source.import",
                "source.choices",
                "source.units",
                "source.extract",
                "source.preview_export",
                "model.status",
                "model.credential_store",
                "model.budget_restore",
                "desktop.self_test",
                "source.cancel",
                "source.saved",
                "corpus.pick",
                "corpus.preflight",
                "corpus.import",
                "corpus.list",
                "corpus.status",
                "corpus.pages",
                "corpus.process",
                "corpus.pause",
                "corpus.retry",
                "compile.create",
                "compile.run",
                "compile.list",
                "compile.status",
                "compile.control",
                "compile.query",
                "compile.evidence",
                "compile.export",
            ]
        );
        assert_eq!(info.cli_help, cli_preview("help").unwrap());
        assert_eq!(
            cli_preview("version").unwrap(),
            format!("noetome {}\n", info.cli_version)
        );
    }

    #[test]
    fn desktop_does_not_accept_arbitrary_cli_commands() {
        for option in [
            "",
            "project init",
            "--help",
            "source",
            "shell",
            "version; whoami",
        ] {
            let error = cli_preview(option).unwrap_err();
            assert!(error.starts_with("CLI_INVALID_ARGUMENT"));
        }
    }

    #[test]
    fn profile_preparation_preserves_existing_content_and_reports_file_collision() {
        with_test_directory("profile", |root| {
            let profile = root.join("应用数据 with spaces");
            prepare_profile(&profile).unwrap();
            let sentinel = profile.join("existing.txt");
            fs::write(&sentinel, b"existing application data").unwrap();
            prepare_profile(&profile).unwrap();
            assert_eq!(fs::read(&sentinel).unwrap(), b"existing application data");
            assert_eq!(fs::read_dir(&profile).unwrap().count(), 1);

            let collision = root.join("occupied-profile");
            fs::write(&collision, b"preserve occupied file").unwrap();
            let error = prepare_profile(&collision).unwrap_err();
            assert!(error.starts_with("DESKTOP_PROFILE_UNAVAILABLE"));
            assert!(error.contains(&collision.display().to_string()));
            assert_eq!(fs::read(collision).unwrap(), b"preserve occupied file");
        });
    }

    #[test]
    fn startup_logs_append_and_preserve_files_that_block_the_log_directory() {
        with_test_directory("startup-log", |root| {
            let log = root.join("logs/startup-error.log");
            append_startup_log(&log, "first failure").unwrap();
            let first = fs::read(&log).unwrap();
            append_startup_log(&log, "second failure").unwrap();
            let appended = fs::read(&log).unwrap();
            assert!(appended.starts_with(&first));
            assert!(
                String::from_utf8(appended)
                    .unwrap()
                    .contains("second failure")
            );

            let collision = root.join("occupied-logs");
            fs::write(&collision, b"preserve occupied file").unwrap();
            assert!(append_startup_log(&collision.join("startup-error.log"), "failure").is_err());
            assert_eq!(fs::read(collision).unwrap(), b"preserve occupied file");
        });
    }
}
