//! Module Overview
//! Tauri command handlers for external client integration workflows.

use crate::app_state::SharedState;
use crate::models::{IntegrationClientKind, IntegrationTarget, IntegrationWriteResult};
use crate::services::integration_service;
use crate::wsl;
use serde_json::json;
use std::path::PathBuf;
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

    let home = user_home_dir()?;
    let candidate = match kind {
        Some(IntegrationClientKind::Claude) => Some(home.join(".claude")),
        Some(IntegrationClientKind::Codex) => Some(home.join(".codex")),
        Some(IntegrationClientKind::Opencode) => {
            let config_path = home.join(".config").join("opencode");
            if config_path.exists() {
                Some(config_path)
            } else {
                Some(home.join(".local").join("share").join("opencode"))
            }
        }
        None => None,
    };
    match candidate {
        Some(path) if path.exists() => Some(path),
        _ => Some(home),
    }
}

/// Resolves user home directory from environment variables.
fn user_home_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var_os("HOME").map(PathBuf::from)
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
