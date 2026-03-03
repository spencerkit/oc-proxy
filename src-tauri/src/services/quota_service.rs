use crate::app_state::SharedState;
use crate::models::{Rule, RuleProtocol, RuleQuotaConfig, RuleQuotaSnapshot, RuleQuotaTestResult};
use crate::quota;

pub async fn get_rule(
    state: &SharedState,
    group_id: String,
    rule_id: String,
) -> Result<RuleQuotaSnapshot, String> {
    let config = state.config_store.get();
    quota::fetch_rule_quota(&config, &group_id, &rule_id).await
}

pub async fn get_group(
    state: &SharedState,
    group_id: String,
) -> Result<Vec<RuleQuotaSnapshot>, String> {
    let config = state.config_store.get();
    quota::fetch_group_quotas(&config, &group_id).await
}

pub async fn test_draft(
    state: &SharedState,
    group_id: String,
    rule_name: String,
    rule_token: String,
    rule_api_address: String,
    rule_default_model: String,
    quota: RuleQuotaConfig,
) -> Result<RuleQuotaTestResult, String> {
    let config = state.config_store.get();
    let group = config
        .groups
        .iter()
        .find(|g| g.id == group_id)
        .ok_or_else(|| format!("group not found: {group_id}"))?;

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
    };

    Ok(quota::test_rule_quota_draft(group, &draft_rule).await)
}
