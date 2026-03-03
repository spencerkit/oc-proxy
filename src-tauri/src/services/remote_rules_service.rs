use crate::app_state::SharedState;
use crate::backup::create_groups_backup_payload;
use crate::models::{RemoteRulesPullResult, RemoteRulesUploadResult};
use crate::remote_sync::{
    has_remote_git_binary, pull_groups_json_from_remote, remote_rules_file_path,
    upload_groups_json_to_remote,
};
use crate::services::config_service;
use chrono::{DateTime, Utc};
use serde_json::Value;
use tauri::{AppHandle, Manager};

fn get_local_config_updated_at(state: &SharedState) -> Option<String> {
    let meta = std::fs::metadata(state.config_store.path()).ok()?;
    let modified = meta.modified().ok()?;
    let dt: DateTime<Utc> = modified.into();
    Some(dt.to_rfc3339())
}

fn parse_rfc3339_utc(ts: &str) -> Option<DateTime<Utc>> {
    chrono::DateTime::parse_from_rfc3339(ts)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

fn read_exported_at_from_json(parsed: &Value) -> Option<String> {
    parsed
        .get("exportedAt")
        .and_then(|v| v.as_str())
        .map(|v| v.to_string())
}

pub async fn upload(
    state: &SharedState,
    app: &AppHandle,
    force: Option<bool>,
) -> Result<RemoteRulesUploadResult, String> {
    if !has_remote_git_binary() {
        return Err("git is not available in current environment".to_string());
    }

    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve app_data_dir failed: {e}"))?;
    let current = state.config_store.get();
    let backup_payload = create_groups_backup_payload(&current.groups);
    let json_text = serde_json::to_string_pretty(&backup_payload)
        .map_err(|e| format!("serialize backup failed: {e}"))?;
    let local_updated_at = get_local_config_updated_at(state);

    upload_groups_json_to_remote(
        app_data_dir.as_path(),
        &current.remote_git,
        &json_text,
        current.groups.len(),
        local_updated_at,
        force.unwrap_or(false),
    )
}

pub async fn pull(
    state: &SharedState,
    app: &AppHandle,
    force: Option<bool>,
) -> Result<RemoteRulesPullResult, String> {
    if !has_remote_git_binary() {
        return Err("git is not available in current environment".to_string());
    }

    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve app_data_dir failed: {e}"))?;
    let current = state.config_store.get();
    let local_updated_at = get_local_config_updated_at(state);
    let json_text = pull_groups_json_from_remote(app_data_dir.as_path(), &current.remote_git)?;
    let parsed = serde_json::from_str::<Value>(&json_text)
        .map_err(|_| "Invalid JSON in remote rules file".to_string())?;
    let remote_updated_at = read_exported_at_from_json(&parsed);

    if !force.unwrap_or(false) {
        if let (Some(local), Some(remote)) = (&local_updated_at, &remote_updated_at) {
            if let (Some(local_dt), Some(remote_dt)) =
                (parse_rfc3339_utc(local), parse_rfc3339_utc(remote))
            {
                if local_dt > remote_dt {
                    return Ok(RemoteRulesPullResult {
                        ok: true,
                        branch: current.remote_git.branch.trim().to_string(),
                        file_path: remote_rules_file_path().to_string(),
                        imported_group_count: None,
                        config: None,
                        restarted: None,
                        status: None,
                        needs_confirmation: true,
                        warning: Some("local_newer_than_remote".to_string()),
                        local_updated_at,
                        remote_updated_at,
                    });
                }
            }
        }
    }

    let (groups_len, saved, restarted, status) =
        config_service::import_groups_payload(state, parsed).await?;

    Ok(RemoteRulesPullResult {
        ok: true,
        branch: current.remote_git.branch.trim().to_string(),
        file_path: remote_rules_file_path().to_string(),
        imported_group_count: Some(groups_len),
        config: Some(saved),
        restarted: Some(restarted),
        status: Some(status),
        needs_confirmation: false,
        warning: None,
        local_updated_at,
        remote_updated_at,
    })
}
