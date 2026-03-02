use crate::log_store::LogStore;
use crate::mappers::{
    map_anthropic_to_openai_request, map_anthropic_to_openai_response,
    map_openai_chat_to_responses, map_openai_to_anthropic_request, map_openai_to_anthropic_response,
};
use crate::models::{
    default_metrics, Group, LogEntry, LogEntryError, ProxyConfig, ProxyMetrics, ProxyStatus,
    Rule, RuleProtocol, TokenUsage,
};
use crate::stats_store::StatsStore;
use axum::body::{to_bytes, Body, Bytes};
use axum::extract::{Path, State};
use axum::http::{HeaderMap, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use futures_util::TryStreamExt;
use reqwest::Client;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::{IpAddr, UdpSocket};
use std::sync::{Arc, Mutex, RwLock};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use url::Url;
use uuid::Uuid;

const MAX_REQUEST_BODY_BYTES: usize = 10 * 1024 * 1024;
const REQUEST_TIMEOUT_MS: u64 = 60_000;

#[derive(Clone)]
pub struct ProxyRuntime {
    inner: Arc<ProxyRuntimeInner>,
}

struct ProxyRuntimeInner {
    config: Arc<RwLock<ProxyConfig>>,
    log_store: LogStore,
    stats_store: StatsStore,
    metrics: Arc<RwLock<ProxyMetrics>>,
    server: Mutex<Option<RunningServer>>,
    client: Client,
}

struct RunningServer {
    address: String,
    lan_address: Option<String>,
    shutdown: Option<oneshot::Sender<()>>,
    handle: JoinHandle<()>,
}

#[derive(Clone)]
struct ServiceState {
    config: Arc<RwLock<ProxyConfig>>,
    log_store: LogStore,
    stats_store: StatsStore,
    metrics: Arc<RwLock<ProxyMetrics>>,
    client: Client,
}

#[derive(Clone, Copy)]
enum EntryProtocol {
    Openai,
    Anthropic,
}

#[derive(Clone, Copy, PartialEq)]
enum EntryEndpoint {
    ChatCompletions,
    Responses,
    Messages,
}

struct ParsedPath {
    group_id: String,
    suffix: String,
}

struct PathEntry {
    protocol: EntryProtocol,
    endpoint: EntryEndpoint,
}

#[derive(Default)]
struct StreamTokenAccumulator {
    line_buffer: String,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_write_tokens: u64,
}

impl StreamTokenAccumulator {
    fn consume_chunk(&mut self, chunk: &[u8]) {
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

    fn into_token_usage(self) -> Option<TokenUsage> {
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

fn normalized_host(host: &str) -> &str {
    host.trim().trim_start_matches('[').trim_end_matches(']')
}

fn format_bind_target(host: &str, port: u16) -> String {
    if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

fn bind_candidates(host: &str) -> Vec<String> {
    let host = normalized_host(host);
    let mut candidates = match host {
        // Prefer IPv4 wildcard first for best compatibility on Windows.
        "0.0.0.0" => vec!["0.0.0.0".to_string(), "::".to_string()],
        "::" => vec!["::".to_string(), "0.0.0.0".to_string()],
        "localhost" => vec!["127.0.0.1".to_string(), "::1".to_string()],
        _ => vec![host.to_string()],
    };
    candidates.dedup();
    candidates
}

fn public_host_for_status(bound_host: &str) -> String {
    match bound_host {
        "0.0.0.0" => "127.0.0.1".to_string(),
        "::" => "[::1]".to_string(),
        _ if bound_host.contains(':') => format!("[{bound_host}]"),
        _ => bound_host.to_string(),
    }
}

fn detect_local_ipv4() -> Option<String> {
    // Use routing table resolution to infer the primary LAN IPv4 of this machine.
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    match socket.local_addr().ok()?.ip() {
        IpAddr::V4(ipv4) if !ipv4.is_loopback() => Some(ipv4.to_string()),
        _ => None,
    }
}

async fn bind_proxy_listener(host: &str, port: u16) -> Result<(TcpListener, String), String> {
    let mut errors = Vec::new();
    for candidate in bind_candidates(host) {
        let target = format_bind_target(&candidate, port);
        match TcpListener::bind(&target).await {
            Ok(listener) => return Ok((listener, candidate)),
            Err(err) => errors.push(format!("{target}: {err}")),
        }
    }
    Err(format!("bind proxy server failed: {}", errors.join(" | ")))
}

impl ProxyRuntime {
    pub fn new(
        config: Arc<RwLock<ProxyConfig>>,
        log_store: LogStore,
        stats_store: StatsStore,
    ) -> Result<Self, String> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_millis(REQUEST_TIMEOUT_MS))
            .build()
            .map_err(|e| format!("create http client failed: {e}"))?;

        Ok(Self {
            inner: Arc::new(ProxyRuntimeInner {
                config,
                log_store,
                stats_store,
                metrics: Arc::new(RwLock::new(default_metrics())),
                server: Mutex::new(None),
                client,
            }),
        })
    }

    pub async fn start(&self) -> Result<ProxyStatus, String> {
        if self.is_running() {
            return Ok(self.get_status());
        }

        let config = self
            .inner
            .config
            .read()
            .map_err(|_| "config lock poisoned".to_string())?
            .clone();

        let (listener, bound_host) = bind_proxy_listener(&config.server.host, config.server.port).await?;

        let (tx, rx) = oneshot::channel();
        let service_state = ServiceState {
            config: self.inner.config.clone(),
            log_store: self.inner.log_store.clone(),
            stats_store: self.inner.stats_store.clone(),
            metrics: self.inner.metrics.clone(),
            client: self.inner.client.clone(),
        };

        let app = Router::new()
            .route("/healthz", get(healthz))
            .route("/metrics-lite", get(metrics_lite))
            .route("/oc/:group_id", post(handle_proxy_root))
            .route("/oc/:group_id/*suffix", post(handle_proxy_suffix))
            .with_state(service_state);

        if let Ok(mut metrics) = self.inner.metrics.write() {
            metrics.uptime_started_at = Some(Utc::now().to_rfc3339());
        }

        let handle = tokio::spawn(async move {
            let server = axum::serve(listener, app);
            let graceful = server.with_graceful_shutdown(async {
                let _ = rx.await;
            });
            let _ = graceful.await;
        });

        let address = format!(
            "http://{}:{}",
            public_host_for_status(&bound_host),
            config.server.port
        );
        let lan_address = match bound_host.as_str() {
            "0.0.0.0" | "::" => detect_local_ipv4().map(|ip| format!("http://{}:{}", ip, config.server.port)),
            _ => None,
        };
        let running = RunningServer {
            address,
            lan_address,
            shutdown: Some(tx),
            handle,
        };

        self.inner
            .server
            .lock()
            .map_err(|_| "server lock poisoned".to_string())?
            .replace(running);

        Ok(self.get_status())
    }

    pub async fn stop(&self) -> Result<ProxyStatus, String> {
        let running = self
            .inner
            .server
            .lock()
            .map_err(|_| "server lock poisoned".to_string())?
            .take();

        if let Some(mut srv) = running {
            if let Some(tx) = srv.shutdown.take() {
                let _ = tx.send(());
            }
            let _ = tokio::time::timeout(std::time::Duration::from_millis(2500), &mut srv.handle).await;
        }

        if let Ok(mut metrics) = self.inner.metrics.write() {
            metrics.uptime_started_at = None;
        }

        Ok(self.get_status())
    }

    pub fn get_status(&self) -> ProxyStatus {
        let running_guard = self.inner.server.lock();
        let (running, address, lan_address) = if let Ok(guard) = running_guard {
            if let Some(srv) = guard.as_ref() {
                (true, Some(srv.address.clone()), srv.lan_address.clone())
            } else {
                (false, None, None)
            }
        } else {
            (false, None, None)
        };

        let metrics = self
            .inner
            .metrics
            .read()
            .map(|m| m.clone())
            .unwrap_or_else(|_| default_metrics());

        ProxyStatus {
            running,
            address,
            lan_address,
            metrics,
        }
    }

    pub fn list_logs(&self, max: usize) -> Vec<LogEntry> {
        self.inner.log_store.list(max)
    }

    pub fn clear_logs(&self) {
        self.inner.log_store.clear();
    }

    pub fn stats_summary(
        &self,
        hours: Option<u32>,
        rule_key: Option<String>,
    ) -> crate::models::StatsSummaryResult {
        self.inner.stats_store.summarize(hours, rule_key)
    }

    fn is_running(&self) -> bool {
        self.inner
            .server
            .lock()
            .map(|g| g.is_some())
            .unwrap_or(false)
    }
}

async fn healthz(State(state): State<ServiceState>) -> Response {
    let trace_id = Uuid::new_v4().to_string();
    let headers = response_headers_json(&trace_id);

    let payload = json!({ "ok": true, "running": true });
    let mut resp = (StatusCode::OK, Json(payload.clone())).into_response();
    apply_headers(&mut resp, &headers);

    log_simple(
        &state,
        trace_id,
        "GET",
        "/healthz",
        "ok",
        Some(200),
        Some(payload),
        None,
    );

    resp
}

async fn metrics_lite(State(state): State<ServiceState>) -> Response {
    let trace_id = Uuid::new_v4().to_string();
    let metrics = state
        .metrics
        .read()
        .map(|m| m.clone())
        .unwrap_or_else(|_| default_metrics());

    let payload = serde_json::to_value(metrics.clone()).unwrap_or_else(|_| json!({}));
    let mut resp = (StatusCode::OK, Json(payload.clone())).into_response();
    apply_headers(&mut resp, &response_headers_json(&trace_id));

    log_simple(
        &state,
        trace_id,
        "GET",
        "/metrics-lite",
        "ok",
        Some(200),
        Some(payload),
        None,
    );

    resp
}

async fn handle_proxy_root(
    State(state): State<ServiceState>,
    Path(group_id): Path<String>,
    method: Method,
    headers: HeaderMap,
    body: Body,
) -> Response {
    handle_proxy_request(state, method, headers, body, ParsedPath {
        group_id,
        suffix: "/chat/completions".to_string(),
    })
    .await
}

async fn handle_proxy_suffix(
    State(state): State<ServiceState>,
    Path((group_id, suffix)): Path<(String, String)>,
    method: Method,
    headers: HeaderMap,
    body: Body,
) -> Response {
    handle_proxy_request(
        state,
        method,
        headers,
        body,
        ParsedPath {
            group_id,
            suffix: format!("/{}", suffix.trim_start_matches('/')),
        },
    )
    .await
}

async fn handle_proxy_request(
    state: ServiceState,
    method: Method,
    headers: HeaderMap,
    body: Body,
    parsed_path: ParsedPath,
) -> Response {
    let trace_id = Uuid::new_v4().to_string();
    let started = std::time::Instant::now();

    if method != Method::POST {
        let payload = json!({"error": {"code": "not_found", "message": "Use POST /oc/:groupId/:endpoint (messages/chat/completions/responses)"}});
        return reject_and_log(&state, trace_id, method, &parsed_path, 404, payload).await;
    }

    let config = match state.config.read() {
        Ok(v) => v.clone(),
        Err(_) => {
            return proxy_error_response(
                500,
                "proxy_error",
                "Failed to acquire config lock",
                None,
                "proxy",
                &trace_id,
            )
        }
    };

    if config.server.auth_enabled {
        let auth = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();
        let expected = format!("Bearer {}", config.server.local_bearer_token);
        if auth != expected {
            return reject_and_log(
                &state,
                trace_id,
                method,
                &parsed_path,
                401,
                json!({"error": {"code": "unauthorized", "message": "Missing or invalid local bearer token"}}),
            )
            .await;
        }
    }

    let entry = match detect_entry_protocol(&parsed_path.suffix) {
        Some(v) => v,
        None => {
            return reject_and_log(
                &state,
                trace_id,
                method,
                &parsed_path,
                404,
                json!({"error": {"code": "not_found", "message": format!("Unsupported entry path: /oc/{}{}", parsed_path.group_id, parsed_path.suffix)}}),
            )
            .await;
        }
    };

    let (group, rule) = match find_group_and_rule(&config, &parsed_path.group_id) {
        Ok(v) => v,
        Err((status, msg)) => {
            return proxy_error_response(status, "proxy_error", &msg, None, "proxy", &trace_id);
        }
    };

    if let Err((status, msg)) = assert_rule_ready(rule) {
        return proxy_error_response(status, "proxy_error", &msg, None, "proxy", &trace_id);
    }

    let body_bytes = match to_bytes(body, MAX_REQUEST_BODY_BYTES).await {
        Ok(v) => v,
        Err(_) => {
            return proxy_error_response(
                413,
                "proxy_error",
                "Request body too large (max 10485760 bytes)",
                None,
                "proxy",
                &trace_id,
            )
        }
    };
    let request_body = if body_bytes.is_empty() {
        json!({})
    } else {
        match serde_json::from_slice::<Value>(&body_bytes) {
            Ok(v) => v,
            Err(_) => {
                return proxy_error_response(
                    400,
                    "proxy_error",
                    "Request body must be valid JSON",
                    None,
                    "proxy",
                    &trace_id,
                )
            }
        }
    };

    let target_model = resolve_target_model(rule, group, &request_body);
    let requested_model = request_body
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or(&rule.default_model)
        .to_string();
    let upstream_path = resolve_upstream_path(&rule.protocol, entry.endpoint);
    let upstream_url = match resolve_upstream_url(&rule.api_address, upstream_path) {
        Ok(v) => v,
        Err(msg) => {
            return proxy_error_response(400, "proxy_error", &msg, None, "proxy", &trace_id);
        }
    };

    let mut upstream_body = match build_upstream_body(
        &config,
        &entry,
        &rule.protocol,
        &request_body,
        &target_model,
    ) {
        Ok(v) => v,
        Err(msg) => {
            return proxy_error_response(422, "proxy_error", &msg, None, "proxy", &trace_id);
        }
    };

    let stream = upstream_body
        .get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let capture_body = config.logging.capture_body;

    increment_requests(&state.metrics, stream);

    let upstream_headers = build_rule_headers(&rule.protocol, rule);

    // Current migration phase streams are proxied as passthrough to keep runtime stable.
    // Cross-protocol stream semantic parity is validated in follow-up hardening.
    if is_cross_protocol(&entry, &rule.protocol) {
        upstream_body["stream"] = Value::Bool(false);
    }

    let upstream_resp = match state
        .client
        .post(upstream_url.clone())
        .headers(reqwest::header::HeaderMap::from_iter(upstream_headers.iter().filter_map(
            |(k, v)| {
                let name = reqwest::header::HeaderName::from_bytes(k.as_bytes()).ok()?;
                let value = reqwest::header::HeaderValue::from_str(v).ok()?;
                Some((name, value))
            },
        )))
        .json(&upstream_body)
        .send()
        .await
    {
        Ok(r) => r,
        Err(err) => {
            return proxy_error_response(
                502,
                "upstream_error",
                &format!("Upstream request failed: {err}"),
                None,
                "proxy",
                &trace_id,
            );
        }
    };

    let upstream_status = upstream_resp.status().as_u16();
    let upstream_headers_plain = plain_headers(upstream_resp.headers());
    let upstream_ct = upstream_resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_lowercase();

    if stream && upstream_ct.contains("text/event-stream") {
        let (tx, rx) = mpsc::channel::<Result<Bytes, std::io::Error>>(32);
        let stream_state = state.clone();
        let stream_trace_id = trace_id.clone();
        let stream_method = method.clone();
        let stream_parsed_path = ParsedPath {
            group_id: parsed_path.group_id.clone(),
            suffix: parsed_path.suffix.clone(),
        };
        let stream_group = group.clone();
        let stream_rule = rule.clone();
        let stream_entry = PathEntry {
            protocol: entry.protocol,
            endpoint: entry.endpoint,
        };
        let stream_requested_model = requested_model.clone();
        let stream_target_model = target_model.clone();
        let stream_upstream_url = upstream_url.clone();
        let stream_request_body = request_body.clone();
        let stream_upstream_body = upstream_body.clone();
        let stream_upstream_headers = upstream_headers_plain.clone();
        let stream_capture_body = capture_body;
        let stream_upstream_status = upstream_status;
        let stream_started = started;

        tokio::spawn(async move {
            let mut bytes_stream = upstream_resp.bytes_stream();
            let mut usage_acc = StreamTokenAccumulator::default();
            let mut stream_failed = false;

            loop {
                match bytes_stream.try_next().await {
                    Ok(Some(bytes)) => {
                        usage_acc.consume_chunk(bytes.as_ref());
                        if tx.send(Ok(bytes)).await.is_err() {
                            break;
                        }
                    }
                    Ok(None) => break,
                    Err(_) => {
                        stream_failed = true;
                        let _ = tx
                            .send(Err(std::io::Error::new(
                                std::io::ErrorKind::Other,
                                "stream read failed",
                            )))
                            .await;
                        break;
                    }
                }
            }

            let token_usage = usage_acc.into_token_usage();
            if let Some(ref usage) = token_usage {
                update_token_metrics(&stream_state.metrics, usage);
            }
            update_latency(
                &stream_state.metrics,
                stream_started.elapsed().as_millis() as u64,
            );

            let mut response_headers = response_headers_sse(&stream_trace_id);
            finalize_log(
                &stream_state,
                &stream_trace_id,
                &stream_method,
                &stream_parsed_path,
                &stream_group,
                &stream_rule,
                &stream_entry,
                Some(&stream_requested_model),
                Some(&stream_target_model),
                Some(&stream_upstream_url),
                Some(stream_request_body),
                Some(stream_upstream_body),
                Some(json!({"stream": true})),
                if stream_failed { Some(502) } else { Some(200) },
                Some(stream_upstream_status),
                Some(stream_upstream_headers),
                Some(response_headers.drain().collect()),
                token_usage,
                stream_started.elapsed().as_millis() as u64,
                if stream_failed { "error" } else { "ok" },
                stream_capture_body,
            );
        });

        let body = Body::from_stream(futures_util::stream::unfold(rx, |mut rx| async move {
            rx.recv().await.map(|item| (item, rx))
        }));
        let mut resp = Response::new(body);
        *resp.status_mut() = StatusCode::OK;

        let response_headers = response_headers_sse(&trace_id);
        for (k, v) in &response_headers {
            let _ = resp.headers_mut().insert(
                axum::http::header::HeaderName::from_bytes(k.as_bytes()).unwrap(),
                axum::http::HeaderValue::from_str(v).unwrap_or_else(|_| axum::http::HeaderValue::from_static("")),
            );
        }
        return resp;
    }

    let upstream_text = match upstream_resp.text().await {
        Ok(v) => v,
        Err(err) => {
            return proxy_error_response(
                502,
                "upstream_error",
                &format!("Failed to read upstream response: {err}"),
                Some(upstream_status),
                "proxy",
                &trace_id,
            );
        }
    };

    let upstream_json = match serde_json::from_str::<Value>(&upstream_text) {
        Ok(v) => v,
        Err(_) => {
            return proxy_error_response(
                502,
                "upstream_error",
                &format!(
                    "Upstream returned non-JSON response: {}",
                    upstream_text.chars().take(200).collect::<String>()
                ),
                Some(upstream_status),
                "proxy",
                &trace_id,
            );
        }
    };

    if upstream_status >= 400 {
        let msg = upstream_json
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|v| v.as_str())
            .map(|v| v.to_string())
            .unwrap_or_else(|| format!("Upstream returned HTTP {upstream_status}"));
        return proxy_error_response(
            upstream_status,
            "upstream_error",
            &msg,
            Some(upstream_status),
            "proxy",
            &trace_id,
        );
    }

    let output_body = map_response_body(
        &entry,
        &rule.protocol,
        &upstream_json,
        &requested_model,
    );

    let token_usage = extract_token_usage(&upstream_json).or_else(|| extract_token_usage(&output_body));
    if let Some(ref usage) = token_usage {
        update_token_metrics(&state.metrics, usage);
    }

    update_latency(&state.metrics, started.elapsed().as_millis() as u64);

    let mut resp = (StatusCode::OK, Json(output_body.clone())).into_response();
    apply_headers(&mut resp, &response_headers_json(&trace_id));

    finalize_log(
        &state,
        &trace_id,
        &method,
        &parsed_path,
        &group,
        &rule,
        &entry,
        Some(&requested_model),
        Some(&target_model),
        Some(&upstream_url),
        Some(request_body),
        Some(upstream_body),
        Some(output_body),
        Some(200),
        Some(upstream_status),
        Some(upstream_headers_plain),
        Some(response_headers_json(&trace_id)),
        token_usage,
        started.elapsed().as_millis() as u64,
        "ok",
        capture_body,
    );

    resp
}

fn increment_requests(metrics: &Arc<RwLock<ProxyMetrics>>, stream: bool) {
    if let Ok(mut m) = metrics.write() {
        m.requests += 1;
        if stream {
            m.stream_requests += 1;
        }
    }
}

fn update_latency(metrics: &Arc<RwLock<ProxyMetrics>>, elapsed_ms: u64) {
    if let Ok(mut m) = metrics.write() {
        let n = m.requests;
        if n <= 1 {
            m.avg_latency_ms = elapsed_ms;
        } else {
            m.avg_latency_ms = ((m.avg_latency_ms * (n - 1) + elapsed_ms) as f64 / n as f64).round() as u64;
        }
    }
}

fn update_token_metrics(metrics: &Arc<RwLock<ProxyMetrics>>, usage: &TokenUsage) {
    if let Ok(mut m) = metrics.write() {
        m.input_tokens += usage.input_tokens;
        m.output_tokens += usage.output_tokens;
        m.cache_read_tokens += usage.cache_read_tokens;
        m.cache_write_tokens += usage.cache_write_tokens;
    }
}

fn detect_entry_protocol(suffix: &str) -> Option<PathEntry> {
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

fn resolve_upstream_path(target_protocol: &RuleProtocol, endpoint: EntryEndpoint) -> &'static str {
    match target_protocol {
        RuleProtocol::Anthropic => "/v1/messages",
        RuleProtocol::Openai => {
            if endpoint == EntryEndpoint::Responses {
                "/v1/responses"
            } else {
                "/v1/chat/completions"
            }
        }
    }
}

fn resolve_upstream_url(api_address: &str, default_path: &str) -> Result<String, String> {
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

fn build_rule_headers(protocol: &RuleProtocol, rule: &Rule) -> HashMap<String, String> {
    let mut headers = HashMap::new();
    headers.insert("content-type".to_string(), "application/json".to_string());
    match protocol {
        RuleProtocol::Anthropic => {
            headers.insert("x-api-key".to_string(), rule.token.clone());
            headers.insert("anthropic-version".to_string(), "2023-06-01".to_string());
        }
        RuleProtocol::Openai => {
            headers.insert("authorization".to_string(), format!("Bearer {}", rule.token));
        }
    }
    headers
}

fn find_group_and_rule<'a>(config: &'a ProxyConfig, group_id: &str) -> Result<(&'a Group, &'a Rule), (u16, String)> {
    let group = config
        .groups
        .iter()
        .find(|g| g.id == group_id)
        .ok_or_else(|| (404, format!("Group not found for id: {group_id}")))?;

    let active_rule_id = group
        .active_rule_id
        .as_ref()
        .ok_or_else(|| (409, format!("Group {} has no active rule", group.name)))?;

    let rule = group
        .rules
        .iter()
        .find(|r| r.id == *active_rule_id)
        .ok_or_else(|| {
            (
                409,
                format!("Active rule {} is missing in group {}", active_rule_id, group.name),
            )
        })?;

    Ok((group, rule))
}

fn assert_rule_ready(rule: &Rule) -> Result<(), (u16, String)> {
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

fn resolve_target_model(rule: &Rule, group: &Group, request_body: &Value) -> String {
    let requested = request_body
        .get("model")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    if let Some(model) = requested {
        if let Some(matched_model) = find_group_model_match(group, &model) {
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

fn find_group_model_match<'a>(group: &'a Group, requested: &str) -> Option<&'a str> {
    let mut best: Option<&str> = None;
    for model in &group.models {
        let candidate = model.trim();
        if candidate.is_empty() {
            continue;
        }
        if !is_model_match(candidate, requested) {
            continue;
        }
        if best.map(|curr| candidate.len() > curr.len()).unwrap_or(true) {
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

fn build_upstream_body(
    config: &ProxyConfig,
    entry: &PathEntry,
    downstream: &RuleProtocol,
    request_body: &Value,
    target_model: &str,
) -> Result<Value, String> {
    let mut with_model = if request_body.is_object() {
        request_body.clone()
    } else {
        json!({})
    };
    with_model["model"] = json!(target_model);

    match (entry.protocol, downstream) {
        (EntryProtocol::Openai, RuleProtocol::Anthropic) => {
            map_openai_to_anthropic_request(&with_model, config.compat.strict_mode, target_model)
        }
        (EntryProtocol::Anthropic, RuleProtocol::Openai) => {
            map_anthropic_to_openai_request(&with_model, config.compat.strict_mode, target_model)
        }
        _ => Ok(with_model),
    }
}

fn map_response_body(
    entry: &PathEntry,
    downstream: &RuleProtocol,
    upstream_json: &Value,
    request_model: &str,
) -> Value {
    match (entry.protocol, downstream) {
        (EntryProtocol::Openai, RuleProtocol::Anthropic) => {
            let chat = map_anthropic_to_openai_response(upstream_json, request_model);
            if entry.endpoint == EntryEndpoint::Responses {
                map_openai_chat_to_responses(&chat)
            } else {
                chat
            }
        }
        (EntryProtocol::Anthropic, RuleProtocol::Openai) => {
            map_openai_to_anthropic_response(upstream_json, request_model)
        }
        _ => upstream_json.clone(),
    }
}

fn is_cross_protocol(entry: &PathEntry, downstream: &RuleProtocol) -> bool {
    match (entry.protocol, downstream) {
        (EntryProtocol::Openai, RuleProtocol::Anthropic) => true,
        (EntryProtocol::Anthropic, RuleProtocol::Openai) => true,
        _ => false,
    }
}

fn extract_token_usage(payload: &Value) -> Option<TokenUsage> {
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

    if input_tokens == 0 && output_tokens == 0 && cache_read_tokens == 0 && cache_write_tokens == 0 {
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

fn response_headers_json(trace_id: &str) -> HashMap<String, String> {
    HashMap::from([
        ("content-type".into(), "application/json; charset=utf-8".into()),
        ("x-trace-id".into(), trace_id.to_string()),
    ])
}

fn response_headers_sse(trace_id: &str) -> HashMap<String, String> {
    HashMap::from([
        ("content-type".into(), "text/event-stream; charset=utf-8".into()),
        ("cache-control".into(), "no-cache, no-transform".into()),
        ("connection".into(), "keep-alive".into()),
        ("x-accel-buffering".into(), "no".into()),
        ("x-trace-id".into(), trace_id.to_string()),
    ])
}

fn apply_headers(resp: &mut Response, headers: &HashMap<String, String>) {
    for (k, v) in headers {
        if let (Ok(name), Ok(value)) = (
            axum::http::header::HeaderName::from_bytes(k.as_bytes()),
            axum::http::HeaderValue::from_str(v),
        ) {
            resp.headers_mut().insert(name, value);
        }
    }
}

fn plain_headers(headers: &reqwest::header::HeaderMap) -> HashMap<String, String> {
    headers
        .iter()
        .filter_map(|(k, v)| Some((k.as_str().to_ascii_lowercase(), v.to_str().ok()?.to_string())))
        .collect()
}

async fn reject_and_log(
    state: &ServiceState,
    trace_id: String,
    method: Method,
    parsed_path: &ParsedPath,
    status: u16,
    payload: Value,
) -> Response {
    let mut resp = (StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_REQUEST), Json(payload.clone())).into_response();
    apply_headers(&mut resp, &response_headers_json(&trace_id));

    log_simple(
        state,
        trace_id,
        method.as_str(),
        &format!("/oc/{}{}", parsed_path.group_id, parsed_path.suffix),
        "rejected",
        Some(status),
        Some(payload),
        Some(LogEntryError {
            message: "rejected".to_string(),
            code: "rejected".to_string(),
        }),
    );

    resp
}

fn proxy_error_response(
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
        StatusCode::from_u16(status_code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
        Json(payload),
    )
        .into_response();
    apply_headers(&mut resp, &response_headers_json(trace_id));
    resp
}

fn log_simple(
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
fn finalize_log(
    state: &ServiceState,
    trace_id: &str,
    method: &Method,
    parsed_path: &ParsedPath,
    group: &Group,
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
    let direction = match (&entry.protocol, &rule.protocol) {
        (EntryProtocol::Openai, RuleProtocol::Anthropic) => Some("oc".to_string()),
        (EntryProtocol::Anthropic, RuleProtocol::Openai) => Some("co".to_string()),
        _ => None,
    };

    let entry = LogEntry {
        timestamp: Utc::now().to_rfc3339(),
        trace_id: trace_id.to_string(),
        phase: "request_chain".to_string(),
        status: status.to_string(),
        method: method.as_str().to_string(),
        request_path: format!("/oc/{}{}", parsed_path.group_id, parsed_path.suffix),
        request_address: format!("{} /oc/{}{}", method.as_str(), parsed_path.group_id, parsed_path.suffix),
        client_address: None,
        group_path: Some(parsed_path.group_id.clone()),
        group_name: Some(group.name.clone()),
        rule_id: Some(rule.id.clone()),
        direction,
        entry_protocol: Some(match entry.protocol {
            EntryProtocol::Openai => "openai".to_string(),
            EntryProtocol::Anthropic => "anthropic".to_string(),
        }),
        downstream_protocol: Some(match rule.protocol {
            RuleProtocol::Openai => "openai".to_string(),
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
        forward_request_body: if capture_body { forward_request_body } else { None },
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
