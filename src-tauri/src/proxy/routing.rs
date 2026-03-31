//! Module Overview
//! Path and rule resolution helpers for proxy routing.
//! Normalizes entry endpoints, selects upstream protocol paths, and computes final upstream URL.

use super::failover::{self, FailoverConfigSnapshot, FailoverRouteDecision};
use super::ServiceState;
use crate::models::{GroupFailoverConfig, ProxyConfig, Rule, RuleProtocol};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use url::Url;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum EntryProtocol {
    Openai,
    Anthropic,
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum EntryEndpoint {
    ChatCompletions,
    Responses,
    Messages,
}

pub(super) struct ParsedPath {
    pub group_id: String,
    pub suffix: String,
}

pub(super) struct PathEntry {
    pub protocol: EntryProtocol,
    pub endpoint: EntryEndpoint,
}

#[derive(Clone)]
pub(super) struct ActiveRoute {
    pub group_id: String,
    pub group_name: String,
    pub group_models: Vec<String>,
    pub provider_ids: Vec<String>,
    pub preferred_provider_id: String,
    pub providers_by_id: HashMap<String, Rule>,
    pub failover: GroupFailoverConfig,
    pub rule: Rule,
}

#[derive(Clone)]
pub(super) enum RouteResolution {
    Ready(ActiveRoute),
    NoActiveRule {
        group_name: String,
    },
    MissingActiveRule {
        group_name: String,
        active_rule_id: String,
    },
}

pub(super) type RouteIndex = HashMap<String, RouteResolution>;

/// Detect downstream request protocol/endpoint from `/oc/:group/*suffix`.
///
/// Compatibility rules:
/// - Supports both `/messages` and `/v1/messages` for Anthropic clients.
/// - Supports both `/chat/completions` and `/v1/chat/completions` for OpenAI chat clients.
/// - Supports both `/responses` and `/v1/responses` for OpenAI responses clients.
/// - Empty suffix defaults to chat-completions for backward compatibility with `/oc/:group`.
pub(super) fn detect_entry_protocol(suffix: &str) -> Option<PathEntry> {
    let normalized = if suffix.is_empty() || suffix == "/" {
        "/chat/completions".to_string()
    } else {
        let mut s = suffix.to_string();
        while s.ends_with('/') && s.len() > 1 {
            s.pop();
        }
        if !s.starts_with('/') {
            s = format!("/{s}");
        }
        s
    };

    match normalized.as_str() {
        "/messages" | "/v1/messages" => Some(PathEntry {
            protocol: EntryProtocol::Anthropic,
            endpoint: EntryEndpoint::Messages,
        }),
        "/chat/completions" | "/v1/chat/completions" => Some(PathEntry {
            protocol: EntryProtocol::Openai,
            endpoint: EntryEndpoint::ChatCompletions,
        }),
        "/responses" | "/v1/responses" => Some(PathEntry {
            protocol: EntryProtocol::Openai,
            endpoint: EntryEndpoint::Responses,
        }),
        _ => None,
    }
}

/// Resolve default upstream endpoint path from the active rule protocol.
///
/// Note that OpenAI paths intentionally omit `/v1` so callers can control versioning
/// via `rule.apiAddress` (for example `https://host` vs `https://host/v1`).
pub(crate) fn resolve_upstream_path(target_protocol: &RuleProtocol) -> &'static str {
    match target_protocol {
        RuleProtocol::Anthropic => "/v1/messages",
        RuleProtocol::Openai => "/responses",
        RuleProtocol::OpenaiCompletion => "/chat/completions",
    }
}

/// Build the final upstream URL from `rule.apiAddress` and protocol default path.
///
/// Behavior summary:
/// - If `apiAddress` has no path, use `default_path` directly.
/// - If `apiAddress` already includes a prefix path (for example `/v1`),
///   append default path under that prefix (`/v1` + `/responses` => `/v1/responses`).
/// - If `default_path` already starts with the prefix path, do not duplicate it.
pub(crate) fn resolve_upstream_url(
    api_address: &str,
    default_path: &str,
) -> Result<String, String> {
    let mut url = Url::parse(api_address)
        .map_err(|_| "rule.apiAddress must be a valid absolute URL".to_string())?;

    let base_path = if url.path().is_empty() || url.path() == "/" {
        String::new()
    } else {
        url.path().trim_end_matches('/').to_string()
    };

    if base_path.is_empty() {
        url.set_path(default_path);
        return Ok(url.to_string());
    }

    if default_path == base_path || default_path.starts_with(&(base_path.clone() + "/")) {
        url.set_path(default_path);
        return Ok(url.to_string());
    }

    url.set_path(&format!("{base_path}{default_path}"));
    Ok(url.to_string())
}

/// Build outbound request headers for the selected upstream protocol.
///
/// - Anthropic uses `x-api-key` + `Anthropic-version`.
/// - OpenAI surfaces use standard `Authorization: Bearer ...`.
pub(crate) fn build_rule_headers(protocol: &RuleProtocol, rule: &Rule) -> HashMap<String, String> {
    let mut headers = HashMap::new();
    headers.insert("content-type".to_string(), "application/json".to_string());
    match protocol {
        RuleProtocol::Anthropic => {
            headers.insert("x-api-key".to_string(), rule.token.clone());
            headers.insert("anthropic-version".to_string(), "2023-06-01".to_string());
        }
        RuleProtocol::Openai | RuleProtocol::OpenaiCompletion => {
            headers.insert(
                "authorization".to_string(),
                format!("Bearer {}", rule.token),
            );
        }
    }
    headers
}

/// Build final outbound request headers, optionally enabling safe passthrough.
pub(super) fn build_forward_headers(
    entry_protocol: EntryProtocol,
    target_protocol: &RuleProtocol,
    rule: &Rule,
    downstream_headers: &axum::http::HeaderMap,
    header_passthrough_enabled: bool,
) -> HashMap<String, String> {
    let mut forwarded_headers = HashMap::new();
    let allow_set = normalized_header_set(&rule.header_passthrough_allow);
    let deny_set = normalized_header_set(&rule.header_passthrough_deny);
    let mut passthrough_anthropic_version = None;

    if header_passthrough_enabled {
        for (name, value) in downstream_headers {
            let normalized_name = normalize_header_name(name.as_str());
            if normalized_name.is_empty() || deny_set.contains(&normalized_name) {
                continue;
            }

            let normalized_value = match value.to_str() {
                Ok(raw) => raw.trim(),
                Err(_) => continue,
            };
            if normalized_value.is_empty() {
                continue;
            }

            if normalized_name == "anthropic-version" {
                if should_passthrough_anthropic_version(
                    entry_protocol,
                    target_protocol,
                    &allow_set,
                    normalized_value,
                ) {
                    passthrough_anthropic_version = Some(normalized_value.to_string());
                }
                continue;
            }

            if is_hard_blocked_passthrough_header(&normalized_name) {
                continue;
            }

            forwarded_headers.insert(normalized_name, normalized_value.to_string());
        }
    }

    let mut rule_headers = build_rule_headers(target_protocol, rule);
    if let Some(version) = passthrough_anthropic_version {
        rule_headers.insert("anthropic-version".to_string(), version);
    }
    forwarded_headers.extend(rule_headers);
    forwarded_headers
}

fn normalize_header_name(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn normalized_header_set(values: &[String]) -> std::collections::HashSet<String> {
    values
        .iter()
        .map(|value| normalize_header_name(value))
        .filter(|value| !value.is_empty())
        .collect()
}

fn is_hard_blocked_passthrough_header(name: &str) -> bool {
    matches!(
        name,
        "accept"
            | "accept-encoding"
            | "anthropic-beta"
            | "anthropic-dangerous-direct-browser-access"
            | "api-key"
            | "authorization"
            | "connection"
            | "content-encoding"
            | "content-length"
            | "cookie"
            | "forwarded"
            | "host"
            | "keep-alive"
            | "openai-organization"
            | "openai-project"
            | "origin"
            | "proxy-authorization"
            | "proxy-connection"
            | "referer"
            | "set-cookie"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
            | "via"
            | "x-api-key"
            | "x-real-ip"
    ) || name.starts_with("cf-")
        || name.starts_with("sec-")
        || name.starts_with("x-forwarded-")
}

fn should_passthrough_anthropic_version(
    entry_protocol: EntryProtocol,
    target_protocol: &RuleProtocol,
    allow_set: &std::collections::HashSet<String>,
    value: &str,
) -> bool {
    entry_protocol == EntryProtocol::Anthropic
        && *target_protocol == RuleProtocol::Anthropic
        && allow_set.contains("anthropic-version")
        && is_valid_anthropic_version(value)
}

fn is_valid_anthropic_version(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
}

/// Refresh in-memory route index when config revision changes.
///
/// This keeps hot-path routing lock-free from full config traversal while still
/// reacting to runtime config updates.
pub(super) fn refresh_route_index_if_needed(state: &ServiceState) -> Result<(), String> {
    let observed_revision = state.config_revision.load(Ordering::Acquire);
    let cached_revision = state.route_index_revision.load(Ordering::Acquire);
    if observed_revision == cached_revision {
        return Ok(());
    }

    let next_index = state
        .config
        .read()
        .map_err(|_| "config lock poisoned".to_string())
        .map(|cfg| build_route_index(&cfg))?;

    let mut guard = state
        .route_index
        .write()
        .map_err(|_| "route index lock poisoned".to_string())?;
    *guard = next_index;
    state
        .route_index_revision
        .store(observed_revision, Ordering::Release);

    Ok(())
}

/// Select the current route provider for a group using runtime failover state.
pub(super) fn select_route_provider(
    state: &ServiceState,
    group_id: &str,
    preferred_provider_id: &str,
    provider_ids: &[String],
    config: &FailoverConfigSnapshot,
) -> Result<FailoverRouteDecision, String> {
    state
        .failover_state
        .write()
        .map_err(|_| "failover state lock poisoned".to_string())
        .map(|mut runtime| {
            failover::select_provider(
                &mut runtime,
                group_id,
                preferred_provider_id,
                provider_ids,
                config,
            )
        })
}

/// Record a provider-side failure for one group/provider pair.
pub(super) fn record_route_provider_failure(
    state: &ServiceState,
    group_id: &str,
    provider_id: &str,
    provider_ids: &[String],
    config: &FailoverConfigSnapshot,
) -> Result<(), String> {
    state
        .failover_state
        .write()
        .map_err(|_| "failover state lock poisoned".to_string())
        .map(|mut runtime| {
            failover::record_provider_failure(
                &mut runtime,
                group_id,
                provider_id,
                provider_ids,
                config,
                chrono::Utc::now(),
            )
        })
}

/// Record a successful provider request and reset that provider's consecutive failures.
pub(super) fn record_route_provider_success(
    state: &ServiceState,
    group_id: &str,
    provider_id: &str,
) -> Result<(), String> {
    state
        .failover_state
        .write()
        .map_err(|_| "failover state lock poisoned".to_string())
        .map(|mut runtime| failover::record_provider_success(&mut runtime, group_id, provider_id))
}

/// Check whether the active failover cooldown has expired for a group.
pub(super) fn failover_cooldown_expired(
    state: &ServiceState,
    group_id: &str,
) -> Result<bool, String> {
    state
        .failover_state
        .read()
        .map_err(|_| "failover state lock poisoned".to_string())
        .map(|runtime| {
            failover::is_failover_cooldown_expired(&runtime, group_id, chrono::Utc::now())
        })
}

pub(super) fn resolve_runtime_active_route(
    state: &ServiceState,
    route: &ActiveRoute,
) -> Result<ActiveRoute, String> {
    let failover_config = FailoverConfigSnapshot {
        enabled: route.failover.enabled,
        failure_threshold: route.failover.failure_threshold,
        cooldown_seconds: route.failover.cooldown_seconds,
    };
    let decision = select_route_provider(
        state,
        &route.group_id,
        &route.preferred_provider_id,
        &route.provider_ids,
        &failover_config,
    )?;
    let rule = route
        .providers_by_id
        .get(&decision.provider_id)
        .cloned()
        .ok_or_else(|| format!("Failover provider {} is missing", decision.provider_id))?;

    let mut resolved = route.clone();
    resolved.rule = rule;
    Ok(resolved)
}

/// Build a fast lookup table `group_id -> active route resolution`.
///
/// The index carries three states so request handling can distinguish:
/// - group exists with ready active provider,
/// - group exists but no active provider configured,
/// - group exists but active_provider_id points to a missing provider.
pub(super) fn build_route_index(config: &ProxyConfig) -> RouteIndex {
    let mut index = HashMap::with_capacity(config.groups.len());
    for group in &config.groups {
        let resolution = match group.active_provider_id.as_ref() {
            Some(active_rule_id) => {
                match group
                    .providers
                    .iter()
                    .find(|rule| rule.id == *active_rule_id)
                {
                    Some(rule) => RouteResolution::Ready(ActiveRoute {
                        group_id: group.id.clone(),
                        group_name: group.name.clone(),
                        group_models: group.models.clone(),
                        provider_ids: group.provider_ids.clone(),
                        preferred_provider_id: active_rule_id.clone(),
                        providers_by_id: group
                            .providers
                            .iter()
                            .cloned()
                            .map(|provider| (provider.id.clone(), provider))
                            .collect(),
                        failover: group.failover.clone(),
                        rule: rule.clone(),
                    }),
                    None => RouteResolution::MissingActiveRule {
                        group_name: group.name.clone(),
                        active_rule_id: active_rule_id.clone(),
                    },
                }
            }
            None => RouteResolution::NoActiveRule {
                group_name: group.name.clone(),
            },
        };
        index.insert(group.id.clone(), resolution);
    }
    index
}

/// Validate required active-rule fields before forwarding traffic.
pub(super) fn assert_rule_ready(rule: &Rule) -> Result<(), (u16, String)> {
    if rule.name.trim().is_empty() {
        return Err((409, "Active rule name is empty".into()));
    }
    if rule.default_model.trim().is_empty() {
        return Err((409, "Active rule defaultModel is empty".into()));
    }
    if rule.token.trim().is_empty() {
        return Err((409, "Active rule token is empty".into()));
    }
    if rule.api_address.trim().is_empty() {
        return Err((409, "Active rule apiAddress is empty".into()));
    }
    Ok(())
}

/// Resolve forwarded model name using request model, group allow-list, and rule mappings.
///
/// Resolution order:
/// 1. requested model if present and allowed by group model list,
/// 2. exact/normalized rule model mapping,
/// 3. rule default model.
pub(super) fn resolve_target_model(
    rule: &Rule,
    group_models: &[String],
    request_body: &Value,
) -> String {
    let requested = request_body
        .get("model")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    if let Some(model) = requested {
        if let Some(matched_model) = find_group_model_match(group_models, &model) {
            return rule
                .model_mappings
                .get(&model)
                .cloned()
                .or_else(|| rule.model_mappings.get(matched_model).cloned())
                .unwrap_or(model);
        }
    }

    rule.default_model.clone()
}

/// Finds the best matching group model pattern for a requested model string.
fn find_group_model_match<'a>(group_models: &'a [String], requested: &str) -> Option<&'a str> {
    let mut best: Option<&str> = None;
    for model in group_models {
        let candidate = model.trim();
        if candidate.is_empty() {
            continue;
        }
        if !is_model_match(candidate, requested) {
            continue;
        }
        if best
            .map(|curr| candidate.len() > curr.len())
            .unwrap_or(true)
        {
            best = Some(candidate);
        }
    }
    best
}

/// Returns true when `candidate` fuzzily matches `requested` (case-insensitive).
fn is_model_match(candidate: &str, requested: &str) -> bool {
    let candidate = candidate.trim();
    let requested = requested.trim();
    if candidate.is_empty() || requested.is_empty() {
        return false;
    }

    if candidate == requested {
        return true;
    }

    let candidate_lower = candidate.to_ascii_lowercase();
    let requested_lower = requested.to_ascii_lowercase();

    requested_lower.contains(&candidate_lower)
}
