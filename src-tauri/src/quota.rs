//! Module Overview
//! Quota probing workflow and response normalization.
//! Executes provider checks, parses payloads, and updates quota snapshot status.

mod parser;

use crate::models::{
    Group, ProxyConfig, QuotaStatus, Rule, RuleQuotaSnapshot, RuleQuotaTestResult,
};
use chrono::Utc;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::{Client, Method};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

const QUOTA_TIMEOUT_SECONDS: u64 = 12;
const QUOTA_LOG_BODY_MAX_CHARS: usize = 12 * 1024;

struct FetchRuleQuotaResult {
    snapshot: RuleQuotaSnapshot,
    raw_response: Option<Value>,
}

/// Performs quota dev log enabled.
fn quota_dev_log_enabled() -> bool {
    cfg!(debug_assertions)
}

/// Performs clip for log.
fn clip_for_log(raw: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for (idx, ch) in raw.chars().enumerate() {
        if idx >= max_chars {
            out.push_str("...(truncated)");
            break;
        }
        out.push(ch);
    }
    out
}

/// Performs headers for log.
fn headers_for_log(headers: &HeaderMap) -> Value {
    let mut pairs: Vec<(String, String)> = Vec::new();
    for (key, value) in headers {
        let key_text = key.as_str().to_string();
        let value_text = value
            .to_str()
            .map(|v| v.to_string())
            .unwrap_or_else(|_| "<non-utf8>".to_string());
        pairs.push((key_text, value_text));
    }
    pairs.sort_by(|a, b| a.0.cmp(&b.0));

    let mut map = serde_json::Map::new();
    for (k, v) in pairs {
        map.insert(k, Value::String(v));
    }
    Value::Object(map)
}

/// Performs log quota event.
fn log_quota_event(group: &Group, rule: &Rule, stage: &str, details: Value) {
    if !quota_dev_log_enabled() {
        return;
    }
    let pretty = serde_json::to_string_pretty(&details).unwrap_or_else(|_| details.to_string());
    eprintln!(
        "[quota][{stage}] group_id={} group_name={} rule_id={} rule_name={} provider={}\n{}",
        group.id, group.name, rule.id, rule.name, rule.quota.provider, pretty
    );
}

/// Performs body to value for debug.
fn body_to_value_for_debug(raw: &str) -> Value {
    if raw.trim().is_empty() {
        Value::String("<empty>".to_string())
    } else {
        serde_json::from_str::<Value>(raw).unwrap_or_else(|_| Value::String(raw.to_string()))
    }
}

/// Fetchs rule quota for this module's workflow.
pub async fn fetch_rule_quota(
    config: &ProxyConfig,
    group_id: &str,
    rule_id: &str,
) -> Result<RuleQuotaSnapshot, String> {
    let group = config
        .groups
        .iter()
        .find(|g| g.id == group_id)
        .ok_or_else(|| format!("group not found: {group_id}"))?;
    let rule = group
        .providers
        .iter()
        .find(|r| r.id == rule_id)
        .ok_or_else(|| format!("rule not found: {rule_id}"))?;

    Ok(fetch_single_rule_quota(group, rule, false).await.snapshot)
}

/// Fetchs group quotas for this module's workflow.
pub async fn fetch_group_quotas(
    config: &ProxyConfig,
    group_id: &str,
) -> Result<Vec<RuleQuotaSnapshot>, String> {
    let group = config
        .groups
        .iter()
        .find(|g| g.id == group_id)
        .ok_or_else(|| format!("group not found: {group_id}"))?;

    let mut snapshots = Vec::with_capacity(group.providers.len());
    for rule in &group.providers {
        snapshots.push(fetch_single_rule_quota(group, rule, false).await.snapshot);
    }
    Ok(snapshots)
}

/// Runs a unit test for the expected behavior contract.
pub async fn test_rule_quota_draft(group: &Group, rule: &Rule) -> RuleQuotaTestResult {
    let result = fetch_single_rule_quota(group, rule, true).await;
    let snapshot = result.snapshot;
    let ok = matches!(
        snapshot.status,
        QuotaStatus::Ok | QuotaStatus::Low | QuotaStatus::Empty
    );
    let message = if ok {
        None
    } else {
        snapshot
            .message
            .clone()
            .or_else(|| Some(default_test_failure_message(&snapshot.status)))
    };

    RuleQuotaTestResult {
        ok,
        snapshot: Some(snapshot),
        raw_response: result.raw_response,
        message,
    }
}

/// Performs default test failure message.
fn default_test_failure_message(status: &QuotaStatus) -> String {
    match status {
        QuotaStatus::Unknown => "remaining quota mapping returned empty result".to_string(),
        QuotaStatus::Unsupported => "quota query disabled".to_string(),
        QuotaStatus::Error => "quota query failed".to_string(),
        _ => "quota query failed".to_string(),
    }
}

/// Fetchs single rule quota for this module's workflow.
async fn fetch_single_rule_quota(
    group: &Group,
    rule: &Rule,
    include_raw_response: bool,
) -> FetchRuleQuotaResult {
    let started_at = Instant::now();
    let mut snapshot = new_snapshot(group, rule);
    if !rule.quota.enabled {
        snapshot.status = QuotaStatus::Unsupported;
        snapshot.message = Some("quota query disabled".to_string());
        log_quota_event(
            group,
            rule,
            "skip",
            json!({
                "message": "quota query disabled",
                "enabled": false
            }),
        );
        return FetchRuleQuotaResult {
            snapshot,
            raw_response: None,
        };
    }

    let endpoint = render_template(group, rule, &rule.quota.endpoint);
    if endpoint.trim().is_empty() {
        snapshot.status = QuotaStatus::Error;
        snapshot.message = Some("quota endpoint is empty".to_string());
        log_quota_event(
            group,
            rule,
            "error",
            json!({
                "message": "quota endpoint is empty"
            }),
        );
        return FetchRuleQuotaResult {
            snapshot,
            raw_response: None,
        };
    }

    let client = match Client::builder()
        .timeout(Duration::from_secs(QUOTA_TIMEOUT_SECONDS))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            snapshot.status = QuotaStatus::Error;
            snapshot.message = Some(format!("create quota http client failed: {error}"));
            log_quota_event(
                group,
                rule,
                "error",
                json!({
                    "message": "create quota http client failed",
                    "error": error.to_string()
                }),
            );
            return FetchRuleQuotaResult {
                snapshot,
                raw_response: None,
            };
        }
    };

    let method_name = normalize_method_name(&rule.quota.method);
    let method = match Method::from_bytes(method_name.as_bytes()) {
        Ok(method) => method,
        Err(_) => {
            snapshot.status = QuotaStatus::Error;
            snapshot.message = Some(format!("invalid quota method: {method_name}"));
            log_quota_event(
                group,
                rule,
                "error",
                json!({
                    "message": "invalid quota method",
                    "method": method_name
                }),
            );
            return FetchRuleQuotaResult {
                snapshot,
                raw_response: None,
            };
        }
    };

    let headers = match build_headers(group, rule) {
        Ok(headers) => headers,
        Err(error) => {
            snapshot.status = QuotaStatus::Error;
            snapshot.message = Some(error);
            log_quota_event(
                group,
                rule,
                "error",
                json!({
                    "message": "build quota headers failed",
                    "error": snapshot.message.clone()
                }),
            );
            return FetchRuleQuotaResult {
                snapshot,
                raw_response: None,
            };
        }
    };

    log_quota_event(
        group,
        rule,
        "start",
        json!({
            "message": "quota query start",
            "requestAddress": endpoint,
            "requestMethod": method_name,
            "requestHeaders": headers_for_log(&headers),
            "requestBody": "<empty>"
        }),
    );

    let mut request = client.request(method, endpoint.clone());
    if !headers.is_empty() {
        request = request.headers(headers);
    }

    let response = match request.send().await {
        Ok(response) => response,
        Err(error) => {
            snapshot.status = QuotaStatus::Error;
            snapshot.message = Some(format!("quota request failed: {error}"));
            log_quota_event(
                group,
                rule,
                "error",
                json!({
                    "message": "quota request failed",
                    "requestAddress": endpoint,
                    "requestMethod": method_name,
                    "elapsedMs": started_at.elapsed().as_millis(),
                    "error": error.to_string()
                }),
            );
            return FetchRuleQuotaResult {
                snapshot,
                raw_response: None,
            };
        }
    };

    let status = response.status();
    let response_headers = response.headers().clone();
    let response_body = response.text().await.unwrap_or_default();

    log_quota_event(
        group,
        rule,
        "response",
        json!({
            "requestAddress": endpoint,
            "requestMethod": method_name,
            "httpStatus": status.as_u16(),
            "elapsedMs": started_at.elapsed().as_millis(),
            "responseHeaders": headers_for_log(&response_headers),
            "responseBody": body_to_value_for_debug(&clip_for_log(&response_body, QUOTA_LOG_BODY_MAX_CHARS))
        }),
    );

    let raw_response = if include_raw_response {
        Some(body_to_value_for_debug(&response_body))
    } else {
        None
    };

    if !status.is_success() {
        snapshot.status = QuotaStatus::Error;
        snapshot.message = Some(format!(
            "quota endpoint returned HTTP {}{}",
            status.as_u16(),
            render_body_suffix(&response_body)
        ));
        return FetchRuleQuotaResult {
            snapshot,
            raw_response,
        };
    }

    let payload = match serde_json::from_str::<Value>(&response_body) {
        Ok(payload) => payload,
        Err(error) => {
            snapshot.status = QuotaStatus::Error;
            snapshot.message = Some(format!("invalid quota response JSON: {error}"));
            log_quota_event(
                group,
                rule,
                "error",
                json!({
                    "message": "invalid quota response JSON",
                    "requestAddress": endpoint,
                    "requestMethod": method_name,
                    "error": error.to_string(),
                    "responseBody": clip_for_log(&response_body, QUOTA_LOG_BODY_MAX_CHARS)
                }),
            );
            return FetchRuleQuotaResult {
                snapshot,
                raw_response,
            };
        }
    };

    match map_payload_to_snapshot(&mut snapshot, rule, &payload) {
        Ok(()) => {
            log_quota_event(
                group,
                rule,
                "finish",
                json!({
                    "status": "ok",
                    "snapshot": snapshot.clone(),
                    "elapsedMs": started_at.elapsed().as_millis()
                }),
            );
            FetchRuleQuotaResult {
                snapshot,
                raw_response,
            }
        }
        Err(error) => {
            snapshot.status = QuotaStatus::Error;
            snapshot.message = Some(error);
            log_quota_event(
                group,
                rule,
                "error",
                json!({
                    "message": "map payload to snapshot failed",
                    "snapshot": snapshot.clone(),
                    "elapsedMs": started_at.elapsed().as_millis()
                }),
            );
            FetchRuleQuotaResult {
                snapshot,
                raw_response,
            }
        }
    }
}

/// Performs new snapshot.
fn new_snapshot(group: &Group, rule: &Rule) -> RuleQuotaSnapshot {
    RuleQuotaSnapshot {
        group_id: group.id.clone(),
        rule_id: rule.id.clone(),
        provider: if rule.quota.provider.trim().is_empty() {
            "custom".to_string()
        } else {
            rule.quota.provider.trim().to_string()
        },
        status: QuotaStatus::Unknown,
        remaining: None,
        total: None,
        percent: None,
        unit: None,
        reset_at: None,
        fetched_at: Utc::now().to_rfc3339(),
        message: None,
    }
}

/// Normalizes method name for this module's workflow.
fn normalize_method_name(method: &str) -> String {
    let trimmed = method.trim();
    if trimmed.is_empty() {
        "GET".to_string()
    } else {
        trimmed.to_ascii_uppercase()
    }
}

/// Performs render template.
fn render_template(group: &Group, rule: &Rule, raw: &str) -> String {
    let resolved_token = if rule.quota.use_rule_token {
        rule.token.as_str()
    } else {
        rule.quota.custom_token.as_str()
    };

    raw.replace("{{group.id}}", &group.id)
        .replace("{{group.name}}", &group.name)
        .replace("{{rule.id}}", &rule.id)
        .replace("{{rule.name}}", &rule.name)
        .replace("{{rule.apiAddress}}", &rule.api_address)
        .replace("{{rule.defaultModel}}", &rule.default_model)
        .replace("{{rule.token}}", &rule.token)
        .replace("{{quota.token}}", resolved_token)
}

/// Builds headers.
fn build_headers(group: &Group, rule: &Rule) -> Result<HeaderMap, String> {
    let mut headers = HeaderMap::new();

    for (key, value) in &rule.quota.custom_headers {
        let key_name = HeaderName::from_bytes(key.trim().as_bytes())
            .map_err(|_| format!("invalid quota header name: {key}"))?;
        let rendered = render_template(group, rule, value);
        let header_value = HeaderValue::from_str(rendered.trim())
            .map_err(|_| format!("invalid quota header value for {key}"))?;
        headers.insert(key_name, header_value);
    }

    let auth_header_name = rule.quota.auth_header.trim();
    let resolved_token = if rule.quota.use_rule_token {
        rule.token.trim()
    } else {
        rule.quota.custom_token.trim()
    };

    if !auth_header_name.is_empty() && !resolved_token.is_empty() {
        let key_name = HeaderName::from_bytes(auth_header_name.as_bytes())
            .map_err(|_| format!("invalid auth header name: {auth_header_name}"))?;
        if !headers.contains_key(&key_name) {
            let auth_value = if rule.quota.auth_scheme.trim().is_empty() {
                resolved_token.to_string()
            } else {
                format!("{} {}", rule.quota.auth_scheme.trim(), resolved_token)
            };
            let header_value = HeaderValue::from_str(auth_value.trim())
                .map_err(|_| "invalid auth header value".to_string())?;
            headers.insert(key_name, header_value);
        }
    }

    Ok(headers)
}

/// Performs render body suffix.
fn render_body_suffix(raw_body: &str) -> String {
    let trimmed = raw_body.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let max_len = 140usize;
    let shown = trimmed.chars().take(max_len).collect::<String>();
    if trimmed.chars().count() > max_len {
        format!(": {shown}...")
    } else {
        format!(": {shown}")
    }
}

/// Maps payload to snapshot for this module's workflow.
fn map_payload_to_snapshot(
    snapshot: &mut RuleQuotaSnapshot,
    rule: &Rule,
    payload: &Value,
) -> Result<(), String> {
    let parsed = parser::parse_quota_payload(rule, payload)?;
    snapshot.remaining = parsed.remaining;
    snapshot.total = parsed.total;
    snapshot.unit = parsed.unit;
    snapshot.reset_at = parsed.reset_at;
    snapshot.percent = parsed.percent;
    snapshot.status = parsed.status;
    Ok(())
}
