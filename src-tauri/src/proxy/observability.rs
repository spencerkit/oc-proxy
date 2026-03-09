//! Module Overview
//! Observability helpers for metrics, response headers, and request chain logs.
//! Provides token usage extraction and unified error response construction.

use super::routing::{EntryProtocol, ParsedPath, PathEntry};
use super::ServiceState;
use axum::{
    http::HeaderMap,
    response::{IntoResponse, Response},
};
use chrono::Utc;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;

use crate::models::{
    default_metrics, CostSnapshot, LogEntry, LogEntryError, ProxyMetrics, Rule, RuleProtocol,
    TokenUsage,
};

pub(super) struct MetricsState {
    requests: AtomicU64,
    stream_requests: AtomicU64,
    errors: AtomicU64,
    total_latency_ms: AtomicU64,
    input_tokens: AtomicU64,
    output_tokens: AtomicU64,
    cache_read_tokens: AtomicU64,
    cache_write_tokens: AtomicU64,
    uptime_started_at: RwLock<Option<String>>,
}

impl MetricsState {
    /// Performs new.
    pub(super) fn new() -> Self {
        Self {
            requests: AtomicU64::new(0),
            stream_requests: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            total_latency_ms: AtomicU64::new(0),
            input_tokens: AtomicU64::new(0),
            output_tokens: AtomicU64::new(0),
            cache_read_tokens: AtomicU64::new(0),
            cache_write_tokens: AtomicU64::new(0),
            uptime_started_at: RwLock::new(None),
        }
    }

    /// Read a consistent metrics snapshot for API responses/UI polling.
    pub(super) fn snapshot(&self) -> ProxyMetrics {
        let requests = self.requests.load(Ordering::Relaxed);
        let total_latency_ms = self.total_latency_ms.load(Ordering::Relaxed);
        let avg_latency_ms = if requests == 0 {
            0
        } else {
            total_latency_ms / requests
        };
        let mut metrics = default_metrics();
        metrics.requests = requests;
        metrics.stream_requests = self.stream_requests.load(Ordering::Relaxed);
        metrics.errors = self.errors.load(Ordering::Relaxed);
        metrics.avg_latency_ms = avg_latency_ms;
        metrics.input_tokens = self.input_tokens.load(Ordering::Relaxed);
        metrics.output_tokens = self.output_tokens.load(Ordering::Relaxed);
        metrics.cache_read_tokens = self.cache_read_tokens.load(Ordering::Relaxed);
        metrics.cache_write_tokens = self.cache_write_tokens.load(Ordering::Relaxed);
        metrics.uptime_started_at = self
            .uptime_started_at
            .read()
            .map(|v| v.clone())
            .unwrap_or(None);
        metrics
    }

    /// Performs mark started.
    pub(super) fn mark_started(&self) {
        if let Ok(mut guard) = self.uptime_started_at.write() {
            *guard = Some(Utc::now().to_rfc3339());
        }
    }

    /// Performs mark stopped.
    pub(super) fn mark_stopped(&self) {
        if let Ok(mut guard) = self.uptime_started_at.write() {
            *guard = None;
        }
    }

    /// Record one request and optionally one stream request.
    pub(super) fn increment_request(&self, stream: bool) {
        let _ = self.requests.fetch_add(1, Ordering::Relaxed);
        if stream {
            let _ = self.stream_requests.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Record one request-level error.
    pub(super) fn increment_error(&self) {
        let _ = self.errors.fetch_add(1, Ordering::Relaxed);
    }

    /// Add elapsed latency to running aggregate used for average latency reporting.
    pub(super) fn add_latency(&self, elapsed_ms: u64) {
        let _ = self
            .total_latency_ms
            .fetch_add(elapsed_ms, Ordering::Relaxed);
    }

    /// Aggregate token counters from one completed request/stream.
    pub(super) fn add_token_usage(&self, usage: &TokenUsage) {
        let _ = self
            .input_tokens
            .fetch_add(usage.input_tokens, Ordering::Relaxed);
        let _ = self
            .output_tokens
            .fetch_add(usage.output_tokens, Ordering::Relaxed);
        let _ = self
            .cache_read_tokens
            .fetch_add(usage.cache_read_tokens, Ordering::Relaxed);
        let _ = self
            .cache_write_tokens
            .fetch_add(usage.cache_write_tokens, Ordering::Relaxed);
    }
}

#[derive(Default)]
pub(super) struct StreamTokenAccumulator {
    line_buffer: String,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_write_tokens: u64,
}

impl StreamTokenAccumulator {
    /// Consume raw SSE bytes and extract best-effort usage snapshots from `data:` lines.
    pub(super) fn consume_chunk(&mut self, chunk: &[u8]) {
        self.line_buffer.push_str(&String::from_utf8_lossy(chunk));
        while let Some(newline_idx) = self.line_buffer.find('\n') {
            let mut line = self.line_buffer[..newline_idx].to_string();
            if line.ends_with('\r') {
                let _ = line.pop();
            }
            self.consume_line(&line);
            self.line_buffer.drain(..=newline_idx);
        }
    }

    /// Performs consume line.
    fn consume_line(&mut self, line: &str) {
        let Some(rest) = line.strip_prefix("data:") else {
            return;
        };
        let payload = rest.trim_start();
        if payload.is_empty() || payload == "[DONE]" {
            return;
        }

        if let Ok(parsed) = serde_json::from_str::<Value>(payload) {
            if let Some(usage) = extract_token_usage(&parsed) {
                // Stream payloads can contain repeated/cumulative usage snapshots.
                self.input_tokens = self.input_tokens.max(usage.input_tokens);
                self.output_tokens = self.output_tokens.max(usage.output_tokens);
                self.cache_read_tokens = self.cache_read_tokens.max(usage.cache_read_tokens);
                self.cache_write_tokens = self.cache_write_tokens.max(usage.cache_write_tokens);
            }
        }
    }

    /// Return final usage when at least one non-zero token dimension is observed.
    pub(super) fn into_token_usage(self) -> Option<TokenUsage> {
        if self.input_tokens == 0
            && self.output_tokens == 0
            && self.cache_read_tokens == 0
            && self.cache_write_tokens == 0
        {
            return None;
        }

        Some(TokenUsage {
            input_tokens: self.input_tokens,
            output_tokens: self.output_tokens,
            cache_read_tokens: self.cache_read_tokens,
            cache_write_tokens: self.cache_write_tokens,
        })
    }
}

/// Extract token usage from different upstream/downstream payload shapes.
///
/// Supports OpenAI/Anthropic variants and nested detail fields used by streaming events.
pub(super) fn extract_token_usage(payload: &Value) -> Option<TokenUsage> {
    let usage = payload
        .get("usage")
        .or_else(|| payload.get("response").and_then(|r| r.get("usage")))
        .or_else(|| payload.get("message").and_then(|m| m.get("usage")))
        .or_else(|| payload.get("delta").and_then(|d| d.get("usage")))?;

    let raw_input_tokens = first_u64(
        usage,
        &[
            "input_tokens",
            "prompt_tokens",
            "inputTokens",
            "promptTokens",
        ],
    );
    let output_tokens = first_u64(
        usage,
        &[
            "output_tokens",
            "completion_tokens",
            "outputTokens",
            "completionTokens",
        ],
    );
    let cache_read_tokens = first_u64(
        usage,
        &[
            "cache_read_input_tokens",
            "cache_read_tokens",
            "prompt_cache_hit_tokens",
            "cached_tokens",
        ],
    );
    let cache_write_tokens = first_u64(
        usage,
        &[
            "cache_creation_input_tokens",
            "cache_write_input_tokens",
            "prompt_cache_miss_tokens",
            "cache_creation_tokens",
        ],
    );
    let input_tokens = normalize_input_tokens(usage, raw_input_tokens, cache_read_tokens);

    if input_tokens == 0 && output_tokens == 0 && cache_read_tokens == 0 && cache_write_tokens == 0
    {
        return None;
    }

    Some(TokenUsage {
        input_tokens,
        output_tokens,
        cache_read_tokens,
        cache_write_tokens,
    })
}

/// Normalizes input tokens for this module's workflow.
fn normalize_input_tokens(usage: &Value, raw_input_tokens: u64, cache_read_tokens: u64) -> u64 {
    // OpenAI usage may report `input_tokens` including cached tokens.
    // Anthropic reports cache read separately from input.
    // To align app semantics with Anthropic, only normalize for OpenAI-style payloads.
    let has_openai_cached_details = usage
        .get("input_tokens_details")
        .or_else(|| usage.get("prompt_tokens_details"))
        .and_then(|details| details.get("cached_tokens"))
        .and_then(|value| value.as_u64())
        .is_some();
    let has_openai_prompt_fields = usage
        .get("prompt_tokens")
        .and_then(|value| value.as_u64())
        .is_some()
        || usage
            .get("completion_tokens")
            .and_then(|value| value.as_u64())
            .is_some();

    if (has_openai_cached_details || has_openai_prompt_fields)
        && raw_input_tokens >= cache_read_tokens
    {
        raw_input_tokens.saturating_sub(cache_read_tokens)
    } else {
        raw_input_tokens
    }
}

/// Return the first non-zero u64 among candidate fields; fall back to the first seen value.
///
/// This keeps canonical-field precedence while allowing alias fallback when canonical value is zero.
fn first_u64(obj: &Value, fields: &[&str]) -> u64 {
    let mut fallback: Option<u64> = None;
    for field in fields {
        if let Some(v) = obj.get(*field).and_then(|v| v.as_u64()) {
            if fallback.is_none() {
                fallback = Some(v);
            }
            if v > 0 {
                return v;
            }
        }
        if let Some(v) = obj
            .get("input_tokens_details")
            .and_then(|d| d.get(*field))
            .and_then(|v| v.as_u64())
        {
            if fallback.is_none() {
                fallback = Some(v);
            }
            if v > 0 {
                return v;
            }
        }
        if let Some(v) = obj
            .get("prompt_tokens_details")
            .and_then(|d| d.get(*field))
            .and_then(|v| v.as_u64())
        {
            if fallback.is_none() {
                fallback = Some(v);
            }
            if v > 0 {
                return v;
            }
        }
    }
    fallback.unwrap_or(0)
}

/// Headers used for JSON responses emitted by local proxy.
pub(super) fn response_headers_json(trace_id: &str) -> HashMap<String, String> {
    HashMap::from([
        (
            "content-type".into(),
            "application/json; charset=utf-8".into(),
        ),
        ("x-trace-id".into(), trace_id.to_string()),
    ])
}

/// Headers used for SSE responses emitted by local proxy.
pub(super) fn response_headers_sse(trace_id: &str) -> HashMap<String, String> {
    HashMap::from([
        (
            "content-type".into(),
            "text/event-stream; charset=utf-8".into(),
        ),
        ("cache-control".into(), "no-cache, no-transform".into()),
        ("connection".into(), "keep-alive".into()),
        ("x-accel-buffering".into(), "no".into()),
        ("x-trace-id".into(), trace_id.to_string()),
    ])
}

/// Apply header map to Axum response object, skipping invalid name/value pairs.
pub(super) fn apply_headers(resp: &mut Response, headers: &HashMap<String, String>) {
    for (k, v) in headers {
        if let (Ok(name), Ok(value)) = (
            axum::http::header::HeaderName::from_bytes(k.as_bytes()),
            axum::http::HeaderValue::from_str(v),
        ) {
            resp.headers_mut().insert(name, value);
        }
    }
}

/// Converts arbitrary header value bytes into safe log text.
fn header_value_to_text(bytes: &[u8]) -> String {
    if let Ok(text) = std::str::from_utf8(bytes) {
        return text.to_string();
    }

    const HEX: [char; 16] = [
        '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'a', 'b', 'c', 'd', 'e', 'f',
    ];
    let mut text = String::with_capacity(bytes.len() * 2 + 2);
    text.push_str("0x");
    for value in bytes {
        text.push(HEX[(value >> 4) as usize]);
        text.push(HEX[(value & 0x0f) as usize]);
    }
    text
}

/// Inserts or appends one header key/value for log map.
fn merge_header_text(target: &mut HashMap<String, String>, key: String, value: String) {
    if let Some(existing) = target.get_mut(&key) {
        existing.push_str(", ");
        existing.push_str(&value);
    } else {
        target.insert(key, value);
    }
}

/// Convert downstream request headers into lowercase plain-string map for logging.
pub(super) fn plain_downstream_headers(headers: &HeaderMap) -> HashMap<String, String> {
    let mut plain = HashMap::new();
    for (name, value) in headers {
        merge_header_text(
            &mut plain,
            name.as_str().to_ascii_lowercase(),
            header_value_to_text(value.as_bytes()),
        );
    }
    plain
}

/// Convert upstream response headers into lowercase plain-string map for logging.
pub(super) fn plain_headers(headers: &reqwest::header::HeaderMap) -> HashMap<String, String> {
    let mut plain = HashMap::new();
    for (name, value) in headers {
        merge_header_text(
            &mut plain,
            name.as_str().to_ascii_lowercase(),
            header_value_to_text(value.as_bytes()),
        );
    }
    plain
}

/// Formats entry protocol label.
fn entry_protocol_text(protocol: EntryProtocol) -> String {
    match protocol {
        EntryProtocol::Openai => "openai".to_string(),
        EntryProtocol::Anthropic => "anthropic".to_string(),
    }
}

/// Formats downstream protocol label.
fn downstream_protocol_text(protocol: &RuleProtocol) -> String {
    match protocol {
        RuleProtocol::Openai => "openai".to_string(),
        RuleProtocol::OpenaiCompletion => "openai_completion".to_string(),
        RuleProtocol::Anthropic => "anthropic".to_string(),
    }
}

/// Append a simple log line for non-forwarding endpoints such as healthz/metrics.
pub(super) fn log_simple(
    state: &ServiceState,
    trace_id: String,
    method: &str,
    request_path: &str,
    status: &str,
    http_status: Option<u16>,
    response_body: Option<Value>,
    error: Option<LogEntryError>,
) {
    let capture_body = should_capture_body(state);
    let response_body_for_log = if capture_body {
        response_body.clone()
    } else {
        None
    };
    let entry = LogEntry {
        timestamp: Utc::now().to_rfc3339(),
        trace_id,
        phase: "request_chain".to_string(),
        status: status.to_string(),
        method: method.to_string(),
        request_path: request_path.to_string(),
        request_address: format!("{} {}", method, request_path),
        client_address: None,
        group_path: None,
        group_name: None,
        rule_id: None,
        direction: None,
        entry_protocol: None,
        downstream_protocol: None,
        model: None,
        forwarded_model: None,
        forwarding_address: None,
        request_headers: None,
        forward_request_headers: None,
        upstream_response_headers: None,
        response_headers: None,
        request_body: None,
        forward_request_body: None,
        response_body: response_body_for_log,
        transformed_response_body: None,
        transform_debug: None,
        token_usage: None,
        cost_snapshot: None,
        http_status,
        upstream_status: None,
        duration_ms: 0,
        error,
    };
    let mut dev_entry = entry.clone();
    dev_entry.response_body = response_body;
    state
        .log_store
        .append_with_dev_entry(entry.clone(), Some(dev_entry));
    state.stats_store.append_log(&entry);
}

#[allow(clippy::too_many_arguments)]
/// Appends request-started log so UI can display request data before response arrives.
pub(super) fn append_processing_log(
    state: &ServiceState,
    timestamp: &str,
    trace_id: &str,
    method: &axum::http::Method,
    parsed_path: &ParsedPath,
    group_name: &str,
    rule: &Rule,
    entry: &PathEntry,
    model: Option<&str>,
    forwarded_model: Option<&str>,
    forwarding_address: Option<&str>,
    request_headers: Option<HashMap<String, String>>,
    forward_request_headers: Option<HashMap<String, String>>,
    request_body: Option<Value>,
    forward_request_body: Option<Value>,
    capture_body: bool,
) {
    append_processing_log_with_stream_debug(
        state,
        timestamp,
        trace_id,
        method,
        parsed_path,
        group_name,
        rule,
        entry,
        model,
        forwarded_model,
        forwarding_address,
        request_headers,
        forward_request_headers,
        request_body,
        forward_request_body,
        capture_body,
        None,
    );
}

#[allow(clippy::too_many_arguments)]
pub(super) fn append_processing_log_with_stream_debug(
    state: &ServiceState,
    timestamp: &str,
    trace_id: &str,
    method: &axum::http::Method,
    parsed_path: &ParsedPath,
    group_name: &str,
    rule: &Rule,
    entry: &PathEntry,
    model: Option<&str>,
    forwarded_model: Option<&str>,
    forwarding_address: Option<&str>,
    request_headers: Option<HashMap<String, String>>,
    forward_request_headers: Option<HashMap<String, String>>,
    request_body: Option<Value>,
    forward_request_body: Option<Value>,
    capture_body: bool,
    stream_debug: Option<Value>,
) {
    let request_body_for_log = if capture_body {
        request_body.clone()
    } else {
        None
    };
    let forward_request_body_for_log = if capture_body {
        forward_request_body.clone()
    } else {
        None
    };
    let entry = LogEntry {
        timestamp: timestamp.to_string(),
        trace_id: trace_id.to_string(),
        phase: "request_chain".to_string(),
        status: "processing".to_string(),
        method: method.as_str().to_string(),
        request_path: format!("/oc/{}{}", parsed_path.group_id, parsed_path.suffix),
        request_address: format!(
            "{} /oc/{}{}",
            method.as_str(),
            parsed_path.group_id,
            parsed_path.suffix
        ),
        client_address: None,
        group_path: Some(parsed_path.group_id.clone()),
        group_name: Some(group_name.to_string()),
        rule_id: Some(rule.id.clone()),
        direction: None,
        entry_protocol: Some(entry_protocol_text(entry.protocol)),
        downstream_protocol: Some(downstream_protocol_text(&rule.protocol)),
        model: model.map(|value| value.to_string()),
        forwarded_model: forwarded_model.map(|value| value.to_string()),
        forwarding_address: forwarding_address.map(|value| value.to_string()),
        request_headers,
        forward_request_headers,
        upstream_response_headers: None,
        response_headers: None,
        request_body: request_body_for_log,
        forward_request_body: forward_request_body_for_log,
        response_body: None,
        transformed_response_body: None,
        transform_debug: build_transform_debug(
            request_body.as_ref(),
            forward_request_body.as_ref(),
            None,
            None,
            stream_debug,
        ),
        token_usage: None,
        cost_snapshot: None,
        http_status: None,
        upstream_status: None,
        duration_ms: 0,
        error: None,
    };
    let mut dev_entry = entry.clone();
    dev_entry.request_body = request_body;
    dev_entry.forward_request_body = forward_request_body;
    state
        .log_store
        .upsert_by_trace_id_with_dev_entry(entry, Some(dev_entry));
}

#[allow(clippy::too_many_arguments)]
/// Append finalized request-chain log with full forwarding context.
///
/// This is called exactly once for successful flows and for handled error flows
/// where request context is available.
pub(super) fn finalize_log(
    state: &ServiceState,
    timestamp: &str,
    trace_id: &str,
    method: &axum::http::Method,
    parsed_path: &ParsedPath,
    group_name: &str,
    rule: &Rule,
    entry: &PathEntry,
    model: Option<&str>,
    forwarded_model: Option<&str>,
    forwarding_address: Option<&str>,
    request_headers: Option<HashMap<String, String>>,
    forward_request_headers: Option<HashMap<String, String>>,
    request_body: Option<Value>,
    forward_request_body: Option<Value>,
    response_body: Option<Value>,
    transformed_response_body: Option<Value>,
    debug_response_body: Option<Value>,
    http_status: Option<u16>,
    upstream_status: Option<u16>,
    upstream_headers: Option<HashMap<String, String>>,
    response_headers: Option<HashMap<String, String>>,
    token_usage: Option<TokenUsage>,
    duration_ms: u64,
    status: &str,
    capture_body: bool,
) {
    finalize_log_with_stream_debug(
        state,
        timestamp,
        trace_id,
        method,
        parsed_path,
        group_name,
        rule,
        entry,
        model,
        forwarded_model,
        forwarding_address,
        request_headers,
        forward_request_headers,
        request_body,
        forward_request_body,
        response_body,
        transformed_response_body,
        debug_response_body,
        http_status,
        upstream_status,
        upstream_headers,
        response_headers,
        token_usage,
        duration_ms,
        status,
        capture_body,
        None,
    );
}

#[allow(clippy::too_many_arguments)]
pub(super) fn finalize_log_with_stream_debug(
    state: &ServiceState,
    timestamp: &str,
    trace_id: &str,
    method: &axum::http::Method,
    parsed_path: &ParsedPath,
    group_name: &str,
    rule: &Rule,
    entry: &PathEntry,
    model: Option<&str>,
    forwarded_model: Option<&str>,
    forwarding_address: Option<&str>,
    request_headers: Option<HashMap<String, String>>,
    forward_request_headers: Option<HashMap<String, String>>,
    request_body: Option<Value>,
    forward_request_body: Option<Value>,
    response_body: Option<Value>,
    transformed_response_body: Option<Value>,
    debug_response_body: Option<Value>,
    http_status: Option<u16>,
    upstream_status: Option<u16>,
    upstream_headers: Option<HashMap<String, String>>,
    response_headers: Option<HashMap<String, String>>,
    token_usage: Option<TokenUsage>,
    duration_ms: u64,
    status: &str,
    capture_body: bool,
    stream_debug: Option<Value>,
) {
    let request_body_for_log = if capture_body {
        request_body.clone()
    } else {
        None
    };
    let forward_request_body_for_log = if capture_body {
        forward_request_body.clone()
    } else {
        None
    };
    let response_body_for_log = if capture_body {
        response_body.clone()
    } else {
        None
    };
    let transformed_response_body_for_log = if capture_body {
        transformed_response_body.clone()
    } else {
        None
    };
    let debug_response_body_for_dev = debug_response_body.or_else(|| response_body.clone());

    let cost_snapshot = token_usage
        .as_ref()
        .map(|usage| build_cost_snapshot(rule, usage));
    let transform_debug = build_transform_debug(
        request_body.as_ref(),
        forward_request_body.as_ref(),
        response_body.as_ref(),
        transformed_response_body.as_ref(),
        stream_debug,
    );

    let entry = LogEntry {
        timestamp: timestamp.to_string(),
        trace_id: trace_id.to_string(),
        phase: "request_chain".to_string(),
        status: status.to_string(),
        method: method.as_str().to_string(),
        request_path: format!("/oc/{}{}", parsed_path.group_id, parsed_path.suffix),
        request_address: format!(
            "{} /oc/{}{}",
            method.as_str(),
            parsed_path.group_id,
            parsed_path.suffix
        ),
        client_address: None,
        group_path: Some(parsed_path.group_id.clone()),
        group_name: Some(group_name.to_string()),
        rule_id: Some(rule.id.clone()),
        direction: None,
        entry_protocol: Some(entry_protocol_text(entry.protocol)),
        downstream_protocol: Some(downstream_protocol_text(&rule.protocol)),
        model: model.map(|m| m.to_string()),
        forwarded_model: forwarded_model.map(|m| m.to_string()),
        forwarding_address: forwarding_address.map(|v| v.to_string()),
        request_headers,
        forward_request_headers,
        upstream_response_headers: upstream_headers,
        response_headers,
        request_body: request_body_for_log,
        forward_request_body: forward_request_body_for_log,
        response_body: response_body_for_log,
        transformed_response_body: transformed_response_body_for_log,
        transform_debug,
        token_usage,
        cost_snapshot,
        http_status,
        upstream_status,
        duration_ms,
        error: None,
    };
    let mut dev_entry = entry.clone();
    dev_entry.request_body = request_body;
    dev_entry.forward_request_body = forward_request_body;
    dev_entry.response_body = debug_response_body_for_dev;
    dev_entry.transformed_response_body = transformed_response_body;
    state
        .log_store
        .upsert_by_trace_id_with_dev_entry(entry.clone(), Some(dev_entry));
    state.stats_store.append_log(&entry);
}

fn build_transform_debug(
    request_body: Option<&Value>,
    forward_request_body: Option<&Value>,
    response_body: Option<&Value>,
    transformed_response_body: Option<&Value>,
    stream_debug: Option<Value>,
) -> Option<Value> {
    if request_body.is_none()
        && forward_request_body.is_none()
        && response_body.is_none()
        && transformed_response_body.is_none()
        && stream_debug.is_none()
    {
        return None;
    }

    let mut debug = json!({
        "request": {
            "originalStream": request_body.and_then(|body| body.get("stream")).cloned(),
            "forwardStream": forward_request_body.and_then(|body| body.get("stream")).cloned(),
            "originalToolChoice": request_body.and_then(|body| body.get("tool_choice")).cloned(),
            "forwardToolChoice": forward_request_body.and_then(|body| body.get("tool_choice")).cloned(),
            "forwardToolCount": forward_request_body
                .and_then(|body| body.get("tools"))
                .and_then(|tools| tools.as_array())
                .map(|tools| tools.len()),
            "forwardInputCount": forward_request_body
                .and_then(|body| body.get("input"))
                .and_then(|input| input.as_array())
                .map(|input| input.len()),
            "hasPriorToolResult": request_body.map(request_has_tool_result),
        },
        "response": {
            "responseBodyKind": response_body.map(body_kind),
            "responseBodySource": response_body.and_then(body_source),
            "responseStopReason": response_body.and_then(extract_response_stop_reason),
            "responseHasFunctionCall": response_body.map(body_has_function_call),
            "responseHasToolUse": response_body.map(body_has_tool_use),
            "transformedBodyKind": transformed_response_body.map(body_kind),
            "transformedBodySource": transformed_response_body.and_then(body_source),
            "transformedStopReason": transformed_response_body.and_then(extract_response_stop_reason),
            "transformedHasFunctionCall": transformed_response_body.map(body_has_function_call),
            "transformedHasToolUse": transformed_response_body.map(body_has_tool_use),
            "responseStreamEventCount": response_body.and_then(stream_event_count),
            "transformedStreamEventCount": transformed_response_body.and_then(stream_event_count),
            "responseFirstStreamEvent": response_body.and_then(first_stream_event_name),
            "transformedFirstStreamEvent": transformed_response_body.and_then(first_stream_event_name),
            "responseFirstDataType": response_body.and_then(first_stream_data_type),
            "transformedFirstDataType": transformed_response_body.and_then(first_stream_data_type),
            "responseHasDone": response_body.map(stream_has_done),
            "transformedHasDone": transformed_response_body.map(stream_has_done),
            "transformedHasMessageStart": transformed_response_body.map(|body| stream_payload_contains(body, "\"type\":\"message_start\"")),
            "transformedHasMessageDelta": transformed_response_body.map(|body| stream_payload_contains(body, "\"type\":\"message_delta\"")),
            "transformedHasMessageStop": transformed_response_body.map(|body| stream_payload_contains(body, "\"type\":\"message_stop\"")),
            "streamPayloadDiffers": match (response_body.and_then(stream_payload), transformed_response_body.and_then(stream_payload)) {
                (Some(raw), Some(transformed)) => Some(raw != transformed),
                _ => None,
            },
        }
    });

    if let Some(stream_debug) = stream_debug {
        if let Some(object) = debug.as_object_mut() {
            object.insert("stream".to_string(), stream_debug);
        }
    }

    Some(debug)
}

fn request_has_tool_result(request_body: &Value) -> bool {
    if let Some(messages) = request_body
        .get("messages")
        .and_then(|messages| messages.as_array())
    {
        return messages.iter().any(|message| {
            message
                .get("content")
                .and_then(|content| content.as_array())
                .map(|blocks| {
                    blocks.iter().any(|block| {
                        block.get("type").and_then(|value| value.as_str()) == Some("tool_result")
                    })
                })
                .unwrap_or(false)
        });
    }

    request_body
        .get("input")
        .and_then(|input| input.as_array())
        .map(|items| {
            items.iter().any(|item| {
                item.get("type").and_then(|value| value.as_str()) == Some("function_call_output")
            })
        })
        .unwrap_or(false)
}

fn body_kind(body: &Value) -> &'static str {
    if stream_payload(body).is_some() {
        "stream"
    } else if body.is_object() || body.is_array() {
        "json"
    } else {
        "scalar"
    }
}

fn body_source(body: &Value) -> Option<String> {
    body.get("source")
        .and_then(|source| source.as_str())
        .map(|source| source.to_string())
}

fn extract_response_stop_reason(body: &Value) -> Option<String> {
    if let Some(payload) = stream_payload(body) {
        return detect_stream_stop_reason(payload).map(|reason| reason.to_string());
    }

    body.get("stop_reason")
        .or_else(|| body.get("finish_reason"))
        .and_then(|reason| reason.as_str())
        .map(|reason| reason.to_string())
}

fn body_has_function_call(body: &Value) -> bool {
    if let Some(payload) = stream_payload(body) {
        return payload.contains("\"type\":\"function_call\"")
            || payload.contains("response.function_call_arguments.delta")
            || payload.contains("response.function_call_arguments.done");
    }

    serde_json::to_string(body)
        .map(|text| {
            text.contains("\"type\":\"function_call\"")
                || text.contains("\"finish_reason\":\"tool_calls\"")
        })
        .unwrap_or(false)
}

fn body_has_tool_use(body: &Value) -> bool {
    if let Some(payload) = stream_payload(body) {
        return payload.contains("\"type\":\"tool_use\"")
            || payload.contains("\"stop_reason\":\"tool_use\"");
    }

    serde_json::to_string(body)
        .map(|text| text.contains("\"type\":\"tool_use\""))
        .unwrap_or(false)
}

fn stream_payload(body: &Value) -> Option<&str> {
    if body.get("stream").and_then(|stream| stream.as_bool()) == Some(true) {
        return body.get("payload").and_then(|payload| payload.as_str());
    }
    None
}

fn stream_event_count(body: &Value) -> Option<usize> {
    let payload = stream_payload(body)?;
    Some(split_stream_events(payload).len())
}

fn first_stream_event_name(body: &Value) -> Option<String> {
    let payload = stream_payload(body)?;
    let first = split_stream_events(payload).into_iter().next()?;
    for line in first.lines() {
        if let Some(value) = line.trim_start().strip_prefix("event:") {
            return Some(value.trim().to_string());
        }
    }
    first_stream_data_type(body)
}

fn first_stream_data_type(body: &Value) -> Option<String> {
    let payload = stream_payload(body)?;
    let first = split_stream_events(payload).into_iter().next()?;
    for line in first.lines() {
        let Some(value) = line.trim_start().strip_prefix("data:") else {
            continue;
        };
        let data = value.trim();
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        if let Ok(parsed) = serde_json::from_str::<Value>(data) {
            if let Some(kind) = parsed.get("type").and_then(|value| value.as_str()) {
                return Some(kind.to_string());
            }
        }
    }
    None
}

fn stream_has_done(body: &Value) -> bool {
    stream_payload_contains(body, "[DONE]")
}

fn stream_payload_contains(body: &Value, needle: &str) -> bool {
    stream_payload(body)
        .map(|payload| payload.contains(needle))
        .unwrap_or(false)
}

fn split_stream_events(payload: &str) -> Vec<String> {
    payload
        .replace("\r\n", "\n")
        .split("\n\n")
        .filter_map(|event| {
            let trimmed = event.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
        .collect()
}

fn detect_stream_stop_reason(payload: &str) -> Option<&'static str> {
    if payload.contains("\"stop_reason\":\"tool_use\"")
        || payload.contains("\"finish_reason\":\"tool_calls\"")
    {
        return Some("tool_use");
    }
    if payload.contains("\"stop_reason\":\"end_turn\"")
        || payload.contains("\"finish_reason\":\"stop\"")
    {
        return Some("end_turn");
    }
    None
}

/// Builds cost snapshot.
fn build_cost_snapshot(rule: &Rule, usage: &TokenUsage) -> CostSnapshot {
    let cost = &rule.cost;
    if !cost.enabled {
        return CostSnapshot {
            enabled: false,
            currency: cost.currency.clone(),
            input_price_per_m: cost.input_price_per_m,
            output_price_per_m: cost.output_price_per_m,
            cache_input_price_per_m: cost.cache_input_price_per_m,
            cache_output_price_per_m: cost.cache_output_price_per_m,
            total_cost: 0.0,
        };
    }

    let input_cost = (usage.input_tokens as f64 / 1_000_000.0) * cost.input_price_per_m;
    let output_cost = (usage.output_tokens as f64 / 1_000_000.0) * cost.output_price_per_m;
    let cache_input_cost =
        (usage.cache_read_tokens as f64 / 1_000_000.0) * cost.cache_input_price_per_m;
    let cache_output_cost =
        (usage.cache_write_tokens as f64 / 1_000_000.0) * cost.cache_output_price_per_m;

    CostSnapshot {
        enabled: true,
        currency: cost.currency.clone(),
        input_price_per_m: cost.input_price_per_m,
        output_price_per_m: cost.output_price_per_m,
        cache_input_price_per_m: cost.cache_input_price_per_m,
        cache_output_price_per_m: cost.cache_output_price_per_m,
        total_cost: input_cost + output_cost + cache_input_cost + cache_output_cost,
    }
}

/// Performs should capture body.
fn should_capture_body(state: &ServiceState) -> bool {
    state
        .config
        .read()
        .map(|cfg| cfg.logging.capture_body)
        .unwrap_or(false)
}

/// Performs proxy error response.
pub(super) fn proxy_error_response(
    status_code: u16,
    code: &str,
    message: &str,
    upstream_status: Option<u16>,
    protocol: &str,
    trace_id: &str,
) -> Response {
    let payload = json!({
        "error": {
            "code": code,
            "message": message,
            "upstreamStatus": upstream_status,
            "protocol": protocol,
            "traceId": trace_id,
        }
    });

    let mut resp = (
        axum::http::StatusCode::from_u16(status_code)
            .unwrap_or(axum::http::StatusCode::INTERNAL_SERVER_ERROR),
        axum::Json(payload),
    )
        .into_response();
    apply_headers(&mut resp, &response_headers_json(trace_id));
    resp
}
