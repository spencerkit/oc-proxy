//! Module Overview
//! Canonical config schema and default values.
//! Defines persisted config version contract and default initialization behavior.

use crate::config::migrator::CURRENT_CONFIG_VERSION;
use crate::domain::entities::{
    CompatConfig, LoggingConfig, ProxyConfig, ProxyMetrics, RemoteGitConfig, ServerConfig, UiConfig,
};
use serde::Deserialize;

/// Performs default config version.
pub fn default_config_version() -> u32 {
    CURRENT_CONFIG_VERSION
}

/// Performs default quota auto refresh minutes.
pub fn default_quota_auto_refresh_minutes() -> u32 {
    5
}

/// Performs default remote git config.
pub fn default_remote_git_config() -> RemoteGitConfig {
    RemoteGitConfig {
        enabled: false,
        repo_url: String::new(),
        token: String::new(),
        branch: "main".to_string(),
    }
}

/// Performs default config.
pub fn default_config() -> ProxyConfig {
    ProxyConfig {
        config_version: default_config_version(),
        server: ServerConfig {
            host: "0.0.0.0".to_string(),
            port: 8899,
            auth_enabled: false,
            local_bearer_token: String::new(),
        },
        compat: CompatConfig { strict_mode: false },
        logging: LoggingConfig {
            level: "info".to_string(),
            capture_body: false,
            redact_rules: vec![
                "authorization".to_string(),
                "x-api-key".to_string(),
                "api-key".to_string(),
                "api_key".to_string(),
                "token".to_string(),
                "password".to_string(),
            ],
        },
        ui: UiConfig {
            theme: "light".to_string(),
            locale: "en-US".to_string(),
            locale_mode: "auto".to_string(),
            launch_on_startup: false,
            close_to_tray: true,
            quota_auto_refresh_minutes: default_quota_auto_refresh_minutes(),
        },
        remote_git: default_remote_git_config(),
        groups: vec![],
    }
}

/// Performs default metrics.
pub fn default_metrics() -> ProxyMetrics {
    ProxyMetrics {
        requests: 0,
        stream_requests: 0,
        errors: 0,
        avg_latency_ms: 0,
        input_tokens: 0,
        output_tokens: 0,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        uptime_started_at: None,
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PartialServerConfig {
    host: Option<String>,
    port: Option<u16>,
    auth_enabled: Option<bool>,
    local_bearer_token: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PartialCompatConfig {
    strict_mode: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PartialLoggingConfig {
    level: Option<String>,
    capture_body: Option<bool>,
    redact_rules: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PartialUiConfig {
    theme: Option<String>,
    locale: Option<String>,
    locale_mode: Option<String>,
    launch_on_startup: Option<bool>,
    close_to_tray: Option<bool>,
    quota_auto_refresh_minutes: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PartialRemoteGitConfig {
    enabled: Option<bool>,
    repo_url: Option<String>,
    token: Option<String>,
    branch: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PartialProxyConfig {
    config_version: Option<u32>,
    server: Option<PartialServerConfig>,
    compat: Option<PartialCompatConfig>,
    logging: Option<PartialLoggingConfig>,
    ui: Option<PartialUiConfig>,
    remote_git: Option<PartialRemoteGitConfig>,
    groups: Option<serde_json::Value>,
}

/// Normalizes config for this module's workflow.
pub fn normalize_config(input: serde_json::Value) -> Result<ProxyConfig, String> {
    let defaults = default_config();
    let partial = serde_json::from_value::<PartialProxyConfig>(input)
        .map_err(|e| format!("invalid config structure: {e}"))?;

    let groups = if let Some(raw_groups) = partial.groups {
        serde_json::from_value(raw_groups).map_err(|e| format!("invalid groups structure: {e}"))?
    } else {
        defaults.groups
    };

    let locale = partial
        .ui
        .as_ref()
        .and_then(|u| u.locale.clone())
        .unwrap_or_else(|| defaults.ui.locale.clone());
    let normalized_locale = if locale == "zh-CN" { "zh-CN" } else { "en-US" }.to_string();

    let locale_mode = partial
        .ui
        .as_ref()
        .and_then(|u| u.locale_mode.clone())
        .unwrap_or_else(|| {
            if locale == "zh-CN" {
                "manual".to_string()
            } else {
                defaults.ui.locale_mode.clone()
            }
        });
    let normalized_locale_mode = if locale_mode == "manual" {
        "manual"
    } else {
        "auto"
    }
    .to_string();

    let remote_repo_url = partial
        .remote_git
        .as_ref()
        .and_then(|r| r.repo_url.clone())
        .unwrap_or(defaults.remote_git.repo_url);
    let remote_token = partial
        .remote_git
        .as_ref()
        .and_then(|r| r.token.clone())
        .unwrap_or(defaults.remote_git.token);
    let remote_branch = partial
        .remote_git
        .as_ref()
        .and_then(|r| r.branch.clone())
        .filter(|v| !v.trim().is_empty())
        .unwrap_or(defaults.remote_git.branch);
    let remote_enabled = partial
        .remote_git
        .as_ref()
        .and_then(|r| r.enabled)
        .unwrap_or_else(|| !remote_repo_url.trim().is_empty() || !remote_token.trim().is_empty());

    Ok(ProxyConfig {
        config_version: partial.config_version.unwrap_or(default_config_version()),
        server: ServerConfig {
            host: defaults.server.host,
            port: partial
                .server
                .as_ref()
                .and_then(|s| s.port)
                .unwrap_or(defaults.server.port),
            auth_enabled: partial
                .server
                .as_ref()
                .and_then(|s| s.auth_enabled)
                .unwrap_or(defaults.server.auth_enabled),
            local_bearer_token: partial
                .server
                .as_ref()
                .and_then(|s| s.local_bearer_token.clone())
                .unwrap_or(defaults.server.local_bearer_token),
        },
        compat: CompatConfig {
            strict_mode: partial
                .compat
                .as_ref()
                .and_then(|c| c.strict_mode)
                .unwrap_or(defaults.compat.strict_mode),
        },
        logging: LoggingConfig {
            level: partial
                .logging
                .as_ref()
                .and_then(|l| l.level.clone())
                .unwrap_or(defaults.logging.level),
            capture_body: partial
                .logging
                .as_ref()
                .and_then(|l| l.capture_body)
                .unwrap_or(defaults.logging.capture_body),
            redact_rules: partial
                .logging
                .as_ref()
                .and_then(|l| l.redact_rules.clone())
                .filter(|arr| arr.iter().all(|v| !v.trim().is_empty()))
                .unwrap_or(defaults.logging.redact_rules),
        },
        ui: UiConfig {
            theme: partial
                .ui
                .as_ref()
                .and_then(|u| u.theme.clone())
                .filter(|v| v == "light" || v == "dark")
                .unwrap_or(defaults.ui.theme),
            locale: normalized_locale,
            locale_mode: normalized_locale_mode,
            launch_on_startup: partial
                .ui
                .as_ref()
                .and_then(|u| u.launch_on_startup)
                .unwrap_or(defaults.ui.launch_on_startup),
            close_to_tray: partial
                .ui
                .as_ref()
                .and_then(|u| u.close_to_tray)
                .unwrap_or(defaults.ui.close_to_tray),
            quota_auto_refresh_minutes: partial
                .ui
                .as_ref()
                .and_then(|u| u.quota_auto_refresh_minutes)
                .unwrap_or(defaults.ui.quota_auto_refresh_minutes),
        },
        remote_git: RemoteGitConfig {
            enabled: remote_enabled,
            repo_url: remote_repo_url,
            token: remote_token,
            branch: remote_branch,
        },
        groups,
    })
}
