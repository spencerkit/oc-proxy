//! Module Overview
//! Service layer orchestration for feature-specific workflows.
//! Coordinates validation, persistence, runtime sync, and structured results.

use crate::app_state::{apply_launch_on_startup_setting, sync_runtime_config, SharedState};
use crate::backup::extract_groups_from_import_payload;
use crate::domain::entities::Group;
use crate::models::{GroupBackupImportResult, ProxyConfig, ProxyStatus, SaveConfigResult};
use crate::services::{AppError, AppResult};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use tauri::AppHandle;
use uuid::Uuid;

/// Performs get config.
pub fn get_config(state: &SharedState) -> ProxyConfig {
    state.config_store.get()
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
) -> AppResult<(usize, ProxyConfig, bool, ProxyStatus)> {
    let imported_groups = extract_groups_from_import_payload(&parsed).map_err(AppError::validation)?;
    let prev = state.config_store.get();
    let mut next = prev.clone();
    next.groups = merge_imported_groups(&prev.groups, &imported_groups);

    let saved = state.config_store.save_config(next)?;
    let (restarted, status) = sync_runtime_config(state, prev, saved.clone()).await?;

    Ok((imported_groups.len(), saved, restarted, status))
}

/// Performs import groups with source.
pub async fn import_groups_with_source(
    state: &SharedState,
    parsed: Value,
    source: &str,
    file_path: Option<String>,
) -> AppResult<GroupBackupImportResult> {
    let (groups_len, saved, restarted, status) = import_groups_payload(state, parsed).await?;

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
        .and_then(|active_id| imported.providers.iter().find(|provider| provider.id == *active_id))
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
        if let Some(provider) = providers.iter().find(|provider| provider.name == active_name) {
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
        active_provider_id: next_active_provider_id,
        providers,
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
            let exists = group.providers.iter().any(|provider| provider.id == active_id);
            if !exists {
                group.active_provider_id = None;
            }
        }
    }

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
    use crate::domain::entities::{
        default_rule_cost_config, default_rule_quota_config, Rule, RuleProtocol,
    };
    use std::collections::HashMap;

    /// Performs provider.
    fn provider(id: &str, name: &str, model: &str) -> Rule {
        Rule {
            id: id.to_string(),
            name: name.to_string(),
            protocol: RuleProtocol::Openai,
            token: "token".to_string(),
            api_address: "https://example.com".to_string(),
            default_model: model.to_string(),
            model_mappings: HashMap::new(),
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
            active_provider_id: active.map(|v| v.to_string()),
            providers,
        }
    }

    #[test]
    /// Performs import merge updates by group ID and provider name.
    fn import_merge_updates_by_group_id_and_provider_name() {
        let current = vec![group(
            "group-a",
            "Local",
            Some("p-local"),
            vec![provider("p-local", "alpha", "old-model"), provider("p-keep", "keep", "m2")],
        )];
        let imported = vec![group(
            "group-a",
            "Imported",
            Some("p-import"),
            vec![provider("p-import", "alpha", "new-model"), provider("p-new", "beta", "m3")],
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
    /// Performs import merge keeps local groups missing in import.
    fn import_merge_keeps_local_groups_missing_in_import() {
        let current = vec![group("group-local", "Local", None, vec![provider("p1", "x", "m1")])];
        let imported = vec![group("group-new", "New", None, vec![provider("p2", "y", "m2")])];
        let merged = merge_imported_groups(&current, &imported);
        assert_eq!(merged.len(), 2);
        assert!(merged.iter().any(|group| group.id == "group-local"));
        assert!(merged.iter().any(|group| group.id == "group-new"));
    }
}
