//! Module Overview
//! Service layer orchestration for feature-specific workflows.
//! Coordinates validation, persistence, runtime sync, and structured results.

use crate::app_state::SharedState;
use crate::backup::create_groups_backup_payload;
use crate::models::{RemoteRulesPullResult, RemoteRulesUploadResult};
use crate::remote_sync::{
    has_remote_git_binary, pull_groups_json_from_remote, remote_rules_file_path,
    upload_groups_json_to_remote,
};
use crate::services::config_service;
use crate::services::{AppError, AppResult};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::path::Path;
use tauri::{AppHandle, Manager};

/// Performs get local config updated at.
fn get_local_config_updated_at(state: &SharedState) -> Option<String> {
    let groups_db_path = state.config_store.path().with_file_name("providers.sqlite");
    let meta = std::fs::metadata(&groups_db_path)
        .or_else(|_| std::fs::metadata(state.config_store.path()))
        .ok()?;
    let modified = meta.modified().ok()?;
    let dt: DateTime<Utc> = modified.into();
    Some(dt.to_rfc3339())
}

/// Parses rfc3339 utc.
fn parse_rfc3339_utc(ts: &str) -> Option<DateTime<Utc>> {
    chrono::DateTime::parse_from_rfc3339(ts)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

/// Reads exported at from JSON for this module's workflow.
fn read_exported_at_from_json(parsed: &Value) -> Option<String> {
    parsed
        .get("exportedAt")
        .and_then(|v| v.as_str())
        .map(|v| v.to_string())
}

/// Performs upload.
pub async fn upload(
    state: &SharedState,
    app: &AppHandle,
    force: Option<bool>,
) -> AppResult<RemoteRulesUploadResult> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::external(format!("resolve app_data_dir failed: {e}")))?;
    upload_with_dir(state, app_data_dir.as_path(), force).await
}

/// Uploads rules using an explicit app data directory (headless-friendly).
pub async fn upload_with_dir(
    state: &SharedState,
    app_data_dir: &Path,
    force: Option<bool>,
) -> AppResult<RemoteRulesUploadResult> {
    if !has_remote_git_binary() {
        return Err(AppError::external(
            "git is not available in current environment",
        ));
    }

    let current = state.config_store.get();
    let backup_payload = create_groups_backup_payload(&current.groups);
    let json_text = serde_json::to_string_pretty(&backup_payload)
        .map_err(|e| AppError::internal(format!("serialize backup failed: {e}")))?;
    let local_updated_at = get_local_config_updated_at(state);

    upload_groups_json_to_remote(
        app_data_dir,
        &current.remote_git,
        &json_text,
        current.groups.len(),
        local_updated_at,
        force.unwrap_or(false),
    )
    .map_err(AppError::external)
}

/// Performs pull.
pub async fn pull(
    state: &SharedState,
    app: &AppHandle,
    force: Option<bool>,
) -> AppResult<RemoteRulesPullResult> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::external(format!("resolve app_data_dir failed: {e}")))?;
    pull_with_dir(state, app_data_dir.as_path(), force).await
}

/// Pulls rules using an explicit app data directory (headless-friendly).
pub async fn pull_with_dir(
    state: &SharedState,
    app_data_dir: &Path,
    force: Option<bool>,
) -> AppResult<RemoteRulesPullResult> {
    if !has_remote_git_binary() {
        return Err(AppError::external(
            "git is not available in current environment",
        ));
    }

    let current = state.config_store.get();
    let local_updated_at = get_local_config_updated_at(state);
    let json_text = pull_groups_json_from_remote(app_data_dir, &current.remote_git)
        .map_err(AppError::external)?;
    let parsed = serde_json::from_str::<Value>(&json_text)
        .map_err(|_| AppError::validation("Invalid JSON in remote rules file"))?;
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
        config_service::import_groups_payload(state, parsed, None).await?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_state::AppState;
    use crate::auth::RemoteAdminAuthStore;
    use crate::integration_store::IntegrationStore;
    use crate::log_store::LogStore;
    use crate::models::AppInfo;
    use crate::proxy::ProxyRuntime;
    use crate::stats_store::StatsStore;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    fn test_shared_state() -> SharedState {
        let unique_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time must move forward")
            .as_nanos();
        let base_dir = std::env::temp_dir().join(format!("oc-proxy-remote-rules-{unique_id}"));
        std::fs::create_dir_all(&base_dir).expect("temp dir should be created");

        let config_store = crate::config_store::ConfigStore::new(base_dir.join("config.json"));
        config_store
            .initialize()
            .expect("config store should initialize");

        let integration_store = IntegrationStore::new(base_dir.join("integrations.json"));
        integration_store
            .initialize()
            .expect("integration store should initialize");

        let remote_admin_auth = RemoteAdminAuthStore::new(base_dir.join("remote-admin-auth.json"));
        remote_admin_auth
            .initialize()
            .expect("remote admin auth should initialize");

        let stats_store = StatsStore::new(base_dir.join("stats.sqlite"));
        stats_store
            .initialize()
            .expect("stats store should initialize");

        let runtime = ProxyRuntime::new(
            config_store.shared_config(),
            config_store.shared_revision(),
            LogStore::new(64),
            stats_store,
        )
        .expect("runtime should initialize");

        Arc::new(AppState {
            app_info: AppInfo {
                name: "test".to_string(),
                version: "0.0.0".to_string(),
            },
            config_store,
            integration_store,
            remote_admin_auth,
            runtime,
            renderer_ready: AtomicBool::new(false),
        })
    }

    fn file_updated_at(path: &Path) -> String {
        let meta = std::fs::metadata(path).expect("metadata should exist");
        let modified = meta.modified().expect("mtime should exist");
        let dt: DateTime<Utc> = modified.into();
        dt.to_rfc3339()
    }

    #[test]
    fn local_rules_timestamp_tracks_providers_db_not_config_json() {
        let state = test_shared_state();
        let config_path = state.config_store.path().to_path_buf();
        let providers_db_path = config_path.with_file_name("providers.sqlite");
        let config_raw = std::fs::read_to_string(&config_path).expect("config file should exist");

        std::thread::sleep(Duration::from_millis(1200));
        std::fs::write(&config_path, config_raw).expect("config file rewrite should succeed");

        let config_updated_at = file_updated_at(&config_path);
        let providers_updated_at = file_updated_at(&providers_db_path);
        assert_ne!(config_updated_at, providers_updated_at);

        assert_eq!(
            get_local_config_updated_at(&state).expect("local timestamp should exist"),
            providers_updated_at
        );
    }
}
