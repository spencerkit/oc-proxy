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
    remote_rules_service::upload(&state, &app, force).await
}

#[tauri::command]
pub async fn config_remote_rules_pull(
    state: State<'_, SharedState>,
    app: AppHandle,
    force: Option<bool>,
) -> Result<RemoteRulesPullResult, String> {
    remote_rules_service::pull(&state, &app, force).await
}
