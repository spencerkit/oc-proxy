use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub auth_enabled: bool,
    pub local_bearer_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompatConfig {
    pub strict_mode: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoggingConfig {
    pub level: String,
    pub capture_body: bool,
    pub redact_rules: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiConfig {
    pub theme: String,
    pub locale: String,
    pub locale_mode: String,
    pub launch_on_startup: bool,
    pub close_to_tray: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteGitConfig {
    pub enabled: bool,
    pub repo_url: String,
    pub token: String,
    pub branch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RuleProtocol {
    #[serde(rename = "openai")]
    Openai,
    #[serde(rename = "openai_completion", alias = "openaiCompletion", alias = "openai-completion")]
    OpenaiCompletion,
    #[serde(rename = "anthropic")]
    Anthropic,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    pub id: String,
    pub name: String,
    pub protocol: RuleProtocol,
    pub token: String,
    pub api_address: String,
    pub default_model: String,
    #[serde(default)]
    pub model_mappings: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub models: Vec<String>,
    pub active_rule_id: Option<String>,
    #[serde(default)]
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyConfig {
    pub server: ServerConfig,
    pub compat: CompatConfig,
    pub logging: LoggingConfig,
    pub ui: UiConfig,
    #[serde(default = "default_remote_git_config")]
    pub remote_git: RemoteGitConfig,
    #[serde(default)]
    pub groups: Vec<Group>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyMetrics {
    pub requests: u64,
    pub stream_requests: u64,
    pub errors: u64,
    pub avg_latency_ms: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub uptime_started_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyStatus {
    pub running: bool,
    pub address: Option<String>,
    pub lan_address: Option<String>,
    pub metrics: ProxyMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntryError {
    pub message: String,
    pub code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub timestamp: String,
    pub trace_id: String,
    pub phase: String,
    pub status: String,
    pub method: String,
    pub request_path: String,
    pub request_address: String,
    pub client_address: Option<String>,
    pub group_path: Option<String>,
    pub group_name: Option<String>,
    pub rule_id: Option<String>,
    pub direction: Option<String>,
    pub entry_protocol: Option<String>,
    pub downstream_protocol: Option<String>,
    pub model: Option<String>,
    pub forwarded_model: Option<String>,
    pub forwarding_address: Option<String>,
    pub request_headers: Option<HashMap<String, String>>,
    pub forward_request_headers: Option<HashMap<String, String>>,
    pub upstream_response_headers: Option<HashMap<String, String>>,
    pub response_headers: Option<HashMap<String, String>>,
    pub request_body: Option<serde_json::Value>,
    pub forward_request_body: Option<serde_json::Value>,
    pub response_body: Option<serde_json::Value>,
    pub token_usage: Option<TokenUsage>,
    pub http_status: Option<u16>,
    pub upstream_status: Option<u16>,
    pub duration_ms: u64,
    pub error: Option<LogEntryError>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveConfigResult {
    pub ok: bool,
    pub config: ProxyConfig,
    pub restarted: bool,
    pub status: ProxyStatus,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupBackupExportResult {
    pub ok: bool,
    pub canceled: bool,
    pub source: Option<String>,
    pub file_path: Option<String>,
    pub group_count: usize,
    pub char_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupBackupImportResult {
    pub ok: bool,
    pub canceled: bool,
    pub source: Option<String>,
    pub file_path: Option<String>,
    pub imported_group_count: Option<usize>,
    pub config: Option<ProxyConfig>,
    pub restarted: Option<bool>,
    pub status: Option<ProxyStatus>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteRulesUploadResult {
    pub ok: bool,
    pub changed: bool,
    pub branch: String,
    pub file_path: String,
    pub group_count: usize,
    pub needs_confirmation: bool,
    pub warning: Option<String>,
    pub local_updated_at: Option<String>,
    pub remote_updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteRulesPullResult {
    pub ok: bool,
    pub branch: String,
    pub file_path: String,
    pub imported_group_count: Option<usize>,
    pub config: Option<ProxyConfig>,
    pub restarted: Option<bool>,
    pub status: Option<ProxyStatus>,
    pub needs_confirmation: bool,
    pub warning: Option<String>,
    pub local_updated_at: Option<String>,
    pub remote_updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatsRuleOption {
    pub key: String,
    pub label: String,
    pub group_id: String,
    pub rule_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HourlyStatsPoint {
    pub hour: String,
    pub requests: u64,
    pub errors: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatsSummaryResult {
    pub hours: u32,
    pub rule_key: Option<String>,
    pub requests: u64,
    pub errors: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub hourly: Vec<HourlyStatsPoint>,
    pub options: Vec<StatsRuleOption>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClipboardTextResult {
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupsBackupPayload {
    pub format: String,
    pub version: u8,
    pub exported_at: String,
    pub groups: Vec<Group>,
}

pub fn default_remote_git_config() -> RemoteGitConfig {
    RemoteGitConfig {
        enabled: false,
        repo_url: String::new(),
        token: String::new(),
        branch: "main".to_string(),
    }
}

pub fn default_config() -> ProxyConfig {
    ProxyConfig {
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
        },
        remote_git: default_remote_git_config(),
        groups: vec![],
    }
}

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

pub fn validate_config(config: &ProxyConfig) -> Result<(), String> {
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
    use super::{default_config, validate_config, Group, Rule, RuleProtocol};
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
}
