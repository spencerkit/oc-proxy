//! Module Overview
//! Service layer orchestration for feature-specific workflows.
//! Coordinates validation, persistence, runtime sync, and structured results.

use crate::app_state::SharedState;
use crate::models::{
    default_rule_cost_config, Rule, RuleProtocol, RuleQuotaConfig, RuleQuotaSnapshot,
    RuleQuotaTestResult,
};
use crate::quota;
use crate::services::{AppError, AppResult};

pub async fn get_rule(
    state: &SharedState,
    group_id: String,
    rule_id: String,
) -> AppResult<RuleQuotaSnapshot> {
    let config = state.config_store.get();
    quota::fetch_rule_quota(&config, &group_id, &rule_id)
        .await
        .map_err(AppError::external)
}

pub async fn get_group(state: &SharedState, group_id: String) -> AppResult<Vec<RuleQuotaSnapshot>> {
    let config = state.config_store.get();
    quota::fetch_group_quotas(&config, &group_id)
        .await
        .map_err(AppError::external)
}

pub async fn test_draft(
    state: &SharedState,
    group_id: String,
    rule_name: String,
    rule_token: String,
    rule_api_address: String,
    rule_default_model: String,
    quota: RuleQuotaConfig,
) -> AppResult<RuleQuotaTestResult> {
    let config = state.config_store.get();
    let group = config
        .groups
        .iter()
        .find(|g| g.id == group_id)
        .ok_or_else(|| AppError::not_found(format!("group not found: {group_id}")))?;

    let draft_rule = Rule {
        id: "draft-rule".to_string(),
        name: if rule_name.trim().is_empty() {
            "Draft Rule".to_string()
        } else {
            rule_name
        },
        protocol: RuleProtocol::Openai,
        token: rule_token,
        api_address: rule_api_address,
        default_model: rule_default_model,
        model_mappings: std::collections::HashMap::new(),
        quota,
        cost: default_rule_cost_config(),
    };

    Ok(quota::test_rule_quota_draft(group, &draft_rule).await)
}
