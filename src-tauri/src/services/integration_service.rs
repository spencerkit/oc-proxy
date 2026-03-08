//! Module Overview
//! Service-layer operations for external client integrations.
//! Handles target persistence and one-click write for Claude/Codex/OpenCode configs.

use crate::api::dto::{AgentConfig, AgentConfigFile, WriteAgentConfigResult};
use crate::app_state::SharedState;
use crate::models::{
    IntegrationClientKind, IntegrationTarget, IntegrationWriteItem, IntegrationWriteResult,
    ProxyConfig,
};
use crate::services::{AppError, AppResult};
use crate::wsl;
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use std::fs::OpenOptions;
use std::io::Write;
use std::net::{Ipv4Addr, UdpSocket};
use std::path::{Path, PathBuf};
use toml_edit::{value, DocumentMut, Item, Table};
use url::Url;

/// Writes debug log to a file in app data directory.
fn write_debug_log(message: &str) {
    // Try to write to a log file in temp directory
    if let Ok(log_dir) = std::env::var("LOCALAPPDATA") {
        let log_dir = PathBuf::from(log_dir).join("art.shier.aiopenrouter");
        let _ = std::fs::create_dir_all(&log_dir);
        let log_path = log_dir.join("wsl-debug.log");
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&log_path) {
            let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S%.3f");
            let _ = writeln!(file, "[{}] {}", timestamp, message);
        }
    }
    // Also print to stderr
    eprintln!("{}", message);
}

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

fn is_wsl_path(path: &Path) -> bool {
    wsl::is_wsl_path(path)
}

/// Writes content to a file, handling WSL paths by using WSL command.
fn write_file_content(file_path: &Path, content: &str) -> AppResult<()> {
    let file_path_str = file_path.to_string_lossy();
    write_debug_log(&format!(
        "write_file_content called with path: {}",
        file_path_str
    ));

    if is_wsl_path(file_path) {
        if let Some(resolved) = wsl::resolve_path(file_path) {
            write_debug_log(&format!(
                "Writing via WSL distro={}, path={}",
                resolved.distro, resolved.linux_path
            ));
        }
        wsl::write_file(file_path, content).map_err(|e| {
            write_debug_log(&format!("ERROR: WSL write failed: {}", e));
            AppError::external(format!(
                "write WSL file failed ({}): {e}",
                file_path.display()
            ))
        })?;
        write_debug_log("Write successful!");
        return Ok(());
    }

    write_debug_log("Non-WSL path, using std::fs");

    // Normal file write for non-WSL paths
    // Create parent directory if needed
    if let Some(parent) = file_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AppError::external(format!("create directory failed: {}", e)))?;
    }

    std::fs::write(file_path, content)
        .map_err(|e| AppError::external(format!("write file failed: {}", e)))
}

fn normalize_wsl_path(path: &Path) -> Option<PathBuf> {
    wsl::normalize_windows_path(path)
}

/// Writes one target entry URL by kind.
fn write_target_entry(target: &IntegrationTarget, entry_url: &str) -> AppResult<PathBuf> {
    write_debug_log(&format!(
        "write_target_entry called, config_dir: {}",
        target.config_dir
    ));
    let config_dir_raw = PathBuf::from(target.config_dir.trim());

    // Try to normalize WSL path - convert \\?\UNC\wsl.localhost\ to \\wsl$\
    let config_dir = match normalize_wsl_path(&config_dir_raw) {
        Some(normalized) => {
            write_debug_log(&format!("Normalized path to: {}", normalized.display()));
            normalized
        }
        None => {
            write_debug_log("Not a WSL path, using raw");
            config_dir_raw.clone()
        }
    };

    write_debug_log(&format!("Final config_dir: {}", config_dir.display()));

    if !is_wsl_path(&config_dir) {
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
    let file_path = resolve_opencode_config_path(config_dir)?;
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
    let model_providers = ensure_toml_table(doc.as_table_mut(), "model_providers");
    let aor_shared = ensure_toml_table(model_providers, "aor_shared");
    aor_shared["base_url"] = value(entry_url);

    let mut output = doc.to_string();
    if !output.ends_with('\n') {
        output.push('\n');
    }

    write_file_content(&file_path, &output)?;
    Ok(file_path)
}

/// Resolves OpenCode config file path.
fn resolve_opencode_config_path(config_dir: &Path) -> AppResult<PathBuf> {
    let jsonc = config_dir.join("opencode.jsonc");
    if is_wsl_path(config_dir) {
        if wsl::is_file(&jsonc).map_err(|e| {
            AppError::external(format!(
                "check WSL config file failed ({}): {e}",
                jsonc.display()
            ))
        })? {
            return Ok(jsonc);
        }

        let json = config_dir.join("opencode.json");
        if wsl::is_file(&json).map_err(|e| {
            AppError::external(format!(
                "check WSL config file failed ({}): {e}",
                json.display()
            ))
        })? {
            return Ok(json);
        }

        return Ok(json);
    }

    if jsonc.exists() {
        return Ok(jsonc);
    }
    let json = config_dir.join("opencode.json");
    if json.exists() {
        return Ok(json);
    }
    Ok(json)
}

/// Reads JSON or JSONC object from file.
fn read_json_like_object(file_path: &Path) -> AppResult<Map<String, Value>> {
    // For WSL paths, read via wsl command
    if is_wsl_path(file_path) {
        let Some(content) = read_file_via_wsl(file_path)? else {
            return Ok(Map::new());
        };
        if content.trim().is_empty() {
            return Ok(Map::new());
        }
        let parsed = serde_json::from_str::<Value>(&content)
            .or_else(|_| json5::from_str::<Value>(&content))
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
        return Ok(map);
    }

    // Normal read for non-WSL paths
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
    if is_wsl_path(file_path) {
        let Some(content) = read_file_via_wsl(file_path)? else {
            return Ok(DocumentMut::new());
        };
        if content.trim().is_empty() {
            return Ok(DocumentMut::new());
        }
        return content.parse::<DocumentMut>().map_err(|e| {
            AppError::validation(format!(
                "parse TOML config failed ({}): {}",
                file_path.display(),
                e
            ))
        });
    }

    // Normal read for non-WSL paths
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

/// Reads file content via WSL command.
fn read_file_via_wsl(file_path: &Path) -> AppResult<Option<String>> {
    if let Some(resolved) = wsl::resolve_path(file_path) {
        write_debug_log(&format!(
            "Reading via WSL distro={}, path={}",
            resolved.distro, resolved.linux_path
        ));
    }

    let content = wsl::read_file(file_path).map_err(|e| {
        write_debug_log(&format!("ERROR: WSL read failed: {}", e));
        AppError::external(format!(
            "read WSL file failed ({}): {e}",
            file_path.display()
        ))
    })?;
    if let Some(ref content) = content {
        write_debug_log(&format!("Read {} bytes", content.len()));
    } else {
        write_debug_log("WSL file not found");
    }
    Ok(content)
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
    let text = serde_json::to_string_pretty(&Value::Object(root.clone()))
        .map_err(|e| AppError::internal(format!("serialize JSON failed: {e}")))?;
    write_file_content(file_path, &text)
}

fn ensure_toml_table<'a>(parent: &'a mut Table, key: &str) -> &'a mut Table {
    let item = parent.entry(key).or_insert(Item::Table(Table::new()));
    if !item.is_table() {
        *item = Item::Table(Table::new());
    }
    item.as_table_mut()
        .expect("table must exist after normalization")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_config_shape_can_be_created_from_empty_document() {
        let mut doc = DocumentMut::new();
        let model_providers = ensure_toml_table(doc.as_table_mut(), "model_providers");
        let aor_shared = ensure_toml_table(model_providers, "aor_shared");
        aor_shared["base_url"] = value("http://127.0.0.1:11434");

        let output = doc.to_string();
        assert!(output.contains("model_providers"));
        assert!(output.contains("aor_shared"));
        assert!(output.contains("base_url"));
    }
}

/// Reads Agent configuration file content.
pub fn read_agent_config(state: &SharedState, target_id: &str) -> AppResult<AgentConfigFile> {
    let targets = state.integration_store.list();
    let target = targets
        .into_iter()
        .find(|t| t.id == target_id)
        .ok_or_else(|| AppError::not_found(format!("target not found: {}", target_id)))?;

    let config_dir = PathBuf::from(&target.config_dir);

    // Normalize config_dir for WSL paths
    let config_dir = match normalize_wsl_path(&config_dir) {
        Some(normalized) => normalized,
        None => config_dir,
    };

    let (file_path, root) = match target.kind {
        IntegrationClientKind::Claude => {
            let path = config_dir.join("settings.json");
            let root = read_json_like_object(&path).unwrap_or_else(|_| Map::new());
            (path, root)
        }
        IntegrationClientKind::Codex => {
            let path = config_dir.join("config.toml");
            let doc = read_toml_document(&path).unwrap_or_else(|_| DocumentMut::new());
            // Convert TOML document to JSON-like Map for parsing
            let root = toml_to_map(&doc);
            (path, root)
        }
        IntegrationClientKind::Opencode => {
            let path = resolve_opencode_config_path(&config_dir)?;
            let root = read_json_like_object(&path).unwrap_or_else(|_| Map::new());
            (path, root)
        }
    };

    // Serialize root back to string for content field
    let content =
        serde_json::to_string(&Value::Object(root.clone())).unwrap_or_else(|_| "{}".to_string());

    let parsed_config = parse_agent_config(&target.kind, &root).ok();

    Ok(AgentConfigFile {
        target_id: target.id,
        kind: target.kind,
        config_dir: target.config_dir,
        file_path: file_path.to_string_lossy().to_string(),
        content,
        parsed_config,
    })
}

/// Converts TOML DocumentMut to a JSON-like Map.
fn toml_to_map(doc: &DocumentMut) -> Map<String, Value> {
    let mut map = Map::new();
    for (key, item) in doc.as_table() {
        map.insert(key.to_string(), toml_item_to_value(item));
    }
    map
}

fn toml_item_to_value(item: &toml_edit::Item) -> Value {
    match item {
        toml_edit::Item::None => Value::Null,
        toml_edit::Item::Value(v) => toml_value_to_value(v),
        toml_edit::Item::Table(t) => {
            let mut map = Map::new();
            for (k, v) in t {
                map.insert(k.to_string(), toml_item_to_value(v));
            }
            Value::Object(map)
        }
        toml_edit::Item::ArrayOfTables(arr) => Value::Array(
            arr.iter()
                .map(|t| {
                    let mut map = Map::new();
                    for (k, v) in t {
                        map.insert(k.to_string(), toml_item_to_value(v));
                    }
                    Value::Object(map)
                })
                .collect(),
        ),
    }
}

fn toml_value_to_value(v: &toml_edit::Value) -> Value {
    match v {
        toml_edit::Value::String(s) => Value::String(s.value().to_string()),
        toml_edit::Value::Integer(i) => Value::Number((*i.value()).into()),
        toml_edit::Value::Float(f) => serde_json::Number::from_f64(*f.value())
            .map(Value::Number)
            .unwrap_or(Value::Null),
        toml_edit::Value::Boolean(b) => Value::Bool(*b.value()),
        toml_edit::Value::Datetime(dt) => Value::String(dt.value().to_string()),
        toml_edit::Value::Array(arr) => Value::Array(arr.iter().map(toml_value_to_value).collect()),
        toml_edit::Value::InlineTable(t) => {
            let mut map = Map::new();
            for (k, v) in t {
                map.insert(k.to_string(), toml_value_to_value(v));
            }
            Value::Object(map)
        }
    }
}

/// Parses configuration into AgentConfig.
fn parse_agent_config(
    kind: &IntegrationClientKind,
    root: &Map<String, Value>,
) -> AppResult<AgentConfig> {
    match kind {
        IntegrationClientKind::Claude => parse_claude_config(root),
        IntegrationClientKind::Opencode => parse_opencode_config(root),
        IntegrationClientKind::Codex => parse_codex_config(root),
    }
}

/// Parses Claude config from JSON-like Map.
fn parse_claude_config(root: &Map<String, Value>) -> AppResult<AgentConfig> {
    // Extract env field
    let env = root.get("env").and_then(|v| v.as_object());
    let url = env
        .and_then(|e| e.get("ANTHROPIC_BASE_URL"))
        .and_then(|v| v.as_str())
        .map(String::from);
    let api_token = env
        .and_then(|e| e.get("ANTHROPIC_AUTH_TOKEN"))
        .and_then(|v| v.as_str())
        .map(String::from);
    let model = env
        .and_then(|e| e.get("ANTHROPIC_MODEL"))
        .and_then(|v| v.as_str())
        .map(String::from);
    let timeout = env
        .and_then(|e| e.get("API_TIMEOUT_MS"))
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<u64>().ok());

    // Extract behavior options
    let always_thinking_enabled = root.get("alwaysThinkingEnabled").and_then(|v| v.as_bool());
    let include_coauthored_by = root.get("includeCoAuthoredBy").and_then(|v| v.as_bool());
    let skip_dangerous_mode_permission_prompt = root
        .get("skipDangerousModePermissionPrompt")
        .and_then(|v| v.as_bool());

    Ok(AgentConfig {
        url,
        api_token,
        model,
        timeout,
        always_thinking_enabled,
        include_coauthored_by,
        skip_dangerous_mode_permission_prompt,
    })
}

/// Parses OpenCode config from JSON-like Map.
fn parse_opencode_config(root: &Map<String, Value>) -> AppResult<AgentConfig> {
    // Extract provider.aor_shared config
    let provider = root.get("provider").and_then(|v| v.as_object());
    let aor_shared = provider
        .and_then(|p| p.get("aor_shared"))
        .and_then(|v| v.as_object());
    let url = aor_shared
        .and_then(|a| a.get("options"))
        .and_then(|o| o.get("baseURL"))
        .and_then(|v| v.as_str())
        .map(String::from);
    let timeout = aor_shared
        .and_then(|a| a.get("options"))
        .and_then(|o| o.get("timeout"))
        .and_then(|v| v.as_u64());
    let model = root.get("model").and_then(|v| v.as_str()).map(String::from);

    Ok(AgentConfig {
        url,
        api_token: None,
        model,
        timeout,
        always_thinking_enabled: None,
        include_coauthored_by: None,
        skip_dangerous_mode_permission_prompt: None,
    })
}

/// Parses Codex config from JSON-like Map.
fn parse_codex_config(root: &Map<String, Value>) -> AppResult<AgentConfig> {
    let model_providers = root.get("model_providers").and_then(|v| v.as_object());
    let aor_shared = model_providers
        .and_then(|mp| mp.get("aor_shared"))
        .and_then(|v| v.as_object());

    let url = aor_shared
        .and_then(|a| a.get("base_url"))
        .and_then(|v| v.as_str())
        .map(String::from);
    let api_token = aor_shared
        .and_then(|a| a.get("api_key"))
        .and_then(|v| v.as_str())
        .map(String::from);
    let model = root.get("model").and_then(|v| v.as_str()).map(String::from);

    Ok(AgentConfig {
        url,
        api_token,
        model,
        timeout: None,
        always_thinking_enabled: None,
        include_coauthored_by: None,
        skip_dangerous_mode_permission_prompt: None,
    })
}

/// Writes Agent configuration to file.
pub fn write_agent_config(
    state: &SharedState,
    target_id: &str,
    config: AgentConfig,
) -> AppResult<WriteAgentConfigResult> {
    use std::path::PathBuf;

    let targets = state.integration_store.list();
    let target = targets
        .into_iter()
        .find(|t| t.id == target_id)
        .ok_or_else(|| AppError::not_found(format!("target not found: {}", target_id)))?;

    let config_dir = PathBuf::from(&target.config_dir);

    // Normalize WSL path
    let config_dir = match normalize_wsl_path(&config_dir) {
        Some(normalized) => normalized,
        None => config_dir,
    };

    let file_path = match target.kind {
        IntegrationClientKind::Claude => write_claude_full_config(&config_dir, &config)?,
        IntegrationClientKind::Opencode => write_opencode_full_config(&config_dir, &config)?,
        IntegrationClientKind::Codex => write_codex_full_config(&config_dir, &config)?,
    };

    // Update stored config
    let _ =
        state
            .integration_store
            .update_target_config(target_id, target.config_dir, Some(config));

    Ok(WriteAgentConfigResult {
        ok: true,
        target_id: target_id.to_string(),
        file_path: file_path.to_string_lossy().to_string(),
        message: None,
    })
}

fn write_claude_full_config(config_dir: &Path, config: &AgentConfig) -> AppResult<PathBuf> {
    let file_path = config_dir.join("settings.json");
    let mut root = read_json_like_object(&file_path)?;

    // Write env
    let env = ensure_child_object(&mut root, "env");
    if let Some(url) = &config.url {
        env.insert("ANTHROPIC_BASE_URL".to_string(), Value::String(url.clone()));
    }
    if let Some(token) = &config.api_token {
        env.insert(
            "ANTHROPIC_AUTH_TOKEN".to_string(),
            Value::String(token.clone()),
        );
    }
    if let Some(model) = &config.model {
        env.insert("ANTHROPIC_MODEL".to_string(), Value::String(model.clone()));
    }
    if let Some(timeout) = config.timeout {
        env.insert(
            "API_TIMEOUT_MS".to_string(),
            Value::String(timeout.to_string()),
        );
    }

    // Write behavior options
    if let Some(enabled) = config.always_thinking_enabled {
        root.insert("alwaysThinkingEnabled".to_string(), Value::Bool(enabled));
    }
    if let Some(enabled) = config.include_coauthored_by {
        root.insert("includeCoAuthoredBy".to_string(), Value::Bool(enabled));
    }
    if let Some(enabled) = config.skip_dangerous_mode_permission_prompt {
        root.insert(
            "skipDangerousModePermissionPrompt".to_string(),
            Value::Bool(enabled),
        );
    }

    write_json_object(&file_path, &root)?;
    Ok(file_path)
}

fn write_opencode_full_config(config_dir: &Path, config: &AgentConfig) -> AppResult<PathBuf> {
    let file_path = resolve_opencode_config_path(config_dir)?;
    let mut root = read_json_like_object(&file_path)?;

    // Ensure provider structure
    let provider = ensure_child_object(&mut root, "provider");
    let aor_shared = ensure_child_object(provider, "aor_shared");
    let options = ensure_child_object(aor_shared, "options");

    if let Some(url) = &config.url {
        options.insert("baseURL".to_string(), Value::String(url.clone()));
    }
    if let Some(timeout) = config.timeout {
        options.insert("timeout".to_string(), Value::Number(timeout.into()));
    }

    // Write model at root level
    if let Some(model) = &config.model {
        root.insert("model".to_string(), Value::String(model.clone()));
    }

    write_json_object(&file_path, &root)?;
    Ok(file_path)
}

fn write_codex_full_config(config_dir: &Path, config: &AgentConfig) -> AppResult<PathBuf> {
    let file_path = config_dir.join("config.toml");
    let mut doc = read_toml_document(&file_path)?;

    if !doc["model_providers"].is_table() {
        doc["model_providers"] = Item::Table(Table::new());
    }
    if !doc["model_providers"]["aor_shared"].is_table() {
        doc["model_providers"]["aor_shared"] = Item::Table(Table::new());
    }

    if let Some(url) = &config.url {
        doc["model_providers"]["aor_shared"]["base_url"] = value(url);
    }
    if let Some(token) = &config.api_token {
        doc["model_providers"]["aor_shared"]["api_key"] = value(token);
    }
    if let Some(model) = &config.model {
        doc["model"] = value(model);
    }

    // Write to file
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
