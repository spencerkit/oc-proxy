//! Module Overview
//! Service-layer operations for external client integrations.
//! Handles target persistence and one-click write for Claude/Codex/OpenCode configs.

use crate::app_state::SharedState;
use crate::models::{
    IntegrationClientKind, IntegrationTarget, IntegrationWriteItem, IntegrationWriteResult,
    ProxyConfig,
};
use crate::services::{AppError, AppResult};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use std::net::{Ipv4Addr, UdpSocket};
use std::path::{Path, PathBuf};
use toml_edit::{value, DocumentMut, Item, Table};
use url::Url;

/// Performs list targets.
pub fn list_targets(state: &SharedState) -> Vec<IntegrationTarget> {
    state.integration_store.list()
}

/// Adds target for this module's workflow.
pub fn add_target(
    state: &SharedState,
    kind: IntegrationClientKind,
    config_dir: String,
) -> AppResult<IntegrationTarget> {
    state
        .integration_store
        .add_target(kind, config_dir)
        .map_err(AppError::validation)
}

/// Updates target for this module's workflow.
pub fn update_target(
    state: &SharedState,
    target_id: &str,
    config_dir: String,
) -> AppResult<IntegrationTarget> {
    state
        .integration_store
        .update_target(target_id, config_dir)
        .map_err(AppError::validation)
}

/// Removes target for this module's workflow.
pub fn remove_target(state: &SharedState, target_id: &str) -> AppResult<bool> {
    state
        .integration_store
        .remove_target(target_id)
        .map_err(AppError::validation)
}

/// Writes selected targets with current group entry URL.
pub fn write_group_entry(
    state: &SharedState,
    group_id: &str,
    target_ids: Vec<String>,
) -> AppResult<IntegrationWriteResult> {
    let normalized_group_id = group_id.trim();
    if normalized_group_id.is_empty() {
        return Err(AppError::validation("group id is required"));
    }
    if target_ids.is_empty() {
        return Err(AppError::validation("at least one target is required"));
    }

    let config = state.config_store.get();
    if !config
        .groups
        .iter()
        .any(|group| group.id == normalized_group_id)
    {
        return Err(AppError::not_found(format!(
            "group not found: {normalized_group_id}"
        )));
    }

    let entry_url = build_group_entry_url(state, &config, normalized_group_id);
    let targets = state.integration_store.list();
    let target_map: HashMap<String, IntegrationTarget> = targets
        .into_iter()
        .map(|target| (target.id.clone(), target))
        .collect();

    let mut items = Vec::new();
    let mut seen_ids = HashSet::new();
    let mut succeeded = 0usize;
    let mut failed = 0usize;

    for raw_target_id in target_ids {
        let target_id = raw_target_id.trim().to_string();
        if target_id.is_empty() || !seen_ids.insert(target_id.clone()) {
            continue;
        }

        let Some(target) = target_map.get(&target_id) else {
            failed += 1;
            items.push(IntegrationWriteItem {
                target_id: target_id.clone(),
                kind: None,
                config_dir: String::new(),
                file_path: None,
                ok: false,
                message: Some("integration target not found".to_string()),
            });
            continue;
        };

        match write_target_entry(target, &entry_url) {
            Ok(file_path) => {
                succeeded += 1;
                items.push(IntegrationWriteItem {
                    target_id: target.id.clone(),
                    kind: Some(target.kind.clone()),
                    config_dir: target.config_dir.clone(),
                    file_path: Some(file_path.to_string_lossy().to_string()),
                    ok: true,
                    message: None,
                });
            }
            Err(err) => {
                failed += 1;
                items.push(IntegrationWriteItem {
                    target_id: target.id.clone(),
                    kind: Some(target.kind.clone()),
                    config_dir: target.config_dir.clone(),
                    file_path: None,
                    ok: false,
                    message: Some(err.to_string()),
                });
            }
        }
    }

    if succeeded == 0 && failed == 0 {
        return Err(AppError::validation("no valid targets selected"));
    }

    Ok(IntegrationWriteResult {
        ok: failed == 0,
        group_id: normalized_group_id.to_string(),
        entry_url,
        succeeded,
        failed,
        items,
    })
}

/// Builds group entry URL for external client configs.
fn build_group_entry_url(state: &SharedState, config: &ProxyConfig, group_id: &str) -> String {
    let port = config.server.port;

    let status = state.runtime.get_status();
    if let Some(base_url) = choose_ip_base_url_from_status(status.lan_address.as_deref(), port) {
        return format!("{base_url}/oc/{group_id}");
    }
    if let Some(base_url) = choose_ip_base_url_from_status(status.address.as_deref(), port) {
        return format!("{base_url}/oc/{group_id}");
    }

    if let Some(host) = choose_ip_host_from_config(&config.server.host) {
        return format!("http://{host}:{port}/oc/{group_id}");
    }

    if let Some(ip) = detect_local_ipv4() {
        return format!("http://{ip}:{port}/oc/{group_id}");
    }

    format!("http://127.0.0.1:{port}/oc/{group_id}")
}

/// Chooses base URL from runtime status address and enforces IP host.
fn choose_ip_base_url_from_status(raw: Option<&str>, fallback_port: u16) -> Option<String> {
    let raw = raw?.trim();
    if raw.is_empty() {
        return None;
    }

    let with_scheme = if raw.starts_with("http://") || raw.starts_with("https://") {
        raw.to_string()
    } else {
        format!("http://{raw}")
    };
    let mut parsed = Url::parse(&with_scheme).ok()?;
    let host_text = parsed.host_str()?.trim().to_ascii_lowercase();
    if is_loopback_or_localhost_host(&host_text) || is_wildcard_host(&host_text) {
        return None;
    }

    if parsed.port().is_none() {
        let _ = parsed.set_port(Some(fallback_port));
    }
    parsed.set_path("");
    parsed.set_query(None);
    parsed.set_fragment(None);
    let normalized = parsed.to_string();
    Some(normalized.trim_end_matches('/').to_string())
}

/// Chooses IP host from configured server host.
fn choose_ip_host_from_config(raw_host: &str) -> Option<String> {
    let host = raw_host
        .trim()
        .trim_matches(&['[', ']'][..])
        .to_ascii_lowercase();
    if host.is_empty() || is_wildcard_host(&host) || is_loopback_or_localhost_host(&host) {
        return None;
    }
    Some(host)
}

/// Detects local IPv4 via UDP route probing.
fn detect_local_ipv4() -> Option<Ipv4Addr> {
    let socket = UdpSocket::bind(("0.0.0.0", 0)).ok()?;
    if socket.connect(("8.8.8.8", 80)).is_err() {
        return None;
    }
    match socket.local_addr().ok()?.ip() {
        std::net::IpAddr::V4(v4) if !v4.is_loopback() => Some(v4),
        _ => None,
    }
}

/// Returns whether host is wildcard bind address.
fn is_wildcard_host(host: &str) -> bool {
    matches!(host, "0.0.0.0" | "::" | "::0")
}

/// Returns whether host is loopback or localhost.
fn is_loopback_or_localhost_host(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

/// Writes one target entry URL by kind.
fn write_target_entry(target: &IntegrationTarget, entry_url: &str) -> AppResult<PathBuf> {
    let config_dir = PathBuf::from(target.config_dir.trim());
    if !config_dir.exists() {
        return Err(AppError::external(format!(
            "config directory does not exist: {}",
            config_dir.display()
        )));
    }
    if !config_dir.is_dir() {
        return Err(AppError::external(format!(
            "config directory is not a folder: {}",
            config_dir.display()
        )));
    }

    match target.kind {
        IntegrationClientKind::Claude => write_claude_settings(&config_dir, entry_url),
        IntegrationClientKind::Codex => write_codex_config(&config_dir, entry_url),
        IntegrationClientKind::Opencode => write_opencode_config(&config_dir, entry_url),
    }
}

/// Writes Claude settings.json env.ANTHROPIC_BASE_URL.
fn write_claude_settings(config_dir: &Path, entry_url: &str) -> AppResult<PathBuf> {
    let file_path = config_dir.join("settings.json");
    let mut root = read_json_like_object(&file_path)?;
    let env = ensure_child_object(&mut root, "env");
    env.insert(
        "ANTHROPIC_BASE_URL".to_string(),
        Value::String(entry_url.to_string()),
    );
    write_json_object(&file_path, &root)?;
    Ok(file_path)
}

/// Writes OpenCode provider.aor_shared.options.baseURL.
fn write_opencode_config(config_dir: &Path, entry_url: &str) -> AppResult<PathBuf> {
    let file_path = resolve_opencode_config_path(config_dir);
    let mut root = read_json_like_object(&file_path)?;
    let provider = ensure_child_object(&mut root, "provider");
    let aor_shared = ensure_child_object(provider, "aor_shared");
    let options = ensure_child_object(aor_shared, "options");
    options.insert("baseURL".to_string(), Value::String(entry_url.to_string()));
    write_json_object(&file_path, &root)?;
    Ok(file_path)
}

/// Writes Codex model_providers.aor_shared.base_url.
fn write_codex_config(config_dir: &Path, entry_url: &str) -> AppResult<PathBuf> {
    let file_path = config_dir.join("config.toml");
    let mut doc = read_toml_document(&file_path)?;
    if !doc["model_providers"].is_table() {
        doc["model_providers"] = Item::Table(Table::new());
    }
    if !doc["model_providers"]["aor_shared"].is_table() {
        doc["model_providers"]["aor_shared"] = Item::Table(Table::new());
    }
    doc["model_providers"]["aor_shared"]["base_url"] = value(entry_url);

    if let Some(parent) = file_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            AppError::external(format!(
                "create codex config dir failed ({}): {e}",
                parent.display()
            ))
        })?;
    }
    let mut output = doc.to_string();
    if !output.ends_with('\n') {
        output.push('\n');
    }
    std::fs::write(&file_path, output).map_err(|e| {
        AppError::external(format!(
            "write codex config failed ({}): {e}",
            file_path.display()
        ))
    })?;
    Ok(file_path)
}

/// Resolves OpenCode config file path.
fn resolve_opencode_config_path(config_dir: &Path) -> PathBuf {
    let jsonc = config_dir.join("opencode.jsonc");
    if jsonc.exists() {
        return jsonc;
    }
    let json = config_dir.join("opencode.json");
    if json.exists() {
        return json;
    }
    json
}

/// Reads JSON or JSONC object from file.
fn read_json_like_object(file_path: &Path) -> AppResult<Map<String, Value>> {
    if !file_path.exists() {
        return Ok(Map::new());
    }
    let raw = std::fs::read_to_string(file_path).map_err(|e| {
        AppError::external(format!("read file failed ({}): {e}", file_path.display()))
    })?;
    if raw.trim().is_empty() {
        return Ok(Map::new());
    }

    let parsed = serde_json::from_str::<Value>(&raw)
        .or_else(|_| json5::from_str::<Value>(&raw))
        .map_err(|e| {
            AppError::validation(format!(
                "parse JSON config failed ({}): {e}",
                file_path.display()
            ))
        })?;
    let Value::Object(map) = parsed else {
        return Err(AppError::validation(format!(
            "JSON config root must be object: {}",
            file_path.display()
        )));
    };
    Ok(map)
}

/// Reads TOML document from file.
fn read_toml_document(file_path: &Path) -> AppResult<DocumentMut> {
    if !file_path.exists() {
        return Ok(DocumentMut::new());
    }
    let raw = std::fs::read_to_string(file_path).map_err(|e| {
        AppError::external(format!("read file failed ({}): {e}", file_path.display()))
    })?;
    if raw.trim().is_empty() {
        return Ok(DocumentMut::new());
    }
    raw.parse::<DocumentMut>().map_err(|e| {
        AppError::validation(format!(
            "parse TOML config failed ({}): {e}",
            file_path.display()
        ))
    })
}

/// Ensures child key is a JSON object.
fn ensure_child_object<'a>(
    parent: &'a mut Map<String, Value>,
    key: &str,
) -> &'a mut Map<String, Value> {
    let child = parent
        .entry(key.to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    ensure_object(child)
}

/// Ensures value is JSON object and returns mutable map ref.
fn ensure_object(value: &mut Value) -> &mut Map<String, Value> {
    if !value.is_object() {
        *value = Value::Object(Map::new());
    }
    value
        .as_object_mut()
        .expect("json object must exist after normalization")
}

/// Writes JSON object to file.
fn write_json_object(file_path: &Path, root: &Map<String, Value>) -> AppResult<()> {
    if let Some(parent) = file_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            AppError::external(format!(
                "create config dir failed ({}): {e}",
                parent.display()
            ))
        })?;
    }
    let text = serde_json::to_string_pretty(&Value::Object(root.clone()))
        .map_err(|e| AppError::internal(format!("serialize JSON failed: {e}")))?;
    std::fs::write(file_path, text).map_err(|e| {
        AppError::external(format!("write file failed ({}): {e}", file_path.display()))
    })
}
