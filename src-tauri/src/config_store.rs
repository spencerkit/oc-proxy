use crate::models::{default_config, validate_config, ProxyConfig};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

#[derive(Clone)]
pub struct ConfigStore {
    file_path: PathBuf,
    config: Arc<RwLock<ProxyConfig>>,
}

impl ConfigStore {
    pub fn new(file_path: PathBuf) -> Self {
        Self {
            file_path,
            config: Arc::new(RwLock::new(default_config())),
        }
    }

    pub fn initialize(&self) -> Result<(), String> {
        if let Some(parent) = self.file_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("create config dir failed: {e}"))?;
        }

        if !self.file_path.exists() {
            let defaults = default_config();
            self.write_file(&defaults)?;
            self.set_in_memory(defaults);
            return Ok(());
        }

        let raw = std::fs::read_to_string(&self.file_path)
            .map_err(|e| format!("read config failed: {e}"))?;

        let parsed = serde_json::from_str::<serde_json::Value>(&raw).unwrap_or_else(|_| serde_json::json!({}));
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
        self.config
            .read()
            .expect("config rwlock poisoned")
            .clone()
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
        let text = serde_json::to_string_pretty(cfg).map_err(|e| format!("serialize config failed: {e}"))?;
        std::fs::write(&self.file_path, text).map_err(|e| format!("write config failed: {e}"))
    }

    fn set_in_memory(&self, cfg: ProxyConfig) {
        if let Ok(mut guard) = self.config.write() {
            *guard = cfg;
        }
    }

    pub fn path(&self) -> &Path {
        &self.file_path
    }

    pub fn shared_config(&self) -> Arc<RwLock<ProxyConfig>> {
        self.config.clone()
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PartialServerConfig {
    host: Option<String>,
    port: Option<u16>,
    auth_enabled: Option<bool>,
    local_bearer_token: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PartialCompatConfig {
    strict_mode: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PartialLoggingConfig {
    level: Option<String>,
    capture_body: Option<bool>,
    redact_rules: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PartialUiConfig {
    theme: Option<String>,
    locale: Option<String>,
    locale_mode: Option<String>,
    launch_on_startup: Option<bool>,
    close_to_tray: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PartialRemoteGitConfig {
    enabled: Option<bool>,
    repo_url: Option<String>,
    token: Option<String>,
    branch: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PartialProxyConfig {
    server: Option<PartialServerConfig>,
    compat: Option<PartialCompatConfig>,
    logging: Option<PartialLoggingConfig>,
    ui: Option<PartialUiConfig>,
    remote_git: Option<PartialRemoteGitConfig>,
    groups: Option<serde_json::Value>,
}

pub fn normalize_config(input: serde_json::Value) -> Result<ProxyConfig, String> {
    let defaults = default_config();
    let partial = serde_json::from_value::<PartialProxyConfig>(input)
        .map_err(|e| format!("invalid config structure: {e}"))?;

    let groups = if let Some(raw_groups) = partial.groups {
        serde_json::from_value(raw_groups).unwrap_or_default()
    } else {
        defaults.groups
    };

    let locale = partial
        .ui
        .as_ref()
        .and_then(|u| u.locale.clone())
        .unwrap_or_else(|| defaults.ui.locale.clone());
    let normalized_locale = if locale == "zh-CN" { "zh-CN" } else { "en-US" }.to_string();

    let locale_mode = partial
        .ui
        .as_ref()
        .and_then(|u| u.locale_mode.clone())
        .unwrap_or_else(|| {
            if locale == "zh-CN" {
                "manual".to_string()
            } else {
                defaults.ui.locale_mode.clone()
            }
        });
    let normalized_locale_mode = if locale_mode == "manual" { "manual" } else { "auto" }.to_string();

    let remote_repo_url = partial
        .remote_git
        .as_ref()
        .and_then(|r| r.repo_url.clone())
        .unwrap_or(defaults.remote_git.repo_url);
    let remote_token = partial
        .remote_git
        .as_ref()
        .and_then(|r| r.token.clone())
        .unwrap_or(defaults.remote_git.token);
    let remote_branch = partial
        .remote_git
        .as_ref()
        .and_then(|r| r.branch.clone())
        .filter(|v| !v.trim().is_empty())
        .unwrap_or(defaults.remote_git.branch);
    let remote_enabled = partial
        .remote_git
        .as_ref()
        .and_then(|r| r.enabled)
        .unwrap_or_else(|| !remote_repo_url.trim().is_empty() || !remote_token.trim().is_empty());

    Ok(ProxyConfig {
        server: crate::models::ServerConfig {
            host: defaults.server.host,
            port: partial
                .server
                .as_ref()
                .and_then(|s| s.port)
                .unwrap_or(defaults.server.port),
            auth_enabled: partial
                .server
                .as_ref()
                .and_then(|s| s.auth_enabled)
                .unwrap_or(defaults.server.auth_enabled),
            local_bearer_token: partial
                .server
                .as_ref()
                .and_then(|s| s.local_bearer_token.clone())
                .unwrap_or(defaults.server.local_bearer_token),
        },
        compat: crate::models::CompatConfig {
            strict_mode: partial
                .compat
                .as_ref()
                .and_then(|c| c.strict_mode)
                .unwrap_or(defaults.compat.strict_mode),
        },
        logging: crate::models::LoggingConfig {
            level: partial
                .logging
                .as_ref()
                .and_then(|l| l.level.clone())
                .unwrap_or(defaults.logging.level),
            capture_body: partial
                .logging
                .as_ref()
                .and_then(|l| l.capture_body)
                .unwrap_or(defaults.logging.capture_body),
            redact_rules: partial
                .logging
                .as_ref()
                .and_then(|l| l.redact_rules.clone())
                .filter(|arr| arr.iter().all(|v| !v.trim().is_empty()))
                .unwrap_or(defaults.logging.redact_rules),
        },
        ui: crate::models::UiConfig {
            theme: partial
                .ui
                .as_ref()
                .and_then(|u| u.theme.clone())
                .filter(|v| v == "light" || v == "dark")
                .unwrap_or(defaults.ui.theme),
            locale: normalized_locale,
            locale_mode: normalized_locale_mode,
            launch_on_startup: partial
                .ui
                .as_ref()
                .and_then(|u| u.launch_on_startup)
                .unwrap_or(defaults.ui.launch_on_startup),
            close_to_tray: partial
                .ui
                .as_ref()
                .and_then(|u| u.close_to_tray)
                .unwrap_or(defaults.ui.close_to_tray),
        },
        remote_git: crate::models::RemoteGitConfig {
            enabled: remote_enabled,
            repo_url: remote_repo_url,
            token: remote_token,
            branch: remote_branch,
        },
        groups,
    })
}
