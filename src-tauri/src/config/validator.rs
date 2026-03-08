//! Module Overview
//! Config semantic validation rules.
//! Ensures persisted and imported config data is internally consistent before runtime use.

use crate::config::migrator::CURRENT_CONFIG_VERSION;
use crate::domain::entities::ProxyConfig;

/// Validates config for this module's workflow.
pub fn validate_config(config: &ProxyConfig) -> Result<(), String> {
    if config.config_version != CURRENT_CONFIG_VERSION {
        return Err(format!("configVersion must be {CURRENT_CONFIG_VERSION}"));
    }
    if config.server.host.trim().is_empty() {
        return Err("server.host must be non-empty".into());
    }
    if config.server.port == 0 {
        return Err("server.port must be between 1 and 65535".into());
    }
    if config.server.auth_enabled && config.server.local_bearer_token.trim().is_empty() {
        return Err("server.localBearerToken must be set when authEnabled=true".into());
    }
    if config.ui.theme != "light" && config.ui.theme != "dark" {
        return Err("ui.theme must be light|dark".into());
    }
    if config.ui.locale != "en-US" && config.ui.locale != "zh-CN" {
        return Err("ui.locale must be en-US|zh-CN".into());
    }
    if config.ui.locale_mode != "auto" && config.ui.locale_mode != "manual" {
        return Err("ui.localeMode must be auto|manual".into());
    }
    if config.ui.quota_auto_refresh_minutes < 1 || config.ui.quota_auto_refresh_minutes > 1440 {
        return Err("ui.quotaAutoRefreshMinutes must be between 1 and 1440".into());
    }
    if config.remote_git.branch.trim().is_empty() {
        return Err("remoteGit.branch must be non-empty".into());
    }

    for group in &config.groups {
        if group.id.trim().is_empty() {
            return Err("group.id is required".into());
        }
        if !group
            .id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err(format!("group.id invalid: {}", group.id));
        }
        if group.name.trim().is_empty() {
            return Err(format!("group.name is required for {}", group.id));
        }
        for provider in &group.providers {
            if provider.id.trim().is_empty() {
                return Err(format!("provider.id is required in group {}", group.id));
            }
            if provider.default_model.trim().is_empty() {
                return Err(format!(
                    "provider.defaultModel required for {}",
                    provider.id
                ));
            }
        }
        if let Some(active) = &group.active_provider_id {
            if !group.providers.iter().any(|r| r.id == *active) {
                return Err(format!(
                    "group.activeProviderId not found in providers for {}",
                    group.id
                ));
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_config;
    use crate::config::schema::default_config;
    use crate::domain::entities::{
        default_rule_cost_config, default_rule_quota_config, Group, Rule, RuleProtocol,
    };
    use std::collections::HashMap;

    #[test]
    /// Performs default config validates.
    fn default_config_validates() {
        let cfg = default_config();
        let result = validate_config(&cfg);
        assert!(result.is_ok());
        assert!(!cfg.logging.capture_body);
    }

    #[test]
    /// Performs invalid config returns error.
    fn invalid_config_returns_error() {
        let mut cfg = default_config();
        cfg.server.host = String::new();

        let err = validate_config(&cfg).expect_err("validation should fail");
        assert!(err.contains("server.host"));
    }

    #[test]
    /// Performs group active provider must exist.
    fn group_active_provider_must_exist() {
        let mut cfg = default_config();
        cfg.groups = vec![Group {
            id: "g1".to_string(),
            name: "demo".to_string(),
            models: vec!["a1".to_string()],
            active_provider_id: Some("not_exists".to_string()),
            providers: vec![Rule {
                id: "r1".to_string(),
                name: "rule-1".to_string(),
                protocol: RuleProtocol::Anthropic,
                token: "t1".to_string(),
                api_address: "https://api.example.com".to_string(),
                default_model: "m1".to_string(),
                model_mappings: HashMap::new(),
                quota: default_rule_quota_config(),
                cost: default_rule_cost_config(),
            }],
        }];

        let err = validate_config(&cfg).expect_err("validation should fail");
        assert!(err.contains("group.activeProviderId"));
    }

    #[test]
    /// Performs ui theme must be valid.
    fn ui_theme_must_be_valid() {
        let mut cfg = default_config();
        cfg.ui.theme = "system".to_string();

        let err = validate_config(&cfg).expect_err("validation should fail");
        assert!(err.contains("ui.theme"));
    }

    #[test]
    /// Performs config version must match current.
    fn config_version_must_match_current() {
        let mut cfg = default_config();
        cfg.config_version += 1;

        let err = validate_config(&cfg).expect_err("validation should fail");
        assert!(err.contains("configVersion"));
    }

    #[test]
    /// Performs quota auto refresh minutes must be valid.
    fn quota_auto_refresh_minutes_must_be_valid() {
        let mut cfg = default_config();
        cfg.ui.quota_auto_refresh_minutes = 0;

        let err = validate_config(&cfg).expect_err("validation should fail");
        assert!(err.contains("ui.quotaAutoRefreshMinutes"));
    }

    #[test]
    /// Performs auto start server defaults to enabled.
    fn auto_start_server_defaults_enabled() {
        let cfg = default_config();
        assert!(cfg.ui.auto_start_server);
    }
}
