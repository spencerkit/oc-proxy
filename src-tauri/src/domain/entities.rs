//! Module Overview
//! Domain entity definitions reused by multiple modules.
//! Keeps core data contracts independent from transport-specific concerns.

use serde::{Deserialize, Serialize};
use serde_json::Value;
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
    #[serde(default = "default_text_tool_call_fallback_enabled")]
    pub text_tool_call_fallback_enabled: bool,
}

fn default_text_tool_call_fallback_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoggingConfig {
    pub capture_body: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiConfig {
    pub theme: String,
    pub locale: String,
    pub locale_mode: String,
    pub launch_on_startup: bool,
    #[serde(default = "crate::config::schema::default_auto_start_server")]
    pub auto_start_server: bool,
    pub close_to_tray: bool,
    #[serde(default = "crate::config::schema::default_quota_auto_refresh_minutes")]
    pub quota_auto_refresh_minutes: u32,
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
    #[serde(
        rename = "openai_completion",
        alias = "openaiCompletion",
        alias = "openai-completion"
    )]
    OpenaiCompletion,
    #[serde(rename = "anthropic")]
    Anthropic,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RuleQuotaResponseMapping {
    #[serde(default)]
    pub remaining: Value,
    #[serde(default)]
    pub unit: Value,
    #[serde(default)]
    pub total: Value,
    #[serde(default)]
    pub reset_at: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QuotaUnitType {
    Percentage,
    Amount,
    Tokens,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleQuotaConfig {
    pub enabled: bool,
    pub provider: String,
    pub endpoint: String,
    #[serde(default = "default_quota_method")]
    pub method: String,
    #[serde(default = "default_quota_use_rule_token")]
    pub use_rule_token: bool,
    #[serde(default)]
    pub custom_token: String,
    #[serde(default = "default_quota_auth_header")]
    pub auth_header: String,
    #[serde(default = "default_quota_auth_scheme")]
    pub auth_scheme: String,
    #[serde(default)]
    pub custom_headers: HashMap<String, String>,
    #[serde(default = "default_quota_unit_type")]
    pub unit_type: QuotaUnitType,
    #[serde(default = "default_quota_low_threshold_percent")]
    pub low_threshold_percent: f64,
    #[serde(default)]
    pub response: RuleQuotaResponseMapping,
}

/// Performs default quota method.
pub fn default_quota_method() -> String {
    "GET".to_string()
}

/// Performs default quota use rule token.
pub fn default_quota_use_rule_token() -> bool {
    true
}

/// Performs default quota auth header.
pub fn default_quota_auth_header() -> String {
    "Authorization".to_string()
}

/// Performs default quota auth scheme.
pub fn default_quota_auth_scheme() -> String {
    "Bearer".to_string()
}

/// Performs default quota unit type.
pub fn default_quota_unit_type() -> QuotaUnitType {
    QuotaUnitType::Percentage
}

/// Performs default quota low threshold percent.
pub fn default_quota_low_threshold_percent() -> f64 {
    10.0
}

/// Performs default rule quota config.
pub fn default_rule_quota_config() -> RuleQuotaConfig {
    RuleQuotaConfig {
        enabled: false,
        provider: "custom".to_string(),
        endpoint: String::new(),
        method: default_quota_method(),
        use_rule_token: default_quota_use_rule_token(),
        custom_token: String::new(),
        auth_header: default_quota_auth_header(),
        auth_scheme: default_quota_auth_scheme(),
        custom_headers: HashMap::new(),
        unit_type: default_quota_unit_type(),
        low_threshold_percent: default_quota_low_threshold_percent(),
        response: RuleQuotaResponseMapping::default(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleCostConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub input_price_per_m: f64,
    #[serde(default)]
    pub output_price_per_m: f64,
    #[serde(default)]
    pub cache_input_price_per_m: f64,
    #[serde(default)]
    pub cache_output_price_per_m: f64,
    #[serde(default = "default_cost_currency")]
    pub currency: String,
}

/// Performs default cost currency.
pub fn default_cost_currency() -> String {
    "USD".to_string()
}

/// Performs default rule cost config.
pub fn default_rule_cost_config() -> RuleCostConfig {
    RuleCostConfig {
        enabled: false,
        input_price_per_m: 0.0,
        output_price_per_m: 0.0,
        cache_input_price_per_m: 0.0,
        cache_output_price_per_m: 0.0,
        currency: default_cost_currency(),
    }
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
    #[serde(default = "default_rule_quota_config")]
    pub quota: RuleQuotaConfig,
    #[serde(default = "default_rule_cost_config")]
    pub cost: RuleCostConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(rename = "activeProviderId", alias = "activeRuleId")]
    pub active_provider_id: Option<String>,
    #[serde(default, rename = "providers", alias = "rules")]
    pub providers: Vec<Rule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyConfig {
    #[serde(default = "crate::config::schema::default_config_version")]
    pub config_version: u32,
    pub server: ServerConfig,
    pub compat: CompatConfig,
    pub logging: LoggingConfig,
    pub ui: UiConfig,
    #[serde(default = "crate::config::schema::default_remote_git_config")]
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
#[serde(rename_all = "camelCase")]
pub struct CostSnapshot {
    pub enabled: bool,
    pub currency: String,
    pub input_price_per_m: f64,
    pub output_price_per_m: f64,
    pub cache_input_price_per_m: f64,
    pub cache_output_price_per_m: f64,
    pub total_cost: f64,
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
    pub transformed_response_body: Option<serde_json::Value>,
    pub transform_debug: Option<serde_json::Value>,
    pub token_usage: Option<TokenUsage>,
    pub cost_snapshot: Option<CostSnapshot>,
    pub http_status: Option<u16>,
    pub upstream_status: Option<u16>,
    pub duration_ms: u64,
    pub error: Option<LogEntryError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QuotaStatus {
    Ok,
    Low,
    Empty,
    Unknown,
    Unsupported,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleQuotaSnapshot {
    pub group_id: String,
    pub rule_id: String,
    pub provider: String,
    pub status: QuotaStatus,
    pub remaining: Option<f64>,
    pub total: Option<f64>,
    pub percent: Option<f64>,
    pub unit: Option<String>,
    pub reset_at: Option<String>,
    pub fetched_at: String,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleQuotaTestResult {
    pub ok: bool,
    pub snapshot: Option<RuleQuotaSnapshot>,
    pub raw_response: Option<Value>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderModelTestResult {
    pub ok: bool,
    pub resolved_model: Option<String>,
    pub raw_text: Option<String>,
    pub message: Option<String>,
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
    pub total_duration_ms: u64,
    pub total_cost: f64,
    pub input_tps: f64,
    pub output_tps: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatsSummaryResult {
    pub dimension: String,
    pub hours: u32,
    pub rule_key: Option<String>,
    pub rule_keys: Option<Vec<String>>,
    pub requests: u64,
    pub errors: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub total_cost: f64,
    pub cost_currency: Option<String>,
    pub input_tps: f64,
    pub output_tps: f64,
    pub peak_input_tps: f64,
    pub peak_output_tps: f64,
    pub comparison: Option<ComparisonSummary>,
    pub breakdowns: Option<StatsBreakdowns>,
    pub hourly: Vec<HourlyStatsPoint>,
    pub options: Vec<StatsRuleOption>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComparisonSummary {
    pub requests_delta_pct: f64,
    pub errors_delta_pct: f64,
    pub total_cost_delta_pct: f64,
    pub input_tps_delta_pct: f64,
    pub output_tps_delta_pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatsCountBreakdownItem {
    pub key: String,
    pub count: u64,
    pub ratio: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatsTokenBreakdownItem {
    pub key: String,
    pub tokens: u64,
    pub ratio: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatsRuleCountBreakdownItem {
    pub key: String,
    pub label: String,
    pub count: u64,
    pub ratio: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatsRuleTokenBreakdownItem {
    pub key: String,
    pub label: String,
    pub tokens: u64,
    pub ratio: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatsBreakdowns {
    pub errors_by_status: Vec<StatsCountBreakdownItem>,
    pub requests_by_protocol: Vec<StatsCountBreakdownItem>,
    pub tokens_by_protocol: Vec<StatsTokenBreakdownItem>,
    pub requests_by_rule: Vec<StatsRuleCountBreakdownItem>,
    pub tokens_by_rule: Vec<StatsRuleTokenBreakdownItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleCardHourlyPoint {
    pub hour: String,
    pub requests: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleCardStatsItem {
    pub group_id: String,
    pub rule_id: String,
    pub requests: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_tokens: u64,
    pub tokens: u64,
    pub total_cost: f64,
    pub hourly: Vec<RuleCardHourlyPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupsBackupPayload {
    pub format: String,
    pub version: u8,
    pub exported_at: String,
    pub groups: Vec<Group>,
}
