//! Module Overview
//! DTO structures returned to or accepted from command/API boundaries.
//! Separates response payload shape from internal service implementation details.

use crate::domain::entities::{ProxyConfig, ProxyStatus};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthSessionStatus {
    pub authenticated: bool,
    pub remote_request: bool,
    pub password_configured: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OkResult {
    pub ok: bool,
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
pub struct GroupsExportJsonResult {
    pub text: String,
    pub file_name: String,
    pub group_count: usize,
    pub char_count: usize,
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

#[derive(Debug, Clone, Serialize)]
pub struct ClipboardTextResult {
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum IntegrationClientKind {
    Claude,
    Codex,
    Openclaw,
    Opencode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentConfig {
    pub agent_id: Option<String>,
    pub provider_id: Option<String>,
    pub url: Option<String>,
    pub api_token: Option<String>,
    pub api_format: Option<String>,
    pub model: Option<String>,
    pub fallback_models: Option<Vec<String>>,
    pub timeout: Option<u64>,
    // Claude行为选项
    pub always_thinking_enabled: Option<bool>,
    pub include_coauthored_by: Option<bool>,
    pub skip_dangerous_mode_permission_prompt: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSourceFile {
    pub source_id: String,
    pub label: String,
    pub file_path: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentConfigFile {
    pub target_id: String,
    pub kind: IntegrationClientKind,
    pub config_dir: String,
    pub file_path: String,
    pub content: String,
    pub source_files: Vec<AgentSourceFile>,
    pub updated_at: Option<String>,
    pub parsed_config: Option<AgentConfig>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteAgentConfigResult {
    pub ok: bool,
    pub target_id: String,
    pub file_path: String,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationTarget {
    pub id: String,
    pub kind: IntegrationClientKind,
    pub config_dir: String,
    pub config: Option<AgentConfig>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationWriteItem {
    pub target_id: String,
    pub kind: Option<IntegrationClientKind>,
    pub config_dir: String,
    pub file_path: Option<String>,
    pub ok: bool,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationWriteResult {
    pub ok: bool,
    pub group_id: String,
    pub entry_url: String,
    pub succeeded: usize,
    pub failed: usize,
    pub items: Vec<IntegrationWriteItem>,
}
