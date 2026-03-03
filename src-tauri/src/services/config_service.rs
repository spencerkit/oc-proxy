//! Module Overview
//! Service layer orchestration for feature-specific workflows.
//! Coordinates validation, persistence, runtime sync, and structured results.

use crate::app_state::{apply_launch_on_startup_setting, sync_runtime_config, SharedState};
use crate::backup::extract_groups_from_import_payload;
use crate::models::{GroupBackupImportResult, ProxyConfig, ProxyStatus, SaveConfigResult};
use crate::services::{AppError, AppResult};
use serde_json::Value;
use tauri::AppHandle;

pub fn get_config(state: &SharedState) -> ProxyConfig {
    state.config_store.get()
}

pub async fn save_config(
    state: &SharedState,
    app: &AppHandle,
    next_config: Value,
) -> AppResult<SaveConfigResult> {
    let prev = state.config_store.get();
    let saved = state.config_store.save(next_config)?;

    apply_launch_on_startup_setting(app, saved.ui.launch_on_startup);
    let (restarted, status) = sync_runtime_config(state, prev, saved.clone()).await?;

    Ok(SaveConfigResult {
        ok: true,
        config: saved,
        restarted,
        status,
    })
}

pub async fn import_groups_payload(
    state: &SharedState,
    parsed: Value,
) -> AppResult<(usize, ProxyConfig, bool, ProxyStatus)> {
    let groups = extract_groups_from_import_payload(&parsed).map_err(AppError::validation)?;
    let prev = state.config_store.get();
    let mut next = prev.clone();
    next.groups = groups.clone();

    let saved = state.config_store.save_config(next)?;
    let (restarted, status) = sync_runtime_config(state, prev, saved.clone()).await?;

    Ok((groups.len(), saved, restarted, status))
}

pub async fn import_groups_with_source(
    state: &SharedState,
    parsed: Value,
    source: &str,
    file_path: Option<String>,
) -> AppResult<GroupBackupImportResult> {
    let (groups_len, saved, restarted, status) = import_groups_payload(state, parsed).await?;

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
