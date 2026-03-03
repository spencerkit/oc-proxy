//! Module Overview
//! Config semantic validation rules.
//! Ensures persisted and imported config data is internally consistent before runtime use.

use crate::config::migrator::CURRENT_CONFIG_VERSION;
use crate::domain::entities::ProxyConfig;

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
        for rule in &group.rules {
            if rule.id.trim().is_empty() {
                return Err(format!("rule.id is required in group {}", group.id));
            }
            if rule.default_model.trim().is_empty() {
                return Err(format!("rule.defaultModel required for {}", rule.id));
            }
        }
        if let Some(active) = &group.active_rule_id {
            if !group.rules.iter().any(|r| r.id == *active) {
                return Err(format!(
                    "group.activeRuleId not found in rules for {}",
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
    use crate::domain::entities::{default_rule_quota_config, Group, Rule, RuleProtocol};
    use std::collections::HashMap;

    #[test]
    fn default_config_validates() {
        let cfg = default_config();
        let result = validate_config(&cfg);
        assert!(result.is_ok());
        assert!(!cfg.logging.capture_body);
    }

    #[test]
    fn invalid_config_returns_error() {
        let mut cfg = default_config();
        cfg.server.host = String::new();

        let err = validate_config(&cfg).expect_err("validation should fail");
        assert!(err.contains("server.host"));
    }

    #[test]
    fn group_active_rule_must_exist() {
        let mut cfg = default_config();
        cfg.groups = vec![Group {
            id: "g1".to_string(),
            name: "demo".to_string(),
            models: vec!["a1".to_string()],
            active_rule_id: Some("not_exists".to_string()),
            rules: vec![Rule {
                id: "r1".to_string(),
                name: "rule-1".to_string(),
                protocol: RuleProtocol::Anthropic,
                token: "t1".to_string(),
                api_address: "https://api.example.com".to_string(),
                default_model: "m1".to_string(),
                model_mappings: HashMap::new(),
                quota: default_rule_quota_config(),
            }],
        }];

        let err = validate_config(&cfg).expect_err("validation should fail");
        assert!(err.contains("group.activeRuleId"));
    }

    #[test]
    fn ui_theme_must_be_valid() {
        let mut cfg = default_config();
        cfg.ui.theme = "system".to_string();

        let err = validate_config(&cfg).expect_err("validation should fail");
        assert!(err.contains("ui.theme"));
    }

    #[test]
    fn config_version_must_match_current() {
        let mut cfg = default_config();
        cfg.config_version += 1;

        let err = validate_config(&cfg).expect_err("validation should fail");
        assert!(err.contains("configVersion"));
    }
}
