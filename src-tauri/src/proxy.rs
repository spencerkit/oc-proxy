use crate::log_store::LogStore;
use crate::models::{
    default_metrics, LogEntry, LogEntryError, ProxyConfig, ProxyMetrics, ProxyStatus, Rule,
    RuleProtocol, TokenUsage,
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
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use url::Url;
use uuid::Uuid;

const MAX_REQUEST_BODY_BYTES: usize = 10 * 1024 * 1024;
const MAX_STREAM_LOG_BODY_BYTES: usize = 256 * 1024;
const NON_STREAM_REQUEST_TIMEOUT_MS: u64 = 60_000;
const STREAM_REQUEST_TIMEOUT_MS: u64 = 600_000;
const UPSTREAM_CONNECT_TIMEOUT_MS: u64 = 10_000;

#[derive(Clone)]
pub struct ProxyRuntime {
    inner: Arc<ProxyRuntimeInner>,
}

struct ProxyRuntimeInner {
    config: Arc<RwLock<ProxyConfig>>,
    config_revision: Arc<AtomicU64>,
    route_index: Arc<RwLock<RouteIndex>>,
    route_index_revision: Arc<AtomicU64>,
    log_store: LogStore,
    stats_store: StatsStore,
    metrics: Arc<MetricsState>,
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
    config_revision: Arc<AtomicU64>,
    route_index: Arc<RwLock<RouteIndex>>,
    route_index_revision: Arc<AtomicU64>,
    log_store: LogStore,
    stats_store: StatsStore,
    metrics: Arc<MetricsState>,
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

#[derive(Clone)]
struct ActiveRoute {
    group_name: String,
    group_models: Vec<String>,
    rule: Rule,
}

#[derive(Clone)]
enum RouteResolution {
    Ready(ActiveRoute),
    NoActiveRule {
        group_name: String,
    },
    MissingActiveRule {
        group_name: String,
        active_rule_id: String,
    },
}

type RouteIndex = HashMap<String, RouteResolution>;

struct MetricsState {
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
    fn new() -> Self {
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

    fn snapshot(&self) -> ProxyMetrics {
        let requests = self.requests.load(Ordering::Relaxed);
        let total_latency_ms = self.total_latency_ms.load(Ordering::Relaxed);
        let avg_latency_ms = if requests == 0 { 0 } else { total_latency_ms / requests };
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

    fn mark_started(&self) {
        if let Ok(mut guard) = self.uptime_started_at.write() {
            *guard = Some(Utc::now().to_rfc3339());
        }
    }

    fn mark_stopped(&self) {
        if let Ok(mut guard) = self.uptime_started_at.write() {
            *guard = None;
        }
    }

    fn increment_request(&self, stream: bool) {
        let _ = self.requests.fetch_add(1, Ordering::Relaxed);
        if stream {
            let _ = self.stream_requests.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn increment_error(&self) {
        let _ = self.errors.fetch_add(1, Ordering::Relaxed);
    }

    fn add_latency(&self, elapsed_ms: u64) {
        let _ = self.total_latency_ms.fetch_add(elapsed_ms, Ordering::Relaxed);
    }

    fn add_token_usage(&self, usage: &TokenUsage) {
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
        config_revision: Arc<AtomicU64>,
        log_store: LogStore,
        stats_store: StatsStore,
    ) -> Result<Self, String> {
        let initial_route_index = config
            .read()
            .map_err(|_| "config lock poisoned".to_string())
            .map(|cfg| build_route_index(&cfg))?;
        let client = Client::builder()
            .connect_timeout(std::time::Duration::from_millis(UPSTREAM_CONNECT_TIMEOUT_MS))
            .build()
            .map_err(|e| format!("create http client failed: {e}"))?;

        Ok(Self {
            inner: Arc::new(ProxyRuntimeInner {
                config,
                config_revision: config_revision.clone(),
                route_index: Arc::new(RwLock::new(initial_route_index)),
                route_index_revision: Arc::new(AtomicU64::new(
                    config_revision.load(Ordering::Acquire),
                )),
                log_store,
                stats_store,
                metrics: Arc::new(MetricsState::new()),
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
            config_revision: self.inner.config_revision.clone(),
            route_index: self.inner.route_index.clone(),
            route_index_revision: self.inner.route_index_revision.clone(),
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

        self.inner.metrics.mark_started();

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

        self.inner.metrics.mark_stopped();

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

        let metrics = self.inner.metrics.snapshot();

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

    pub fn clear_stats(&self) -> Result<(), String> {
        self.inner.stats_store.clear()
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
    let metrics = state.metrics.snapshot();

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

    if let Err(msg) = refresh_route_index_if_needed(&state) {
        state.metrics.increment_error();
        return proxy_error_response(500, "proxy_error", &msg, None, "proxy", &trace_id);
    }

    let (auth_enabled, expected_auth, capture_body) = match state.config.read() {
        Ok(cfg) => {
            let expected = format!("Bearer {}", cfg.server.local_bearer_token);
            (cfg.server.auth_enabled, expected, cfg.logging.capture_body)
        }
        Err(_) => {
            state.metrics.increment_error();
            return proxy_error_response(
                500,
                "proxy_error",
                "Failed to acquire config lock",
                None,
                "proxy",
                &trace_id,
            );
        }
    };

    if auth_enabled {
        let auth = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();
        if auth != expected_auth {
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

    let active_route = match state.route_index.read() {
        Ok(index) => index.get(&parsed_path.group_id).cloned(),
        Err(_) => {
            state.metrics.increment_error();
            return proxy_error_response(
                500,
                "proxy_error",
                "Failed to acquire route index lock",
                None,
                "proxy",
                &trace_id,
            );
        }
    };
    let active_route = match active_route {
        Some(RouteResolution::Ready(route)) => route,
        Some(RouteResolution::NoActiveRule { group_name }) => {
            state.metrics.increment_error();
            return proxy_error_response(
                409,
                "proxy_error",
                &format!("Group {} has no active rule", group_name),
                None,
                "proxy",
                &trace_id,
            );
        }
        Some(RouteResolution::MissingActiveRule {
            group_name,
            active_rule_id,
        }) => {
            state.metrics.increment_error();
            return proxy_error_response(
                409,
                "proxy_error",
                &format!(
                    "Active rule {} is missing in group {}",
                    active_rule_id, group_name
                ),
                None,
                "proxy",
                &trace_id,
            );
        }
        None => {
            state.metrics.increment_error();
            return proxy_error_response(
                404,
                "proxy_error",
                &format!("Group not found for id: {}", parsed_path.group_id),
                None,
                "proxy",
                &trace_id,
            );
        }
    };

    if let Err((status, msg)) = assert_rule_ready(&active_route.rule) {
        state.metrics.increment_error();
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
                state.metrics.increment_error();
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

    let target_model = resolve_target_model(&active_route.rule, &active_route.group_models, &request_body);
    let requested_model = request_body
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or(&active_route.rule.default_model)
        .to_string();
    let downstream_protocol = protocol_from_entry(&entry);
    let upstream_path = resolve_upstream_path(&downstream_protocol);
    let upstream_url = match resolve_upstream_url(&active_route.rule.api_address, upstream_path) {
        Ok(v) => v,
        Err(msg) => {
            state.metrics.increment_error();
            return proxy_error_response(400, "proxy_error", &msg, None, "proxy", &trace_id);
        }
    };

    let upstream_body = match build_upstream_body(&request_body, &target_model) {
        Ok(v) => v,
        Err(msg) => {
            state.metrics.increment_error();
            return proxy_error_response(422, "proxy_error", &msg, None, "proxy", &trace_id);
        }
    };

    let stream = upstream_body
        .get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    state.metrics.increment_request(stream);

    let upstream_headers = build_rule_headers(&downstream_protocol, &active_route.rule);
    let request_timeout_ms = if stream {
        STREAM_REQUEST_TIMEOUT_MS
    } else {
        NON_STREAM_REQUEST_TIMEOUT_MS
    };

    let upstream_resp = match state
        .client
        .post(upstream_url.clone())
        .timeout(std::time::Duration::from_millis(request_timeout_ms))
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
            state.metrics.increment_error();
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
        let stream_group_name = active_route.group_name.clone();
        let stream_rule = active_route.rule.clone();
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
            let mut stream_body = Vec::<u8>::new();
            let mut stream_body_truncated = false;

            loop {
                match bytes_stream.try_next().await {
                    Ok(Some(bytes)) => {
                        usage_acc.consume_chunk(bytes.as_ref());
                        if stream_capture_body && !stream_body_truncated {
                            let remaining = MAX_STREAM_LOG_BODY_BYTES.saturating_sub(stream_body.len());
                            if remaining == 0 {
                                stream_body_truncated = true;
                            } else if bytes.len() <= remaining {
                                stream_body.extend_from_slice(bytes.as_ref());
                            } else {
                                stream_body.extend_from_slice(&bytes.as_ref()[..remaining]);
                                stream_body_truncated = true;
                            }
                        }
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
                stream_state.metrics.add_token_usage(usage);
            }
            stream_state
                .metrics
                .add_latency(stream_started.elapsed().as_millis() as u64);
            if stream_failed {
                stream_state.metrics.increment_error();
            }

            let stream_response_body = if stream_capture_body {
                Some(json!({
                    "stream": true,
                    "payload": String::from_utf8_lossy(&stream_body).to_string(),
                    "truncated": stream_body_truncated,
                }))
            } else {
                Some(json!({"stream": true}))
            };
            let mut response_headers = response_headers_sse(&stream_trace_id);
            finalize_log(
                &stream_state,
                &stream_trace_id,
                &stream_method,
                &stream_parsed_path,
                &stream_group_name,
                &stream_rule,
                &stream_entry,
                Some(&stream_requested_model),
                Some(&stream_target_model),
                Some(&stream_upstream_url),
                Some(stream_request_body),
                Some(stream_upstream_body),
                stream_response_body,
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
            state.metrics.increment_error();
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
            state.metrics.increment_error();
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
        state.metrics.increment_error();
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
        &downstream_protocol,
        &upstream_json,
        &requested_model,
    );

    let token_usage = extract_token_usage(&upstream_json).or_else(|| extract_token_usage(&output_body));
    if let Some(ref usage) = token_usage {
        state.metrics.add_token_usage(usage);
    }

    state
        .metrics
        .add_latency(started.elapsed().as_millis() as u64);

    let mut resp = (StatusCode::OK, Json(output_body.clone())).into_response();
    apply_headers(&mut resp, &response_headers_json(&trace_id));

    finalize_log(
        &state,
        &trace_id,
        &method,
        &parsed_path,
        &active_route.group_name,
        &active_route.rule,
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

fn resolve_upstream_path(target_protocol: &RuleProtocol) -> &'static str {
    match target_protocol {
        RuleProtocol::Anthropic => "/v1/messages",
        RuleProtocol::Openai => "/v1/responses",
        RuleProtocol::OpenaiCompletion => "/v1/chat/completions",
    }
}

fn protocol_from_entry(entry: &PathEntry) -> RuleProtocol {
    match entry.endpoint {
        EntryEndpoint::Responses => RuleProtocol::Openai,
        EntryEndpoint::ChatCompletions => RuleProtocol::OpenaiCompletion,
        EntryEndpoint::Messages => RuleProtocol::Anthropic,
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
        RuleProtocol::Openai | RuleProtocol::OpenaiCompletion => {
            headers.insert("authorization".to_string(), format!("Bearer {}", rule.token));
        }
    }
    headers
}

fn refresh_route_index_if_needed(state: &ServiceState) -> Result<(), String> {
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

fn build_route_index(config: &ProxyConfig) -> RouteIndex {
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

fn resolve_target_model(rule: &Rule, group_models: &[String], request_body: &Value) -> String {
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
    request_body: &Value,
    target_model: &str,
) -> Result<Value, String> {
    let mut with_model = if request_body.is_object() {
        request_body.clone()
    } else {
        json!({})
    };
    with_model["model"] = json!(target_model);
    Ok(with_model)
}

fn map_response_body(
    _entry: &PathEntry,
    _downstream: &RuleProtocol,
    upstream_json: &Value,
    _request_model: &str,
) -> Value {
    upstream_json.clone()
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
    if status >= 400 {
        state.metrics.increment_error();
    }
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
        request_address: format!("{} /oc/{}{}", method.as_str(), parsed_path.group_id, parsed_path.suffix),
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

#[cfg(test)]
mod tests {
    use super::{
        build_upstream_body, extract_token_usage, resolve_target_model, resolve_upstream_path,
        StreamTokenAccumulator,
    };
    use crate::models::{default_rule_quota_config, Group, Rule, RuleProtocol};
    use serde_json::json;
    use std::collections::HashMap;

    #[test]
    fn extract_token_usage_reads_nested_response_usage_payload() {
        let payload = json!({
            "response": {
                "usage": {
                    "input_tokens": 44,
                    "output_tokens": 12,
                    "cache_read_input_tokens": 9
                }
            }
        });

        let usage = extract_token_usage(&payload).expect("usage should exist");
        assert_eq!(usage.input_tokens, 44);
        assert_eq!(usage.output_tokens, 12);
        assert_eq!(usage.cache_read_tokens, 9);
        assert_eq!(usage.cache_write_tokens, 0);
    }

    #[test]
    fn extract_token_usage_maps_openai_prompt_and_cache_fields() {
        let payload = json!({
            "usage": {
                "prompt_tokens": 100,
                "completion_tokens": 20,
                "prompt_tokens_details": {
                    "cached_tokens": 30,
                    "cache_creation_tokens": 5
                }
            }
        });

        let usage = extract_token_usage(&payload).expect("usage should exist");
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 20);
        assert_eq!(usage.cache_read_tokens, 30);
        assert_eq!(usage.cache_write_tokens, 5);
    }

    #[test]
    fn extract_token_usage_reads_message_and_delta_usage_payloads() {
        let message_payload = json!({
            "message": {
                "usage": {
                    "input_tokens": 7,
                    "output_tokens": 3
                }
            }
        });
        let delta_payload = json!({
            "delta": {
                "usage": {
                    "input_tokens": 11,
                    "output_tokens": 4
                }
            }
        });

        let message_usage = extract_token_usage(&message_payload).expect("message usage should exist");
        assert_eq!(message_usage.input_tokens, 7);
        assert_eq!(message_usage.output_tokens, 3);

        let delta_usage = extract_token_usage(&delta_payload).expect("delta usage should exist");
        assert_eq!(delta_usage.input_tokens, 11);
        assert_eq!(delta_usage.output_tokens, 4);
    }

    #[test]
    fn stream_token_accumulator_aggregates_usage_from_chunked_sse() {
        let mut acc = StreamTokenAccumulator::default();
        acc.consume_chunk(
            b"event: message_delta\ndata: {\"usage\":{\"input_tokens\":7,\"output_tokens\":3}}\n\n",
        );
        acc.consume_chunk(
            b"data: {\"usage\":{\"input_tokens\":9,\"output_tokens\":3,\"cache_read_input_tokens\":1}}\n",
        );
        acc.consume_chunk(
            b"\ndata: {\"usage\":{\"prompt_tokens\":12,\"completion_tokens\":4,\"prompt_tokens_details\":{\"cache_creation_tokens\":2}}}\n\n",
        );
        acc.consume_chunk(b"data: [DONE]\n\n");

        let usage = acc.into_token_usage().expect("usage should be captured");
        assert_eq!(usage.input_tokens, 12);
        assert_eq!(usage.output_tokens, 4);
        assert_eq!(usage.cache_read_tokens, 1);
        assert_eq!(usage.cache_write_tokens, 2);
    }

    #[test]
    fn stream_token_accumulator_returns_none_when_usage_missing() {
        let mut acc = StreamTokenAccumulator::default();
        acc.consume_chunk(b"event: ping\ndata: hello\n\n");
        acc.consume_chunk(b": keep-alive\n\n");
        assert!(acc.into_token_usage().is_none());
    }

    #[test]
    fn resolve_target_model_uses_group_and_rule_mapping() {
        let mut mappings = HashMap::new();
        mappings.insert("m1".to_string(), "gpt-x".to_string());
        let rule = Rule {
            id: "r1".to_string(),
            name: "rule".to_string(),
            protocol: RuleProtocol::Openai,
            token: "t".to_string(),
            api_address: "https://api.example.com".to_string(),
            default_model: "fallback".to_string(),
            model_mappings: mappings,
            quota: default_rule_quota_config(),
        };
        let group = Group {
            id: "g1".to_string(),
            name: "Group".to_string(),
            models: vec!["m1".to_string()],
            active_rule_id: Some("r1".to_string()),
            rules: vec![rule.clone()],
        };

        let model = resolve_target_model(&rule, &group.models, &json!({ "model": "m1" }));
        assert_eq!(model, "gpt-x");
    }

    #[test]
    fn resolve_target_model_falls_back_to_default_model_when_unmatched() {
        let rule = Rule {
            id: "r1".to_string(),
            name: "rule".to_string(),
            protocol: RuleProtocol::Openai,
            token: "t".to_string(),
            api_address: "https://api.example.com".to_string(),
            default_model: "fallback".to_string(),
            model_mappings: HashMap::new(),
            quota: default_rule_quota_config(),
        };
        let group = Group {
            id: "g1".to_string(),
            name: "Group".to_string(),
            models: vec!["m1".to_string()],
            active_rule_id: Some("r1".to_string()),
            rules: vec![rule.clone()],
        };

        let model = resolve_target_model(&rule, &group.models, &json!({ "model": "unknown" }));
        assert_eq!(model, "fallback");
    }

    #[test]
    fn build_upstream_body_passes_through_request_shape_with_target_model() {
        let out = build_upstream_body(
            &json!({
                "model": "gpt-4.1",
                "input": "hello from responses"
            }),
            "gpt-4.1",
        )
        .expect("mapping should succeed");

        assert_eq!(out["model"], "gpt-4.1");
        assert_eq!(out["input"], "hello from responses");
    }

    #[test]
    fn resolve_upstream_path_uses_rule_protocol_enum_directly() {
        assert_eq!(resolve_upstream_path(&RuleProtocol::Anthropic), "/v1/messages");
        assert_eq!(resolve_upstream_path(&RuleProtocol::Openai), "/v1/responses");
        assert_eq!(
            resolve_upstream_path(&RuleProtocol::OpenaiCompletion),
            "/v1/chat/completions"
        );
    }
}
