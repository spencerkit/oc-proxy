//! Module Overview
//! Tauri command handlers for provider-specific actions.
//! Keeps renderer IPC thin and delegates business logic to the service layer.

use crate::app_state::SharedState;
use crate::models::ProviderModelTestResult;
use crate::services::provider_service;
use tauri::State;

#[tauri::command]
/// Tests upstream model identity for a saved provider.
pub async fn provider_test_model(
    state: State<'_, SharedState>,
    group_id: Option<String>,
    provider_id: String,
) -> Result<ProviderModelTestResult, String> {
    provider_service::test_model(&state, group_id, provider_id)
        .await
        .map_err(|error| error.to_string())
}
