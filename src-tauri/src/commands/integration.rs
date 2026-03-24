//! Module Overview
//! Tauri command handlers for external client integration workflows.

use crate::api::dto::{AgentConfig, AgentConfigFile, WriteAgentConfigResult};
use crate::app_state::SharedState;
use crate::models::{IntegrationClientKind, IntegrationTarget, IntegrationWriteResult};
use crate::services::integration_service;
use crate::user_home::user_home_dir;
use crate::wsl;
use serde_json::json;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

#[tauri::command]
/// Lists all saved integration targets.
pub async fn integration_list_targets(
    state: State<'_, SharedState>,
) -> Result<Vec<IntegrationTarget>, String> {
    Ok(integration_service::list_targets(&state))
}

#[tauri::command]
/// Opens folder picker and returns selected directory path.
pub async fn integration_pick_directory(
    app: AppHandle,
    initial_dir: Option<String>,
    kind: Option<IntegrationClientKind>,
) -> Result<Option<String>, String> {
    let mut builder = app
        .dialog()
        .file()
        .set_title("Select Configuration Directory");

    if let Some(starting_dir) = resolve_starting_directory(initial_dir, kind) {
        builder = builder.set_directory(starting_dir);
    }

    let picked = builder.blocking_pick_folder();
    match picked {
        Some(path) => {
            let path = path
                .into_path()
                .map_err(|e| format!("invalid folder path: {e}"))?;
            Ok(Some(path.to_string_lossy().to_string()))
        }
        None => Ok(None),
    }
}

/// Resolves starting directory for picker.
fn resolve_starting_directory(
    initial_dir: Option<String>,
    kind: Option<IntegrationClientKind>,
) -> Option<PathBuf> {
    let home = user_home_dir()?;
    resolve_starting_directory_with_root_paths(
        initial_dir,
        kind,
        &home,
        Path::new("/root"),
        Path::new("/"),
    )
}

fn resolve_starting_directory_with_root_paths(
    initial_dir: Option<String>,
    kind: Option<IntegrationClientKind>,
    home: &Path,
    root_home: &Path,
    root_base: &Path,
) -> Option<PathBuf> {
    if let Some(dir) = initial_dir {
        let path = PathBuf::from(dir.trim());
        if wsl::is_wsl_path(&path) {
            if wsl::is_dir(&path).ok()? {
                return wsl::normalize_windows_path(&path);
            }
        } else if path.exists() {
            return Some(path);
        }
    }

    let candidate = match kind {
        Some(kind) => Some(
            integration_service::preferred_client_config_dir_with_root_paths(
                &kind, home, root_home, root_base,
            ),
        ),
        None => None,
    };
    match candidate {
        Some(path) if path.exists() => Some(path),
        _ => Some(home.to_path_buf()),
    }
}

#[tauri::command]
/// Adds one integration target.
pub async fn integration_add_target(
    state: State<'_, SharedState>,
    kind: IntegrationClientKind,
    config_dir: String,
) -> Result<IntegrationTarget, String> {
    integration_service::add_target(&state, kind, config_dir).map_err(|e| e.to_string())
}

#[tauri::command]
/// Updates one integration target directory.
pub async fn integration_update_target(
    state: State<'_, SharedState>,
    target_id: String,
    config_dir: String,
) -> Result<IntegrationTarget, String> {
    integration_service::update_target(&state, &target_id, config_dir).map_err(|e| e.to_string())
}

#[tauri::command]
/// Removes one integration target.
pub async fn integration_remove_target(
    state: State<'_, SharedState>,
    target_id: String,
) -> Result<serde_json::Value, String> {
    let removed =
        integration_service::remove_target(&state, &target_id).map_err(|e| e.to_string())?;
    Ok(json!({
        "ok": true,
        "removed": removed,
    }))
}

#[tauri::command]
/// Writes current group entry URL into selected integration targets.
pub async fn integration_write_group_entry(
    state: State<'_, SharedState>,
    group_id: String,
    target_ids: Vec<String>,
) -> Result<IntegrationWriteResult, String> {
    integration_service::write_group_entry(&state, &group_id, target_ids).map_err(|e| e.to_string())
}

#[tauri::command]
/// Reads agent configuration file content.
pub async fn integration_read_agent_config(
    state: State<'_, SharedState>,
    target_id: String,
) -> Result<AgentConfigFile, String> {
    integration_service::read_agent_config(&state, &target_id).map_err(|e| e.to_string())
}

#[tauri::command]
/// Writes agent configuration to file.
pub async fn integration_write_agent_config(
    state: State<'_, SharedState>,
    target_id: String,
    config: AgentConfig,
) -> Result<WriteAgentConfigResult, String> {
    integration_service::write_agent_config(&state, &target_id, config).map_err(|e| e.to_string())
}

#[tauri::command]
/// Writes raw agent configuration source to file.
pub async fn integration_write_agent_config_source(
    state: State<'_, SharedState>,
    target_id: String,
    content: String,
    source_id: Option<String>,
) -> Result<WriteAgentConfigResult, String> {
    integration_service::write_agent_config_source(
        &state,
        &target_id,
        &content,
        source_id.as_deref(),
    )
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::resolve_starting_directory_with_root_paths;
    use crate::models::IntegrationClientKind;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn resolve_starting_directory_prefers_root_level_hidden_dir_for_root_home() {
        let unique_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time must move forward")
            .as_nanos();
        let sandbox_root =
            std::env::temp_dir().join(format!("oc-proxy-picker-root-start-{unique_id}"));
        let fake_root_home = sandbox_root.join("root-home");
        let fake_root_base = sandbox_root.join("fs-root");
        let root_level = fake_root_base.join(".openclaw");

        std::fs::create_dir_all(&fake_root_home).expect("fake root home should be created");
        std::fs::create_dir_all(&root_level).expect("root-level openclaw dir should be created");

        let preferred = resolve_starting_directory_with_root_paths(
            None,
            Some(IntegrationClientKind::Openclaw),
            &fake_root_home,
            &fake_root_home,
            &fake_root_base,
        );
        assert_eq!(preferred, Some(PathBuf::from(&root_level)));

        let _ = std::fs::remove_dir_all(&sandbox_root);
    }
}
