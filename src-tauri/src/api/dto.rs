use crate::domain::entities::{ProxyConfig, ProxyStatus};
use serde::Serialize;

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

#[derive(Debug, Clone, Serialize)]
pub struct ClipboardTextResult {
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppInfo {
    pub name: String,
    pub version: String,
}
