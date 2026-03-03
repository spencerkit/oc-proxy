use crate::app_state::SharedState;
use crate::models::{
    GroupBackupExportResult, GroupBackupImportResult, ProxyConfig, SaveConfigResult,
};
use crate::services::{config_service, group_backup_service};
use serde_json::Value;
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn config_get(state: State<'_, SharedState>) -> Result<ProxyConfig, String> {
    Ok(config_service::get_config(&state))
}

#[tauri::command]
pub async fn config_save(
    state: State<'_, SharedState>,
    app: AppHandle,
    next_config: Value,
) -> Result<SaveConfigResult, String> {
    config_service::save_config(&state, &app, next_config).await
}

#[tauri::command]
pub async fn config_export_groups(
    state: State<'_, SharedState>,
    app: AppHandle,
) -> Result<GroupBackupExportResult, String> {
    group_backup_service::export_groups_to_file(&state, &app).await
}

#[tauri::command]
pub async fn config_export_groups_folder(
    state: State<'_, SharedState>,
    app: AppHandle,
) -> Result<GroupBackupExportResult, String> {
    group_backup_service::export_groups_to_folder(&state, &app).await
}

#[tauri::command]
pub async fn config_export_groups_clipboard(
    state: State<'_, SharedState>,
    app: AppHandle,
) -> Result<GroupBackupExportResult, String> {
    group_backup_service::export_groups_to_clipboard(&state, &app).await
}

#[tauri::command]
pub async fn config_import_groups(
    state: State<'_, SharedState>,
    app: AppHandle,
) -> Result<GroupBackupImportResult, String> {
    group_backup_service::import_groups_from_file(&state, &app).await
}

#[tauri::command]
pub async fn config_import_groups_json(
    state: State<'_, SharedState>,
    json_text: String,
) -> Result<GroupBackupImportResult, String> {
    group_backup_service::import_groups_from_json_text(&state, json_text).await
}
