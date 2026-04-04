//! Module Overview
//! Service-layer operations for external client integrations.
//! Handles target persistence and one-click write for Claude/Codex/OpenCode configs.

use crate::api::dto::{
    AgentConfig, AgentConfigFile, AgentSourceFile, OpenClawEditorConfigDto, WriteAgentConfigResult,
};
use crate::app_state::SharedState;
use crate::models::{
    IntegrationClientKind, IntegrationTarget, IntegrationWriteItem, IntegrationWriteResult,
    ProxyConfig,
};
use crate::services::{AppError, AppResult};
use crate::user_home::user_home_dir;
use crate::wsl;
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use std::fs::OpenOptions;
use std::io::Write;
use std::net::{Ipv4Addr, UdpSocket};
use std::path::{Path, PathBuf};
use toml_edit::{value, DocumentMut, Item, Table};
use url::Url;

const SOURCE_PRIMARY: &str = "primary";
const SOURCE_AUTH: &str = "auth";
const SOURCE_MODELS: &str = "models";
const SOURCE_AUTH_PROFILES: &str = "auth-profiles";
const HEADLESS_DEFAULT_PREFIX: &str = "default";
const DEFAULT_OPENCLAW_AGENT_ID: &str = "default";
const DEFAULT_OPENCLAW_PROVIDER_ID: &str = "aor_shared";
const DEFAULT_OPENCLAW_API_FORMAT: &str = "openai-responses";

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

fn root_hidden_config_fallback_with_root_home(
    home: &Path,
    dir_name: &str,
    root_home: &Path,
    root_base: &Path,
) -> Option<PathBuf> {
    if home != root_home {
        return None;
    }

    let root_level_dir = root_base.join(dir_name);
    if root_level_dir.exists() {
        Some(root_level_dir)
    } else {
        None
    }
}

fn preferred_hidden_config_dir_with_root_paths(
    home: &Path,
    dir_name: &str,
    root_home: &Path,
    root_base: &Path,
) -> PathBuf {
    let home_candidate = home.join(dir_name);
    if home_candidate.exists() {
        return home_candidate;
    }

    root_hidden_config_fallback_with_root_home(home, dir_name, root_home, root_base)
        .unwrap_or(home_candidate)
}

pub(crate) fn preferred_client_config_dir_with_root_paths(
    kind: &IntegrationClientKind,
    home: &Path,
    root_home: &Path,
    root_base: &Path,
) -> PathBuf {
    match kind {
        IntegrationClientKind::Claude => {
            preferred_hidden_config_dir_with_root_paths(home, ".claude", root_home, root_base)
        }
        IntegrationClientKind::Codex => {
            preferred_hidden_config_dir_with_root_paths(home, ".codex", root_home, root_base)
        }
        IntegrationClientKind::Openclaw => {
            preferred_hidden_config_dir_with_root_paths(home, ".openclaw", root_home, root_base)
        }
        IntegrationClientKind::Opencode => preferred_opencode_config_dir(home),
    }
}

fn integration_kind_slug(kind: &IntegrationClientKind) -> &'static str {
    match kind {
        IntegrationClientKind::Claude => "claude",
        IntegrationClientKind::Codex => "codex",
        IntegrationClientKind::Openclaw => "openclaw",
        IntegrationClientKind::Opencode => "opencode",
    }
}

fn headless_default_id(kind: &IntegrationClientKind) -> String {
    format!("{HEADLESS_DEFAULT_PREFIX}:{}", integration_kind_slug(kind))
}

fn build_default_target(kind: IntegrationClientKind, config_dir: PathBuf) -> IntegrationTarget {
    let timestamp = chrono::Utc::now().to_rfc3339();
    IntegrationTarget {
        id: headless_default_id(&kind),
        kind,
        config_dir: config_dir.to_string_lossy().to_string(),
        config: None,
        group_id: None,
        created_at: timestamp.clone(),
        updated_at: timestamp,
    }
}

fn opencode_dir_has_config(config_dir: &Path) -> bool {
    config_dir.join("opencode.jsonc").exists() || config_dir.join("opencode.json").exists()
}

pub(crate) fn preferred_opencode_config_dir(home: &Path) -> PathBuf {
    let config_dir = home.join(".config").join("opencode");
    if opencode_dir_has_config(&config_dir) {
        return config_dir;
    }

    let data_dir = home.join(".local").join("share").join("opencode");
    if opencode_dir_has_config(&data_dir) {
        return data_dir;
    }

    config_dir
}

fn list_default_targets_with_root_paths(
    home: &Path,
    root_home: &Path,
    root_base: &Path,
) -> Vec<IntegrationTarget> {
    [
        IntegrationClientKind::Claude,
        IntegrationClientKind::Codex,
        IntegrationClientKind::Openclaw,
        IntegrationClientKind::Opencode,
    ]
    .into_iter()
    .map(|kind| {
        let config_dir =
            preferred_client_config_dir_with_root_paths(&kind, home, root_home, root_base);
        build_default_target(kind, config_dir)
    })
    .collect()
}

fn list_default_targets_with_home(home: &Path) -> Vec<IntegrationTarget> {
    list_default_targets_with_root_paths(home, Path::new("/root"), Path::new("/"))
}

fn target_kind_dir_key(kind: &IntegrationClientKind, config_dir: &str) -> String {
    format!("{}|{}", integration_kind_slug(kind), config_dir.trim())
}

fn merge_default_and_saved_targets(
    default_targets: Vec<IntegrationTarget>,
    saved_targets: Vec<IntegrationTarget>,
) -> Vec<IntegrationTarget> {
    let saved_ids = saved_targets
        .iter()
        .map(|target| target.id.as_str())
        .collect::<HashSet<_>>();
    let saved_kind_dir_keys = saved_targets
        .iter()
        .map(|target| target_kind_dir_key(&target.kind, &target.config_dir))
        .collect::<HashSet<_>>();

    let mut merged = default_targets
        .into_iter()
        .filter(|target| {
            !saved_ids.contains(target.id.as_str())
                && !saved_kind_dir_keys
                    .contains(&target_kind_dir_key(&target.kind, &target.config_dir))
        })
        .collect::<Vec<_>>();
    merged.extend(saved_targets);
    merged
}

fn list_targets_with_saved_and_root_paths(
    saved_targets: Vec<IntegrationTarget>,
    home: &Path,
    root_home: &Path,
    root_base: &Path,
) -> Vec<IntegrationTarget> {
    merge_default_and_saved_targets(
        list_default_targets_with_root_paths(home, root_home, root_base),
        saved_targets,
    )
}

fn list_targets_with_saved(
    saved_targets: Vec<IntegrationTarget>,
    home: &Path,
) -> Vec<IntegrationTarget> {
    list_targets_with_saved_and_root_paths(saved_targets, home, Path::new("/root"), Path::new("/"))
}

pub fn list_default_targets() -> Vec<IntegrationTarget> {
    let Some(home) = user_home_dir() else {
        return Vec::new();
    };

    list_default_targets_with_home(&home)
}

/// Performs list targets.
pub fn list_targets(state: &SharedState) -> Vec<IntegrationTarget> {
    let saved_targets = state.integration_store.list();
    let Some(home) = user_home_dir() else {
        return saved_targets;
    };

    list_targets_with_saved(saved_targets, &home)
}

fn resolve_target_by_id(
    targets: &[IntegrationTarget],
    target_id: &str,
) -> AppResult<IntegrationTarget> {
    let normalized_id = target_id.trim();
    if normalized_id.is_empty() {
        return Err(AppError::validation("target id is required"));
    }
    targets
        .iter()
        .find(|target| target.id == normalized_id)
        .cloned()
        .ok_or_else(|| AppError::not_found(format!("target not found: {}", normalized_id)))
}

/// Adds target for this module's workflow.
pub fn add_target(
    state: &SharedState,
    kind: IntegrationClientKind,
    config_dir: String,
) -> AppResult<IntegrationTarget> {
    let normalized_config_dir = config_dir.trim();
    if list_targets(state)
        .iter()
        .any(|target| target.kind == kind && target.config_dir == normalized_config_dir)
    {
        return Err(AppError::validation("same config directory already exists"));
    }

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
    let target = resolve_target_by_id(&list_targets(state), target_id)?;

    state
        .integration_store
        .put_target(target_id, target.kind, config_dir, target.config)
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
    let targets = list_targets(state);
    write_group_entry_with_targets(state, group_id, targets, target_ids)
}

/// Writes selected targets with current group entry URL using explicit target list.
pub fn write_group_entry_with_targets(
    state: &SharedState,
    group_id: &str,
    targets: Vec<IntegrationTarget>,
    target_ids: Vec<String>,
) -> AppResult<IntegrationWriteResult> {
    write_group_entry_with_targets_and_base_url(state, group_id, targets, target_ids, None)
}

/// Writes selected targets with current group entry URL using explicit target list and optional
/// request-derived base URL override.
pub fn write_group_entry_with_targets_and_base_url(
    state: &SharedState,
    group_id: &str,
    targets: Vec<IntegrationTarget>,
    target_ids: Vec<String>,
    base_url_override: Option<&str>,
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

    let entry_url = build_group_entry_url(state, &config, normalized_group_id, base_url_override);
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

        let config_dir = PathBuf::from(target.config_dir.trim());
        if target.id.starts_with(HEADLESS_DEFAULT_PREFIX) && !is_wsl_path(&config_dir) {
            if !config_dir.exists() {
                failed += 1;
                items.push(IntegrationWriteItem {
                    target_id: target.id.clone(),
                    kind: Some(target.kind.clone()),
                    config_dir: target.config_dir.clone(),
                    file_path: None,
                    ok: false,
                    message: Some(
                        "config directory not found. Please confirm the installation path."
                            .to_string(),
                    ),
                });
                continue;
            }
        }

        match write_target_entry(target, &entry_url) {
            Ok(file_path) => {
                let persist_result = state.integration_store.put_target(
                    &target.id,
                    target.kind.clone(),
                    target.config_dir.clone(),
                    target.config.clone(),
                );
                let persist_result = persist_result.and_then(|_| {
                    state
                        .integration_store
                        .set_target_group_id(&target.id, Some(normalized_group_id.to_string()))
                });
                if let Err(err) = persist_result {
                    failed += 1;
                    items.push(IntegrationWriteItem {
                        target_id: target.id.clone(),
                        kind: Some(target.kind.clone()),
                        config_dir: target.config_dir.clone(),
                        file_path: Some(file_path.to_string_lossy().to_string()),
                        ok: false,
                        message: Some(format!(
                            "entry written but target binding was not saved: {err}"
                        )),
                    });
                    continue;
                }
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
fn build_group_entry_url(
    state: &SharedState,
    config: &ProxyConfig,
    group_id: &str,
    base_url_override: Option<&str>,
) -> String {
    let port = config.server.port;
    if let Some(base_url) = base_url_override.and_then(|raw| normalize_base_url_override(raw, port))
    {
        return format!("{base_url}/oc/{group_id}");
    }

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

fn normalize_base_url_override(raw: &str, fallback_port: u16) -> Option<String> {
    let raw = raw.trim();
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
    if host_text.is_empty() || is_wildcard_host(&host_text) {
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
        IntegrationClientKind::Openclaw => {
            write_openclaw_config(&config_dir, &target.config, entry_url)
        }
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

/// Writes Codex model_providers.<model_provider>.base_url.
fn write_codex_config(config_dir: &Path, entry_url: &str) -> AppResult<PathBuf> {
    let file_path = config_dir.join("config.toml");
    let mut doc = read_toml_document(&file_path)?;
    let provider_name = resolve_codex_provider_name_from_doc(&doc)?;
    let model_providers = ensure_toml_table(doc.as_table_mut(), "model_providers");
    let provider_table = ensure_toml_table(model_providers, &provider_name);
    provider_table["base_url"] = value(entry_url);

    let mut output = doc.to_string();
    if !output.ends_with('\n') {
        output.push('\n');
    }

    write_file_content(&file_path, &output)?;
    Ok(file_path)
}

fn normalize_non_empty(raw: Option<&str>, fallback: &str) -> String {
    raw.map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback)
        .to_string()
}

fn build_openclaw_entry_url(entry_url: &str) -> String {
    let trimmed = entry_url.trim().trim_end_matches('/');
    if trimmed.ends_with("/v1") {
        trimmed.to_string()
    } else {
        format!("{trimmed}/v1")
    }
}

fn resolve_openclaw_agent_id_from_config(config: Option<&AgentConfig>) -> String {
    normalize_non_empty(
        config.and_then(|item| item.agent_id.as_deref()),
        DEFAULT_OPENCLAW_AGENT_ID,
    )
}

fn resolve_openclaw_agent_id(target: &IntegrationTarget) -> String {
    resolve_openclaw_agent_id_from_config(target.config.as_ref())
}

fn openclaw_agent_root(config_dir: &Path, agent_id: &str) -> PathBuf {
    config_dir.join("agents").join(agent_id)
}

fn preferred_openclaw_agent_dir(config_dir: &Path, agent_id: &str) -> PathBuf {
    openclaw_agent_root(config_dir, agent_id).join("agent")
}

fn file_exists(file_path: &Path) -> AppResult<bool> {
    Ok(read_file_content(file_path)?.is_some())
}

fn resolve_openclaw_auth_profiles_file_path(
    config_dir: &Path,
    agent_id: &str,
) -> AppResult<PathBuf> {
    let preferred = preferred_openclaw_agent_dir(config_dir, agent_id).join("auth-profiles.json");
    if file_exists(&preferred)? {
        return Ok(preferred);
    }

    let legacy = openclaw_agent_root(config_dir, agent_id).join("auth-profiles.json");
    if file_exists(&legacy)? {
        return Ok(legacy);
    }

    Ok(preferred)
}

fn resolve_openclaw_models_file_path(config_dir: &Path, agent_id: &str) -> AppResult<PathBuf> {
    let preferred = preferred_openclaw_agent_dir(config_dir, agent_id).join("models.json");
    if file_exists(&preferred)? {
        return Ok(preferred);
    }

    let legacy = openclaw_agent_root(config_dir, agent_id).join("models.json");
    if file_exists(&legacy)? {
        return Ok(legacy);
    }

    Ok(preferred)
}

fn get_openclaw_providers_object(root: &Map<String, Value>) -> Option<&Map<String, Value>> {
    root.get("models")
        .and_then(|value| value.as_object())
        .and_then(|models| models.get("providers"))
        .and_then(|value| value.as_object())
        .or_else(|| root.get("providers").and_then(|value| value.as_object()))
}

fn get_openclaw_provider_object<'a>(
    root: &'a Map<String, Value>,
    provider_id: &str,
) -> Option<&'a Map<String, Value>> {
    get_openclaw_providers_object(root)
        .and_then(|providers| providers.get(provider_id))
        .and_then(|value| value.as_object())
}

fn resolve_openclaw_provider_name(
    primary_root: &Map<String, Value>,
    models_root: Option<&Map<String, Value>>,
    config: Option<&AgentConfig>,
) -> String {
    let explicit = config
        .and_then(|item| item.provider_id.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(String::from);
    if let Some(provider_id) = explicit {
        return provider_id;
    }

    for root in [Some(primary_root), models_root].into_iter().flatten() {
        if let Some(providers) = get_openclaw_providers_object(root) {
            if providers.contains_key(DEFAULT_OPENCLAW_PROVIDER_ID) {
                return DEFAULT_OPENCLAW_PROVIDER_ID.to_string();
            }
            if let Some(provider_id) = providers.keys().find(|value| !value.trim().is_empty()) {
                return provider_id.to_string();
            }
        }
    }

    DEFAULT_OPENCLAW_PROVIDER_ID.to_string()
}

fn parse_string_array(value: Option<&Value>) -> Option<Vec<String>> {
    let list = match value {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| {
                item.as_str()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(String::from)
                    .or_else(|| {
                        item.as_object()
                            .and_then(|entry| entry.get("id"))
                            .and_then(|id| id.as_str())
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                            .map(String::from)
                    })
            })
            .collect::<Vec<_>>(),
        Some(Value::String(raw)) => raw
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(String::from)
            .collect::<Vec<_>>(),
        _ => Vec::new(),
    };

    if list.is_empty() {
        None
    } else {
        Some(list)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OpenClawEditorConfig {
    agent_id: String,
    provider_id: String,
    primary_model: Option<String>,
    fallback_models: Vec<String>,
    api_format: Option<String>,
    base_url: Option<String>,
    api_key: Option<String>,
}

fn openclaw_editor_from_agent_config(config: &AgentConfig) -> OpenClawEditorConfig {
    OpenClawEditorConfig {
        agent_id: resolve_openclaw_agent_id_from_config(Some(config)),
        provider_id: config
            .provider_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or(DEFAULT_OPENCLAW_PROVIDER_ID)
            .to_string(),
        primary_model: config.model.clone(),
        fallback_models: config.fallback_models.clone().unwrap_or_default(),
        api_format: config.api_format.clone(),
        base_url: config.url.clone(),
        api_key: config.api_token.clone(),
    }
}

fn openclaw_editor_to_agent_config(editor: OpenClawEditorConfig) -> AgentConfig {
    AgentConfig {
        agent_id: Some(editor.agent_id),
        provider_id: Some(editor.provider_id),
        url: editor.base_url,
        api_token: editor.api_key,
        api_format: editor.api_format,
        model: editor.primary_model,
        fallback_models: if editor.fallback_models.is_empty() {
            None
        } else {
            Some(editor.fallback_models)
        },
        timeout: None,
        always_thinking_enabled: None,
        include_coauthored_by: None,
        skip_dangerous_mode_permission_prompt: None,
    }
}

fn build_openclaw_provider_snapshot(
    editor: &OpenClawEditorConfig,
    include_api_key: bool,
) -> Map<String, Value> {
    let mut provider = Map::new();
    let api_format = editor
        .api_format
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_OPENCLAW_API_FORMAT);
    provider.insert("api".to_string(), Value::String(api_format.to_string()));

    if let Some(url) = editor
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        provider.insert("baseUrl".to_string(), Value::String(url.to_string()));
    }

    if include_api_key {
        if let Some(token) = editor
            .api_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            provider.insert("apiKey".to_string(), Value::String(token.to_string()));
        }
    }

    provider
}

fn openclaw_editor_to_dto(editor: OpenClawEditorConfig) -> OpenClawEditorConfigDto {
    OpenClawEditorConfigDto {
        agent_id: editor.agent_id,
        provider_id: editor.provider_id,
        primary_model: editor.primary_model,
        fallback_models: editor.fallback_models,
        api_format: editor.api_format,
        base_url: editor.base_url,
        api_key: editor.api_key,
    }
}

fn parse_openclaw_editor_config(
    primary_root: &Map<String, Value>,
    models_root: Option<&Map<String, Value>>,
    auth_profiles_root: Option<&Map<String, Value>>,
    config: Option<&AgentConfig>,
) -> AppResult<OpenClawEditorConfig> {
    let parsed =
        parse_openclaw_config_with_sources(primary_root, models_root, auth_profiles_root, config)?;
    Ok(OpenClawEditorConfig {
        agent_id: parsed
            .agent_id
            .unwrap_or_else(|| resolve_openclaw_agent_id_from_config(config)),
        provider_id: parsed
            .provider_id
            .unwrap_or_else(|| resolve_openclaw_provider_name(primary_root, models_root, config)),
        primary_model: parsed.model,
        fallback_models: parsed.fallback_models.unwrap_or_default(),
        api_format: parsed.api_format,
        base_url: parsed.url,
        api_key: parsed.api_token,
    })
}

fn validate_openclaw_editor_config(
    editor: &OpenClawEditorConfig,
    primary_root: &Map<String, Value>,
    models_root: Option<&Map<String, Value>>,
    auth_profiles_root: Option<&Map<String, Value>>,
) -> AppResult<()> {
    let provider_id = editor.provider_id.trim();
    if provider_id.is_empty() {
        return Err(AppError::validation("openclaw provider is required"));
    }
    let provider_exists = get_openclaw_provider_object(primary_root, provider_id).is_some()
        || models_root
            .and_then(|root| get_openclaw_provider_object(root, provider_id))
            .is_some();
    if !provider_exists {
        return Err(AppError::validation(format!(
            "openclaw provider reference not found: {provider_id}"
        )));
    }

    let provider = get_openclaw_provider_object(primary_root, provider_id)
        .or_else(|| models_root.and_then(|root| get_openclaw_provider_object(root, provider_id)));
    if let Some(profile_id) = provider.and_then(resolve_openclaw_auth_profile_id) {
        if !openclaw_auth_profile_exists(auth_profiles_root, profile_id.as_str()) {
            return Err(AppError::validation(format!(
                "openclaw auth profile reference not found: {profile_id}"
            )));
        }
    }
    Ok(())
}

fn format_openclaw_primary_source(
    editor: &OpenClawEditorConfig,
    primary_root: &mut Map<String, Value>,
) -> AppResult<()> {
    let provider_id = editor.provider_id.trim();
    if provider_id.is_empty() {
        return Err(AppError::validation("openclaw provider is required"));
    }

    let provider_snapshot = build_openclaw_provider_snapshot(editor, false);
    let providers = ensure_openclaw_primary_provider_map(primary_root);
    let provider = ensure_child_object(providers, provider_id);
    *provider = provider_snapshot;

    let agents = ensure_child_object(primary_root, "agents");
    let defaults = ensure_child_object(agents, "defaults");
    let model_config = ensure_child_object(defaults, "model");
    match editor
        .primary_model
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(model) => {
            model_config.insert("primary".to_string(), Value::String(model.to_string()));
        }
        None => {
            model_config.remove("primary");
        }
    }
    if editor.fallback_models.is_empty() {
        model_config.remove("fallbacks");
    } else {
        model_config.insert(
            "fallbacks".to_string(),
            Value::Array(
                editor
                    .fallback_models
                    .iter()
                    .map(|item| Value::String(item.trim().to_string()))
                    .filter(|item| {
                        item.as_str()
                            .map(|value| !value.is_empty())
                            .unwrap_or(false)
                    })
                    .collect(),
            ),
        );
    }

    Ok(())
}

fn lookup_openclaw_auth_profile_token(
    root: Option<&Map<String, Value>>,
    profile_id: &str,
) -> Option<String> {
    let normalized_profile_id = profile_id.trim();
    if normalized_profile_id.is_empty() {
        return None;
    }

    let root = root?;
    let profiles = root
        .get("profiles")
        .and_then(|value| value.as_object())
        .unwrap_or(root);
    let profile = profiles.get(normalized_profile_id)?.as_object()?;

    for key in ["apiKey", "key", "token", "secret"] {
        if let Some(value) = profile.get(key).and_then(|item| item.as_str()) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }

    None
}

fn resolve_openclaw_auth_profile_id(provider: &Map<String, Value>) -> Option<String> {
    provider
        .get("authProfile")
        .and_then(|value| value.as_str())
        .or_else(|| {
            provider
                .get("authProfileId")
                .and_then(|value| value.as_str())
        })
        .or_else(|| provider.get("profile").and_then(|value| value.as_str()))
        .or_else(|| {
            provider
                .get("auth")
                .and_then(|value| value.as_object())
                .and_then(|auth| auth.get("profile").or_else(|| auth.get("profileId")))
                .and_then(|value| value.as_str())
        })
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(String::from)
}

fn openclaw_auth_profile_exists(root: Option<&Map<String, Value>>, profile_id: &str) -> bool {
    let normalized_profile_id = profile_id.trim();
    if normalized_profile_id.is_empty() {
        return false;
    }

    let Some(root) = root else {
        return false;
    };
    let profiles = root
        .get("profiles")
        .and_then(|value| value.as_object())
        .unwrap_or(root);
    profiles.contains_key(normalized_profile_id)
}

fn extract_openclaw_api_token(
    provider: &Map<String, Value>,
    auth_profiles_root: Option<&Map<String, Value>>,
) -> Option<String> {
    for key in ["apiKey", "api_key", "key", "token"] {
        if let Some(value) = provider.get(key).and_then(|item| item.as_str()) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }

    let profile_id = resolve_openclaw_auth_profile_id(provider);

    profile_id
        .and_then(|value| lookup_openclaw_auth_profile_token(auth_profiles_root, value.as_str()))
}

fn parse_openclaw_config_with_sources(
    primary_root: &Map<String, Value>,
    models_root: Option<&Map<String, Value>>,
    auth_profiles_root: Option<&Map<String, Value>>,
    config: Option<&AgentConfig>,
) -> AppResult<AgentConfig> {
    let provider_id = resolve_openclaw_provider_name(primary_root, models_root, config);
    let mut provider = Map::new();

    if let Some(primary_provider) = get_openclaw_provider_object(primary_root, &provider_id) {
        provider.extend(primary_provider.clone());
    }
    if let Some(models_root) = models_root {
        if let Some(models_provider) = get_openclaw_provider_object(models_root, &provider_id) {
            for (key, value) in models_provider {
                provider.insert(key.clone(), value.clone());
            }
        }
    }

    let url = provider
        .get("baseUrl")
        .or_else(|| provider.get("baseURL"))
        .and_then(|value| value.as_str())
        .map(String::from);
    let api_token = extract_openclaw_api_token(&provider, auth_profiles_root);
    let api_format = provider
        .get("api")
        .and_then(|value| value.as_str())
        .map(String::from);
    let model = primary_root
        .get("agents")
        .and_then(|value| value.as_object())
        .and_then(|agents| agents.get("defaults"))
        .and_then(|value| value.as_object())
        .and_then(|defaults| defaults.get("model"))
        .and_then(|value| value.as_object())
        .and_then(|model_config| model_config.get("primary"))
        .and_then(|value| value.as_str())
        .map(String::from);
    let fallback_models = primary_root
        .get("agents")
        .and_then(|value| value.as_object())
        .and_then(|agents| agents.get("defaults"))
        .and_then(|value| value.as_object())
        .and_then(|defaults| defaults.get("model"))
        .and_then(|value| value.as_object())
        .and_then(|model_config| parse_string_array(model_config.get("fallbacks")));

    Ok(AgentConfig {
        agent_id: Some(resolve_openclaw_agent_id_from_config(config)),
        provider_id: Some(provider_id),
        url,
        api_token,
        api_format,
        model,
        fallback_models,
        timeout: None,
        always_thinking_enabled: None,
        include_coauthored_by: None,
        skip_dangerous_mode_permission_prompt: None,
    })
}

fn ensure_openclaw_primary_provider_map(root: &mut Map<String, Value>) -> &mut Map<String, Value> {
    let models = ensure_child_object(root, "models");
    ensure_child_object(models, "providers")
}

fn ensure_openclaw_registry_provider_map(root: &mut Map<String, Value>) -> &mut Map<String, Value> {
    if root.contains_key("providers") || !root.contains_key("models") {
        return ensure_child_object(root, "providers");
    }

    let models = ensure_child_object(root, "models");
    ensure_child_object(models, "providers")
}

fn sync_openclaw_provider_to_models_file(
    config_dir: &Path,
    agent_id: &str,
    provider_id: &str,
    provider_value: &Map<String, Value>,
    preserve_existing: bool,
) -> AppResult<()> {
    let models_file_path = resolve_openclaw_models_file_path(config_dir, agent_id)?;
    let mut models_root = read_json_like_object(&models_file_path)?;
    let providers = ensure_openclaw_registry_provider_map(&mut models_root);
    let next_provider = if preserve_existing {
        let existing_provider = providers
            .get(provider_id)
            .and_then(|value| value.as_object());
        Value::Object(merge_openclaw_provider(existing_provider, provider_value))
    } else {
        Value::Object(provider_value.clone())
    };
    providers.insert(provider_id.to_string(), next_provider);
    write_json_object(&models_file_path, &models_root)
}

fn merge_openclaw_provider(
    existing_provider: Option<&Map<String, Value>>,
    provider_patch: &Map<String, Value>,
) -> Map<String, Value> {
    let mut merged = existing_provider.cloned().unwrap_or_default();

    if provider_patch.contains_key("baseUrl") {
        merged.remove("baseURL");
    }
    if provider_patch.contains_key("baseURL") {
        merged.remove("baseUrl");
    }
    if provider_patch.contains_key("apiKey") {
        merged.remove("api_key");
    }
    if provider_patch.contains_key("api_key") {
        merged.remove("apiKey");
    }

    for (key, value) in provider_patch {
        merged.insert(key.clone(), value.clone());
    }

    merged
}

fn write_openclaw_config(
    config_dir: &Path,
    existing_config: &Option<AgentConfig>,
    entry_url: &str,
) -> AppResult<PathBuf> {
    let file_path = config_dir.join("openclaw.json");
    let mut root = read_json_like_object(&file_path)?;
    let mut editor = parse_openclaw_editor_config(&root, None, None, existing_config.as_ref())?;
    editor.base_url = Some(build_openclaw_entry_url(entry_url));
    format_openclaw_primary_source(&editor, &mut root)?;
    let provider_snapshot = build_openclaw_provider_snapshot(&editor, false);

    write_json_object(&file_path, &root)?;
    sync_openclaw_provider_to_models_file(
        config_dir,
        &editor.agent_id,
        &editor.provider_id,
        &provider_snapshot,
        true,
    )?;

    Ok(file_path)
}

fn resolve_codex_provider_name_from_map(config_root: &Map<String, Value>) -> Option<String> {
    config_root
        .get("model_provider")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(String::from)
}

fn resolve_codex_provider_name_from_doc(doc: &DocumentMut) -> AppResult<String> {
    let explicit = doc
        .get("model_provider")
        .and_then(|item| item.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(String::from);
    if let Some(name) = explicit {
        return Ok(name);
    }
    Err(AppError::validation(
        "codex config missing required `model_provider`".to_string(),
    ))
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

/// Reads raw file content from local or WSL paths.
fn read_file_content(file_path: &Path) -> AppResult<Option<String>> {
    if is_wsl_path(file_path) {
        return read_file_via_wsl(file_path);
    }

    if !file_path.exists() {
        return Ok(None);
    }

    let raw = std::fs::read_to_string(file_path).map_err(|e| {
        AppError::external(format!("read file failed ({}): {e}", file_path.display()))
    })?;
    Ok(Some(raw))
}

/// Parses JSON or JSONC text into an object map.
fn parse_json_like_content(content: &str, file_path: &Path) -> AppResult<Map<String, Value>> {
    if content.trim().is_empty() {
        return Ok(Map::new());
    }

    let parsed = serde_json::from_str::<Value>(content)
        .or_else(|_| json5::from_str::<Value>(content))
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

/// Reads JSON or JSONC object from file.
fn read_json_like_object(file_path: &Path) -> AppResult<Map<String, Value>> {
    let Some(content) = read_file_content(file_path)? else {
        return Ok(Map::new());
    };
    parse_json_like_content(&content, file_path)
}

/// Reads TOML document from file.
fn read_toml_document(file_path: &Path) -> AppResult<DocumentMut> {
    let Some(content) = read_file_content(file_path)? else {
        return Ok(DocumentMut::new());
    };
    parse_toml_content(&content, file_path)
}

/// Parses TOML text into a mutable document.
fn parse_toml_content(content: &str, file_path: &Path) -> AppResult<DocumentMut> {
    if content.trim().is_empty() {
        return Ok(DocumentMut::new());
    }

    content.parse::<DocumentMut>().map_err(|e| {
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
    use crate::app_state::{AppState, SharedState};
    use crate::auth::RemoteAdminAuthStore;
    use crate::integration_store::IntegrationStore;
    use crate::log_store::LogStore;
    use crate::models::AppInfo;
    use crate::proxy::ProxyRuntime;
    use crate::stats_store::StatsStore;
    use serde_json::json;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_shared_state() -> SharedState {
        let unique_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time must move forward")
            .as_nanos();
        let base_dir =
            std::env::temp_dir().join(format!("oc-proxy-integration-service-{unique_id}"));
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

    #[test]
    fn codex_config_shape_can_be_created_from_empty_document() {
        let mut doc = DocumentMut::new();
        doc["model_provider"] = value("custom_provider");
        let model_providers = ensure_toml_table(doc.as_table_mut(), "model_providers");
        let provider = ensure_toml_table(model_providers, "custom_provider");
        provider["base_url"] = value("http://127.0.0.1:11434");

        let output = doc.to_string();
        assert!(output.contains("model_provider"));
        assert!(output.contains("model_providers"));
        assert!(output.contains("custom_provider"));
        assert!(output.contains("base_url"));
    }

    #[test]
    fn normalize_base_url_override_rejects_wildcard_hosts_and_adds_missing_port() {
        assert_eq!(
            normalize_base_url_override("https://remote-aor.test", 8899).as_deref(),
            Some("https://remote-aor.test:8899")
        );
        assert_eq!(normalize_base_url_override("0.0.0.0:8899", 8899), None);
        assert_eq!(normalize_base_url_override("::", 8899), None);
    }

    #[test]
    fn build_group_entry_url_prefers_base_url_override() {
        let state = test_shared_state();
        let config = state.config_store.get();

        assert_eq!(
            build_group_entry_url(
                &state,
                &config,
                "dev",
                Some("https://remote-aor.test:17777")
            ),
            "https://remote-aor.test:17777/oc/dev"
        );
    }

    #[test]
    fn write_group_entry_persists_target_group_binding() {
        let state = test_shared_state();
        let unique_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time must move forward")
            .as_nanos();
        let config_dir = std::env::temp_dir().join(format!("oc-proxy-claude-target-{unique_id}"));
        std::fs::create_dir_all(&config_dir).expect("claude config dir should be created");

        let mut config = state.config_store.get();
        config.groups = vec![crate::models::Group {
            id: "dev".to_string(),
            name: "Dev".to_string(),
            models: vec!["claude-test".to_string()],
            provider_ids: Vec::new(),
            active_provider_id: None,
            providers: Vec::new(),
            failover: crate::models::default_group_failover_config(),
        }];
        state
            .config_store
            .save_config(config)
            .expect("config with group should save");

        state
            .integration_store
            .put_target(
                "claude-target",
                IntegrationClientKind::Claude,
                config_dir.to_string_lossy().to_string(),
                Some(AgentConfig {
                    agent_id: None,
                    provider_id: None,
                    url: None,
                    api_token: None,
                    api_format: None,
                    model: None,
                    fallback_models: None,
                    timeout: None,
                    always_thinking_enabled: Some(true),
                    include_coauthored_by: None,
                    skip_dangerous_mode_permission_prompt: None,
                }),
            )
            .expect("target should save");

        write_group_entry(&state, "dev", vec!["claude-target".to_string()])
            .expect("write group entry should succeed");

        let targets = state.integration_store.list();
        let target = targets
            .iter()
            .find(|item| item.id == "claude-target")
            .expect("target should exist after write");
        assert_eq!(target.group_id.as_deref(), Some("dev"));

        let _ = std::fs::remove_dir_all(&config_dir);
    }

    #[test]
    fn codex_config_reads_token_from_auth_json_first() {
        let config_root = json!({
            "model_provider": "custom_provider",
            "model_providers": {
                "custom_provider": {
                    "base_url": "http://127.0.0.1:11434",
                    "api_key": "legacy-token"
                },
                "aor_shared": {
                    "base_url": "http://ignored.example"
                }
            },
            "model": "gpt-5"
        })
        .as_object()
        .cloned()
        .expect("config root must be object");
        let auth_root = json!({
            "OPENAI_API_KEY": "auth-token"
        })
        .as_object()
        .cloned()
        .expect("auth root must be object");

        let parsed = parse_codex_config_with_auth(&config_root, Some(&auth_root))
            .expect("codex parse should succeed");

        assert_eq!(parsed.api_token.as_deref(), Some("auth-token"));
        assert_eq!(parsed.url.as_deref(), Some("http://127.0.0.1:11434"));
        assert_eq!(parsed.model.as_deref(), Some("gpt-5"));
    }

    #[test]
    fn codex_config_reads_legacy_api_key_when_auth_missing() {
        let config_root = json!({
            "model_provider": "custom_provider",
            "model_providers": {
                "custom_provider": {
                    "base_url": "http://127.0.0.1:11434",
                    "api_key": "legacy-token"
                }
            },
            "model": "gpt-5"
        })
        .as_object()
        .cloned()
        .expect("config root must be object");

        let parsed =
            parse_codex_config_with_auth(&config_root, None).expect("codex parse should succeed");

        assert_eq!(parsed.api_token.as_deref(), Some("legacy-token"));
        assert_eq!(parsed.url.as_deref(), Some("http://127.0.0.1:11434"));
    }

    #[test]
    fn write_codex_full_config_moves_token_to_auth_json_and_removes_legacy_key() {
        let unique_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time must move forward")
            .as_nanos();
        let temp_dir = std::env::temp_dir().join(format!("oc-proxy-codex-{unique_id}"));
        std::fs::create_dir_all(&temp_dir).expect("temp dir should be created");

        let config_path = temp_dir.join("config.toml");
        std::fs::write(
            &config_path,
            r#"model_provider = "custom_provider"

[model_providers.custom_provider]
base_url = "http://legacy"
api_key = "legacy-token"

[model_providers.aor_shared]
base_url = "http://should-not-change"
"#,
        )
        .expect("seed config.toml");

        let first = AgentConfig {
            agent_id: None,
            provider_id: None,
            url: Some("http://127.0.0.1:8080/oc/test".to_string()),
            api_token: Some("fresh-token".to_string()),
            api_format: None,
            model: Some("gpt-5".to_string()),
            fallback_models: None,
            timeout: None,
            always_thinking_enabled: None,
            include_coauthored_by: None,
            skip_dangerous_mode_permission_prompt: None,
        };
        write_codex_full_config(&temp_dir, &first).expect("first write should succeed");

        let updated_config = std::fs::read_to_string(&config_path).expect("read config.toml");
        assert!(updated_config.contains("model_provider = \"custom_provider\""));
        assert!(updated_config.contains("base_url = \"http://127.0.0.1:8080/oc/test\""));
        assert!(updated_config.contains("[model_providers.custom_provider]"));
        assert!(updated_config.contains("[model_providers.aor_shared]"));
        assert!(updated_config.contains("http://should-not-change"));
        assert!(updated_config.contains("model = \"gpt-5\""));
        assert!(!updated_config.contains("api_key"));

        let auth_path = temp_dir.join("auth.json");
        let auth_raw = std::fs::read_to_string(&auth_path).expect("read auth.json");
        let auth_doc = serde_json::from_str::<Value>(&auth_raw).expect("auth.json must be valid");
        assert_eq!(auth_doc["OPENAI_API_KEY"].as_str(), Some("fresh-token"));

        let second = AgentConfig {
            agent_id: None,
            provider_id: None,
            url: Some("http://127.0.0.1:8080/oc/test".to_string()),
            api_token: None,
            api_format: None,
            model: Some("gpt-5".to_string()),
            fallback_models: None,
            timeout: None,
            always_thinking_enabled: None,
            include_coauthored_by: None,
            skip_dangerous_mode_permission_prompt: None,
        };
        write_codex_full_config(&temp_dir, &second).expect("second write should succeed");

        let auth_raw = std::fs::read_to_string(&auth_path).expect("read auth.json after clear");
        let auth_doc =
            serde_json::from_str::<Value>(&auth_raw).expect("auth.json must remain valid");
        assert!(auth_doc.get("OPENAI_API_KEY").is_none());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn write_codex_full_config_requires_model_provider() {
        let unique_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time must move forward")
            .as_nanos();
        let temp_dir = std::env::temp_dir().join(format!("oc-proxy-codex-no-provider-{unique_id}"));
        std::fs::create_dir_all(&temp_dir).expect("temp dir should be created");
        let config_path = temp_dir.join("config.toml");
        std::fs::write(&config_path, "").expect("seed empty config.toml");

        let config = AgentConfig {
            agent_id: None,
            provider_id: None,
            url: Some("http://127.0.0.1:8080/oc/test".to_string()),
            api_token: Some("fresh-token".to_string()),
            api_format: None,
            model: Some("gpt-5".to_string()),
            fallback_models: None,
            timeout: None,
            always_thinking_enabled: None,
            include_coauthored_by: None,
            skip_dangerous_mode_permission_prompt: None,
        };

        let err = write_codex_full_config(&temp_dir, &config).expect_err("write should fail");
        assert!(err.to_string().contains("model_provider"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn parse_opencode_config_reads_api_key_from_options() {
        let root = json!({
            "provider": {
                "aor_shared": {
                    "options": {
                        "baseURL": "http://127.0.0.1:11434",
                        "apiKey": "local-opencode-token",
                        "timeout": 45000
                    }
                }
            },
            "model": "gpt-5-mini"
        })
        .as_object()
        .cloned()
        .expect("root must be object");

        let parsed = parse_opencode_config(&root).expect("opencode parse should succeed");

        assert_eq!(parsed.url.as_deref(), Some("http://127.0.0.1:11434"));
        assert_eq!(parsed.api_token.as_deref(), Some("local-opencode-token"));
        assert_eq!(parsed.timeout, Some(45000));
        assert_eq!(parsed.model.as_deref(), Some("gpt-5-mini"));
    }

    #[test]
    fn write_opencode_full_config_persists_api_key() {
        let unique_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time must move forward")
            .as_nanos();
        let temp_dir = std::env::temp_dir().join(format!("oc-proxy-opencode-{unique_id}"));
        std::fs::create_dir_all(&temp_dir).expect("temp dir should be created");

        let config_path = temp_dir.join("opencode.json");
        std::fs::write(
            &config_path,
            serde_json::to_string_pretty(&json!({
                "provider": {
                    "aor_shared": {
                        "options": {
                            "baseURL": "http://legacy",
                            "apiKey": "legacy-token"
                        }
                    },
                    "unchanged": {
                        "options": {
                            "baseURL": "http://keep-me"
                        }
                    }
                },
                "model": "legacy-model"
            }))
            .expect("serialize opencode.json"),
        )
        .expect("seed opencode.json");

        let first = AgentConfig {
            agent_id: None,
            provider_id: Some("aor_shared".to_string()),
            url: Some("http://127.0.0.1:8080/oc/test".to_string()),
            api_token: Some("fresh-token".to_string()),
            api_format: None,
            model: Some("gpt-5".to_string()),
            fallback_models: None,
            timeout: Some(60000),
            always_thinking_enabled: None,
            include_coauthored_by: None,
            skip_dangerous_mode_permission_prompt: None,
        };
        write_opencode_full_config(&temp_dir, &first).expect("first write should succeed");

        let raw = std::fs::read_to_string(&config_path).expect("read opencode.json");
        let root = serde_json::from_str::<Value>(&raw).expect("opencode.json must be valid");
        assert_eq!(
            root["provider"]["aor_shared"]["options"]["baseURL"].as_str(),
            Some("http://127.0.0.1:8080/oc/test")
        );
        assert_eq!(
            root["provider"]["aor_shared"]["options"]["apiKey"].as_str(),
            Some("fresh-token")
        );
        assert_eq!(
            root["provider"]["aor_shared"]["options"]["timeout"].as_u64(),
            Some(60000)
        );
        assert_eq!(root["model"].as_str(), Some("gpt-5"));
        assert_eq!(
            root["provider"]["unchanged"]["options"]["baseURL"].as_str(),
            Some("http://keep-me")
        );

        let second = AgentConfig {
            agent_id: None,
            provider_id: Some("aor_shared".to_string()),
            url: Some("http://127.0.0.1:8080/oc/test".to_string()),
            api_token: None,
            api_format: None,
            model: Some("gpt-5".to_string()),
            fallback_models: None,
            timeout: Some(60000),
            always_thinking_enabled: None,
            include_coauthored_by: None,
            skip_dangerous_mode_permission_prompt: None,
        };
        write_opencode_full_config(&temp_dir, &second).expect("second write should succeed");

        let raw = std::fs::read_to_string(&config_path).expect("read opencode.json after clear");
        let root = serde_json::from_str::<Value>(&raw).expect("opencode.json must remain valid");
        assert!(root["provider"]["aor_shared"]["options"]
            .get("apiKey")
            .is_none());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn preferred_opencode_config_dir_prefers_config_and_supports_legacy_data_dir() {
        let unique_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time must move forward")
            .as_nanos();
        let home_dir = std::env::temp_dir().join(format!("oc-proxy-opencode-home-{unique_id}"));
        let config_dir = home_dir.join(".config").join("opencode");
        let data_dir = home_dir.join(".local").join("share").join("opencode");

        std::fs::create_dir_all(&data_dir).expect("legacy data dir should be created");
        std::fs::write(data_dir.join("opencode.json"), "{}").expect("seed legacy config");
        assert_eq!(preferred_opencode_config_dir(&home_dir), data_dir);

        std::fs::create_dir_all(&config_dir).expect("config dir should be created");
        std::fs::write(config_dir.join("opencode.jsonc"), "{}").expect("seed config jsonc");
        assert_eq!(preferred_opencode_config_dir(&home_dir), config_dir);

        let _ = std::fs::remove_dir_all(&home_dir);
    }

    #[test]
    fn preferred_hidden_config_dir_falls_back_to_root_level_for_root_home() {
        let unique_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time must move forward")
            .as_nanos();
        let sandbox_root = std::env::temp_dir().join(format!("oc-proxy-root-fallback-{unique_id}"));
        let fake_root_home = sandbox_root.join("root-home");
        let fake_root_base = sandbox_root.join("fs-root");
        let root_level = fake_root_base.join(".claude");

        std::fs::create_dir_all(&fake_root_home).expect("fake root home should be created");
        std::fs::create_dir_all(&fake_root_base).expect("fake root base should be created");
        std::fs::create_dir_all(&root_level).expect("root-level claude dir should be created");

        let preferred = preferred_hidden_config_dir_with_root_paths(
            &fake_root_home,
            ".claude",
            &fake_root_home,
            &fake_root_base,
        );
        assert_eq!(preferred, root_level);

        let _ = std::fs::remove_dir_all(&sandbox_root);
    }

    #[test]
    fn preferred_hidden_config_dir_prefers_home_candidate_when_present() {
        let unique_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time must move forward")
            .as_nanos();
        let home_dir = std::env::temp_dir().join(format!("oc-proxy-home-hidden-{unique_id}"));
        let home_candidate = home_dir.join(".openclaw");

        std::fs::create_dir_all(&home_candidate).expect("home hidden dir should be created");

        let preferred = preferred_hidden_config_dir_with_root_paths(
            &home_dir,
            ".openclaw",
            Path::new("/root"),
            Path::new("/"),
        );
        assert_eq!(preferred, home_candidate);

        let _ = std::fs::remove_dir_all(&home_dir);
    }

    #[test]
    fn list_default_targets_with_root_paths_prefers_root_level_hidden_dirs() {
        let unique_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time must move forward")
            .as_nanos();
        let sandbox_root =
            std::env::temp_dir().join(format!("oc-proxy-default-targets-root-{unique_id}"));
        let fake_root_home = sandbox_root.join("root-home");
        let fake_root_base = sandbox_root.join("fs-root");
        let root_claude = fake_root_base.join(".claude");
        let root_codex = fake_root_base.join(".codex");
        let root_openclaw = fake_root_base.join(".openclaw");

        std::fs::create_dir_all(&fake_root_home).expect("fake root home should be created");
        std::fs::create_dir_all(&root_claude).expect("root-level claude dir should be created");
        std::fs::create_dir_all(&root_codex).expect("root-level codex dir should be created");
        std::fs::create_dir_all(&root_openclaw).expect("root-level openclaw dir should be created");

        let targets =
            list_default_targets_with_root_paths(&fake_root_home, &fake_root_home, &fake_root_base);

        let claude = targets
            .iter()
            .find(|target| target.kind == IntegrationClientKind::Claude)
            .expect("claude target should exist");
        assert_eq!(PathBuf::from(&claude.config_dir), root_claude);

        let codex = targets
            .iter()
            .find(|target| target.kind == IntegrationClientKind::Codex)
            .expect("codex target should exist");
        assert_eq!(PathBuf::from(&codex.config_dir), root_codex);

        let openclaw = targets
            .iter()
            .find(|target| target.kind == IntegrationClientKind::Openclaw)
            .expect("openclaw target should exist");
        assert_eq!(PathBuf::from(&openclaw.config_dir), root_openclaw);

        let opencode = targets
            .iter()
            .find(|target| target.kind == IntegrationClientKind::Opencode)
            .expect("opencode target should exist");
        assert_eq!(
            PathBuf::from(&opencode.config_dir),
            fake_root_home.join(".config").join("opencode")
        );

        let _ = std::fs::remove_dir_all(&sandbox_root);
    }

    #[test]
    fn list_targets_with_saved_and_root_paths_merges_default_targets_for_desktop() {
        let unique_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time must move forward")
            .as_nanos();
        let sandbox_root =
            std::env::temp_dir().join(format!("oc-proxy-desktop-targets-root-{unique_id}"));
        let fake_root_home = sandbox_root.join("root-home");
        let fake_root_base = sandbox_root.join("fs-root");
        let root_claude = fake_root_base.join(".claude");
        let custom_codex = sandbox_root.join("custom-codex");

        std::fs::create_dir_all(&fake_root_home).expect("fake root home should be created");
        std::fs::create_dir_all(&root_claude).expect("root-level claude dir should be created");
        std::fs::create_dir_all(&custom_codex).expect("custom codex dir should be created");

        let saved_targets = vec![IntegrationTarget {
            id: "custom-codex".to_string(),
            kind: IntegrationClientKind::Codex,
            config_dir: custom_codex.to_string_lossy().to_string(),
            config: None,
            group_id: None,
            created_at: "2026-03-24T00:00:00Z".to_string(),
            updated_at: "2026-03-24T00:00:00Z".to_string(),
        }];

        let targets = list_targets_with_saved_and_root_paths(
            saved_targets,
            &fake_root_home,
            &fake_root_home,
            &fake_root_base,
        );

        assert!(targets.iter().any(|target| {
            target.id == "default:claude"
                && target.kind == IntegrationClientKind::Claude
                && PathBuf::from(&target.config_dir) == root_claude
        }));
        assert!(targets.iter().any(|target| {
            target.id == "custom-codex"
                && target.kind == IntegrationClientKind::Codex
                && PathBuf::from(&target.config_dir) == custom_codex
        }));

        let _ = std::fs::remove_dir_all(&sandbox_root);
    }

    #[test]
    fn merge_default_and_saved_targets_prefers_saved_duplicates() {
        let default_target = IntegrationTarget {
            id: "default:claude".to_string(),
            kind: IntegrationClientKind::Claude,
            config_dir: "/.claude".to_string(),
            config: None,
            group_id: None,
            created_at: "2026-03-24T00:00:00Z".to_string(),
            updated_at: "2026-03-24T00:00:00Z".to_string(),
        };
        let saved_target = IntegrationTarget {
            id: "saved-claude".to_string(),
            kind: IntegrationClientKind::Claude,
            config_dir: "/.claude".to_string(),
            config: None,
            group_id: None,
            created_at: "2026-03-24T01:00:00Z".to_string(),
            updated_at: "2026-03-24T01:00:00Z".to_string(),
        };

        let merged =
            merge_default_and_saved_targets(vec![default_target], vec![saved_target.clone()]);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].id, saved_target.id);
    }

    #[test]
    fn openclaw_validate_detects_missing_provider_reference() {
        let editor = OpenClawEditorConfig {
            agent_id: "workspace-alpha".to_string(),
            provider_id: "missing-provider".to_string(),
            primary_model: Some("gpt-4.1".to_string()),
            fallback_models: vec!["gpt-4.1-mini".to_string()],
            api_format: Some("openai-responses".to_string()),
            base_url: Some("http://127.0.0.1:8899/oc/dev/v1".to_string()),
            api_key: Some("secret".to_string()),
        };
        let primary_root = json!({
            "models": {"providers": {"aor_shared": {"api": "openai-responses"}}}
        })
        .as_object()
        .expect("primary root object")
        .clone();

        let err = validate_openclaw_editor_config(&editor, &primary_root, None, None)
            .expect_err("validation should fail");
        assert!(err.to_string().contains("provider"));
    }

    #[test]
    fn write_openclaw_source_rejects_missing_provider_reference() {
        let unique_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time must move forward")
            .as_nanos();
        let temp_dir =
            std::env::temp_dir().join(format!("oc-proxy-openclaw-source-write-{unique_id}"));
        std::fs::create_dir_all(&temp_dir).expect("temp dir should be created");

        let target = IntegrationTarget {
            id: "openclaw-target".to_string(),
            kind: IntegrationClientKind::Openclaw,
            config_dir: temp_dir.to_string_lossy().to_string(),
            config: Some(AgentConfig {
                agent_id: Some("workspace-alpha".to_string()),
                provider_id: Some("missing-provider".to_string()),
                url: None,
                api_token: None,
                api_format: None,
                model: None,
                fallback_models: None,
                timeout: None,
                always_thinking_enabled: None,
                include_coauthored_by: None,
                skip_dangerous_mode_permission_prompt: None,
            }),
            group_id: None,
            created_at: "2026-03-26T00:00:00Z".to_string(),
            updated_at: "2026-03-26T00:00:00Z".to_string(),
        };
        let content = serde_json::to_string_pretty(&json!({
            "models": {"providers": {"aor_shared": {"api": "openai-responses"}}}
        }))
        .expect("content should serialize");

        let err = write_agent_config_source_with_targets(
            None,
            vec![target],
            "openclaw-target",
            &content,
            Some(SOURCE_PRIMARY),
        )
        .expect_err("write should fail when provider reference is missing");

        assert!(err.to_string().contains("provider"));
        assert!(!temp_dir.join("openclaw.json").exists());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn write_openclaw_source_rejects_missing_auth_profile_reference() {
        let unique_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time must move forward")
            .as_nanos();
        let temp_dir =
            std::env::temp_dir().join(format!("oc-proxy-openclaw-auth-profile-{unique_id}"));
        let agent_dir = temp_dir
            .join("agents")
            .join("workspace-alpha")
            .join("agent");
        std::fs::create_dir_all(&agent_dir).expect("agent dir should be created");

        std::fs::write(
            temp_dir.join("openclaw.json"),
            serde_json::to_string_pretty(&json!({
                "models": {
                    "providers": {
                        "aor_shared": {
                            "api": "openai-responses",
                            "authProfile": "missing-profile"
                        }
                    }
                }
            }))
            .expect("primary content should serialize"),
        )
        .expect("primary config should write");
        std::fs::write(
            agent_dir.join("models.json"),
            serde_json::to_string_pretty(&json!({
                "providers": {
                    "aor_shared": {
                        "baseUrl": "http://registry.local/v1"
                    }
                }
            }))
            .expect("models content should serialize"),
        )
        .expect("models config should write");

        let target = IntegrationTarget {
            id: "openclaw-target".to_string(),
            kind: IntegrationClientKind::Openclaw,
            config_dir: temp_dir.to_string_lossy().to_string(),
            config: Some(AgentConfig {
                agent_id: Some("workspace-alpha".to_string()),
                provider_id: Some("aor_shared".to_string()),
                url: None,
                api_token: None,
                api_format: None,
                model: None,
                fallback_models: None,
                timeout: None,
                always_thinking_enabled: None,
                include_coauthored_by: None,
                skip_dangerous_mode_permission_prompt: None,
            }),
            group_id: None,
            created_at: "2026-03-26T00:00:00Z".to_string(),
            updated_at: "2026-03-26T00:00:00Z".to_string(),
        };
        let content = serde_json::to_string_pretty(&json!({"profiles": {}}))
            .expect("auth profiles content should serialize");

        let err = write_agent_config_source_with_targets(
            None,
            vec![target],
            "openclaw-target",
            &content,
            Some(SOURCE_AUTH_PROFILES),
        )
        .expect_err("write should fail when auth profile reference is missing");

        assert!(err.to_string().contains("auth profile"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn write_openclaw_source_surfaces_related_models_file_parse_errors() {
        let unique_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time must move forward")
            .as_nanos();
        let temp_dir =
            std::env::temp_dir().join(format!("oc-proxy-openclaw-models-parse-{unique_id}"));
        let agent_dir = temp_dir
            .join("agents")
            .join("workspace-alpha")
            .join("agent");
        std::fs::create_dir_all(&agent_dir).expect("agent dir should be created");

        std::fs::write(
            temp_dir.join("openclaw.json"),
            serde_json::to_string_pretty(&json!({
                "agents": {
                    "defaults": {"model": {"primary": "gpt-4.1"}}
                },
                "models": {
                    "providers": {
                        "aor_shared": {
                            "api": "openai-responses",
                            "baseUrl": "http://127.0.0.1:8899/oc/dev/v1"
                        }
                    }
                }
            }))
            .expect("primary content should serialize"),
        )
        .expect("primary config should write");
        std::fs::write(agent_dir.join("models.json"), "{")
            .expect("invalid models config should write");

        let target = IntegrationTarget {
            id: "openclaw-target".to_string(),
            kind: IntegrationClientKind::Openclaw,
            config_dir: temp_dir.to_string_lossy().to_string(),
            config: Some(AgentConfig {
                agent_id: Some("workspace-alpha".to_string()),
                provider_id: Some("aor_shared".to_string()),
                url: None,
                api_token: None,
                api_format: None,
                model: None,
                fallback_models: None,
                timeout: None,
                always_thinking_enabled: None,
                include_coauthored_by: None,
                skip_dangerous_mode_permission_prompt: None,
            }),
            group_id: None,
            created_at: "2026-03-26T00:00:00Z".to_string(),
            updated_at: "2026-03-26T00:00:00Z".to_string(),
        };
        let content = serde_json::to_string_pretty(&json!({
            "profiles": {
                "workspace-profile": {
                    "apiKey": "secret"
                }
            }
        }))
        .expect("auth profiles content should serialize");

        let err = write_agent_config_source_with_targets(
            None,
            vec![target],
            "openclaw-target",
            &content,
            Some(SOURCE_AUTH_PROFILES),
        )
        .expect_err("write should fail when related models file is invalid");

        assert!(err.to_string().contains("models.json"));
        assert!(err.to_string().contains("parse JSON config failed"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn openclaw_parse_editor_config_preserves_multifile_state() {
        let primary_root = json!({
            "agents": {
                "defaults": {"model": {"primary": "gpt-4.1", "fallbacks": ["gpt-4.1-mini"]}}
            },
            "models": {
                "providers": {
                    "aor_shared": {
                        "api": "openai-responses",
                        "baseUrl": "http://127.0.0.1:8899/oc/dev/v1"
                    }
                }
            }
        })
        .as_object()
        .expect("primary root object")
        .clone();
        let models_root = json!({
            "providers": {
                "aor_shared": {
                    "apiKey": "token-from-registry",
                    "baseUrl": "http://override.local/v1"
                }
            }
        })
        .as_object()
        .expect("models root object")
        .clone();

        let editor = parse_openclaw_editor_config(&primary_root, Some(&models_root), None, None)
            .expect("openclaw editor parse should succeed");

        assert_eq!(editor.provider_id, "aor_shared");
        assert_eq!(editor.base_url.as_deref(), Some("http://override.local/v1"));
        assert_eq!(editor.api_key.as_deref(), Some("token-from-registry"));
        assert_eq!(editor.primary_model.as_deref(), Some("gpt-4.1"));
        assert_eq!(editor.fallback_models, vec!["gpt-4.1-mini".to_string()]);
    }

    #[test]
    fn openclaw_format_primary_source_updates_primary_file_only() {
        let mut primary_root = json!({
            "agents": {
                "defaults": {"model": {"primary": "gpt-4.1", "fallbacks": ["gpt-4.1-mini"]}}
            },
            "models": {
                "providers": {
                    "aor_shared": {
                        "api": "openai-responses",
                        "baseUrl": "http://legacy.local/v1"
                    }
                }
            }
        })
        .as_object()
        .expect("primary root object")
        .clone();
        let models_root = json!({
            "providers": {
                "aor_shared": {
                    "apiKey": "keep-me",
                    "baseUrl": "http://registry.local/v1"
                }
            }
        })
        .as_object()
        .expect("models root object")
        .clone();
        let editor = OpenClawEditorConfig {
            agent_id: "workspace-alpha".to_string(),
            provider_id: "aor_shared".to_string(),
            primary_model: Some("gpt-4.1-updated".to_string()),
            fallback_models: vec!["gpt-4.1-mini".to_string(), "gpt-4o-mini".to_string()],
            api_format: Some("openai-responses".to_string()),
            base_url: Some("http://127.0.0.1:8899/oc/dev/v1".to_string()),
            api_key: Some("ignored-for-primary".to_string()),
        };

        format_openclaw_primary_source(&editor, &mut primary_root)
            .expect("primary source formatting should succeed");

        assert_eq!(
            primary_root["models"]["providers"]["aor_shared"]["baseUrl"].as_str(),
            Some("http://127.0.0.1:8899/oc/dev/v1")
        );
        assert_eq!(
            primary_root["agents"]["defaults"]["model"]["primary"].as_str(),
            Some("gpt-4.1-updated")
        );
        assert_eq!(
            models_root["providers"]["aor_shared"]["apiKey"].as_str(),
            Some("keep-me")
        );
        assert_eq!(
            models_root["providers"]["aor_shared"]["baseUrl"].as_str(),
            Some("http://registry.local/v1")
        );
    }

    #[test]
    fn openclaw_config_merges_primary_and_agent_registry_sources() {
        let primary_root = json!({
            "agents": {
                "defaults": {
                    "model": {
                        "primary": "gpt-4.1",
                        "fallbacks": ["gpt-4.1-mini", "gpt-4o-mini"]
                    }
                }
            },
            "models": {
                "providers": {
                    "aor_shared": {
                        "api": "openai-responses",
                        "baseUrl": "http://127.0.0.1:8899/oc/dev/v1"
                    }
                }
            }
        })
        .as_object()
        .expect("primary root object")
        .clone();
        let models_root = json!({
            "providers": {
                "aor_shared": {
                    "apiKey": "token-from-registry",
                    "baseUrl": "http://override.local/v1"
                }
            }
        })
        .as_object()
        .expect("models root object")
        .clone();
        let auth_profiles_root = json!({
            "profiles": {
                "aor_shared": {
                    "apiKey": "token-from-profile"
                }
            }
        })
        .as_object()
        .expect("auth profiles object")
        .clone();
        let existing = AgentConfig {
            agent_id: Some("workspace-alpha".to_string()),
            provider_id: Some("aor_shared".to_string()),
            url: None,
            api_token: None,
            api_format: None,
            model: None,
            fallback_models: None,
            timeout: None,
            always_thinking_enabled: None,
            include_coauthored_by: None,
            skip_dangerous_mode_permission_prompt: None,
        };

        let parsed = parse_openclaw_config_with_sources(
            &primary_root,
            Some(&models_root),
            Some(&auth_profiles_root),
            Some(&existing),
        )
        .expect("openclaw parse should succeed");

        assert_eq!(parsed.agent_id.as_deref(), Some("workspace-alpha"));
        assert_eq!(parsed.provider_id.as_deref(), Some("aor_shared"));
        assert_eq!(parsed.url.as_deref(), Some("http://override.local/v1"));
        assert_eq!(parsed.api_token.as_deref(), Some("token-from-registry"));
        assert_eq!(parsed.api_format.as_deref(), Some("openai-responses"));
        assert_eq!(parsed.model.as_deref(), Some("gpt-4.1"));
        assert_eq!(
            parsed.fallback_models,
            Some(vec!["gpt-4.1-mini".to_string(), "gpt-4o-mini".to_string()])
        );
    }

    #[test]
    fn read_openclaw_agent_config_includes_editor_payload_in_serialized_result() {
        let unique_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time must move forward")
            .as_nanos();
        let temp_dir = std::env::temp_dir().join(format!("oc-proxy-openclaw-read-{unique_id}"));
        let agent_dir = temp_dir
            .join("agents")
            .join("workspace-alpha")
            .join("agent");
        std::fs::create_dir_all(&agent_dir).expect("agent dir should be created");

        std::fs::write(
            temp_dir.join("openclaw.json"),
            serde_json::to_string_pretty(&json!({
                "agents": {
                    "defaults": {
                        "model": {
                            "primary": "gpt-4.1",
                            "fallbacks": ["gpt-4.1-mini", "gpt-4o-mini"]
                        }
                    }
                },
                "models": {
                    "providers": {
                        "aor_shared": {
                            "api": "openai-responses",
                            "baseUrl": "http://127.0.0.1:8899/oc/dev/v1"
                        }
                    }
                }
            }))
            .expect("openclaw primary config should serialize"),
        )
        .expect("openclaw primary config should be written");

        std::fs::write(
            agent_dir.join("models.json"),
            serde_json::to_string_pretty(&json!({
                "providers": {
                    "aor_shared": {
                        "apiKey": "registry-token",
                        "baseUrl": "http://override.local/v1"
                    }
                }
            }))
            .expect("openclaw models registry should serialize"),
        )
        .expect("models registry should be written");

        std::fs::write(
            agent_dir.join("auth-profiles.json"),
            serde_json::to_string_pretty(&json!({
                "profiles": {
                    "workspace-profile": {
                        "apiKey": "profile-token"
                    }
                }
            }))
            .expect("openclaw auth profiles should serialize"),
        )
        .expect("auth profiles should be written");

        let result = read_agent_config_with_targets(
            vec![IntegrationTarget {
                id: "openclaw-target".to_string(),
                kind: IntegrationClientKind::Openclaw,
                config_dir: temp_dir.to_string_lossy().to_string(),
                config: Some(AgentConfig {
                    agent_id: Some("workspace-alpha".to_string()),
                    provider_id: Some("aor_shared".to_string()),
                    url: None,
                    api_token: None,
                    api_format: None,
                    model: None,
                    fallback_models: None,
                    timeout: None,
                    always_thinking_enabled: None,
                    include_coauthored_by: None,
                    skip_dangerous_mode_permission_prompt: None,
                }),
                group_id: None,
                created_at: "2026-03-26T00:00:00Z".to_string(),
                updated_at: "2026-03-26T00:00:00Z".to_string(),
            }],
            "openclaw-target",
        )
        .expect("openclaw config should read successfully");

        assert_eq!(
            result
                .openclaw_editor
                .as_ref()
                .map(|value| value.agent_id.as_str()),
            Some("workspace-alpha")
        );
        assert_eq!(
            result
                .openclaw_editor
                .as_ref()
                .map(|value| value.provider_id.as_str()),
            Some("aor_shared")
        );
        assert_eq!(
            result
                .openclaw_editor
                .as_ref()
                .and_then(|value| value.primary_model.as_deref()),
            Some("gpt-4.1")
        );
        assert_eq!(
            result
                .openclaw_editor
                .as_ref()
                .map(|value| value.fallback_models.clone()),
            Some(vec!["gpt-4.1-mini".to_string(), "gpt-4o-mini".to_string()])
        );
        assert_eq!(
            result
                .openclaw_editor
                .as_ref()
                .and_then(|value| value.api_format.as_deref()),
            Some("openai-responses")
        );
        assert_eq!(
            result
                .openclaw_editor
                .as_ref()
                .and_then(|value| value.base_url.as_deref()),
            Some("http://override.local/v1")
        );
        assert_eq!(
            result
                .openclaw_editor
                .as_ref()
                .and_then(|value| value.api_key.as_deref()),
            Some("registry-token")
        );

        let serialized = serde_json::to_value(&result).expect("agent config file should serialize");
        assert_eq!(
            serialized["openclawEditor"]["agentId"].as_str(),
            Some("workspace-alpha")
        );
        assert_eq!(
            serialized["openclawEditor"]["providerId"].as_str(),
            Some("aor_shared")
        );
        assert_eq!(
            serialized["openclawEditor"]["primaryModel"].as_str(),
            Some("gpt-4.1")
        );
        assert_eq!(
            serialized["openclawEditor"]["fallbackModels"][0].as_str(),
            Some("gpt-4.1-mini")
        );
        assert_eq!(
            serialized["openclawEditor"]["apiFormat"].as_str(),
            Some("openai-responses")
        );
        assert_eq!(
            serialized["openclawEditor"]["baseUrl"].as_str(),
            Some("http://override.local/v1")
        );
        assert_eq!(
            serialized["openclawEditor"]["apiKey"].as_str(),
            Some("registry-token")
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn write_openclaw_full_config_syncs_models_registry() {
        let unique_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time must move forward")
            .as_nanos();
        let temp_dir = std::env::temp_dir().join(format!("oc-proxy-openclaw-{unique_id}"));
        std::fs::create_dir_all(&temp_dir).expect("temp dir should be created");

        let config = AgentConfig {
            agent_id: Some("workspace-alpha".to_string()),
            provider_id: Some("aor_shared".to_string()),
            url: Some("http://127.0.0.1:8899/oc/dev/v1".to_string()),
            api_token: Some("local-openclaw-key".to_string()),
            api_format: Some("openai-responses".to_string()),
            model: Some("gpt-4.1".to_string()),
            fallback_models: Some(vec!["gpt-4.1-mini".to_string(), "gpt-4o-mini".to_string()]),
            timeout: None,
            always_thinking_enabled: None,
            include_coauthored_by: None,
            skip_dangerous_mode_permission_prompt: None,
        };

        let file_path =
            write_openclaw_full_config(&temp_dir, &config).expect("openclaw write should succeed");
        assert_eq!(file_path, temp_dir.join("openclaw.json"));

        let raw = std::fs::read_to_string(&file_path).expect("read openclaw.json");
        let root = serde_json::from_str::<Value>(&raw).expect("openclaw.json must be valid");
        assert_eq!(
            root["models"]["providers"]["aor_shared"]["baseUrl"].as_str(),
            Some("http://127.0.0.1:8899/oc/dev/v1")
        );
        assert_eq!(
            root["models"]["providers"]["aor_shared"]["apiKey"].as_str(),
            Some("local-openclaw-key")
        );
        assert_eq!(
            root["agents"]["defaults"]["model"]["primary"].as_str(),
            Some("gpt-4.1")
        );
        assert_eq!(
            root["agents"]["defaults"]["model"]["fallbacks"][0].as_str(),
            Some("gpt-4.1-mini")
        );

        let models_path = temp_dir
            .join("agents")
            .join("workspace-alpha")
            .join("agent")
            .join("models.json");
        let models_raw = std::fs::read_to_string(&models_path).expect("read models.json");
        let models_root = serde_json::from_str::<Value>(&models_raw).expect("models.json valid");
        assert_eq!(
            models_root["providers"]["aor_shared"]["baseUrl"].as_str(),
            Some("http://127.0.0.1:8899/oc/dev/v1")
        );
        assert_eq!(
            models_root["providers"]["aor_shared"]["apiKey"].as_str(),
            Some("local-openclaw-key")
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn write_openclaw_config_preserves_registry_credentials_on_group_write() {
        let unique_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time must move forward")
            .as_nanos();
        let temp_dir = std::env::temp_dir().join(format!("oc-proxy-openclaw-write-{unique_id}"));
        let agent_dir = temp_dir
            .join("agents")
            .join("workspace-alpha")
            .join("agent");
        std::fs::create_dir_all(&agent_dir).expect("agent dir should be created");

        let openclaw_path = temp_dir.join("openclaw.json");
        std::fs::write(
            &openclaw_path,
            serde_json::to_string_pretty(&json!({
                "models": {
                    "providers": {
                        "aor_shared": {
                            "api": "openai-responses",
                            "baseUrl": "http://legacy.local/v1"
                        }
                    }
                }
            }))
            .expect("serialize openclaw.json"),
        )
        .expect("seed openclaw.json");

        let models_path = agent_dir.join("models.json");
        std::fs::write(
            &models_path,
            serde_json::to_string_pretty(&json!({
                "providers": {
                    "aor_shared": {
                        "api": "openai-responses",
                        "baseUrl": "http://legacy.local/v1",
                        "apiKey": "keep-me",
                        "authProfile": "workspace-profile"
                    }
                }
            }))
            .expect("serialize models.json"),
        )
        .expect("seed models.json");

        let existing = Some(AgentConfig {
            agent_id: Some("workspace-alpha".to_string()),
            provider_id: Some("aor_shared".to_string()),
            url: None,
            api_token: None,
            api_format: None,
            model: None,
            fallback_models: None,
            timeout: None,
            always_thinking_enabled: None,
            include_coauthored_by: None,
            skip_dangerous_mode_permission_prompt: None,
        });

        write_openclaw_config(&temp_dir, &existing, "http://127.0.0.1:8899/oc/dev")
            .expect("group write should succeed");

        let models_raw = std::fs::read_to_string(&models_path).expect("read models.json");
        let models_root = serde_json::from_str::<Value>(&models_raw).expect("models json valid");
        let provider = models_root["providers"]["aor_shared"]
            .as_object()
            .expect("provider object");
        assert_eq!(
            provider.get("baseUrl").and_then(|value| value.as_str()),
            Some("http://127.0.0.1:8899/oc/dev/v1")
        );
        assert_eq!(
            provider.get("apiKey").and_then(|value| value.as_str()),
            Some("keep-me")
        );
        assert_eq!(
            provider.get("authProfile").and_then(|value| value.as_str()),
            Some("workspace-profile")
        );
        assert!(!provider.contains_key("baseURL"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}

/// Reads Agent configuration file content.
pub fn read_agent_config(state: &SharedState, target_id: &str) -> AppResult<AgentConfigFile> {
    let targets = list_targets(state);
    read_agent_config_with_targets(targets, target_id)
}

/// Reads Agent configuration file content using explicit target list.
pub fn read_agent_config_with_targets(
    targets: Vec<IntegrationTarget>,
    target_id: &str,
) -> AppResult<AgentConfigFile> {
    let target = resolve_target_by_id(&targets, target_id)?;

    let config_dir = PathBuf::from(&target.config_dir);

    // Normalize config_dir for WSL paths
    let config_dir = match normalize_wsl_path(&config_dir) {
        Some(normalized) => normalized,
        None => config_dir,
    };

    let file_path = resolve_agent_config_file_path(&target.kind, &config_dir)?;
    let content = read_file_content(&file_path)?.unwrap_or_default();
    let parsed_root = parse_agent_config_content(&target.kind, &content, &file_path).ok();
    let source_files = build_agent_source_files(
        &target,
        &config_dir,
        &file_path,
        &content,
        parsed_root.as_ref(),
    )?;
    let parsed_config = match target.kind {
        IntegrationClientKind::Codex => {
            let auth_file_path = resolve_codex_auth_file_path(&config_dir);
            let auth_content = read_file_content(&auth_file_path)?.unwrap_or_default();
            let auth_root = parse_codex_auth_root(&auth_content, &auth_file_path).ok();

            if let Some(root) = parsed_root.as_ref() {
                parse_codex_config_with_auth(root, auth_root.as_ref()).ok()
            } else {
                None
            }
        }
        IntegrationClientKind::Openclaw => {
            let agent_id = resolve_openclaw_agent_id(&target);
            let models_file_path = resolve_openclaw_models_file_path(&config_dir, &agent_id)?;
            let models_content = read_file_content(&models_file_path)?.unwrap_or_default();
            let models_root = parse_json_like_content(&models_content, &models_file_path).ok();
            let auth_profiles_file_path =
                resolve_openclaw_auth_profiles_file_path(&config_dir, &agent_id)?;
            let auth_profiles_content =
                read_file_content(&auth_profiles_file_path)?.unwrap_or_default();
            let auth_profiles_root =
                parse_json_like_content(&auth_profiles_content, &auth_profiles_file_path).ok();

            if let Some(root) = parsed_root.as_ref() {
                parse_openclaw_editor_config(
                    root,
                    models_root.as_ref(),
                    auth_profiles_root.as_ref(),
                    target.config.as_ref(),
                )
                .ok()
                .map(openclaw_editor_to_agent_config)
            } else {
                None
            }
        }
        _ => parsed_root
            .as_ref()
            .and_then(|root| parse_agent_config(&target.kind, root).ok()),
    };

    let openclaw_editor = match target.kind {
        IntegrationClientKind::Openclaw => {
            let agent_id = resolve_openclaw_agent_id(&target);
            let models_file_path = resolve_openclaw_models_file_path(&config_dir, &agent_id)?;
            let models_content = read_file_content(&models_file_path)?.unwrap_or_default();
            let models_root = parse_json_like_content(&models_content, &models_file_path).ok();
            let auth_profiles_file_path =
                resolve_openclaw_auth_profiles_file_path(&config_dir, &agent_id)?;
            let auth_profiles_content =
                read_file_content(&auth_profiles_file_path)?.unwrap_or_default();
            let auth_profiles_root =
                parse_json_like_content(&auth_profiles_content, &auth_profiles_file_path).ok();

            parsed_root.as_ref().and_then(|root| {
                parse_openclaw_editor_config(
                    root,
                    models_root.as_ref(),
                    auth_profiles_root.as_ref(),
                    target.config.as_ref(),
                )
                .ok()
                .map(openclaw_editor_to_dto)
            })
        }
        _ => None,
    };

    Ok(AgentConfigFile {
        target_id: target.id,
        kind: target.kind,
        config_dir: target.config_dir,
        file_path: file_path.to_string_lossy().to_string(),
        content,
        source_files,
        updated_at: Some(target.updated_at),
        parsed_config,
        openclaw_editor,
    })
}

fn resolve_agent_config_file_path(
    kind: &IntegrationClientKind,
    config_dir: &Path,
) -> AppResult<PathBuf> {
    match kind {
        IntegrationClientKind::Claude => Ok(config_dir.join("settings.json")),
        IntegrationClientKind::Codex => Ok(config_dir.join("config.toml")),
        IntegrationClientKind::Openclaw => Ok(config_dir.join("openclaw.json")),
        IntegrationClientKind::Opencode => resolve_opencode_config_path(config_dir),
    }
}

fn resolve_codex_auth_file_path(config_dir: &Path) -> PathBuf {
    config_dir.join("auth.json")
}

fn resolve_agent_source_file_path(
    target: &IntegrationTarget,
    config_dir: &Path,
    source_id: Option<&str>,
) -> AppResult<PathBuf> {
    if let Some(source) = source_id.map(str::trim).filter(|value| !value.is_empty()) {
        return match (&target.kind, source) {
            (IntegrationClientKind::Codex, SOURCE_AUTH) => {
                Ok(resolve_codex_auth_file_path(config_dir))
            }
            (IntegrationClientKind::Openclaw, SOURCE_AUTH_PROFILES) => {
                Ok(resolve_openclaw_auth_profiles_file_path(
                    config_dir,
                    &resolve_openclaw_agent_id(target),
                )?)
            }
            (IntegrationClientKind::Openclaw, SOURCE_MODELS) => Ok(
                resolve_openclaw_models_file_path(config_dir, &resolve_openclaw_agent_id(target))?,
            ),
            (_, SOURCE_PRIMARY) => resolve_agent_config_file_path(&target.kind, config_dir),
            (_, _) => Err(AppError::validation(format!(
                "unsupported source id: {source}"
            ))),
        };
    }
    resolve_agent_config_file_path(&target.kind, config_dir)
}

fn build_agent_source_files(
    target: &IntegrationTarget,
    config_dir: &Path,
    primary_file_path: &Path,
    primary_content: &str,
    _primary_root: Option<&Map<String, Value>>,
) -> AppResult<Vec<AgentSourceFile>> {
    let mut files = vec![AgentSourceFile {
        source_id: SOURCE_PRIMARY.to_string(),
        label: primary_file_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("config")
            .to_string(),
        file_path: primary_file_path.to_string_lossy().to_string(),
        content: primary_content.to_string(),
    }];

    if matches!(target.kind, IntegrationClientKind::Codex) {
        let auth_file_path = resolve_codex_auth_file_path(config_dir);
        let auth_content = read_file_content(&auth_file_path)?.unwrap_or_default();
        files.push(AgentSourceFile {
            source_id: SOURCE_AUTH.to_string(),
            label: "auth.json".to_string(),
            file_path: auth_file_path.to_string_lossy().to_string(),
            content: auth_content,
        });
    }

    if matches!(target.kind, IntegrationClientKind::Openclaw) {
        let agent_id = resolve_openclaw_agent_id(target);
        let auth_profiles_file_path =
            resolve_openclaw_auth_profiles_file_path(config_dir, &agent_id)?;
        let auth_profiles_content =
            read_file_content(&auth_profiles_file_path)?.unwrap_or_default();
        files.push(AgentSourceFile {
            source_id: SOURCE_AUTH_PROFILES.to_string(),
            label: "auth-profiles.json".to_string(),
            file_path: auth_profiles_file_path.to_string_lossy().to_string(),
            content: auth_profiles_content,
        });

        let models_file_path = resolve_openclaw_models_file_path(config_dir, &agent_id)?;
        let models_content = read_file_content(&models_file_path)?.unwrap_or_default();
        files.push(AgentSourceFile {
            source_id: SOURCE_MODELS.to_string(),
            label: "models.json".to_string(),
            file_path: models_file_path.to_string_lossy().to_string(),
            content: models_content,
        });
    }

    Ok(files)
}

fn parse_agent_config_content(
    kind: &IntegrationClientKind,
    content: &str,
    file_path: &Path,
) -> AppResult<Map<String, Value>> {
    match kind {
        IntegrationClientKind::Claude
        | IntegrationClientKind::Openclaw
        | IntegrationClientKind::Opencode => parse_json_like_content(content, file_path),
        IntegrationClientKind::Codex => {
            let doc = parse_toml_content(content, file_path)?;
            Ok(toml_to_map(&doc))
        }
    }
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
        IntegrationClientKind::Openclaw => {
            parse_openclaw_config_with_sources(root, None, None, None)
        }
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
        agent_id: None,
        provider_id: None,
        url,
        api_token,
        api_format: None,
        model,
        fallback_models: None,
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
    let options = aor_shared
        .and_then(|a| a.get("options"))
        .and_then(|o| o.as_object());
    let url = aor_shared
        .and_then(|a| a.get("options"))
        .and_then(|o| o.get("baseURL"))
        .and_then(|v| v.as_str())
        .map(String::from);
    let api_token = options
        .and_then(|o| o.get("apiKey").or_else(|| o.get("api_key")))
        .or_else(|| aor_shared.and_then(|a| a.get("apiKey").or_else(|| a.get("api_key"))))
        .and_then(|v| v.as_str())
        .map(String::from);
    let timeout = aor_shared
        .and_then(|a| a.get("options"))
        .and_then(|o| o.get("timeout"))
        .and_then(|v| v.as_u64());
    let model = root.get("model").and_then(|v| v.as_str()).map(String::from);

    Ok(AgentConfig {
        agent_id: None,
        provider_id: Some("aor_shared".to_string()),
        url,
        api_token,
        api_format: None,
        model,
        fallback_models: None,
        timeout,
        always_thinking_enabled: None,
        include_coauthored_by: None,
        skip_dangerous_mode_permission_prompt: None,
    })
}

/// Parses Codex config from JSON-like Map.
fn parse_codex_config(root: &Map<String, Value>) -> AppResult<AgentConfig> {
    parse_codex_config_with_auth(root, None)
}

fn parse_codex_config_with_auth(
    config_root: &Map<String, Value>,
    auth_root: Option<&Map<String, Value>>,
) -> AppResult<AgentConfig> {
    let model_providers = config_root
        .get("model_providers")
        .and_then(|v| v.as_object());
    let provider_name = resolve_codex_provider_name_from_map(config_root);
    let provider = provider_name
        .as_deref()
        .and_then(|name| model_providers.and_then(|providers| providers.get(name)))
        .and_then(|v| v.as_object());

    let url = provider
        .and_then(|a| a.get("base_url"))
        .and_then(|v| v.as_str())
        .map(String::from);
    let legacy_token = provider
        .and_then(|a| a.get("api_key"))
        .and_then(|v| v.as_str());
    let api_token = auth_root
        .and_then(|auth| auth.get("OPENAI_API_KEY"))
        .and_then(|v| v.as_str())
        .or(legacy_token)
        .map(String::from);
    let model = config_root
        .get("model")
        .and_then(|v| v.as_str())
        .map(String::from);

    Ok(AgentConfig {
        agent_id: None,
        provider_id: provider_name,
        url,
        api_token,
        api_format: None,
        model,
        fallback_models: None,
        timeout: None,
        always_thinking_enabled: None,
        include_coauthored_by: None,
        skip_dangerous_mode_permission_prompt: None,
    })
}

fn parse_codex_auth_root(content: &str, file_path: &Path) -> AppResult<Map<String, Value>> {
    parse_json_like_content(content, file_path)
}

/// Writes Agent configuration to file.
pub fn write_agent_config(
    state: &SharedState,
    target_id: &str,
    config: AgentConfig,
) -> AppResult<WriteAgentConfigResult> {
    let targets = list_targets(state);
    write_agent_config_with_targets(Some(state), targets, target_id, config)
}

/// Writes Agent configuration to file using explicit target list.
pub fn write_agent_config_with_targets(
    state: Option<&SharedState>,
    targets: Vec<IntegrationTarget>,
    target_id: &str,
    config: AgentConfig,
) -> AppResult<WriteAgentConfigResult> {
    let target = resolve_target_by_id(&targets, target_id)?;

    let config_dir = PathBuf::from(&target.config_dir);

    // Normalize WSL path
    let config_dir = match normalize_wsl_path(&config_dir) {
        Some(normalized) => normalized,
        None => config_dir,
    };

    let file_path = match target.kind {
        IntegrationClientKind::Claude => write_claude_full_config(&config_dir, &config)?,
        IntegrationClientKind::Openclaw => write_openclaw_full_config(&config_dir, &config)?,
        IntegrationClientKind::Opencode => write_opencode_full_config(&config_dir, &config)?,
        IntegrationClientKind::Codex => write_codex_full_config(&config_dir, &config)?,
    };

    if let Some(state) = state {
        let _ = state.integration_store.put_target(
            target_id,
            target.kind.clone(),
            target.config_dir,
            Some(config),
        );
    }

    Ok(WriteAgentConfigResult {
        ok: true,
        target_id: target_id.to_string(),
        file_path: file_path.to_string_lossy().to_string(),
        message: None,
    })
}

/// Writes raw agent configuration source to file and refreshes parsed store config.
pub fn write_agent_config_source(
    state: &SharedState,
    target_id: &str,
    content: &str,
    source_id: Option<&str>,
) -> AppResult<WriteAgentConfigResult> {
    let targets = list_targets(state);
    write_agent_config_source_with_targets(Some(state), targets, target_id, content, source_id)
}

/// Writes raw agent configuration source to file using explicit target list.
pub fn write_agent_config_source_with_targets(
    state: Option<&SharedState>,
    targets: Vec<IntegrationTarget>,
    target_id: &str,
    content: &str,
    source_id: Option<&str>,
) -> AppResult<WriteAgentConfigResult> {
    let target = resolve_target_by_id(&targets, target_id)?;

    let config_dir = PathBuf::from(&target.config_dir);
    let config_dir = match normalize_wsl_path(&config_dir) {
        Some(normalized) => normalized,
        None => config_dir,
    };

    let normalized_source_id = source_id.map(str::trim).filter(|value| !value.is_empty());
    let file_path = resolve_agent_source_file_path(&target, &config_dir, normalized_source_id)?;
    let parsed_root = if matches!(target.kind, IntegrationClientKind::Codex)
        && normalized_source_id == Some(SOURCE_AUTH)
    {
        parse_codex_auth_root(content, &file_path)?
    } else {
        parse_agent_config_content(&target.kind, content, &file_path)?
    };
    if !matches!(target.kind, IntegrationClientKind::Openclaw) {
        write_file_content(&file_path, content)?;
    }

    let parsed_config = match target.kind {
        IntegrationClientKind::Codex => {
            let config_file_path = resolve_agent_config_file_path(&target.kind, &config_dir)?;
            let config_content = read_file_content(&config_file_path)?.unwrap_or_default();
            let config_root =
                parse_agent_config_content(&target.kind, &config_content, &config_file_path).ok();

            let auth_file_path = resolve_codex_auth_file_path(&config_dir);
            let auth_root = if normalized_source_id == Some(SOURCE_AUTH) {
                Some(parsed_root)
            } else {
                let auth_content = read_file_content(&auth_file_path)?.unwrap_or_default();
                parse_codex_auth_root(&auth_content, &auth_file_path).ok()
            };

            config_root
                .as_ref()
                .and_then(|root| parse_codex_config_with_auth(root, auth_root.as_ref()).ok())
        }
        IntegrationClientKind::Openclaw => {
            let config_file_path = resolve_agent_config_file_path(&target.kind, &config_dir)?;
            let config_root = if normalized_source_id == Some(SOURCE_PRIMARY) {
                parsed_root.clone()
            } else {
                let config_content = read_file_content(&config_file_path)?.unwrap_or_default();
                parse_agent_config_content(&target.kind, &config_content, &config_file_path)?
            };

            let agent_id = resolve_openclaw_agent_id(&target);
            let models_file_path = resolve_openclaw_models_file_path(&config_dir, &agent_id)?;
            let models_root = if normalized_source_id == Some(SOURCE_MODELS) {
                parsed_root.clone()
            } else {
                let models_content = read_file_content(&models_file_path)?.unwrap_or_default();
                parse_json_like_content(&models_content, &models_file_path)?
            };

            let auth_profiles_file_path =
                resolve_openclaw_auth_profiles_file_path(&config_dir, &agent_id)?;
            let auth_profiles_root = if normalized_source_id == Some(SOURCE_AUTH_PROFILES) {
                parsed_root
            } else {
                let auth_profiles_content =
                    read_file_content(&auth_profiles_file_path)?.unwrap_or_default();
                parse_json_like_content(&auth_profiles_content, &auth_profiles_file_path)?
            };

            let editor = parse_openclaw_editor_config(
                &config_root,
                Some(&models_root),
                Some(&auth_profiles_root),
                target.config.as_ref(),
            )?;
            validate_openclaw_editor_config(
                &editor,
                &config_root,
                Some(&models_root),
                Some(&auth_profiles_root),
            )?;

            write_file_content(&file_path, content)?;

            parse_openclaw_editor_config(
                &config_root,
                Some(&models_root),
                Some(&auth_profiles_root),
                target.config.as_ref(),
            )
            .ok()
            .map(openclaw_editor_to_agent_config)
        }
        _ => parse_agent_config(&target.kind, &parsed_root).ok(),
    };
    if let Some(state) = state {
        let _ = state.integration_store.put_target(
            target_id,
            target.kind.clone(),
            target.config_dir,
            parsed_config,
        );
    }

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
    match config
        .api_token
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(token) => {
            options.insert("apiKey".to_string(), Value::String(token.to_string()));
            options.remove("api_key");
        }
        None => {
            options.remove("apiKey");
            options.remove("api_key");
        }
    }

    // Write model at root level
    if let Some(model) = &config.model {
        root.insert("model".to_string(), Value::String(model.clone()));
    }

    write_json_object(&file_path, &root)?;
    Ok(file_path)
}

fn write_openclaw_full_config(config_dir: &Path, config: &AgentConfig) -> AppResult<PathBuf> {
    let file_path = config_dir.join("openclaw.json");
    let mut root = read_json_like_object(&file_path)?;
    let editor = openclaw_editor_from_agent_config(config);
    format_openclaw_primary_source(&editor, &mut root)?;
    if let Some(token) = editor
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let providers = ensure_openclaw_primary_provider_map(&mut root);
        let provider = ensure_child_object(providers, &editor.provider_id);
        provider.insert("apiKey".to_string(), Value::String(token.to_string()));
        provider.remove("api_key");
    }
    let provider_snapshot = build_openclaw_provider_snapshot(&editor, true);

    write_json_object(&file_path, &root)?;
    sync_openclaw_provider_to_models_file(
        config_dir,
        &editor.agent_id,
        &editor.provider_id,
        &provider_snapshot,
        false,
    )?;

    Ok(file_path)
}

fn write_codex_full_config(config_dir: &Path, config: &AgentConfig) -> AppResult<PathBuf> {
    let file_path = config_dir.join("config.toml");
    let mut doc = read_toml_document(&file_path)?;
    let provider_name = resolve_codex_provider_name_from_doc(&doc)?;

    if !doc["model_providers"].is_table() {
        doc["model_providers"] = Item::Table(Table::new());
    }
    if !doc["model_providers"][&provider_name].is_table() {
        doc["model_providers"][&provider_name] = Item::Table(Table::new());
    }

    if let Some(url) = &config.url {
        doc["model_providers"][&provider_name]["base_url"] = value(url);
    }
    // auth token is persisted in auth.json (OPENAI_API_KEY), not in config.toml.
    if let Some(table) = doc["model_providers"][&provider_name].as_table_mut() {
        table.remove("api_key");
    }
    if let Some(model) = &config.model {
        doc["model"] = value(model);
    }

    let mut output = doc.to_string();
    if !output.ends_with('\n') {
        output.push('\n');
    }
    write_file_content(&file_path, &output)?;

    let auth_file_path = resolve_codex_auth_file_path(config_dir);
    let mut auth_root = read_json_like_object(&auth_file_path)?;
    match config.api_token.as_deref() {
        Some(token) if !token.trim().is_empty() => {
            auth_root.insert(
                "OPENAI_API_KEY".to_string(),
                Value::String(token.trim().to_string()),
            );
        }
        _ => {
            auth_root.remove("OPENAI_API_KEY");
        }
    }
    write_json_object(&auth_file_path, &auth_root)?;

    Ok(file_path)
}
