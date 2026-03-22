//! Module Overview
//! Tauri command handlers for renderer IPC invocations.
//! Performs boundary-level argument handling and delegates business logic to runtime/services.

use crate::app_state::SharedState;
use crate::models::{
    AuthSessionStatus, GroupBackupExportResult, GroupBackupImportResult, ProxyConfig,
    SaveConfigResult,
};
use crate::services::{config_service, group_backup_service};
use serde_json::Value;
use tauri::{AppHandle, State};

#[tauri::command]
/// IPC command: returns current in-memory proxy config.
pub async fn config_get(state: State<'_, SharedState>) -> Result<ProxyConfig, String> {
    Ok(config_service::get_config(&state))
}

#[tauri::command]
/// IPC command: validates and persists a new proxy config payload.
pub async fn config_save(
    state: State<'_, SharedState>,
    app: AppHandle,
    next_config: Value,
) -> Result<SaveConfigResult, String> {
    config_service::save_config(&state, &app, next_config)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
/// IPC command: sets the remote admin password for `/api/*` and `/management`.
pub async fn config_set_remote_admin_password(
    state: State<'_, SharedState>,
    password: String,
) -> Result<AuthSessionStatus, String> {
    config_service::set_remote_admin_password(&state, password, false).map_err(|e| e.to_string())
}

#[tauri::command]
/// IPC command: clears the remote admin password for `/api/*` and `/management`.
pub async fn config_clear_remote_admin_password(
    state: State<'_, SharedState>,
) -> Result<AuthSessionStatus, String> {
    config_service::clear_remote_admin_password(&state, false).map_err(|e| e.to_string())
}

#[tauri::command]
/// IPC command: exports groups backup to a user-selected file.
pub async fn config_export_groups(
    state: State<'_, SharedState>,
    app: AppHandle,
) -> Result<GroupBackupExportResult, String> {
    group_backup_service::export_groups_to_file(&state, &app)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
/// IPC command: exports groups backup into a selected folder path.
pub async fn config_export_groups_folder(
    state: State<'_, SharedState>,
    app: AppHandle,
) -> Result<GroupBackupExportResult, String> {
    group_backup_service::export_groups_to_folder(&state, &app)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
/// IPC command: exports groups backup JSON into clipboard.
pub async fn config_export_groups_clipboard(
    state: State<'_, SharedState>,
    app: AppHandle,
) -> Result<GroupBackupExportResult, String> {
    group_backup_service::export_groups_to_clipboard(&state, &app)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
/// IPC command: imports groups backup from a selected file.
pub async fn config_import_groups(
    state: State<'_, SharedState>,
    app: AppHandle,
) -> Result<GroupBackupImportResult, String> {
    group_backup_service::import_groups_from_file(&state, &app)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
/// IPC command: imports groups backup from raw JSON text.
pub async fn config_import_groups_json(
    state: State<'_, SharedState>,
    json_text: String,
) -> Result<GroupBackupImportResult, String> {
    group_backup_service::import_groups_from_json_text(&state, json_text)
        .await
        .map_err(|e| e.to_string())
}
