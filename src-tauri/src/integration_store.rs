//! Module Overview
//! Persistent store for external client integration targets.
//! Keeps selected config directories for Claude/Codex/OpenCode in local app data.

use crate::models::{IntegrationClientKind, IntegrationTarget};
use chrono::Utc;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Clone)]
pub struct IntegrationStore {
    file_path: PathBuf,
    targets: Arc<Mutex<Vec<IntegrationTarget>>>,
}

impl IntegrationStore {
    /// Performs new.
    pub fn new(file_path: PathBuf) -> Self {
        Self {
            file_path,
            targets: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Initializes data for this module's workflow.
    pub fn initialize(&self) -> Result<(), String> {
        if let Some(parent) = self.file_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("create integration store dir failed: {e}"))?;
        }

        if !self.file_path.exists() {
            self.persist(&[])?;
            return Ok(());
        }

        let raw = std::fs::read_to_string(&self.file_path)
            .map_err(|e| format!("read integration store failed: {e}"))?;
        if raw.trim().is_empty() {
            self.persist(&[])?;
            return Ok(());
        }

        let parsed = serde_json::from_str::<Vec<IntegrationTarget>>(&raw)
            .map_err(|e| format!("parse integration store failed: {e}"))?;
        let mut guard = self
            .targets
            .lock()
            .map_err(|_| "integration store lock poisoned".to_string())?;
        *guard = parsed;
        Ok(())
    }

    /// Performs list.
    pub fn list(&self) -> Vec<IntegrationTarget> {
        let guard = self
            .targets
            .lock()
            .expect("integration store lock poisoned");
        let mut items = guard.clone();
        items.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.id.cmp(&right.id))
        });
        items
    }

    /// Adds target for this module's workflow.
    pub fn add_target(
        &self,
        kind: IntegrationClientKind,
        config_dir: String,
    ) -> Result<IntegrationTarget, String> {
        let normalized_dir = normalize_config_dir(&config_dir)?;
        let now = Utc::now().to_rfc3339();
        let mut guard = self
            .targets
            .lock()
            .map_err(|_| "integration store lock poisoned".to_string())?;

        if guard
            .iter()
            .any(|item| item.kind == kind && item.config_dir == normalized_dir)
        {
            return Err("same config directory already exists".to_string());
        }

        let target = IntegrationTarget {
            id: Uuid::new_v4().to_string(),
            kind,
            config_dir: normalized_dir,
            created_at: now.clone(),
            updated_at: now,
        };
        guard.push(target.clone());
        self.persist(&guard)?;
        Ok(target)
    }

    /// Updates target directory for this module's workflow.
    pub fn update_target(
        &self,
        target_id: &str,
        config_dir: String,
    ) -> Result<IntegrationTarget, String> {
        let normalized_dir = normalize_config_dir(&config_dir)?;
        let normalized_id = target_id.trim();
        if normalized_id.is_empty() {
            return Err("target id is required".to_string());
        }
        let mut guard = self
            .targets
            .lock()
            .map_err(|_| "integration store lock poisoned".to_string())?;

        let index = guard
            .iter()
            .position(|item| item.id == normalized_id)
            .ok_or_else(|| "integration target not found".to_string())?;

        let kind = guard[index].kind.clone();
        if guard.iter().enumerate().any(|(item_index, item)| {
            item_index != index && item.kind == kind && item.config_dir == normalized_dir
        }) {
            return Err("same config directory already exists".to_string());
        }

        guard[index].config_dir = normalized_dir;
        guard[index].updated_at = Utc::now().to_rfc3339();
        let updated = guard[index].clone();
        self.persist(&guard)?;
        Ok(updated)
    }

    /// Removes target for this module's workflow.
    pub fn remove_target(&self, target_id: &str) -> Result<bool, String> {
        let normalized_id = target_id.trim();
        if normalized_id.is_empty() {
            return Err("target id is required".to_string());
        }
        let mut guard = self
            .targets
            .lock()
            .map_err(|_| "integration store lock poisoned".to_string())?;
        let before = guard.len();
        guard.retain(|item| item.id != normalized_id);
        let changed = before != guard.len();
        if changed {
            self.persist(&guard)?;
        }
        Ok(changed)
    }

    /// Persists data for this module's workflow.
    fn persist(&self, targets: &[IntegrationTarget]) -> Result<(), String> {
        let text = serde_json::to_string_pretty(targets)
            .map_err(|e| format!("serialize integration store failed: {e}"))?;
        std::fs::write(&self.file_path, text)
            .map_err(|e| format!("write integration store failed: {e}"))
    }
}

/// Normalizes config directory path for this module's workflow.
fn normalize_config_dir(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("config directory is required".to_string());
    }
    let path = PathBuf::from(trimmed);
    if !path.exists() {
        return Err(format!("config directory does not exist: {trimmed}"));
    }
    if !path.is_dir() {
        return Err(format!("config directory is not a folder: {trimmed}"));
    }
    let normalized = std::fs::canonicalize(&path).unwrap_or(path);
    Ok(normalized.to_string_lossy().to_string())
}
