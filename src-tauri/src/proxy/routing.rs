//! Module Overview
//! Path and rule resolution helpers for proxy routing.
//! Normalizes entry endpoints, selects upstream protocol paths, and computes final upstream URL.

use super::ServiceState;
use crate::models::{ProxyConfig, Rule, RuleProtocol};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use url::Url;

#[derive(Clone, Copy)]
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
    pub group_name: String,
    pub group_models: Vec<String>,
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
                        group_name: group.name.clone(),
                        group_models: group.models.clone(),
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
