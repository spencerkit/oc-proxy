use crate::config::schema::normalize_config;
use crate::models::{default_config, validate_config, ProxyConfig};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

#[derive(Clone)]
pub struct ConfigStore {
    file_path: PathBuf,
    config: Arc<RwLock<ProxyConfig>>,
    revision: Arc<AtomicU64>,
}

impl ConfigStore {
    pub fn new(file_path: PathBuf) -> Self {
        Self {
            file_path,
            config: Arc::new(RwLock::new(default_config())),
            revision: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn initialize(&self) -> Result<(), String> {
        if let Some(parent) = self.file_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("create config dir failed: {e}"))?;
        }

        if !self.file_path.exists() {
            let defaults = default_config();
            self.write_file(&defaults)?;
            self.set_in_memory(defaults);
            return Ok(());
        }

        let raw = std::fs::read_to_string(&self.file_path)
            .map_err(|e| format!("read config failed: {e}"))?;

        let parsed = serde_json::from_str::<serde_json::Value>(&raw)
            .unwrap_or_else(|_| serde_json::json!({}));
        let normalized = normalize_config(parsed)?;

        if let Err(err) = validate_config(&normalized) {
            let defaults = default_config();
            self.write_file(&defaults)?;
            self.set_in_memory(defaults);
            return Err(format!("config invalid, reset to default: {err}"));
        }

        self.set_in_memory(normalized);
        Ok(())
    }

    pub fn get(&self) -> ProxyConfig {
        self.config.read().expect("config rwlock poisoned").clone()
    }

    pub fn save(&self, next_config: serde_json::Value) -> Result<ProxyConfig, String> {
        let normalized = normalize_config(next_config)?;
        validate_config(&normalized)?;
        self.write_file(&normalized)?;
        self.set_in_memory(normalized.clone());
        Ok(normalized)
    }

    pub fn save_config(&self, next_config: ProxyConfig) -> Result<ProxyConfig, String> {
        validate_config(&next_config)?;
        self.write_file(&next_config)?;
        self.set_in_memory(next_config.clone());
        Ok(next_config)
    }

    fn write_file(&self, cfg: &ProxyConfig) -> Result<(), String> {
        let text = serde_json::to_string_pretty(cfg)
            .map_err(|e| format!("serialize config failed: {e}"))?;
        std::fs::write(&self.file_path, text).map_err(|e| format!("write config failed: {e}"))
    }

    fn set_in_memory(&self, cfg: ProxyConfig) {
        if let Ok(mut guard) = self.config.write() {
            *guard = cfg;
            let _ = self.revision.fetch_add(1, Ordering::Release);
        }
    }

    pub fn path(&self) -> &Path {
        &self.file_path
    }

    pub fn shared_config(&self) -> Arc<RwLock<ProxyConfig>> {
        self.config.clone()
    }

    pub fn shared_revision(&self) -> Arc<AtomicU64> {
        self.revision.clone()
    }
}
