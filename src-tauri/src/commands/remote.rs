//! Module Overview
//! Tauri command handlers for renderer IPC invocations.
//! Performs boundary-level argument handling and delegates business logic to runtime/services.

use crate::app_state::SharedState;
use crate::models::{RemoteRulesPullResult, RemoteRulesUploadResult};
use crate::services::remote_rules_service;
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn config_remote_rules_upload(
    state: State<'_, SharedState>,
    app: AppHandle,
    force: Option<bool>,
) -> Result<RemoteRulesUploadResult, String> {
    remote_rules_service::upload(&state, &app, force)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn config_remote_rules_pull(
    state: State<'_, SharedState>,
    app: AppHandle,
    force: Option<bool>,
) -> Result<RemoteRulesPullResult, String> {
    remote_rules_service::pull(&state, &app, force)
        .await
        .map_err(|e| e.to_string())
}
