//! Module Overview
//! Service layer orchestration for feature-specific workflows.
//! Coordinates validation, persistence, runtime sync, and structured results.

use crate::app_state::{apply_launch_on_startup_setting, sync_runtime_config, SharedState};
use crate::backup::extract_groups_from_import_payload;
use crate::domain::entities::Group;
use crate::models::{
    AuthSessionStatus, GroupBackupImportResult, GroupImportMode, ProxyConfig, ProxyStatus,
    SaveConfigResult,
};
use crate::services::{AppError, AppResult};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use tauri::AppHandle;
use uuid::Uuid;

/// Performs get config.
pub fn get_config(state: &SharedState) -> ProxyConfig {
    state.config_store.get()
}

/// Builds the remote admin auth session status for local or remote callers.
pub fn auth_session_status(
    state: &SharedState,
    remote_request: bool,
    authenticated: bool,
) -> AuthSessionStatus {
    let password_configured = state.remote_admin_auth.password_configured();
    AuthSessionStatus {
        authenticated: if remote_request && password_configured {
            authenticated
        } else {
            true
        },
        remote_request,
        password_configured,
    }
}

/// Sets the remote admin password used by `/api/*` and `/management`.
pub fn set_remote_admin_password(
    state: &SharedState,
    password: String,
    remote_request: bool,
) -> AppResult<AuthSessionStatus> {
    state
        .remote_admin_auth
        .set_password(&password)
        .map_err(AppError::validation)?;
    Ok(auth_session_status(state, remote_request, true))
}

/// Clears the remote admin password used by `/api/*` and `/management`.
pub fn clear_remote_admin_password(
    state: &SharedState,
    remote_request: bool,
) -> AppResult<AuthSessionStatus> {
    state
        .remote_admin_auth
        .clear_password()
        .map_err(AppError::internal)?;
    Ok(auth_session_status(state, remote_request, true))
}

/// Saves config for this module's workflow.
pub async fn save_config(
    state: &SharedState,
    app: &AppHandle,
    next_config: Value,
) -> AppResult<SaveConfigResult> {
    let prev = state.config_store.get();
    let saved = state.config_store.save(next_config)?;

    apply_launch_on_startup_setting(app, saved.ui.launch_on_startup);
    let (restarted, status) = sync_runtime_config(state, prev, saved.clone()).await?;

    Ok(SaveConfigResult {
        ok: true,
        config: saved,
        restarted,
        status,
    })
}

/// Performs import groups payload.
pub async fn import_groups_payload(
    state: &SharedState,
    parsed: Value,
    mode: Option<GroupImportMode>,
) -> AppResult<(usize, ProxyConfig, bool, ProxyStatus)> {
    let imported_groups =
        extract_groups_from_import_payload(&parsed).map_err(AppError::validation)?;
    let imported_group_count = imported_groups.len();
    let prev = state.config_store.get();
    let mut next = prev.clone();
    match mode.unwrap_or(GroupImportMode::Incremental) {
        GroupImportMode::Incremental => {
            next.groups = merge_imported_groups(&prev.groups, &imported_groups);
        }
        GroupImportMode::Overwrite => {
            next.groups = imported_groups;
            next.providers = vec![];
        }
    }

    let saved = state.config_store.save_config(next)?;
    let (restarted, status) = sync_runtime_config(state, prev, saved.clone()).await?;

    Ok((imported_group_count, saved, restarted, status))
}

/// Performs import groups with source.
pub async fn import_groups_with_source(
    state: &SharedState,
    parsed: Value,
    source: &str,
    file_path: Option<String>,
    mode: Option<GroupImportMode>,
) -> AppResult<GroupBackupImportResult> {
    let (groups_len, saved, restarted, status) = import_groups_payload(state, parsed, mode).await?;

    Ok(GroupBackupImportResult {
        ok: true,
        canceled: false,
        source: Some(source.to_string()),
        file_path,
        imported_group_count: Some(groups_len),
        config: Some(saved),
        restarted: Some(restarted),
        status: Some(status),
    })
}

/// Merges imported groups for this module's workflow.
fn merge_imported_groups(current: &[Group], imported: &[Group]) -> Vec<Group> {
    let mut merged = current.to_vec();
    let mut index_by_group_path: HashMap<String, usize> = HashMap::new();
    for (index, group) in merged.iter().enumerate() {
        index_by_group_path.insert(group.id.clone(), index);
    }

    for imported_group in imported {
        if let Some(index) = index_by_group_path.get(&imported_group.id).copied() {
            merged[index] = merge_group_by_provider_name(&merged[index], imported_group);
            continue;
        }
        let normalized = normalize_group_provider_ids(imported_group.clone());
        index_by_group_path.insert(normalized.id.clone(), merged.len());
        merged.push(normalized);
    }

    merged
}

/// Merges group by provider name for this module's workflow.
fn merge_group_by_provider_name(current: &Group, imported: &Group) -> Group {
    let mut providers = current.providers.clone();
    let mut current_index_by_name: HashMap<String, usize> = HashMap::new();
    for (index, provider) in providers.iter().enumerate() {
        current_index_by_name
            .entry(provider_name_key(&provider.name))
            .or_insert(index);
    }

    let mut used_provider_ids: HashSet<String> = providers
        .iter()
        .filter_map(|provider| {
            let id = provider.id.trim();
            if id.is_empty() {
                None
            } else {
                Some(id.to_string())
            }
        })
        .collect();

    let imported_active_name = imported
        .active_provider_id
        .as_ref()
        .and_then(|active_id| {
            imported
                .providers
                .iter()
                .find(|provider| provider.id == *active_id)
        })
        .map(|provider| provider.name.clone());

    for imported_provider in &imported.providers {
        let name_key = provider_name_key(&imported_provider.name);
        if let Some(index) = current_index_by_name.get(&name_key).copied() {
            let mut next_provider = imported_provider.clone();
            next_provider.id = providers[index].id.clone();
            providers[index] = next_provider;
            continue;
        }

        let mut next_provider = imported_provider.clone();
        next_provider.id = alloc_provider_id(&imported_provider.id, &mut used_provider_ids);
        current_index_by_name.insert(name_key, providers.len());
        providers.push(next_provider);
    }

    let mut next_active_provider_id = current.active_provider_id.clone();
    if let Some(active_name) = imported_active_name {
        if let Some(provider) = providers
            .iter()
            .find(|provider| provider.name == active_name)
        {
            next_active_provider_id = Some(provider.id.clone());
        }
    }
    if let Some(active_id) = next_active_provider_id.clone() {
        let exists = providers.iter().any(|provider| provider.id == active_id);
        if !exists {
            next_active_provider_id = None;
        }
    }

    Group {
        id: current.id.clone(),
        name: imported.name.clone(),
        models: imported.models.clone(),
        provider_ids: providers
            .iter()
            .map(|provider| provider.id.clone())
            .collect(),
        active_provider_id: next_active_provider_id,
        providers,
        failover: current.failover.clone(),
    }
}

/// Normalizes group provider IDs for this module's workflow.
fn normalize_group_provider_ids(mut group: Group) -> Group {
    let mut used_ids = HashSet::new();
    let mut old_to_new_id: HashMap<String, String> = HashMap::new();
    for provider in &mut group.providers {
        let old_id = provider.id.clone();
        let new_id = alloc_provider_id(&provider.id, &mut used_ids);
        provider.id = new_id.clone();
        if !old_id.trim().is_empty() {
            old_to_new_id.insert(old_id, new_id);
        }
    }

    if let Some(active_id) = group.active_provider_id.clone() {
        if let Some(next_active_id) = old_to_new_id.get(&active_id) {
            group.active_provider_id = Some(next_active_id.clone());
        } else {
            let exists = group
                .providers
                .iter()
                .any(|provider| provider.id == active_id);
            if !exists {
                group.active_provider_id = None;
            }
        }
    }

    group.provider_ids = group
        .providers
        .iter()
        .map(|provider| provider.id.clone())
        .collect();
    group
}

/// Performs alloc provider ID.
fn alloc_provider_id(candidate: &str, used_ids: &mut HashSet<String>) -> String {
    let candidate = candidate.trim();
    if !candidate.is_empty() && !used_ids.contains(candidate) {
        used_ids.insert(candidate.to_string());
        return candidate.to_string();
    }
    loop {
        let id = Uuid::new_v4().to_string();
        if used_ids.insert(id.clone()) {
            return id;
        }
    }
}

/// Performs provider name key.
fn provider_name_key(name: &str) -> String {
    name.trim().to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_state::AppState;
    use crate::config::schema::default_config;
    use crate::domain::entities::{
        default_group_failover_config, default_rule_cost_config, default_rule_quota_config, Rule,
        RuleProtocol,
    };
    use crate::integration_store::IntegrationStore;
    use crate::log_store::LogStore;
    use crate::models::{AppInfo, GroupImportMode};
    use crate::proxy::ProxyRuntime;
    use crate::stats_store::StatsStore;
    use serde_json::json;
    use std::collections::HashMap;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Performs provider.
    fn provider(id: &str, name: &str, model: &str) -> Rule {
        Rule {
            id: id.to_string(),
            name: name.to_string(),
            protocol: RuleProtocol::Openai,
            token: "token".to_string(),
            api_address: "https://example.com".to_string(),
            website: String::new(),
            default_model: model.to_string(),
            model_mappings: HashMap::new(),
            header_passthrough_allow: Vec::new(),
            header_passthrough_deny: Vec::new(),
            quota: default_rule_quota_config(),
            cost: default_rule_cost_config(),
        }
    }

    /// Performs group.
    fn group(id: &str, name: &str, active: Option<&str>, providers: Vec<Rule>) -> Group {
        Group {
            id: id.to_string(),
            name: name.to_string(),
            models: vec!["model-a".to_string()],
            provider_ids: providers
                .iter()
                .map(|provider| provider.id.clone())
                .collect(),
            active_provider_id: active.map(|v| v.to_string()),
            providers,
            failover: default_group_failover_config(),
        }
    }

    fn test_shared_state() -> SharedState {
        let unique_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time must move forward")
            .as_nanos();
        let base_dir = std::env::temp_dir().join(format!("oc-proxy-config-service-{unique_id}"));
        std::fs::create_dir_all(&base_dir).expect("temp dir should be created");

        let config_store = crate::config_store::ConfigStore::new(base_dir.join("config.json"));
        config_store
            .initialize()
            .expect("config store should initialize");

        let integration_store = IntegrationStore::new(base_dir.join("integrations.json"));
        integration_store
            .initialize()
            .expect("integration store should initialize");

        let remote_admin_auth =
            crate::auth::RemoteAdminAuthStore::new(base_dir.join("remote-admin-auth.json"));
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

    #[test]
    /// Performs import merge updates by group ID and provider name.
    fn import_merge_updates_by_group_id_and_provider_name() {
        let current = vec![group(
            "group-a",
            "Local",
            Some("p-local"),
            vec![
                provider("p-local", "alpha", "old-model"),
                provider("p-keep", "keep", "m2"),
            ],
        )];
        let imported = vec![group(
            "group-a",
            "Imported",
            Some("p-import"),
            vec![
                provider("p-import", "alpha", "new-model"),
                provider("p-new", "beta", "m3"),
            ],
        )];

        let merged = merge_imported_groups(&current, &imported);
        assert_eq!(merged.len(), 1);
        let merged_group = &merged[0];
        assert_eq!(merged_group.name, "Imported");
        assert_eq!(merged_group.providers.len(), 3);
        let alpha = merged_group
            .providers
            .iter()
            .find(|provider| provider.name == "alpha")
            .expect("alpha provider exists");
        assert_eq!(alpha.id, "p-local");
        assert_eq!(alpha.default_model, "new-model");
        assert!(merged_group
            .providers
            .iter()
            .any(|provider| provider.name == "keep"));
        assert!(merged_group
            .providers
            .iter()
            .any(|provider| provider.name == "beta"));
        assert_eq!(merged_group.active_provider_id, Some("p-local".to_string()));
    }

    #[test]
    /// Performs import merge preserves current failover config.
    fn import_merge_preserves_current_failover_config() {
        let current = vec![Group {
            failover: crate::domain::entities::GroupFailoverConfig {
                enabled: true,
                failure_threshold: 4,
                cooldown_seconds: 90,
            },
            ..group(
                "group-a",
                "Local",
                Some("p-local"),
                vec![provider("p-local", "alpha", "old-model")],
            )
        }];
        let imported = vec![group(
            "group-a",
            "Imported",
            Some("p-import"),
            vec![provider("p-import", "alpha", "new-model")],
        )];

        let merged = merge_imported_groups(&current, &imported);
        assert_eq!(merged.len(), 1);
        assert!(merged[0].failover.enabled);
        assert_eq!(merged[0].failover.failure_threshold, 4);
        assert_eq!(merged[0].failover.cooldown_seconds, 90);
    }

    #[test]
    /// Performs import merge keeps local groups missing in import.
    fn import_merge_keeps_local_groups_missing_in_import() {
        let current = vec![group(
            "group-local",
            "Local",
            None,
            vec![provider("p1", "x", "m1")],
        )];
        let imported = vec![group(
            "group-new",
            "New",
            None,
            vec![provider("p2", "y", "m2")],
        )];
        let merged = merge_imported_groups(&current, &imported);
        assert_eq!(merged.len(), 2);
        assert!(merged.iter().any(|group| group.id == "group-local"));
        assert!(merged.iter().any(|group| group.id == "group-new"));
    }

    #[test]
    fn set_remote_admin_password_marks_remote_request_authenticated() {
        let state = test_shared_state();

        let status =
            set_remote_admin_password(&state, "correct horse battery staple".to_string(), true)
                .expect("password should be set");

        assert!(status.remote_request);
        assert!(status.password_configured);
        assert!(status.authenticated);
    }

    #[tokio::test]
    async fn import_groups_incremental_keeps_existing_top_level_config() {
        let state = test_shared_state();
        let mut initial = default_config();
        initial.server.host = "127.0.0.1".to_string();
        initial.ui.theme = "dark".to_string();
        initial.groups = vec![group(
            "group-local",
            "Local",
            Some("p-local"),
            vec![provider("p-local", "alpha", "old-model")],
        )];
        initial.providers = initial.groups[0].providers.clone();
        state
            .config_store
            .save_config(initial)
            .expect("initial config should save");

        let parsed = json!({
            "groups": [
                {
                    "id": "group-local",
                    "name": "Imported",
                    "models": ["model-b"],
                    "activeProviderId": "p-import",
                    "providers": [
                        {
                            "id": "p-import",
                            "name": "alpha",
                            "protocol": "openai",
                            "token": "token",
                            "apiAddress": "https://example.com",
                            "defaultModel": "new-model"
                        }
                    ]
                }
            ]
        });

        let (_, saved, _, _) =
            import_groups_payload(&state, parsed, Some(GroupImportMode::Incremental))
                .await
                .expect("incremental import should succeed");

        assert_eq!(saved.server.host, "127.0.0.1");
        assert_eq!(saved.ui.theme, "dark");
        assert_eq!(saved.groups.len(), 1);
        assert_eq!(saved.groups[0].id, "group-local");
        assert_eq!(saved.groups[0].providers.len(), 1);
        assert_eq!(saved.groups[0].providers[0].name, "alpha");
        assert_eq!(saved.groups[0].providers[0].default_model, "new-model");
    }

    #[tokio::test]
    async fn import_groups_without_mode_defaults_to_incremental() {
        let state = test_shared_state();
        let mut initial = default_config();
        initial.groups = vec![group(
            "group-local",
            "Local",
            Some("p-local"),
            vec![provider("p-local", "alpha", "old-model")],
        )];
        initial.providers = initial.groups[0].providers.clone();
        state
            .config_store
            .save_config(initial)
            .expect("initial config should save");

        let parsed = json!({
            "groups": [
                {
                    "id": "group-local",
                    "name": "Imported",
                    "models": ["model-b"],
                    "activeProviderId": "p-import",
                    "providers": [
                        {
                            "id": "p-import",
                            "name": "alpha",
                            "protocol": "openai",
                            "token": "token",
                            "apiAddress": "https://example.com",
                            "defaultModel": "new-model"
                        }
                    ]
                }
            ]
        });

        let (_, saved, _, _) = import_groups_payload(&state, parsed, None)
            .await
            .expect("default import should succeed");

        assert_eq!(saved.groups.len(), 1);
        assert_eq!(saved.groups[0].name, "Imported");
        assert_eq!(saved.groups[0].providers.len(), 1);
        assert_eq!(saved.groups[0].providers[0].name, "alpha");
        assert_eq!(saved.groups[0].providers[0].default_model, "new-model");
    }

    #[tokio::test]
    async fn import_groups_overwrite_replaces_groups_and_global_providers_only() {
        let state = test_shared_state();
        let mut initial = default_config();
        initial.server.host = "127.0.0.1".to_string();
        initial.server.port = 9999;
        initial.ui.theme = "dark".to_string();
        initial.groups = vec![group(
            "group-local",
            "Local",
            Some("p-local"),
            vec![provider("p-local", "alpha", "old-model")],
        )];
        initial.providers = vec![
            provider("p-local", "alpha", "old-model"),
            provider("p-stale", "stale", "stale-model"),
        ];
        state
            .config_store
            .save_config(initial)
            .expect("initial config should save");

        let parsed = json!({
            "groups": [
                {
                    "id": "group-imported",
                    "name": "Imported",
                    "models": ["model-b"],
                    "activeProviderId": "p-import",
                    "providers": [
                        {
                            "id": "p-import",
                            "name": "beta",
                            "protocol": "openai",
                            "token": "token",
                            "apiAddress": "https://example.com",
                            "defaultModel": "new-model"
                        }
                    ]
                }
            ]
        });

        let (_, saved, _, _) =
            import_groups_payload(&state, parsed, Some(GroupImportMode::Overwrite))
                .await
                .expect("overwrite import should succeed");

        assert_eq!(saved.server.host, "127.0.0.1");
        assert_eq!(saved.server.port, 9999);
        assert_eq!(saved.ui.theme, "dark");
        assert_eq!(saved.groups.len(), 1);
        assert_eq!(saved.groups[0].id, "group-imported");
        assert_eq!(saved.groups[0].providers.len(), 1);
        assert_eq!(saved.groups[0].providers[0].name, "beta");
        assert_eq!(saved.providers.len(), 1);
        assert_eq!(saved.providers[0].name, "beta");
        assert!(!saved
            .providers
            .iter()
            .any(|provider| provider.name == "stale"));
    }
}
