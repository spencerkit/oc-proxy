use super::routing::{EntryProtocol, ParsedPath, PathEntry};
use super::ServiceState;
use axum::response::{IntoResponse, Response};
use chrono::Utc;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;

use crate::models::{
    default_metrics, LogEntry, LogEntryError, ProxyMetrics, Rule, RuleProtocol, TokenUsage,
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

    pub(super) fn mark_started(&self) {
        if let Ok(mut guard) = self.uptime_started_at.write() {
            *guard = Some(Utc::now().to_rfc3339());
        }
    }

    pub(super) fn mark_stopped(&self) {
        if let Ok(mut guard) = self.uptime_started_at.write() {
            *guard = None;
        }
    }

    pub(super) fn increment_request(&self, stream: bool) {
        let _ = self.requests.fetch_add(1, Ordering::Relaxed);
        if stream {
            let _ = self.stream_requests.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub(super) fn increment_error(&self) {
        let _ = self.errors.fetch_add(1, Ordering::Relaxed);
    }

    pub(super) fn add_latency(&self, elapsed_ms: u64) {
        let _ = self
            .total_latency_ms
            .fetch_add(elapsed_ms, Ordering::Relaxed);
    }

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

pub(super) fn extract_token_usage(payload: &Value) -> Option<TokenUsage> {
    let usage = payload
        .get("usage")
        .or_else(|| payload.get("response").and_then(|r| r.get("usage")))
        .or_else(|| payload.get("message").and_then(|m| m.get("usage")))
        .or_else(|| payload.get("delta").and_then(|d| d.get("usage")))?;

    let input_tokens = first_u64(
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

fn first_u64(obj: &Value, fields: &[&str]) -> u64 {
    for field in fields {
        if let Some(v) = obj.get(*field).and_then(|v| v.as_u64()) {
            return v;
        }
        if let Some(v) = obj
            .get("input_tokens_details")
            .and_then(|d| d.get(*field))
            .and_then(|v| v.as_u64())
        {
            return v;
        }
        if let Some(v) = obj
            .get("prompt_tokens_details")
            .and_then(|d| d.get(*field))
            .and_then(|v| v.as_u64())
        {
            return v;
        }
    }
    0
}

pub(super) fn response_headers_json(trace_id: &str) -> HashMap<String, String> {
    HashMap::from([
        (
            "content-type".into(),
            "application/json; charset=utf-8".into(),
        ),
        ("x-trace-id".into(), trace_id.to_string()),
    ])
}

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

pub(super) fn plain_headers(headers: &reqwest::header::HeaderMap) -> HashMap<String, String> {
    headers
        .iter()
        .filter_map(|(k, v)| {
            Some((
                k.as_str().to_ascii_lowercase(),
                v.to_str().ok()?.to_string(),
            ))
        })
        .collect()
}

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
        response_body: if capture_body { response_body } else { None },
        token_usage: None,
        http_status,
        upstream_status: None,
        duration_ms: 0,
        error,
    };
    state.log_store.append(entry.clone());
    state.stats_store.append_log(&entry);
}

#[allow(clippy::too_many_arguments)]
pub(super) fn finalize_log(
    state: &ServiceState,
    trace_id: &str,
    method: &axum::http::Method,
    parsed_path: &ParsedPath,
    group_name: &str,
    rule: &Rule,
    entry: &PathEntry,
    model: Option<&str>,
    forwarded_model: Option<&str>,
    forwarding_address: Option<&str>,
    request_body: Option<Value>,
    forward_request_body: Option<Value>,
    response_body: Option<Value>,
    http_status: Option<u16>,
    upstream_status: Option<u16>,
    upstream_headers: Option<HashMap<String, String>>,
    response_headers: Option<HashMap<String, String>>,
    token_usage: Option<TokenUsage>,
    duration_ms: u64,
    status: &str,
    capture_body: bool,
) {
    let entry = LogEntry {
        timestamp: Utc::now().to_rfc3339(),
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
        entry_protocol: Some(match entry.protocol {
            EntryProtocol::Openai => "openai".to_string(),
            EntryProtocol::Anthropic => "anthropic".to_string(),
        }),
        downstream_protocol: Some(match rule.protocol {
            RuleProtocol::Openai => "openai".to_string(),
            RuleProtocol::OpenaiCompletion => "openai_completion".to_string(),
            RuleProtocol::Anthropic => "anthropic".to_string(),
        }),
        model: model.map(|m| m.to_string()),
        forwarded_model: forwarded_model.map(|m| m.to_string()),
        forwarding_address: forwarding_address.map(|v| v.to_string()),
        request_headers: None,
        forward_request_headers: None,
        upstream_response_headers: upstream_headers,
        response_headers,
        request_body: if capture_body { request_body } else { None },
        forward_request_body: if capture_body {
            forward_request_body
        } else {
            None
        },
        response_body: if capture_body { response_body } else { None },
        token_usage,
        http_status,
        upstream_status,
        duration_ms,
        error: None,
    };
    state.log_store.append(entry.clone());
    state.stats_store.append_log(&entry);
}

fn should_capture_body(state: &ServiceState) -> bool {
    state
        .config
        .read()
        .map(|cfg| cfg.logging.capture_body)
        .unwrap_or(false)
}

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
