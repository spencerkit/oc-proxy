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

pub(super) fn resolve_upstream_path(target_protocol: &RuleProtocol) -> &'static str {
    match target_protocol {
        RuleProtocol::Anthropic => "/v1/messages",
        RuleProtocol::Openai => "/v1/responses",
        RuleProtocol::OpenaiCompletion => "/v1/chat/completions",
    }
}

pub(super) fn resolve_upstream_url(
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

pub(super) fn build_rule_headers(protocol: &RuleProtocol, rule: &Rule) -> HashMap<String, String> {
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

pub(super) fn build_route_index(config: &ProxyConfig) -> RouteIndex {
    let mut index = HashMap::with_capacity(config.groups.len());
    for group in &config.groups {
        let resolution = match group.active_rule_id.as_ref() {
            Some(active_rule_id) => {
                match group.rules.iter().find(|rule| rule.id == *active_rule_id) {
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

fn is_model_match(candidate: &str, requested: &str) -> bool {
    if candidate == requested {
        return true;
    }

    if let Some(prefix) = candidate.strip_suffix('*') {
        return !prefix.is_empty() && requested.starts_with(prefix);
    }

    requested.starts_with(candidate) && requested.as_bytes().get(candidate.len()) == Some(&b'-')
}
