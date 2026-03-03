#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod backup;
mod config_store;
mod log_store;
mod mappers;
mod models;
mod proxy;
mod quota;
mod remote_sync;
mod stats_store;

use backup::{backup_default_file_name, create_groups_backup_payload, extract_groups_from_import_payload};
use chrono::{DateTime, Utc};
use config_store::ConfigStore;
use log_store::LogStore;
use models::{
    AppInfo, ClipboardTextResult, GroupBackupExportResult, GroupBackupImportResult,
    RemoteRulesPullResult, RemoteRulesUploadResult, SaveConfigResult, StatsSummaryResult,
};
use proxy::ProxyRuntime;
use remote_sync::{
    has_remote_git_binary, pull_groups_json_from_remote, remote_rules_file_path,
    upload_groups_json_to_remote,
};
use serde_json::{json, Value};
use std::sync::Arc;
use stats_store::StatsStore;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, State, WindowEvent};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_dialog::DialogExt;

struct AppState {
    app_info: AppInfo,
    config_store: ConfigStore,
    runtime: ProxyRuntime,
}

type SharedState = Arc<AppState>;

fn has_server_setting_changed(prev: &models::ProxyConfig, next: &models::ProxyConfig) -> bool {
    prev.server.host != next.server.host
        || prev.server.port != next.server.port
        || prev.server.auth_enabled != next.server.auth_enabled
        || prev.server.local_bearer_token != next.server.local_bearer_token
}

async fn sync_runtime_config(state: &SharedState, prev: models::ProxyConfig, next: models::ProxyConfig) -> Result<(bool, models::ProxyStatus), String> {
    let mut restarted = false;
    let status_before = state.runtime.get_status();
    if status_before.running && has_server_setting_changed(&prev, &next) {
        state.runtime.stop().await?;
        state.runtime.start().await?;
        restarted = true;
    }

    Ok((restarted, state.runtime.get_status()))
}

fn apply_launch_on_startup_setting(app: &AppHandle, enabled: bool) {
    let autostart_manager = app.autolaunch();
    if enabled {
        let _ = autostart_manager.enable();
    } else {
        let _ = autostart_manager.disable();
    }
}

fn get_local_config_updated_at(state: &SharedState) -> Option<String> {
    let meta = std::fs::metadata(state.config_store.path()).ok()?;
    let modified = meta.modified().ok()?;
    let dt: DateTime<Utc> = modified.into();
    Some(dt.to_rfc3339())
}

fn parse_rfc3339_utc(ts: &str) -> Option<DateTime<Utc>> {
    chrono::DateTime::parse_from_rfc3339(ts)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

fn read_exported_at_from_json(parsed: &Value) -> Option<String> {
    parsed
        .get("exportedAt")
        .and_then(|v| v.as_str())
        .map(|v| v.to_string())
}

#[tauri::command]
async fn app_get_info(state: State<'_, SharedState>) -> Result<AppInfo, String> {
    Ok(state.app_info.clone())
}

#[tauri::command]
async fn app_get_status(state: State<'_, SharedState>) -> Result<models::ProxyStatus, String> {
    Ok(state.runtime.get_status())
}

#[tauri::command]
async fn app_start_server(state: State<'_, SharedState>) -> Result<models::ProxyStatus, String> {
    state.runtime.start().await
}

#[tauri::command]
async fn app_stop_server(state: State<'_, SharedState>) -> Result<models::ProxyStatus, String> {
    state.runtime.stop().await
}

#[tauri::command]
async fn config_get(state: State<'_, SharedState>) -> Result<models::ProxyConfig, String> {
    Ok(state.config_store.get())
}

#[tauri::command]
async fn config_save(
    state: State<'_, SharedState>,
    app: AppHandle,
    next_config: Value,
) -> Result<SaveConfigResult, String> {
    let prev = state.config_store.get();
    let saved = state.config_store.save(next_config)?;

    apply_launch_on_startup_setting(&app, saved.ui.launch_on_startup);
    let (restarted, status) = sync_runtime_config(&state, prev, saved.clone()).await?;

    Ok(SaveConfigResult {
        ok: true,
        config: saved,
        restarted,
        status,
    })
}

#[tauri::command]
async fn config_export_groups(state: State<'_, SharedState>, app: AppHandle) -> Result<GroupBackupExportResult, String> {
    let current = state.config_store.get();
    let backup_payload = create_groups_backup_payload(&current.groups);
    let json_text = serde_json::to_string_pretty(&backup_payload)
        .map_err(|e| format!("serialize backup failed: {e}"))?;

    let mut file_path = None;
    let title = "Export Group Rules Backup";
    if let Some(path) = app
        .dialog()
        .file()
        .set_title(title)
        .set_file_name(&backup_default_file_name())
        .blocking_save_file()
    {
        let abs = path
            .into_path()
            .map_err(|e| format!("invalid save file path: {e}"))?;
        std::fs::write(&abs, &json_text).map_err(|e| format!("write backup failed: {e}"))?;
        file_path = Some(abs.to_string_lossy().to_string());
    }

    Ok(GroupBackupExportResult {
        ok: true,
        canceled: file_path.is_none(),
        source: Some("file".to_string()),
        file_path,
        group_count: current.groups.len(),
        char_count: None,
    })
}

#[tauri::command]
async fn config_export_groups_folder(
    state: State<'_, SharedState>,
    app: AppHandle,
) -> Result<GroupBackupExportResult, String> {
    let current = state.config_store.get();
    let backup_payload = create_groups_backup_payload(&current.groups);
    let json_text = serde_json::to_string_pretty(&backup_payload)
        .map_err(|e| format!("serialize backup failed: {e}"))?;

    let mut output_file = None;
    if let Some(folder) = app
        .dialog()
        .file()
        .set_title("Choose Backup Folder")
        .blocking_pick_folder()
    {
        let folder_path = folder
            .into_path()
            .map_err(|e| format!("invalid folder path: {e}"))?;
        let backup_path = folder_path.join(backup_default_file_name());
        std::fs::write(&backup_path, json_text).map_err(|e| format!("write backup failed: {e}"))?;
        output_file = Some(backup_path.to_string_lossy().to_string());
    }

    Ok(GroupBackupExportResult {
        ok: true,
        canceled: output_file.is_none(),
        source: Some("folder".to_string()),
        file_path: output_file,
        group_count: current.groups.len(),
        char_count: None,
    })
}

#[tauri::command]
async fn config_export_groups_clipboard(
    state: State<'_, SharedState>,
    app: AppHandle,
) -> Result<GroupBackupExportResult, String> {
    let current = state.config_store.get();
    let backup_payload = create_groups_backup_payload(&current.groups);
    let json_text = serde_json::to_string_pretty(&backup_payload)
        .map_err(|e| format!("serialize backup failed: {e}"))?;

    app.clipboard()
        .write_text(json_text.clone())
        .map_err(|e| format!("write clipboard failed: {e}"))?;

    Ok(GroupBackupExportResult {
        ok: true,
        canceled: false,
        source: Some("clipboard".to_string()),
        file_path: None,
        group_count: current.groups.len(),
        char_count: Some(json_text.len()),
    })
}

async fn import_groups_to_config(
    state: &SharedState,
    parsed: Value,
) -> Result<(usize, models::ProxyConfig, bool, models::ProxyStatus), String> {
    let groups = extract_groups_from_import_payload(&parsed)?;
    let prev = state.config_store.get();
    let mut next = prev.clone();
    next.groups = groups.clone();

    let saved = state.config_store.save_config(next)?;
    let (restarted, status) = sync_runtime_config(state, prev, saved.clone()).await?;

    Ok((groups.len(), saved, restarted, status))
}

async fn import_groups_and_save(
    state: &SharedState,
    parsed: Value,
    source: &str,
    file_path: Option<String>,
) -> Result<GroupBackupImportResult, String> {
    let (groups_len, saved, restarted, status) = import_groups_to_config(state, parsed).await?;

    Ok(GroupBackupImportResult {
        ok: true,
        canceled: false,
        source: Some(source.to_string()),
        file_path,
        imported_group_count: Some(groups_len),
        config: Some(saved),
        restarted: Some(restarted),
        status: Some(status),
    })
}

#[tauri::command]
async fn config_import_groups(
    state: State<'_, SharedState>,
    app: AppHandle,
) -> Result<GroupBackupImportResult, String> {
    let selected = app
        .dialog()
        .file()
        .set_title("Import Group Rules Backup")
        .add_filter("JSON", &["json"])
        .blocking_pick_file();

    let Some(path) = selected else {
        return Ok(GroupBackupImportResult {
            ok: true,
            canceled: true,
            source: Some("file".to_string()),
            file_path: None,
            imported_group_count: None,
            config: None,
            restarted: None,
            status: None,
        });
    };

    let path_buf = path
        .into_path()
        .map_err(|e| format!("invalid file path: {e}"))?;
    let raw = std::fs::read_to_string(&path_buf).map_err(|e| format!("read file failed: {e}"))?;
    let parsed = serde_json::from_str::<Value>(&raw).map_err(|_| "Invalid JSON file".to_string())?;

    import_groups_and_save(
        &state,
        parsed,
        "file",
        Some(path_buf.to_string_lossy().to_string()),
    )
    .await
}

#[tauri::command]
async fn config_import_groups_json(
    state: State<'_, SharedState>,
    json_text: String,
) -> Result<GroupBackupImportResult, String> {
    if json_text.trim().is_empty() {
        return Err("Invalid JSON text".to_string());
    }
    let parsed = serde_json::from_str::<Value>(&json_text).map_err(|_| "Invalid JSON text".to_string())?;
    import_groups_and_save(&state, parsed, "json", None).await
}

#[tauri::command]
async fn config_remote_rules_upload(
    state: State<'_, SharedState>,
    app: AppHandle,
    force: Option<bool>,
) -> Result<RemoteRulesUploadResult, String> {
    if !has_remote_git_binary() {
        return Err("git is not available in current environment".to_string());
    }

    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve app_data_dir failed: {e}"))?;
    let current = state.config_store.get();
    let backup_payload = create_groups_backup_payload(&current.groups);
    let json_text = serde_json::to_string_pretty(&backup_payload)
        .map_err(|e| format!("serialize backup failed: {e}"))?;
    let local_updated_at = get_local_config_updated_at(&state);

    upload_groups_json_to_remote(
        app_data_dir.as_path(),
        &current.remote_git,
        &json_text,
        current.groups.len(),
        local_updated_at,
        force.unwrap_or(false),
    )
}

#[tauri::command]
async fn config_remote_rules_pull(
    state: State<'_, SharedState>,
    app: AppHandle,
    force: Option<bool>,
) -> Result<RemoteRulesPullResult, String> {
    if !has_remote_git_binary() {
        return Err("git is not available in current environment".to_string());
    }

    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve app_data_dir failed: {e}"))?;
    let current = state.config_store.get();
    let local_updated_at = get_local_config_updated_at(&state);
    let json_text = pull_groups_json_from_remote(app_data_dir.as_path(), &current.remote_git)?;
    let parsed = serde_json::from_str::<Value>(&json_text)
        .map_err(|_| "Invalid JSON in remote rules file".to_string())?;
    let remote_updated_at = read_exported_at_from_json(&parsed);

    if !force.unwrap_or(false) {
        if let (Some(local), Some(remote)) = (&local_updated_at, &remote_updated_at) {
            if let (Some(local_dt), Some(remote_dt)) = (parse_rfc3339_utc(local), parse_rfc3339_utc(remote)) {
                if local_dt > remote_dt {
                    return Ok(RemoteRulesPullResult {
                        ok: true,
                        branch: current.remote_git.branch.trim().to_string(),
                        file_path: remote_rules_file_path().to_string(),
                        imported_group_count: None,
                        config: None,
                        restarted: None,
                        status: None,
                        needs_confirmation: true,
                        warning: Some("local_newer_than_remote".to_string()),
                        local_updated_at,
                        remote_updated_at,
                    });
                }
            }
        }
    }

    let (groups_len, saved, restarted, status) = import_groups_to_config(&state, parsed).await?;

    Ok(RemoteRulesPullResult {
        ok: true,
        branch: current.remote_git.branch.trim().to_string(),
        file_path: remote_rules_file_path().to_string(),
        imported_group_count: Some(groups_len),
        config: Some(saved),
        restarted: Some(restarted),
        status: Some(status),
        needs_confirmation: false,
        warning: None,
        local_updated_at,
        remote_updated_at,
    })
}

#[tauri::command]
async fn app_read_clipboard_text(app: AppHandle) -> Result<ClipboardTextResult, String> {
    let text = app
        .clipboard()
        .read_text()
        .map_err(|e| format!("read clipboard failed: {e}"))?;
    Ok(ClipboardTextResult { text })
}

#[tauri::command]
async fn logs_list(state: State<'_, SharedState>, max: Option<usize>) -> Result<Vec<models::LogEntry>, String> {
    Ok(state.runtime.list_logs(max.unwrap_or(100)))
}

#[tauri::command]
async fn logs_clear(state: State<'_, SharedState>) -> Result<serde_json::Value, String> {
    state.runtime.clear_logs();
    Ok(json!({ "ok": true }))
}

#[tauri::command]
async fn logs_stats_summary(
    state: State<'_, SharedState>,
    hours: Option<u32>,
    rule_key: Option<String>,
) -> Result<StatsSummaryResult, String> {
    Ok(state.runtime.stats_summary(hours, rule_key))
}

#[tauri::command]
async fn logs_stats_clear(state: State<'_, SharedState>) -> Result<serde_json::Value, String> {
    state.runtime.clear_stats()?;
    Ok(json!({ "ok": true }))
}

#[tauri::command]
async fn quota_get_rule(
    state: State<'_, SharedState>,
    group_id: String,
    rule_id: String,
) -> Result<models::RuleQuotaSnapshot, String> {
    let config = state.config_store.get();
    quota::fetch_rule_quota(&config, &group_id, &rule_id).await
}

#[tauri::command]
async fn quota_get_group(
    state: State<'_, SharedState>,
    group_id: String,
) -> Result<Vec<models::RuleQuotaSnapshot>, String> {
    let config = state.config_store.get();
    quota::fetch_group_quotas(&config, &group_id).await
}

#[tauri::command]
async fn quota_test_draft(
    state: State<'_, SharedState>,
    group_id: String,
    rule_name: String,
    rule_token: String,
    rule_api_address: String,
    rule_default_model: String,
    quota: models::RuleQuotaConfig,
) -> Result<models::RuleQuotaTestResult, String> {
    let config = state.config_store.get();
    let group = config
        .groups
        .iter()
        .find(|g| g.id == group_id)
        .ok_or_else(|| format!("group not found: {group_id}"))?;

    let draft_rule = models::Rule {
        id: "draft-rule".to_string(),
        name: if rule_name.trim().is_empty() {
            "Draft Rule".to_string()
        } else {
            rule_name
        },
        protocol: models::RuleProtocol::Openai,
        token: rule_token,
        api_address: rule_api_address,
        default_model: rule_default_model,
        model_mappings: std::collections::HashMap::new(),
        quota,
    };

    Ok(quota::test_rule_quota_draft(group, &draft_rule).await)
}

fn create_tray(app: &AppHandle) -> Result<(), String> {
    let show_hide = MenuItem::with_id(app, "toggle-window", "Show/Hide AI Open Router", true, None::<&str>)
        .map_err(|e| format!("create tray menu failed: {e}"))?;
    let quit = MenuItem::with_id(app, "quit", "Exit", true, None::<&str>)
        .map_err(|e| format!("create tray menu failed: {e}"))?;
    let menu = Menu::with_items(app, &[&show_hide, &quit]).map_err(|e| format!("build menu failed: {e}"))?;

    let mut builder = TrayIconBuilder::with_id("main-tray")
        .menu(&menu)
        .tooltip("AI Open Router")
        .on_menu_event(|app, event| {
            let window = app.get_webview_window("main");
            match event.id().as_ref() {
                "toggle-window" => {
                    if let Some(w) = window {
                        let visible = w.is_visible().unwrap_or(true);
                        if visible {
                            let _ = w.hide();
                        } else {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                }
                "quit" => {
                    std::process::exit(0);
                }
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                if let Some(window) = tray.app_handle().get_webview_window("main") {
                    let visible = window.is_visible().unwrap_or(true);
                    if visible {
                        let _ = window.hide();
                    } else {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
            }
        });

    let tray_icon = Image::from_bytes(include_bytes!("../../assets/icon.png"))
        .map_err(|e| format!("load tray icon failed: {e}"))?;
    builder = builder.icon(tray_icon);

    builder
        .build(app)
        .map_err(|e| format!("create tray icon failed: {e}"))?;

    Ok(())
}

fn setup_close_to_tray(app: &AppHandle, state: SharedState, tray_ready: bool) {
    if let Some(window) = app.get_webview_window("main") {
        let window_for_event = window.clone();
        window.on_window_event(move |event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let close_to_tray = state.config_store.get().ui.close_to_tray;
                if close_to_tray && tray_ready {
                    api.prevent_close();
                    let _ = window_for_event.hide();
                }
            }
        });
    }
}

#[tokio::main]
async fn main() {
    let app_name = "AI Open Router".to_string();
    let app_version = env!("CARGO_PKG_VERSION").to_string();

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_log::Builder::default().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(move |app| {
            let app_data_dir = app
                .path()
                .app_data_dir()
                .map_err(|e| format!("resolve app_data_dir failed: {e}"))?;

            std::fs::create_dir_all(&app_data_dir)
                .map_err(|e| format!("create app data dir failed: {e}"))?;

            let config_path = app_data_dir.join("config.json");
            let config_store = ConfigStore::new(config_path);
            let _ = config_store.initialize();

            let log_store = LogStore::new(100);
            let stats_path = app_data_dir.join("request-stats.json");
            let stats_store = StatsStore::new(stats_path);
            let _ = stats_store.initialize();
            let runtime = ProxyRuntime::new(
                config_store.shared_config(),
                config_store.shared_revision(),
                log_store.clone(),
                stats_store.clone(),
            )?;

            let state = Arc::new(AppState {
                app_info: AppInfo {
                    name: app_name.clone(),
                    version: app_version.clone(),
                },
                config_store,
                runtime,
            });

            apply_launch_on_startup_setting(app.handle(), state.config_store.get().ui.launch_on_startup);

            let runtime_clone = state.runtime.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(err) = runtime_clone.start().await {
                    eprintln!("proxy auto-start failed: {err}");
                }
            });

            let tray_ready = if state.config_store.get().ui.close_to_tray {
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| create_tray(app.handle()))) {
                    Ok(Ok(())) => true,
                    Ok(Err(err)) => {
                        eprintln!("tray icon disabled: {err}");
                        false
                    }
                    Err(_) => {
                        eprintln!("tray icon disabled: appindicator runtime unavailable");
                        false
                    }
                }
            } else {
                false
            };
            setup_close_to_tray(app.handle(), state.clone(), tray_ready);

            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_get_info,
            app_get_status,
            app_start_server,
            app_stop_server,
            config_get,
            config_save,
            config_export_groups,
            config_export_groups_folder,
            config_export_groups_clipboard,
            config_import_groups,
            config_import_groups_json,
            config_remote_rules_upload,
            config_remote_rules_pull,
            app_read_clipboard_text,
            logs_list,
            logs_clear,
            logs_stats_summary,
            logs_stats_clear,
            quota_get_rule,
            quota_get_group,
            quota_test_draft,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
