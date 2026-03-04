//! Module Overview
//! Service layer orchestration for feature-specific workflows.
//! Coordinates validation, persistence, runtime sync, and structured results.

use crate::app_state::SharedState;
use crate::backup::{backup_default_file_name, create_groups_backup_payload};
use crate::models::{GroupBackupExportResult, GroupBackupImportResult};
use crate::services::config_service;
use crate::services::{AppError, AppResult};
use serde_json::Value;
use tauri::AppHandle;
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_dialog::DialogExt;

/// Performs export groups to file.
pub async fn export_groups_to_file(
    state: &SharedState,
    app: &AppHandle,
) -> AppResult<GroupBackupExportResult> {
    let current = state.config_store.get();
    let backup_payload = create_groups_backup_payload(&current.groups);
    let json_text = serde_json::to_string_pretty(&backup_payload)
        .map_err(|e| AppError::internal(format!("serialize backup failed: {e}")))?;

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
            .map_err(|e| AppError::validation(format!("invalid save file path: {e}")))?;
        std::fs::write(&abs, &json_text)
            .map_err(|e| AppError::external(format!("write backup failed: {e}")))?;
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

/// Performs export groups to folder.
pub async fn export_groups_to_folder(
    state: &SharedState,
    app: &AppHandle,
) -> AppResult<GroupBackupExportResult> {
    let current = state.config_store.get();
    let backup_payload = create_groups_backup_payload(&current.groups);
    let json_text = serde_json::to_string_pretty(&backup_payload)
        .map_err(|e| AppError::internal(format!("serialize backup failed: {e}")))?;

    let mut output_file = None;
    if let Some(folder) = app
        .dialog()
        .file()
        .set_title("Choose Backup Folder")
        .blocking_pick_folder()
    {
        let folder_path = folder
            .into_path()
            .map_err(|e| AppError::validation(format!("invalid folder path: {e}")))?;
        let backup_path = folder_path.join(backup_default_file_name());
        std::fs::write(&backup_path, json_text)
            .map_err(|e| AppError::external(format!("write backup failed: {e}")))?;
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

/// Performs export groups to clipboard.
pub async fn export_groups_to_clipboard(
    state: &SharedState,
    app: &AppHandle,
) -> AppResult<GroupBackupExportResult> {
    let current = state.config_store.get();
    let backup_payload = create_groups_backup_payload(&current.groups);
    let json_text = serde_json::to_string_pretty(&backup_payload)
        .map_err(|e| AppError::internal(format!("serialize backup failed: {e}")))?;

    app.clipboard()
        .write_text(json_text.clone())
        .map_err(|e| AppError::external(format!("write clipboard failed: {e}")))?;

    Ok(GroupBackupExportResult {
        ok: true,
        canceled: false,
        source: Some("clipboard".to_string()),
        file_path: None,
        group_count: current.groups.len(),
        char_count: Some(json_text.len()),
    })
}

/// Performs import groups from file.
pub async fn import_groups_from_file(
    state: &SharedState,
    app: &AppHandle,
) -> AppResult<GroupBackupImportResult> {
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
        .map_err(|e| AppError::validation(format!("invalid file path: {e}")))?;
    let raw = std::fs::read_to_string(&path_buf)
        .map_err(|e| AppError::external(format!("read file failed: {e}")))?;
    let parsed = serde_json::from_str::<Value>(&raw)
        .map_err(|_| AppError::validation("Invalid JSON file"))?;

    config_service::import_groups_with_source(
        state,
        parsed,
        "file",
        Some(path_buf.to_string_lossy().to_string()),
    )
    .await
}

/// Performs import groups from JSON text.
pub async fn import_groups_from_json_text(
    state: &SharedState,
    json_text: String,
) -> AppResult<GroupBackupImportResult> {
    if json_text.trim().is_empty() {
        return Err(AppError::validation("Invalid JSON text"));
    }
    let parsed = serde_json::from_str::<Value>(&json_text)
        .map_err(|_| AppError::validation("Invalid JSON text"))?;
    config_service::import_groups_with_source(state, parsed, "json", None).await
}
