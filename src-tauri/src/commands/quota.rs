use crate::app_state::SharedState;
use crate::models::{RuleQuotaConfig, RuleQuotaSnapshot, RuleQuotaTestResult};
use crate::services::quota_service;
use tauri::State;

#[tauri::command]
pub async fn quota_get_rule(
    state: State<'_, SharedState>,
    group_id: String,
    rule_id: String,
) -> Result<RuleQuotaSnapshot, String> {
    quota_service::get_rule(&state, group_id, rule_id).await
}

#[tauri::command]
pub async fn quota_get_group(
    state: State<'_, SharedState>,
    group_id: String,
) -> Result<Vec<RuleQuotaSnapshot>, String> {
    quota_service::get_group(&state, group_id).await
}

#[tauri::command]
pub async fn quota_test_draft(
    state: State<'_, SharedState>,
    group_id: String,
    rule_name: String,
    rule_token: String,
    rule_api_address: String,
    rule_default_model: String,
    quota: RuleQuotaConfig,
) -> Result<RuleQuotaTestResult, String> {
    quota_service::test_draft(
        &state,
        group_id,
        rule_name,
        rule_token,
        rule_api_address,
        rule_default_model,
        quota,
    )
    .await
}
