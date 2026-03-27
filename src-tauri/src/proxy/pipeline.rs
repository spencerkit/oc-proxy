//! Module Overview
//! Request processing pipeline for /oc endpoints.
//! Handles auth, routing, request/response mapping, upstream I/O, streaming, metrics, and final logging.

use super::observability::{
    append_processing_log, append_processing_log_with_stream_debug, apply_headers,
    extract_token_usage, finalize_log, finalize_log_with_stream_debug, log_simple,
    plain_downstream_headers, plain_headers, proxy_error_response, response_headers_json,
    response_headers_sse, StreamTokenAccumulator,
};
use super::routing::{
    assert_rule_ready, build_rule_headers, detect_entry_protocol, record_route_provider_failure,
    record_route_provider_success, refresh_route_index_if_needed, resolve_runtime_active_route,
    resolve_target_model, resolve_upstream_path, resolve_upstream_url, EntryEndpoint,
    EntryProtocol, ParsedPath, PathEntry, RouteResolution,
};
use super::{
    ServiceState, MAX_REQUEST_BODY_BYTES, MAX_STREAM_LOG_BODY_BYTES,
    MESSAGES_TO_RESPONSES_NON_STREAM_REQUEST_TIMEOUT_MS, NON_STREAM_REQUEST_TIMEOUT_MS,
    STREAM_REQUEST_TIMEOUT_MS,
};
use crate::auth::extract_bearer_token;
use crate::models::{RuleProtocol, TokenUsage};
use crate::transformer::convert::claude_openai_responses::ResponsesToClaudeOptions;
use crate::transformer::{StreamContext, Transformer};
use axum::body::{to_bytes, Body, Bytes};
use axum::extract::{Path, State};
use axum::http::{HeaderMap, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::Utc;
use futures_util::TryStreamExt;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::mpsc;
use uuid::Uuid;

const MAX_SSE_PENDING_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone, Default)]
struct StreamDebugState {
    entered_stream_mode: bool,
    transformer_name: String,
    requires_buffer: bool,
    probe_sse_enabled: bool,
    upstream_content_type: String,
    upstream_chunk_count: usize,
    upstream_chunk_bytes: usize,
    upstream_event_count: usize,
    first_upstream_event: Option<String>,
    first_upstream_data_type: Option<String>,
    upstream_done_seen: bool,
    transformed_chunk_count: usize,
    transformed_chunk_bytes: usize,
    transformed_event_count: usize,
    first_transformed_event: Option<String>,
    first_transformed_data_type: Option<String>,
    transformed_done_seen: bool,
    transformed_message_start_seen: bool,
    transformed_message_delta_seen: bool,
    transformed_message_stop_seen: bool,
    downstream_send_count: usize,
    finalizer_count: usize,
    finalizer_bytes: usize,
}

/// Performs healthz.
pub(super) async fn healthz(State(state): State<ServiceState>) -> Response {
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

/// Lightweight runtime metrics endpoint for renderer polling.
pub(super) async fn metrics_lite(State(state): State<ServiceState>) -> Response {
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

/// `/oc/:group_id` shorthand entry.
///
/// Defaults to chat-completions endpoint to preserve legacy behavior.
pub(super) async fn handle_proxy_root(
    State(state): State<ServiceState>,
    Path(group_id): Path<String>,
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
            suffix: "/chat/completions".to_string(),
        },
    )
    .await
}

/// `/oc/:group_id/*suffix` entry that keeps user-provided endpoint suffix.
pub(super) async fn handle_proxy_suffix(
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

/// Main proxy request pipeline.
///
/// High-level stages:
/// 1. Validate method/auth/path and route selection.
/// 2. Parse request body and resolve target model/protocol.
/// 3. Map request surface if cross-protocol routing is needed.
/// 4. Forward to upstream (streaming or non-streaming).
/// 5. Map response surface back to downstream expectation.
/// 6. Finalize metrics + structured log.
pub(super) async fn handle_proxy_request(
    state: ServiceState,
    method: Method,
    headers: HeaderMap,
    body: Body,
    parsed_path: ParsedPath,
) -> Response {
    let trace_id = Uuid::new_v4().to_string();
    let started = std::time::Instant::now();
    let request_timestamp = Utc::now().to_rfc3339();
    let request_headers_plain = plain_downstream_headers(&headers);

    if method != Method::POST {
        let payload = json!({"error": {"code": "not_found", "message": "Use POST /oc/:groupId/:endpoint (messages/chat/completions/responses)"}});
        return reject_and_log(&state, trace_id, method, &parsed_path, 404, payload).await;
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

    if let Err(msg) = refresh_route_index_if_needed(&state) {
        state.metrics.increment_error();
        return proxy_error_response(500, "proxy_error", &msg, None, "proxy", &trace_id);
    }

    let (
        auth_enabled,
        local_access_token,
        capture_body,
        _strict_mode,
        text_tool_call_fallback_enabled,
    ) = match state.config.read() {
        Ok(cfg) => (
            cfg.server.auth_enabled,
            cfg.server.local_bearer_token.clone(),
            cfg.logging.capture_body,
            cfg.compat.strict_mode,
            cfg.compat.text_tool_call_fallback_enabled,
        ),
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

    if auth_enabled && !request_matches_local_access_token(&headers, &entry, &local_access_token) {
        return reject_and_log(
            &state,
            trace_id,
            method,
            &parsed_path,
            401,
            json!({"error": {"code": "unauthorized", "message": "Missing or invalid access token"}}),
        )
        .await;
    }

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
    let active_route = match resolve_runtime_active_route(&state, &active_route) {
        Ok(route) => route,
        Err(msg) => {
            state.metrics.increment_error();
            return proxy_error_response(500, "proxy_error", &msg, None, "proxy", &trace_id);
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
                );
            }
        }
    };

    let target_model = resolve_target_model(
        &active_route.rule,
        &active_route.group_models,
        &request_body,
    );
    let requested_model = request_body
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or(&active_route.rule.default_model)
        .to_string();
    let target_protocol = active_route.rule.protocol.clone();
    let declared_tool_names = extract_declared_tool_names(&request_body);
    let enable_text_tool_call_fallback = text_tool_call_fallback_enabled
        && matches!(entry.endpoint, EntryEndpoint::Messages)
        && matches!(target_protocol, RuleProtocol::Openai)
        && !declared_tool_names.is_empty();
    let transformer = match prepare_transformer_for_route(
        &entry,
        &target_protocol,
        &target_model,
        enable_text_tool_call_fallback,
        &declared_tool_names,
    ) {
        Ok(transformer) => transformer,
        Err(msg) => {
            state.metrics.increment_error();
            finalize_log(
                &state,
                &request_timestamp,
                &trace_id,
                &method,
                &parsed_path,
                &active_route.group_name,
                &active_route.rule,
                &entry,
                Some(&requested_model),
                Some(&target_model),
                None,
                Some(request_headers_plain.clone()),
                None,
                Some(request_body.clone()),
                None,
                Some(json!({
                    "error": {
                        "code": "proxy_error",
                        "message": msg.clone(),
                    }
                })),
                None,
                None,
                Some(422),
                None,
                None,
                Some(response_headers_json(&trace_id)),
                None,
                started.elapsed().as_millis() as u64,
                "error",
                capture_body,
            );
            return proxy_error_response(422, "proxy_error", &msg, None, "proxy", &trace_id);
        }
    };
    let transformer_name = transformer.name().to_string();
    let upstream_path = resolve_upstream_path(&target_protocol);
    let upstream_url = match resolve_upstream_url(&active_route.rule.api_address, upstream_path) {
        Ok(v) => v,
        Err(msg) => {
            state.metrics.increment_error();
            finalize_log(
                &state,
                &request_timestamp,
                &trace_id,
                &method,
                &parsed_path,
                &active_route.group_name,
                &active_route.rule,
                &entry,
                Some(&requested_model),
                Some(&target_model),
                None,
                Some(request_headers_plain.clone()),
                None,
                Some(request_body.clone()),
                None,
                Some(json!({
                    "error": {
                        "code": "proxy_error",
                        "message": msg.clone(),
                    }
                })),
                None,
                None,
                Some(400),
                None,
                None,
                Some(response_headers_json(&trace_id)),
                None,
                started.elapsed().as_millis() as u64,
                "error",
                capture_body,
            );
            return proxy_error_response(400, "proxy_error", &msg, None, "proxy", &trace_id);
        }
    };

    let upstream_body = match build_upstream_body(transformer.as_ref(), &request_body) {
        Ok(v) => v,
        Err(msg) => {
            state.metrics.increment_error();
            finalize_log(
                &state,
                &request_timestamp,
                &trace_id,
                &method,
                &parsed_path,
                &active_route.group_name,
                &active_route.rule,
                &entry,
                Some(&requested_model),
                Some(&target_model),
                Some(&upstream_url),
                Some(request_headers_plain.clone()),
                None,
                Some(request_body.clone()),
                None,
                Some(json!({
                    "error": {
                        "code": "proxy_error",
                        "message": msg.clone(),
                    }
                })),
                None,
                None,
                Some(422),
                None,
                None,
                Some(response_headers_json(&trace_id)),
                None,
                started.elapsed().as_millis() as u64,
                "error",
                capture_body,
            );
            return proxy_error_response(422, "proxy_error", &msg, None, "proxy", &trace_id);
        }
    };

    let stream = upstream_body
        .get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    state.metrics.increment_request(stream);

    let upstream_headers = build_rule_headers(&target_protocol, &active_route.rule);
    append_processing_log(
        &state,
        &request_timestamp,
        &trace_id,
        &method,
        &parsed_path,
        &active_route.group_name,
        &active_route.rule,
        &entry,
        Some(&requested_model),
        Some(&target_model),
        Some(&upstream_url),
        Some(request_headers_plain.clone()),
        Some(upstream_headers.clone()),
        Some(request_body.clone()),
        Some(upstream_body.clone()),
        capture_body,
    );
    let request_timeout_ms = resolve_request_timeout_ms(stream, &transformer_name);

    let request_builder = state
        .client
        .post(upstream_url.clone())
        .headers(reqwest::header::HeaderMap::from_iter(
            upstream_headers.iter().filter_map(|(k, v)| {
                let name = reqwest::header::HeaderName::from_bytes(k.as_bytes()).ok()?;
                let value = reqwest::header::HeaderValue::from_str(v).ok()?;
                Some((name, value))
            }),
        ))
        .json(&upstream_body);

    let upstream_resp = if stream {
        match tokio::time::timeout(
            std::time::Duration::from_millis(request_timeout_ms),
            request_builder.send(),
        )
        .await
        {
            Ok(Ok(r)) => r,
            Ok(Err(err)) => {
                let err_msg = format!("Upstream request failed: {err}");
                if classify_provider_side_failure(Some(&err_msg), None) {
                    let _ = record_route_provider_failure(
                        &state,
                        &active_route.group_id,
                        &active_route.rule.id,
                        &active_route.provider_ids,
                        &crate::proxy::failover::FailoverConfigSnapshot {
                            enabled: active_route.failover.enabled,
                            failure_threshold: active_route.failover.failure_threshold,
                            cooldown_seconds: active_route.failover.cooldown_seconds,
                        },
                    );
                }
                state.metrics.increment_error();
                finalize_log(
                    &state,
                    &request_timestamp,
                    &trace_id,
                    &method,
                    &parsed_path,
                    &active_route.group_name,
                    &active_route.rule,
                    &entry,
                    Some(&requested_model),
                    Some(&target_model),
                    Some(&upstream_url),
                    Some(request_headers_plain.clone()),
                    Some(upstream_headers.clone()),
                    Some(request_body.clone()),
                    Some(upstream_body.clone()),
                    Some(json!({
                        "error": {
                            "code": "upstream_error",
                            "message": err_msg.clone(),
                        }
                    })),
                    None,
                    None,
                    Some(502),
                    None,
                    None,
                    Some(response_headers_json(&trace_id)),
                    None,
                    started.elapsed().as_millis() as u64,
                    "error",
                    capture_body,
                );
                return proxy_error_response(
                    502,
                    "upstream_error",
                    &err_msg,
                    None,
                    "proxy",
                    &trace_id,
                );
            }
            Err(_) => {
                let err_msg = format!(
                    "Upstream response header timeout exceeded after {request_timeout_ms}ms"
                );
                if classify_provider_side_failure(Some(&err_msg), Some(504)) {
                    let _ = record_route_provider_failure(
                        &state,
                        &active_route.group_id,
                        &active_route.rule.id,
                        &active_route.provider_ids,
                        &crate::proxy::failover::FailoverConfigSnapshot {
                            enabled: active_route.failover.enabled,
                            failure_threshold: active_route.failover.failure_threshold,
                            cooldown_seconds: active_route.failover.cooldown_seconds,
                        },
                    );
                }
                state.metrics.increment_error();
                finalize_log(
                    &state,
                    &request_timestamp,
                    &trace_id,
                    &method,
                    &parsed_path,
                    &active_route.group_name,
                    &active_route.rule,
                    &entry,
                    Some(&requested_model),
                    Some(&target_model),
                    Some(&upstream_url),
                    Some(request_headers_plain.clone()),
                    Some(upstream_headers.clone()),
                    Some(request_body.clone()),
                    Some(upstream_body.clone()),
                    Some(json!({
                        "error": {
                            "code": "upstream_error",
                            "message": err_msg.clone(),
                        }
                    })),
                    None,
                    None,
                    Some(504),
                    None,
                    None,
                    Some(response_headers_json(&trace_id)),
                    None,
                    started.elapsed().as_millis() as u64,
                    "error",
                    capture_body,
                );
                return proxy_error_response(
                    504,
                    "upstream_error",
                    &err_msg,
                    None,
                    "proxy",
                    &trace_id,
                );
            }
        }
    } else {
        match request_builder
            .timeout(std::time::Duration::from_millis(request_timeout_ms))
            .send()
            .await
        {
            Ok(r) => r,
            Err(err) => {
                let err_msg = format!("Upstream request failed: {err}");
                if classify_provider_side_failure(Some(&err_msg), None) {
                    let _ = record_route_provider_failure(
                        &state,
                        &active_route.group_id,
                        &active_route.rule.id,
                        &active_route.provider_ids,
                        &crate::proxy::failover::FailoverConfigSnapshot {
                            enabled: active_route.failover.enabled,
                            failure_threshold: active_route.failover.failure_threshold,
                            cooldown_seconds: active_route.failover.cooldown_seconds,
                        },
                    );
                }
                state.metrics.increment_error();
                finalize_log(
                    &state,
                    &request_timestamp,
                    &trace_id,
                    &method,
                    &parsed_path,
                    &active_route.group_name,
                    &active_route.rule,
                    &entry,
                    Some(&requested_model),
                    Some(&target_model),
                    Some(&upstream_url),
                    Some(request_headers_plain.clone()),
                    Some(upstream_headers.clone()),
                    Some(request_body.clone()),
                    Some(upstream_body.clone()),
                    Some(json!({
                        "error": {
                            "code": "upstream_error",
                            "message": err_msg.clone(),
                        }
                    })),
                    None,
                    None,
                    Some(502),
                    None,
                    None,
                    Some(response_headers_json(&trace_id)),
                    None,
                    started.elapsed().as_millis() as u64,
                    "error",
                    capture_body,
                );
                return proxy_error_response(
                    502,
                    "upstream_error",
                    &err_msg,
                    None,
                    "proxy",
                    &trace_id,
                );
            }
        }
    };

    let upstream_status = upstream_resp.status().as_u16();
    let upstream_is_error = upstream_status >= 400;
    if upstream_is_error && classify_provider_side_failure(None, Some(upstream_status)) {
        let _ = record_route_provider_failure(
            &state,
            &active_route.group_id,
            &active_route.rule.id,
            &active_route.provider_ids,
            &crate::proxy::failover::FailoverConfigSnapshot {
                enabled: active_route.failover.enabled,
                failure_threshold: active_route.failover.failure_threshold,
                cooldown_seconds: active_route.failover.cooldown_seconds,
            },
        );
    }
    let upstream_headers_plain = plain_headers(upstream_resp.headers());
    let upstream_ct = upstream_resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_lowercase();
    let sse_fallback_probe_enabled = stream
        && !upstream_ct.contains("text/event-stream")
        && !upstream_ct.contains("application/json")
        && (upstream_ct.is_empty() || upstream_ct.contains("text/plain"));

    if upstream_ct.contains("text/event-stream") || sse_fallback_probe_enabled {
        let mut stream_ctx = StreamContext::new();
        stream_ctx.model_name = target_model.clone();
        if enable_text_tool_call_fallback {
            stream_ctx.text_tool_call_fallback_enabled = true;
            stream_ctx.allowed_tool_names = declared_tool_names.clone();
        }
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
        let stream_request_timestamp = request_timestamp.clone();
        let stream_request_headers = request_headers_plain.clone();
        let stream_forward_request_headers = upstream_headers.clone();
        let stream_request_body = request_body.clone();
        let stream_upstream_body = upstream_body.clone();
        let stream_upstream_headers = upstream_headers_plain.clone();
        let stream_capture_body = capture_body;
        let stream_debug_capture_body = cfg!(debug_assertions);
        let stream_upstream_status = upstream_status;
        let stream_upstream_is_error = upstream_is_error;
        let stream_group_id = active_route.group_id.clone();
        let stream_provider_ids = active_route.provider_ids.clone();
        let stream_failover_config = crate::proxy::failover::FailoverConfigSnapshot {
            enabled: active_route.failover.enabled,
            failure_threshold: active_route.failover.failure_threshold,
            cooldown_seconds: active_route.failover.cooldown_seconds,
        };
        let stream_upstream_ct = upstream_ct.clone();
        let stream_started = started;
        let stream_transformer = transformer.clone();
        let stream_transformer_name = transformer_name.clone();
        let stream_requires_buffer = stream_requires_sse_event_buffer(&stream_transformer_name);
        let mut stream_ctx_moved = stream_ctx;
        let mut stream_probe_sse = sse_fallback_probe_enabled;

        tokio::spawn(async move {
            let mut bytes_stream = upstream_resp.bytes_stream();
            let mut usage_acc = StreamTokenAccumulator::default();
            let mut stream_failed = false;
            let mut provider_side_stream_failure = false;
            let mut downstream_closed = false;
            let mut stream_upstream_response_bytes = Vec::<u8>::new();
            let mut stream_upstream_response_truncated = false;
            let mut stream_upstream_response_debug_bytes = Vec::<u8>::new();
            let mut stream_transformed_response_bytes = Vec::<u8>::new();
            let mut stream_transformed_response_truncated = false;
            let mut stream_transformed_response_debug_bytes = Vec::<u8>::new();
            let mut sse_pending = Vec::<u8>::new();
            let mut stream_debug = StreamDebugState {
                entered_stream_mode: true,
                transformer_name: stream_transformer_name.clone(),
                requires_buffer: stream_requires_buffer,
                probe_sse_enabled: stream_probe_sse,
                upstream_content_type: stream_upstream_ct.clone(),
                ..Default::default()
            };

            stream_debug_terminal(
                &stream_trace_id,
                "enter",
                json!({
                    "transformer": stream_transformer_name.clone(),
                    "requiresBuffer": stream_requires_buffer,
                    "probeSseEnabled": stream_probe_sse,
                    "upstreamStatus": stream_upstream_status,
                    "upstreamContentType": stream_upstream_ct.clone(),
                    "requestPath": format!("/oc/{}{}", stream_parsed_path.group_id, stream_parsed_path.suffix),
                }),
            );
            append_processing_log_with_stream_debug(
                &stream_state,
                &stream_request_timestamp,
                &stream_trace_id,
                &stream_method,
                &stream_parsed_path,
                &stream_group_name,
                &stream_rule,
                &stream_entry,
                Some(&stream_requested_model),
                Some(&stream_target_model),
                Some(&stream_upstream_url),
                Some(stream_request_headers.clone()),
                Some(stream_forward_request_headers.clone()),
                Some(stream_request_body.clone()),
                Some(stream_upstream_body.clone()),
                stream_capture_body,
                Some(stream_debug_summary_json(&stream_debug)),
            );

            loop {
                match bytes_stream.try_next().await {
                    Ok(Some(bytes)) => {
                        stream_debug_update_upstream_chunk(&mut stream_debug, bytes.as_ref());
                        stream_debug_terminal(
                            &stream_trace_id,
                            "upstream_chunk",
                            json!({
                                "index": stream_debug.upstream_chunk_count,
                                "bytes": bytes.len(),
                                "pendingBytesBeforeSplit": sse_pending.len(),
                            }),
                        );
                        if stream_debug.upstream_chunk_count == 1 {
                            append_processing_log_with_stream_debug(
                                &stream_state,
                                &stream_request_timestamp,
                                &stream_trace_id,
                                &stream_method,
                                &stream_parsed_path,
                                &stream_group_name,
                                &stream_rule,
                                &stream_entry,
                                Some(&stream_requested_model),
                                Some(&stream_target_model),
                                Some(&stream_upstream_url),
                                Some(stream_request_headers.clone()),
                                Some(stream_forward_request_headers.clone()),
                                Some(stream_request_body.clone()),
                                Some(stream_upstream_body.clone()),
                                stream_capture_body,
                                Some(stream_debug_summary_json(&stream_debug)),
                            );
                        }
                        if stream_probe_sse {
                            stream_probe_sse = false;
                            if !looks_like_sse_prelude(bytes.as_ref()) {
                                stream_failed = true;
                                let _ = tx
                                    .send(Err(std::io::Error::new(
                                        std::io::ErrorKind::Other,
                                        "upstream stream probe failed: non-SSE payload",
                                    )))
                                    .await;
                                break;
                            }
                        }
                        usage_acc.consume_chunk(bytes.as_ref());
                        capture_stream_chunk(
                            bytes.as_ref(),
                            stream_capture_body,
                            &mut stream_upstream_response_bytes,
                            &mut stream_upstream_response_truncated,
                            stream_debug_capture_body,
                            &mut stream_upstream_response_debug_bytes,
                        );
                        let outgoing_chunks = if !stream_requires_buffer {
                            vec![bytes]
                        } else {
                            sse_pending.extend_from_slice(bytes.as_ref());
                            if sse_pending.len() > MAX_SSE_PENDING_BYTES {
                                stream_failed = true;
                                let finalizer = finalize_stream_transform(
                                    &stream_transformer_name,
                                    &mut stream_ctx_moved,
                                );
                                if !finalizer.is_empty() {
                                    capture_stream_chunk(
                                        finalizer.as_slice(),
                                        stream_capture_body,
                                        &mut stream_transformed_response_bytes,
                                        &mut stream_transformed_response_truncated,
                                        stream_debug_capture_body,
                                        &mut stream_transformed_response_debug_bytes,
                                    );
                                    if tx.send(Ok(Bytes::from(finalizer))).await.is_err() {
                                        downstream_closed = true;
                                    }
                                } else {
                                    let _ = tx
                                        .send(Err(std::io::Error::new(
                                            std::io::ErrorKind::Other,
                                            "stream transform buffer overflow",
                                        )))
                                        .await;
                                }
                                break;
                            }

                            let mut transformed_chunks = Vec::new();
                            while let Some(event) = pop_sse_event(&mut sse_pending) {
                                let upstream_event_detail = stream_debug_update_upstream_event(
                                    &mut stream_debug,
                                    event.as_ref(),
                                );
                                stream_debug_terminal(
                                    &stream_trace_id,
                                    "upstream_event",
                                    json!({
                                        "index": stream_debug.upstream_event_count,
                                        "detail": upstream_event_detail,
                                        "pendingBytesAfterPop": sse_pending.len(),
                                    }),
                                );
                                if stream_debug.upstream_event_count == 1 {
                                    append_processing_log_with_stream_debug(
                                        &stream_state,
                                        &stream_request_timestamp,
                                        &stream_trace_id,
                                        &stream_method,
                                        &stream_parsed_path,
                                        &stream_group_name,
                                        &stream_rule,
                                        &stream_entry,
                                        Some(&stream_requested_model),
                                        Some(&stream_target_model),
                                        Some(&stream_upstream_url),
                                        Some(stream_request_headers.clone()),
                                        Some(stream_forward_request_headers.clone()),
                                        Some(stream_request_body.clone()),
                                        Some(stream_upstream_body.clone()),
                                        stream_capture_body,
                                        Some(stream_debug_summary_json(&stream_debug)),
                                    );
                                }
                                match transform_sse_event(
                                    stream_transformer.as_ref(),
                                    event.as_ref(),
                                    &mut stream_ctx_moved,
                                ) {
                                    Ok(converted) => {
                                        if !converted.is_empty() {
                                            let transformed_detail =
                                                stream_debug_update_transformed_chunk(
                                                    &mut stream_debug,
                                                    converted.as_slice(),
                                                );
                                            stream_debug_terminal(
                                                &stream_trace_id,
                                                "transform_ok",
                                                json!({
                                                    "index": stream_debug.upstream_event_count,
                                                    "detail": transformed_detail,
                                                }),
                                            );
                                            if stream_debug.transformed_chunk_count == 1 {
                                                append_processing_log_with_stream_debug(
                                                    &stream_state,
                                                    &stream_request_timestamp,
                                                    &stream_trace_id,
                                                    &stream_method,
                                                    &stream_parsed_path,
                                                    &stream_group_name,
                                                    &stream_rule,
                                                    &stream_entry,
                                                    Some(&stream_requested_model),
                                                    Some(&stream_target_model),
                                                    Some(&stream_upstream_url),
                                                    Some(stream_request_headers.clone()),
                                                    Some(stream_forward_request_headers.clone()),
                                                    Some(stream_request_body.clone()),
                                                    Some(stream_upstream_body.clone()),
                                                    stream_capture_body,
                                                    Some(stream_debug_summary_json(&stream_debug)),
                                                );
                                            }
                                            transformed_chunks.push(Bytes::from(converted));
                                        } else {
                                            stream_debug_terminal(
                                                &stream_trace_id,
                                                "transform_ok",
                                                json!({
                                                    "index": stream_debug.upstream_event_count,
                                                    "detail": {
                                                        "bytes": 0,
                                                        "eventCount": 0
                                                    },
                                                }),
                                            );
                                        }
                                    }
                                    Err(err) => {
                                        stream_failed = true;
                                        stream_debug_terminal(
                                            &stream_trace_id,
                                            "transform_error",
                                            json!({
                                                "index": stream_debug.upstream_event_count,
                                                "message": err,
                                            }),
                                        );
                                        let finalizer = finalize_stream_transform(
                                            &stream_transformer_name,
                                            &mut stream_ctx_moved,
                                        );
                                        if !finalizer.is_empty() {
                                            stream_debug.finalizer_count += 1;
                                            stream_debug.finalizer_bytes += finalizer.len();
                                            let finalizer_detail =
                                                stream_debug_update_transformed_chunk(
                                                    &mut stream_debug,
                                                    finalizer.as_slice(),
                                                );
                                            stream_debug_terminal(
                                                &stream_trace_id,
                                                "finalizer",
                                                json!({
                                                    "cause": "transform_error",
                                                    "detail": finalizer_detail,
                                                }),
                                            );
                                            capture_stream_chunk(
                                                finalizer.as_slice(),
                                                stream_capture_body,
                                                &mut stream_transformed_response_bytes,
                                                &mut stream_transformed_response_truncated,
                                                stream_debug_capture_body,
                                                &mut stream_transformed_response_debug_bytes,
                                            );
                                            if tx.send(Ok(Bytes::from(finalizer))).await.is_err() {
                                                downstream_closed = true;
                                            }
                                        } else {
                                            let _ = tx
                                                .send(Err(std::io::Error::new(
                                                    std::io::ErrorKind::Other,
                                                    "stream transform failed",
                                                )))
                                                .await;
                                        }
                                        break;
                                    }
                                }
                            }

                            if stream_failed {
                                break;
                            }
                            transformed_chunks
                        };

                        for outgoing in outgoing_chunks {
                            capture_stream_chunk(
                                outgoing.as_ref(),
                                stream_capture_body,
                                &mut stream_transformed_response_bytes,
                                &mut stream_transformed_response_truncated,
                                stream_debug_capture_body,
                                &mut stream_transformed_response_debug_bytes,
                            );
                            stream_debug.downstream_send_count += 1;
                            stream_debug_terminal(
                                &stream_trace_id,
                                "downstream_send",
                                json!({
                                    "index": stream_debug.downstream_send_count,
                                    "bytes": outgoing.len(),
                                }),
                            );
                            if tx.send(Ok(outgoing)).await.is_err() {
                                downstream_closed = true;
                                break;
                            }
                        }
                        if downstream_closed {
                            break;
                        }
                    }
                    Ok(None) => {
                        if stream_requires_buffer && !sse_pending.is_empty() {
                            let tail_event = std::mem::take(&mut sse_pending);
                            match transform_sse_event(
                                stream_transformer.as_ref(),
                                tail_event.as_ref(),
                                &mut stream_ctx_moved,
                            ) {
                                Ok(converted) => {
                                    if !converted.is_empty() {
                                        capture_stream_chunk(
                                            converted.as_slice(),
                                            stream_capture_body,
                                            &mut stream_transformed_response_bytes,
                                            &mut stream_transformed_response_truncated,
                                            stream_debug_capture_body,
                                            &mut stream_transformed_response_debug_bytes,
                                        );
                                        if tx.send(Ok(Bytes::from(converted))).await.is_err() {
                                            downstream_closed = true;
                                        }
                                    }
                                }
                                Err(_) => {
                                    stream_failed = true;
                                    stream_debug_terminal(
                                        &stream_trace_id,
                                        "tail_transform_error",
                                        json!({
                                            "message": "stream transform failed on tail event",
                                        }),
                                    );
                                    let finalizer = finalize_stream_transform(
                                        &stream_transformer_name,
                                        &mut stream_ctx_moved,
                                    );
                                    if !finalizer.is_empty() {
                                        stream_debug.finalizer_count += 1;
                                        stream_debug.finalizer_bytes += finalizer.len();
                                        let finalizer_detail =
                                            stream_debug_update_transformed_chunk(
                                                &mut stream_debug,
                                                finalizer.as_slice(),
                                            );
                                        stream_debug_terminal(
                                            &stream_trace_id,
                                            "finalizer",
                                            json!({
                                                "cause": "tail_transform_error",
                                                "detail": finalizer_detail,
                                            }),
                                        );
                                        capture_stream_chunk(
                                            finalizer.as_slice(),
                                            stream_capture_body,
                                            &mut stream_transformed_response_bytes,
                                            &mut stream_transformed_response_truncated,
                                            stream_debug_capture_body,
                                            &mut stream_transformed_response_debug_bytes,
                                        );
                                        if tx.send(Ok(Bytes::from(finalizer))).await.is_err() {
                                            downstream_closed = true;
                                        }
                                    } else {
                                        let _ = tx
                                            .send(Err(std::io::Error::new(
                                                std::io::ErrorKind::Other,
                                                "stream transform failed",
                                            )))
                                            .await;
                                    }
                                }
                            }
                        }
                        if !downstream_closed {
                            let finalizer = finalize_stream_transform(
                                &stream_transformer_name,
                                &mut stream_ctx_moved,
                            );
                            if !finalizer.is_empty() {
                                stream_debug.finalizer_count += 1;
                                stream_debug.finalizer_bytes += finalizer.len();
                                let finalizer_detail = stream_debug_update_transformed_chunk(
                                    &mut stream_debug,
                                    finalizer.as_slice(),
                                );
                                stream_debug_terminal(
                                    &stream_trace_id,
                                    "finalizer",
                                    json!({
                                        "cause": "eof",
                                        "detail": finalizer_detail,
                                    }),
                                );
                                capture_stream_chunk(
                                    finalizer.as_slice(),
                                    stream_capture_body,
                                    &mut stream_transformed_response_bytes,
                                    &mut stream_transformed_response_truncated,
                                    stream_debug_capture_body,
                                    &mut stream_transformed_response_debug_bytes,
                                );
                                if tx.send(Ok(Bytes::from(finalizer))).await.is_err() {
                                    downstream_closed = true;
                                }
                            }
                        }
                        break;
                    }
                    Err(_) => {
                        stream_failed = true;
                        provider_side_stream_failure = true;
                        stream_debug_terminal(
                            &stream_trace_id,
                            "read_error",
                            json!({
                                "message": "stream read failed",
                            }),
                        );
                        let finalizer = finalize_stream_transform(
                            &stream_transformer_name,
                            &mut stream_ctx_moved,
                        );
                        if !finalizer.is_empty() {
                            stream_debug.finalizer_count += 1;
                            stream_debug.finalizer_bytes += finalizer.len();
                            let finalizer_detail = stream_debug_update_transformed_chunk(
                                &mut stream_debug,
                                finalizer.as_slice(),
                            );
                            stream_debug_terminal(
                                &stream_trace_id,
                                "finalizer",
                                json!({
                                    "cause": "read_error",
                                    "detail": finalizer_detail,
                                }),
                            );
                            capture_stream_chunk(
                                finalizer.as_slice(),
                                stream_capture_body,
                                &mut stream_transformed_response_bytes,
                                &mut stream_transformed_response_truncated,
                                stream_debug_capture_body,
                                &mut stream_transformed_response_debug_bytes,
                            );
                            if tx.send(Ok(Bytes::from(finalizer))).await.is_err() {
                                downstream_closed = true;
                            }
                        } else {
                            let _ = tx
                                .send(Err(std::io::Error::new(
                                    std::io::ErrorKind::Other,
                                    "stream read failed",
                                )))
                                .await;
                        }
                        break;
                    }
                }
            }

            if !stream_failed && !downstream_closed {
                // Transformer finalization removed - passthrough mode
            }

            stream_debug_terminal(
                &stream_trace_id,
                "complete",
                json!({
                    "streamFailed": stream_failed,
                    "downstreamClosed": downstream_closed,
                    "upstreamIsError": stream_upstream_is_error,
                    "summary": stream_debug_summary_json(&stream_debug),
                }),
            );

            let token_usage = usage_acc.into_token_usage();
            if let Some(ref usage) = token_usage {
                stream_state.metrics.add_token_usage(usage);
            }
            stream_state
                .metrics
                .add_latency(stream_started.elapsed().as_millis() as u64);
            if stream_failed || stream_upstream_is_error {
                stream_state.metrics.increment_error();
            }

            match classify_stream_failover_outcome(
                stream_failed,
                downstream_closed,
                stream_upstream_is_error,
                provider_side_stream_failure,
            ) {
                StreamFailoverOutcome::ProviderFailure => {
                    let _ = record_route_provider_failure(
                        &stream_state,
                        &stream_group_id,
                        &stream_rule.id,
                        &stream_provider_ids,
                        &stream_failover_config,
                    );
                }
                StreamFailoverOutcome::Success => {
                    let _ = record_route_provider_success(
                        &stream_state,
                        &stream_group_id,
                        &stream_rule.id,
                    );
                }
                StreamFailoverOutcome::Ignore => {}
            }

            let stream_response_body = if stream_capture_body {
                Some(build_stream_log_body(
                    "upstream_raw",
                    &stream_upstream_response_bytes,
                    stream_upstream_response_truncated,
                ))
            } else {
                Some(json!({"stream": true, "source": "upstream_raw"}))
            };
            let stream_transformed_response_body = if stream_capture_body {
                Some(build_stream_log_body(
                    "downstream_transformed",
                    &stream_transformed_response_bytes,
                    stream_transformed_response_truncated,
                ))
            } else {
                Some(json!({"stream": true, "source": "downstream_transformed"}))
            };
            let stream_debug_response_body = if stream_debug_capture_body {
                Some(build_stream_log_body(
                    "upstream_raw",
                    &stream_upstream_response_debug_bytes,
                    false,
                ))
            } else {
                None
            };
            let mut response_headers = response_headers_sse(&stream_trace_id);
            finalize_log_with_stream_debug(
                &stream_state,
                &stream_request_timestamp,
                &stream_trace_id,
                &stream_method,
                &stream_parsed_path,
                &stream_group_name,
                &stream_rule,
                &stream_entry,
                Some(&stream_requested_model),
                Some(&stream_target_model),
                Some(&stream_upstream_url),
                Some(stream_request_headers),
                Some(stream_forward_request_headers),
                Some(stream_request_body),
                Some(stream_upstream_body),
                stream_response_body,
                stream_transformed_response_body,
                stream_debug_response_body,
                if stream_failed {
                    Some(502)
                } else if stream_upstream_is_error {
                    Some(stream_upstream_status)
                } else {
                    Some(200)
                },
                Some(stream_upstream_status),
                Some(stream_upstream_headers),
                Some(response_headers.drain().collect()),
                token_usage,
                stream_started.elapsed().as_millis() as u64,
                if stream_failed || stream_upstream_is_error {
                    "error"
                } else {
                    "ok"
                },
                stream_capture_body,
                Some(stream_debug_summary_json(&stream_debug)),
            );
        });

        let body = Body::from_stream(futures_util::stream::unfold(rx, |mut rx| async move {
            rx.recv().await.map(|item| (item, rx))
        }));
        let mut resp = Response::new(body);
        *resp.status_mut() = StatusCode::from_u16(if upstream_is_error {
            upstream_status
        } else {
            200
        })
        .unwrap_or(StatusCode::OK);

        let response_headers = response_headers_sse(&trace_id);
        for (k, v) in &response_headers {
            let _ = resp.headers_mut().insert(
                axum::http::header::HeaderName::from_bytes(k.as_bytes()).unwrap(),
                axum::http::HeaderValue::from_str(v)
                    .unwrap_or_else(|_| axum::http::HeaderValue::from_static("")),
            );
        }
        return resp;
    }

    let upstream_text = match upstream_resp.text().await {
        Ok(v) => v,
        Err(err) => {
            let err_msg = format!("Failed to read upstream response: {err}");
            if classify_provider_side_failure(Some(&err_msg), Some(upstream_status)) {
                let _ = record_route_provider_failure(
                    &state,
                    &active_route.group_id,
                    &active_route.rule.id,
                    &active_route.provider_ids,
                    &crate::proxy::failover::FailoverConfigSnapshot {
                        enabled: active_route.failover.enabled,
                        failure_threshold: active_route.failover.failure_threshold,
                        cooldown_seconds: active_route.failover.cooldown_seconds,
                    },
                );
            }
            state.metrics.increment_error();
            finalize_log(
                &state,
                &request_timestamp,
                &trace_id,
                &method,
                &parsed_path,
                &active_route.group_name,
                &active_route.rule,
                &entry,
                Some(&requested_model),
                Some(&target_model),
                Some(&upstream_url),
                Some(request_headers_plain.clone()),
                Some(upstream_headers.clone()),
                Some(request_body.clone()),
                Some(upstream_body.clone()),
                Some(json!({
                    "error": {
                        "code": "upstream_error",
                        "message": err_msg.clone(),
                    }
                })),
                None,
                None,
                Some(502),
                Some(upstream_status),
                Some(upstream_headers_plain.clone()),
                Some(response_headers_json(&trace_id)),
                None,
                started.elapsed().as_millis() as u64,
                "error",
                capture_body,
            );
            return proxy_error_response(
                502,
                "upstream_error",
                &err_msg,
                Some(upstream_status),
                "proxy",
                &trace_id,
            );
        }
    };

    let upstream_json = match serde_json::from_str::<Value>(&upstream_text) {
        Ok(v) => v,
        Err(_) => {
            let err_msg = format!(
                "Upstream returned non-JSON response: {}",
                upstream_text.chars().take(200).collect::<String>()
            );
            state.metrics.increment_error();
            finalize_log(
                &state,
                &request_timestamp,
                &trace_id,
                &method,
                &parsed_path,
                &active_route.group_name,
                &active_route.rule,
                &entry,
                Some(&requested_model),
                Some(&target_model),
                Some(&upstream_url),
                Some(request_headers_plain.clone()),
                Some(upstream_headers.clone()),
                Some(request_body.clone()),
                Some(upstream_body.clone()),
                Some(json!({
                    "error": {
                        "code": "upstream_error",
                        "message": err_msg.clone(),
                    },
                    "upstream_raw": upstream_text.chars().take(200).collect::<String>(),
                })),
                None,
                None,
                Some(502),
                Some(upstream_status),
                Some(upstream_headers_plain.clone()),
                Some(response_headers_json(&trace_id)),
                None,
                started.elapsed().as_millis() as u64,
                "error",
                capture_body,
            );
            return proxy_error_response(
                502,
                "upstream_error",
                &err_msg,
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
        finalize_log(
            &state,
            &request_timestamp,
            &trace_id,
            &method,
            &parsed_path,
            &active_route.group_name,
            &active_route.rule,
            &entry,
            Some(&requested_model),
            Some(&target_model),
            Some(&upstream_url),
            Some(request_headers_plain.clone()),
            Some(upstream_headers.clone()),
            Some(request_body.clone()),
            Some(upstream_body.clone()),
            Some(upstream_json.clone()),
            None,
            None,
            Some(upstream_status),
            Some(upstream_status),
            Some(upstream_headers_plain.clone()),
            Some(response_headers_json(&trace_id)),
            extract_token_usage(&upstream_json),
            started.elapsed().as_millis() as u64,
            "error",
            capture_body,
        );
        return proxy_error_response(
            upstream_status,
            "upstream_error",
            &msg,
            Some(upstream_status),
            "proxy",
            &trace_id,
        );
    }

    let output_body = map_response_body(transformer.as_ref(), &upstream_json);

    let token_usage = merge_token_usage(
        extract_token_usage(&upstream_json),
        extract_token_usage(&output_body),
    );
    let _ = record_route_provider_success(&state, &active_route.group_id, &active_route.rule.id);
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
        &request_timestamp,
        &trace_id,
        &method,
        &parsed_path,
        &active_route.group_name,
        &active_route.rule,
        &entry,
        Some(&requested_model),
        Some(&target_model),
        Some(&upstream_url),
        Some(request_headers_plain),
        Some(upstream_headers),
        Some(request_body),
        Some(upstream_body),
        Some(upstream_json.clone()),
        Some(output_body),
        None,
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

fn classify_provider_side_failure(
    error_message: Option<&str>,
    upstream_status: Option<u16>,
) -> bool {
    if let Some(status) = upstream_status {
        if status == 429 || status >= 500 {
            return true;
        }
    }

    let normalized = error_message
        .map(|message| message.trim().to_ascii_lowercase())
        .unwrap_or_default();
    if normalized.is_empty() {
        return false;
    }

    normalized.contains("timeout")
        || normalized.contains("timed out")
        || normalized.contains("network")
        || normalized.contains("connection")
        || normalized.contains("upstream request failed")
        || normalized.contains("failed to read upstream response")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StreamFailoverOutcome {
    Success,
    ProviderFailure,
    Ignore,
}

fn classify_stream_failover_outcome(
    stream_failed: bool,
    downstream_closed: bool,
    upstream_is_error: bool,
    provider_side_stream_failure: bool,
) -> StreamFailoverOutcome {
    if upstream_is_error {
        return StreamFailoverOutcome::Ignore;
    }
    if provider_side_stream_failure && stream_failed {
        return StreamFailoverOutcome::ProviderFailure;
    }
    if downstream_closed {
        return StreamFailoverOutcome::Ignore;
    }
    if stream_failed {
        return StreamFailoverOutcome::Ignore;
    }
    StreamFailoverOutcome::Success
}

fn extract_header_token(headers: &HeaderMap, header_name: &str) -> Option<String> {
    let raw = headers.get(header_name)?.to_str().ok()?.trim();
    if raw.is_empty() {
        return None;
    }
    Some(raw.to_string())
}

fn request_matches_local_access_token(
    headers: &HeaderMap,
    entry: &PathEntry,
    expected_token: &str,
) -> bool {
    let expected = expected_token.trim();
    if expected.is_empty() {
        return false;
    }

    let bearer_matches = extract_bearer_token(headers)
        .map(|token| token == expected)
        .unwrap_or(false);

    match entry.protocol {
        EntryProtocol::Anthropic => {
            let api_key_matches = extract_header_token(headers, "x-api-key")
                .map(|token| token == expected)
                .unwrap_or(false);
            api_key_matches || bearer_matches
        }
        EntryProtocol::Openai => bearer_matches,
    }
}

/// Build upstream payload for the selected target protocol surface.
pub(super) fn build_upstream_body(
    transformer: &dyn Transformer,
    request_body: &Value,
) -> Result<Value, String> {
    let request_bytes =
        serde_json::to_vec(request_body).map_err(|e| format!("serialize request: {}", e))?;
    let converted = transformer.transform_request(&request_bytes)?;
    serde_json::from_slice(&converted).map_err(|e| format!("parse converted: {}", e))
}

/// Map upstream response body back to the downstream entry surface.
fn map_response_body(transformer: &dyn Transformer, upstream_json: &Value) -> Value {
    let response_bytes = match serde_json::to_vec(upstream_json) {
        Ok(b) => b,
        Err(_) => return upstream_json.clone(),
    };

    let converted = match transformer.transform_response(&response_bytes, false) {
        Ok(converted) => converted,
        Err(_) => return upstream_json.clone(),
    };

    serde_json::from_slice(&converted).unwrap_or_else(|_| upstream_json.clone())
}

fn merge_token_usage(
    upstream_usage: Option<TokenUsage>,
    output_usage: Option<TokenUsage>,
) -> Option<TokenUsage> {
    match (upstream_usage, output_usage) {
        (None, None) => None,
        (Some(upstream), None) => Some(upstream),
        (None, Some(output)) => Some(output),
        (Some(upstream), Some(output)) => Some(TokenUsage {
            input_tokens: if upstream.input_tokens > 0 {
                upstream.input_tokens
            } else {
                output.input_tokens
            },
            output_tokens: if upstream.output_tokens > 0 {
                upstream.output_tokens
            } else {
                output.output_tokens
            },
            cache_read_tokens: if upstream.cache_read_tokens > 0 {
                upstream.cache_read_tokens
            } else {
                output.cache_read_tokens
            },
            cache_write_tokens: if upstream.cache_write_tokens > 0 {
                upstream.cache_write_tokens
            } else {
                output.cache_write_tokens
            },
        }),
    }
}

fn extract_declared_tool_names(request_body: &Value) -> HashSet<String> {
    let mut names = HashSet::new();
    let Some(tools) = request_body.get("tools").and_then(|v| v.as_array()) else {
        return names;
    };

    for tool in tools {
        if let Some(name) = tool.get("name").and_then(|v| v.as_str()) {
            let trimmed = name.trim();
            if !trimmed.is_empty() {
                names.insert(trimmed.to_string());
            }
            continue;
        }

        if let Some(name) = tool
            .get("function")
            .and_then(|f| f.get("name"))
            .and_then(|v| v.as_str())
        {
            let trimmed = name.trim();
            if !trimmed.is_empty() {
                names.insert(trimmed.to_string());
            }
        }
    }

    names
}

/// Resolves request timeout ms for this module's workflow.
pub(super) fn resolve_request_timeout_ms(stream: bool, transformer_name: &str) -> u64 {
    if stream {
        return STREAM_REQUEST_TIMEOUT_MS;
    }

    if transformer_name == "cc_openai2" {
        return MESSAGES_TO_RESPONSES_NON_STREAM_REQUEST_TIMEOUT_MS;
    }

    NON_STREAM_REQUEST_TIMEOUT_MS
}

/// Build immediate reject response and emit a minimal log entry.
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
    let mut resp = (
        StatusCode::from_u16(status).unwrap_or(StatusCode::BAD_REQUEST),
        Json(payload.clone()),
    )
        .into_response();
    apply_headers(&mut resp, &response_headers_json(&trace_id));

    log_simple(
        state,
        trace_id,
        method.as_str(),
        &format!("/oc/{}{}", parsed_path.group_id, parsed_path.suffix),
        "rejected",
        Some(status),
        Some(payload),
        Some(crate::models::LogEntryError {
            message: "rejected".to_string(),
            code: "rejected".to_string(),
        }),
    );

    resp
}

fn prepare_transformer_for_route(
    entry: &PathEntry,
    target_protocol: &RuleProtocol,
    target_model: &str,
    text_tool_call_fallback_enabled: bool,
    declared_tool_names: &HashSet<String>,
) -> Result<Arc<dyn Transformer>, String> {
    let model = target_model.to_string();
    let transformer: Arc<dyn Transformer> = match (entry.endpoint, target_protocol) {
        (EntryEndpoint::Messages, RuleProtocol::Anthropic) => Arc::new(
            crate::transformer::cc::claude::ClaudeTransformer::new(model),
        ),
        (EntryEndpoint::Messages, RuleProtocol::OpenaiCompletion) => Arc::new(
            crate::transformer::cc::openai::OpenAITransformer::new(model),
        ),
        (EntryEndpoint::Messages, RuleProtocol::Openai) => {
            Arc::new(crate::transformer::cc::openai2::OpenAI2Transformer::new(
                model,
                ResponsesToClaudeOptions {
                    text_tool_call_fallback_enabled,
                    allowed_tool_names: declared_tool_names.clone(),
                },
            ))
        }
        (EntryEndpoint::ChatCompletions, RuleProtocol::Anthropic) => {
            Arc::new(crate::transformer::cx::chat::claude::ClaudeTransformer::new(model))
        }
        (EntryEndpoint::ChatCompletions, RuleProtocol::OpenaiCompletion) => {
            Arc::new(crate::transformer::cx::chat::openai::OpenAITransformer::new(model))
        }
        (EntryEndpoint::ChatCompletions, RuleProtocol::Openai) => {
            Arc::new(crate::transformer::cx::chat::openai2::OpenAI2Transformer::new(model))
        }
        (EntryEndpoint::Responses, RuleProtocol::Anthropic) => {
            Arc::new(crate::transformer::cx::responses::claude::ClaudeTransformer::new(model))
        }
        (EntryEndpoint::Responses, RuleProtocol::OpenaiCompletion) => {
            Arc::new(crate::transformer::cx::responses::openai::OpenAITransformer::new(model))
        }
        (EntryEndpoint::Responses, RuleProtocol::Openai) => {
            Arc::new(crate::transformer::cx::responses::openai2::OpenAI2Transformer::new(model))
        }
    };

    Ok(transformer)
}

fn stream_requires_sse_event_buffer(transformer_name: &str) -> bool {
    !matches!(
        transformer_name,
        "cc_claude" | "cx_chat_openai" | "cx_resp_openai2"
    )
}

fn transform_sse_event(
    transformer: &dyn Transformer,
    event: &[u8],
    ctx: &mut StreamContext,
) -> Result<Vec<u8>, String> {
    transformer.transform_response_with_context(event, true, ctx)
}

fn finalize_stream_transform(transformer_name: &str, ctx: &mut StreamContext) -> Vec<u8> {
    match transformer_name {
        "cc_openai" => crate::transformer::convert::claude_openai::finalize_openai_stream_to_claude(ctx),
        "cc_openai2" => crate::transformer::convert::claude_openai_responses_stream::finalize_openai_responses_stream_to_claude(ctx),
        _ => Vec::new(),
    }
}

fn capture_stream_chunk(
    outgoing: &[u8],
    stream_capture_body: bool,
    stream_body: &mut Vec<u8>,
    stream_body_truncated: &mut bool,
    stream_debug_capture_body: bool,
    stream_debug_body: &mut Vec<u8>,
) {
    if stream_capture_body && !*stream_body_truncated {
        let remaining = MAX_STREAM_LOG_BODY_BYTES.saturating_sub(stream_body.len());
        if remaining == 0 {
            *stream_body_truncated = true;
        } else if outgoing.len() <= remaining {
            stream_body.extend_from_slice(outgoing);
        } else {
            stream_body.extend_from_slice(&outgoing[..remaining]);
            *stream_body_truncated = true;
        }
    }
    if stream_debug_capture_body {
        stream_debug_body.extend_from_slice(outgoing);
    }
}

fn build_stream_log_body(source: &str, payload: &[u8], truncated: bool) -> Value {
    json!({
        "stream": true,
        "source": source,
        "payload": String::from_utf8_lossy(payload).to_string(),
        "truncated": truncated,
    })
}

fn stream_debug_summary_json(state: &StreamDebugState) -> Value {
    json!({
        "enteredStreamMode": state.entered_stream_mode,
        "transformerName": state.transformer_name,
        "requiresBuffer": state.requires_buffer,
        "probeSseEnabled": state.probe_sse_enabled,
        "upstreamContentType": state.upstream_content_type,
        "upstreamChunkCount": state.upstream_chunk_count,
        "upstreamChunkBytes": state.upstream_chunk_bytes,
        "upstreamEventCount": state.upstream_event_count,
        "firstUpstreamEvent": state.first_upstream_event,
        "firstUpstreamDataType": state.first_upstream_data_type,
        "upstreamDoneSeen": state.upstream_done_seen,
        "transformedChunkCount": state.transformed_chunk_count,
        "transformedChunkBytes": state.transformed_chunk_bytes,
        "transformedEventCount": state.transformed_event_count,
        "firstTransformedEvent": state.first_transformed_event,
        "firstTransformedDataType": state.first_transformed_data_type,
        "transformedDoneSeen": state.transformed_done_seen,
        "transformedMessageStartSeen": state.transformed_message_start_seen,
        "transformedMessageDeltaSeen": state.transformed_message_delta_seen,
        "transformedMessageStopSeen": state.transformed_message_stop_seen,
        "downstreamSendCount": state.downstream_send_count,
        "finalizerCount": state.finalizer_count,
        "finalizerBytes": state.finalizer_bytes,
    })
}

fn stream_debug_update_upstream_chunk(state: &mut StreamDebugState, payload: &[u8]) {
    state.upstream_chunk_count += 1;
    state.upstream_chunk_bytes += payload.len();
}

fn stream_debug_update_upstream_event(state: &mut StreamDebugState, payload: &[u8]) -> Value {
    let summary = summarize_sse_payload(payload);
    state.upstream_event_count += summary.event_count.max(1);
    if state.first_upstream_event.is_none() {
        state.first_upstream_event = summary.first_event.clone();
    }
    if state.first_upstream_data_type.is_none() {
        state.first_upstream_data_type = summary.first_data_type.clone();
    }
    state.upstream_done_seen |= summary.done_seen;
    json!({
        "bytes": payload.len(),
        "eventCount": summary.event_count,
        "firstEvent": summary.first_event,
        "firstDataType": summary.first_data_type,
        "doneSeen": summary.done_seen,
    })
}

fn stream_debug_update_transformed_chunk(state: &mut StreamDebugState, payload: &[u8]) -> Value {
    let summary = summarize_sse_payload(payload);
    state.transformed_chunk_count += 1;
    state.transformed_chunk_bytes += payload.len();
    state.transformed_event_count += summary.event_count;
    if state.first_transformed_event.is_none() {
        state.first_transformed_event = summary.first_event.clone();
    }
    if state.first_transformed_data_type.is_none() {
        state.first_transformed_data_type = summary.first_data_type.clone();
    }
    state.transformed_done_seen |= summary.done_seen;
    state.transformed_message_start_seen |= summary.message_start_seen;
    state.transformed_message_delta_seen |= summary.message_delta_seen;
    state.transformed_message_stop_seen |= summary.message_stop_seen;
    json!({
        "bytes": payload.len(),
        "eventCount": summary.event_count,
        "firstEvent": summary.first_event,
        "firstDataType": summary.first_data_type,
        "doneSeen": summary.done_seen,
        "messageStartSeen": summary.message_start_seen,
        "messageDeltaSeen": summary.message_delta_seen,
        "messageStopSeen": summary.message_stop_seen,
    })
}

fn stream_debug_terminal(trace_id: &str, phase: &str, detail: Value) {
    eprintln!(
        "[proxy][stream] trace={} phase={} detail={}",
        trace_id, phase, detail
    );
}

#[derive(Debug, Default)]
struct SsePayloadSummary {
    event_count: usize,
    first_event: Option<String>,
    first_data_type: Option<String>,
    done_seen: bool,
    message_start_seen: bool,
    message_delta_seen: bool,
    message_stop_seen: bool,
}

fn summarize_sse_payload(payload: &[u8]) -> SsePayloadSummary {
    let normalized = String::from_utf8_lossy(payload).replace("\r\n", "\n");
    let mut summary = SsePayloadSummary::default();

    for raw_event in normalized.split("\n\n") {
        let trimmed = raw_event.trim();
        if trimmed.is_empty() {
            continue;
        }

        summary.event_count += 1;
        for line in trimmed.lines() {
            let line = line.trim_start();
            if summary.first_event.is_none() {
                if let Some(event_name) = line.strip_prefix("event:") {
                    summary.first_event = Some(event_name.trim().to_string());
                }
            }

            let Some(data_line) = line.strip_prefix("data:") else {
                continue;
            };
            let data = data_line.trim();
            if data == "[DONE]" {
                summary.done_seen = true;
                continue;
            }
            if data.is_empty() {
                continue;
            }
            if let Ok(parsed) = serde_json::from_str::<Value>(data) {
                if summary.first_data_type.is_none() {
                    summary.first_data_type = parsed
                        .get("type")
                        .and_then(|value| value.as_str())
                        .map(|value| value.to_string());
                }
                if parsed.get("type").and_then(|value| value.as_str()) == Some("message_start") {
                    summary.message_start_seen = true;
                }
                if parsed.get("type").and_then(|value| value.as_str()) == Some("message_delta") {
                    summary.message_delta_seen = true;
                }
                if parsed.get("type").and_then(|value| value.as_str()) == Some("message_stop") {
                    summary.message_stop_seen = true;
                }
            }
        }
    }

    summary
}

fn pop_sse_event(buffer: &mut Vec<u8>) -> Option<Vec<u8>> {
    let (delimiter_start, delimiter_len) = find_sse_delimiter(buffer.as_slice())?;
    let tail = buffer.split_off(delimiter_start + delimiter_len);
    let event = std::mem::replace(buffer, tail);
    Some(event)
}

fn find_sse_delimiter(buffer: &[u8]) -> Option<(usize, usize)> {
    let mut idx = 0;
    while idx < buffer.len() {
        if idx + 1 < buffer.len() && buffer[idx] == b'\n' && buffer[idx + 1] == b'\n' {
            return Some((idx, 2));
        }
        if idx + 3 < buffer.len()
            && buffer[idx] == b'\r'
            && buffer[idx + 1] == b'\n'
            && buffer[idx + 2] == b'\r'
            && buffer[idx + 3] == b'\n'
        {
            return Some((idx, 4));
        }
        idx += 1;
    }
    None
}

fn looks_like_sse_prelude(chunk: &[u8]) -> bool {
    if chunk.is_empty() {
        return false;
    }

    let probe_len = chunk.len().min(256);
    let mut probe = String::from_utf8_lossy(&chunk[..probe_len]).to_string();
    if let Some(stripped) = probe.strip_prefix('\u{feff}') {
        probe = stripped.to_string();
    }
    let trimmed = probe.trim_start_matches(|c: char| c.is_whitespace());
    let lowered = trimmed.to_ascii_lowercase();
    lowered.starts_with("event:") || lowered.starts_with("data:") || lowered.starts_with(':')
}

#[cfg(test)]
mod tests {
    use super::{
        classify_provider_side_failure, classify_stream_failover_outcome, find_sse_delimiter,
        handle_proxy_request, looks_like_sse_prelude, merge_token_usage, pop_sse_event,
        prepare_transformer_for_route, request_matches_local_access_token,
        stream_requires_sse_event_buffer, EntryEndpoint, ParsedPath, PathEntry,
        StreamFailoverOutcome,
    };
    use crate::models::{
        default_group_failover_config, default_rule_cost_config, default_rule_quota_config, Group,
        Rule, RuleProtocol, TokenUsage,
    };
    use crate::proxy::routing::EntryProtocol;
    use crate::proxy::{headless_service_state_for_tests, ServiceState};
    use axum::{
        body::{to_bytes, Body},
        extract::State as AxumState,
        http::{HeaderMap, HeaderValue, Method, StatusCode},
        response::{IntoResponse, Response},
        routing::post,
        Json, Router,
    };
    use serde_json::{json, Value};
    use std::collections::HashSet;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
        sync::oneshot,
    };

    #[derive(Clone)]
    struct ScriptedUpstreamState {
        responses: Arc<Vec<(u16, Value)>>,
        hits: Arc<AtomicUsize>,
    }

    struct TestUpstream {
        base_url: String,
        hits: Arc<AtomicUsize>,
        shutdown: Option<oneshot::Sender<()>>,
    }

    impl Drop for TestUpstream {
        fn drop(&mut self) {
            if let Some(shutdown) = self.shutdown.take() {
                let _ = shutdown.send(());
            }
        }
    }

    async fn scripted_upstream_handler(
        AxumState(state): AxumState<ScriptedUpstreamState>,
    ) -> Response {
        let call_index = state.hits.fetch_add(1, Ordering::SeqCst);
        let (status, payload) = state
            .responses
            .get(call_index)
            .cloned()
            .or_else(|| state.responses.last().cloned())
            .expect("scripted upstream must have at least one response");

        (
            StatusCode::from_u16(status).expect("scripted status must be valid"),
            Json(payload),
        )
            .into_response()
    }

    async fn spawn_json_upstream(path: &'static str, responses: Vec<(u16, Value)>) -> TestUpstream {
        let hits = Arc::new(AtomicUsize::new(0));
        let state = ScriptedUpstreamState {
            responses: Arc::new(responses),
            hits: hits.clone(),
        };
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("mock upstream listener should bind");
        let address = listener
            .local_addr()
            .expect("mock upstream listener should have local addr");
        let app = Router::new()
            .route(path, post(scripted_upstream_handler))
            .with_state(state);
        let (shutdown_tx, shutdown_rx) = oneshot::channel();

        tokio::spawn(async move {
            let server = axum::serve(listener, app);
            let graceful = server.with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            });
            let _ = graceful.await;
        });

        TestUpstream {
            base_url: format!("http://{}", address),
            hits,
            shutdown: Some(shutdown_tx),
        }
    }

    async fn spawn_stream_upstream(
        path: &'static str,
        chunks: Vec<Result<Vec<u8>, String>>,
    ) -> TestUpstream {
        spawn_scripted_stream_upstream(path, vec![chunks]).await
    }

    async fn spawn_raw_json_upstream(
        _path: &'static str,
        responses: Vec<Vec<Result<Vec<u8>, String>>>,
    ) -> TestUpstream {
        let hits = Arc::new(AtomicUsize::new(0));
        let responses = Arc::new(responses);
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("mock raw upstream listener should bind");
        let address = listener
            .local_addr()
            .expect("mock raw upstream listener should have local addr");
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
        let hits_for_server = hits.clone();
        let responses_for_server = responses.clone();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => {
                        return;
                    }
                    accept_result = listener.accept() => {
                        let Ok((mut socket, _)) = accept_result else {
                            return;
                        };
                        let call_index = hits_for_server.fetch_add(1, Ordering::SeqCst);
                        let chunks = responses_for_server
                            .get(call_index)
                            .cloned()
                            .or_else(|| responses_for_server.last().cloned())
                            .expect("scripted raw upstream must have at least one response");

                        let mut request_buffer = [0_u8; 4096];
                        let _ = socket.read(&mut request_buffer).await;

                        if socket
                            .write_all(
                                b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ntransfer-encoding: chunked\r\nconnection: close\r\n\r\n",
                            )
                            .await
                            .is_err()
                        {
                            continue;
                        }

                        let mut closed_early = false;
                        for chunk in chunks {
                            match chunk {
                                Ok(bytes) => {
                                    let header = format!("{:X}\r\n", bytes.len());
                                    if socket.write_all(header.as_bytes()).await.is_err() {
                                        closed_early = true;
                                        break;
                                    }
                                    if socket.write_all(&bytes).await.is_err() {
                                        closed_early = true;
                                        break;
                                    }
                                    if socket.write_all(b"\r\n").await.is_err() {
                                        closed_early = true;
                                        break;
                                    }
                                    if socket.flush().await.is_err() {
                                        closed_early = true;
                                        break;
                                    }
                                }
                                Err(_) => {
                                    let _ = socket.shutdown().await;
                                    closed_early = true;
                                    break;
                                }
                            }
                        }

                        if !closed_early {
                            let _ = socket.write_all(b"0\r\n\r\n").await;
                            let _ = socket.flush().await;
                        }
                    }
                }
            }
        });

        TestUpstream {
            base_url: format!("http://{}", address),
            hits,
            shutdown: Some(shutdown_tx),
        }
    }

    async fn spawn_scripted_stream_upstream(
        _path: &'static str,
        responses: Vec<Vec<Result<Vec<u8>, String>>>,
    ) -> TestUpstream {
        let hits = Arc::new(AtomicUsize::new(0));
        let responses = Arc::new(responses);
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("mock stream upstream listener should bind");
        let address = listener
            .local_addr()
            .expect("mock stream upstream listener should have local addr");
        let (shutdown_tx, mut shutdown_rx) = oneshot::channel();
        let hits_for_server = hits.clone();
        let responses_for_server = responses.clone();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => {
                        return;
                    }
                    accept_result = listener.accept() => {
                        let Ok((mut socket, _)) = accept_result else {
                            return;
                        };
                        let call_index = hits_for_server.fetch_add(1, Ordering::SeqCst);
                        let chunks = responses_for_server
                            .get(call_index)
                            .cloned()
                            .or_else(|| responses_for_server.last().cloned())
                            .expect("scripted stream upstream must have at least one response");

                        let mut request_buffer = [0_u8; 4096];
                        let _ = socket.read(&mut request_buffer).await;

                        if socket
                            .write_all(
                                b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ntransfer-encoding: chunked\r\nconnection: close\r\n\r\n",
                            )
                            .await
                            .is_err()
                        {
                            continue;
                        }

                        let mut closed_early = false;
                        for chunk in chunks {
                            match chunk {
                                Ok(bytes) => {
                                    let header = format!("{:X}\r\n", bytes.len());
                                    if socket.write_all(header.as_bytes()).await.is_err() {
                                        closed_early = true;
                                        break;
                                    }
                                    if socket.write_all(&bytes).await.is_err() {
                                        closed_early = true;
                                        break;
                                    }
                                    if socket.write_all(b"\r\n").await.is_err() {
                                        closed_early = true;
                                        break;
                                    }
                                    if socket.flush().await.is_err() {
                                        closed_early = true;
                                        break;
                                    }
                                }
                                Err(_) => {
                                    let _ = socket.shutdown().await;
                                    closed_early = true;
                                    break;
                                }
                            }
                        }

                        if !closed_early {
                            let _ = socket.write_all(b"0\r\n\r\n").await;
                            let _ = socket.flush().await;
                        }
                    }
                }
            }
        });

        TestUpstream {
            base_url: format!("http://{}", address),
            hits,
            shutdown: Some(shutdown_tx),
        }
    }

    fn anthropic_message_response(text: &str) -> Value {
        json!({
            "id": "msg_test",
            "type": "message",
            "role": "assistant",
            "model": "claude-test",
            "content": [{ "type": "text", "text": text }],
            "stop_reason": "end_turn",
            "stop_sequence": Value::Null,
            "usage": {
                "input_tokens": 1,
                "output_tokens": 1
            }
        })
    }

    fn openai_chat_completion_response(text: &str) -> Value {
        json!({
            "id": "chatcmpl-test",
            "object": "chat.completion",
            "created": 1,
            "model": "gpt-test",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": text
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 1,
                "completion_tokens": 1,
                "total_tokens": 2
            }
        })
    }

    fn test_rule(
        id: &str,
        protocol: RuleProtocol,
        api_address: String,
        default_model: &str,
    ) -> Rule {
        Rule {
            id: id.to_string(),
            name: id.to_string(),
            protocol,
            token: "test-token".to_string(),
            api_address,
            website: String::new(),
            default_model: default_model.to_string(),
            model_mappings: Default::default(),
            quota: default_rule_quota_config(),
            cost: default_rule_cost_config(),
        }
    }

    fn install_failover_group(
        service_state: &ServiceState,
        providers: Vec<Rule>,
        failure_threshold: u32,
    ) {
        install_failover_group_with_cooldown(service_state, providers, failure_threshold, 300);
    }

    fn install_failover_group_with_cooldown(
        service_state: &ServiceState,
        providers: Vec<Rule>,
        failure_threshold: u32,
        cooldown_seconds: u32,
    ) {
        let shared = service_state
            .shared_state
            .clone()
            .expect("headless service state should include shared state");
        let preferred_provider_id = providers
            .first()
            .expect("test group needs at least one provider")
            .id
            .clone();
        let provider_ids = providers
            .iter()
            .map(|provider| provider.id.clone())
            .collect();
        let mut failover = default_group_failover_config();
        failover.enabled = true;
        failover.failure_threshold = failure_threshold;
        failover.cooldown_seconds = cooldown_seconds;
        let mut next_config = shared.config_store.get();
        next_config.providers = providers.clone();
        next_config.groups = vec![Group {
            id: "dev".to_string(),
            name: "Dev".to_string(),
            models: vec!["claude-test".to_string()],
            provider_ids,
            active_provider_id: Some(preferred_provider_id),
            providers,
            failover,
        }];
        shared
            .config_store
            .save_config(next_config)
            .expect("test config should save");
    }

    fn runtime_failure_count(
        service_state: &ServiceState,
        group_id: &str,
        provider_id: &str,
    ) -> u32 {
        let runtime = service_state
            .failover_state
            .read()
            .expect("failover state lock should be readable");
        crate::proxy::failover::provider_failure_count(&runtime, group_id, provider_id)
    }

    fn runtime_active_failover_provider(
        service_state: &ServiceState,
        group_id: &str,
    ) -> Option<String> {
        let runtime = service_state
            .failover_state
            .read()
            .expect("failover state lock should be readable");
        crate::proxy::failover::active_failover_provider_id(&runtime, group_id)
    }

    async fn send_messages_request(
        service_state: &ServiceState,
        request_body: Value,
    ) -> (StatusCode, Value) {
        let response = handle_proxy_request(
            service_state.clone(),
            Method::POST,
            HeaderMap::new(),
            Body::from(
                serde_json::to_vec(&request_body).expect("request body should serialize to json"),
            ),
            ParsedPath {
                group_id: "dev".to_string(),
                suffix: "/messages".to_string(),
            },
        )
        .await;
        let status = response.status();
        let body_bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("response body should be readable");
        let payload = serde_json::from_slice(&body_bytes).expect("response body should be json");
        (status, payload)
    }

    async fn send_streaming_messages_request(
        service_state: &ServiceState,
        request_body: Value,
    ) -> (StatusCode, String) {
        let response = handle_proxy_request(
            service_state.clone(),
            Method::POST,
            HeaderMap::new(),
            Body::from(
                serde_json::to_vec(&request_body).expect("request body should serialize to json"),
            ),
            ParsedPath {
                group_id: "dev".to_string(),
                suffix: "/messages".to_string(),
            },
        )
        .await;
        let status = response.status();
        let body_bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("stream response body should be readable");
        (
            status,
            String::from_utf8(body_bytes.to_vec()).expect("stream response should be utf8"),
        )
    }

    #[tokio::test]
    async fn scripted_stream_upstream_accepts_direct_requests() {
        let upstream = spawn_stream_upstream(
            "/chat/completions",
            vec![Ok(b"data: {\"ok\":true}\n\n".to_vec())],
        )
        .await;

        let response = reqwest::Client::new()
            .post(format!("{}/chat/completions", upstream.base_url))
            .send()
            .await
            .expect("direct stream upstream request should connect");

        assert_eq!(response.status(), StatusCode::OK);
        assert!(response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .contains("text/event-stream"));
    }

    #[test]
    fn pop_sse_event_handles_lf_delimiter() {
        let mut buffer = b"event: one\ndata: {\"a\":1}\n\nrest".to_vec();
        let event = pop_sse_event(&mut buffer).expect("event should exist");
        assert_eq!(
            String::from_utf8(event).expect("utf8"),
            "event: one\ndata: {\"a\":1}\n\n"
        );
        assert_eq!(String::from_utf8(buffer).expect("utf8"), "rest");
    }

    #[test]
    fn pop_sse_event_handles_crlf_delimiter() {
        let mut buffer = b"event: one\r\ndata: {\"a\":1}\r\n\r\nnext".to_vec();
        let event = pop_sse_event(&mut buffer).expect("event should exist");
        assert_eq!(
            String::from_utf8(event).expect("utf8"),
            "event: one\r\ndata: {\"a\":1}\r\n\r\n"
        );
        assert_eq!(String::from_utf8(buffer).expect("utf8"), "next");
    }

    #[test]
    fn pop_sse_event_returns_none_for_partial_event() {
        let mut buffer = b"event: one\ndata: {\"a\":1}\n".to_vec();
        assert!(pop_sse_event(&mut buffer).is_none());
        assert_eq!(find_sse_delimiter(&buffer), None);
    }

    #[test]
    fn looks_like_sse_prelude_accepts_event_or_data_lines() {
        assert!(looks_like_sse_prelude(
            b"event: response.created\ndata: {}\n\n"
        ));
        assert!(looks_like_sse_prelude(
            b"  data: {\"type\":\"response.created\"}\n\n"
        ));
        assert!(looks_like_sse_prelude("\u{feff}event: ping\n\n".as_bytes()));
    }

    #[test]
    fn looks_like_sse_prelude_rejects_non_sse_text() {
        assert!(!looks_like_sse_prelude(b"{\"id\":\"resp_1\"}"));
        assert!(!looks_like_sse_prelude(b"plain text"));
        assert!(!looks_like_sse_prelude(b""));
    }

    #[test]
    fn prepare_transformer_for_route_maps_all_supported_routes() {
        let cases = [
            (
                EntryEndpoint::Messages,
                RuleProtocol::Anthropic,
                "cc_claude",
            ),
            (
                EntryEndpoint::Messages,
                RuleProtocol::OpenaiCompletion,
                "cc_openai",
            ),
            (EntryEndpoint::Messages, RuleProtocol::Openai, "cc_openai2"),
            (
                EntryEndpoint::ChatCompletions,
                RuleProtocol::Anthropic,
                "cx_chat_claude",
            ),
            (
                EntryEndpoint::ChatCompletions,
                RuleProtocol::OpenaiCompletion,
                "cx_chat_openai",
            ),
            (
                EntryEndpoint::ChatCompletions,
                RuleProtocol::Openai,
                "cx_chat_openai2",
            ),
            (
                EntryEndpoint::Responses,
                RuleProtocol::Anthropic,
                "cx_resp_claude",
            ),
            (
                EntryEndpoint::Responses,
                RuleProtocol::OpenaiCompletion,
                "cx_resp_openai",
            ),
            (
                EntryEndpoint::Responses,
                RuleProtocol::Openai,
                "cx_resp_openai2",
            ),
        ];

        for (endpoint, target_protocol, expected_name) in cases {
            let entry = PathEntry {
                protocol: if matches!(endpoint, EntryEndpoint::Messages) {
                    EntryProtocol::Anthropic
                } else {
                    EntryProtocol::Openai
                },
                endpoint,
            };

            let transformer = prepare_transformer_for_route(
                &entry,
                &target_protocol,
                "test-model",
                true,
                &HashSet::from(["bash".to_string()]),
            )
            .expect("transformer should exist");

            assert_eq!(transformer.name(), expected_name);
        }
    }

    #[test]
    fn stream_requires_sse_event_buffer_respects_passthrough_transformers() {
        assert!(!stream_requires_sse_event_buffer("cc_claude"));
        assert!(!stream_requires_sse_event_buffer("cx_chat_openai"));
        assert!(!stream_requires_sse_event_buffer("cx_resp_openai2"));
        assert!(stream_requires_sse_event_buffer("cc_openai"));
        assert!(stream_requires_sse_event_buffer("cc_openai2"));
    }

    #[test]
    fn merge_token_usage_falls_back_to_output_input_output_tokens() {
        let upstream = TokenUsage {
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 30587,
            cache_write_tokens: 0,
        };
        let output = TokenUsage {
            input_tokens: 3,
            output_tokens: 633,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
        };

        let merged = merge_token_usage(Some(upstream), Some(output)).expect("usage should exist");
        assert_eq!(merged.input_tokens, 3);
        assert_eq!(merged.output_tokens, 633);
        assert_eq!(merged.cache_read_tokens, 30587);
        assert_eq!(merged.cache_write_tokens, 0);
    }

    #[test]
    fn merge_token_usage_keeps_upstream_non_zero_fields() {
        let upstream = TokenUsage {
            input_tokens: 120,
            output_tokens: 45,
            cache_read_tokens: 7,
            cache_write_tokens: 2,
        };
        let output = TokenUsage {
            input_tokens: 119,
            output_tokens: 44,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
        };

        let merged =
            merge_token_usage(Some(upstream.clone()), Some(output)).expect("usage should exist");
        assert_eq!(merged.input_tokens, upstream.input_tokens);
        assert_eq!(merged.output_tokens, upstream.output_tokens);
        assert_eq!(merged.cache_read_tokens, upstream.cache_read_tokens);
        assert_eq!(merged.cache_write_tokens, upstream.cache_write_tokens);
    }

    #[test]
    fn failover_classification_counts_provider_side_status_codes() {
        assert!(classify_provider_side_failure(None, Some(429)));
        assert!(classify_provider_side_failure(None, Some(500)));
        assert!(!classify_provider_side_failure(None, Some(400)));
        assert!(!classify_provider_side_failure(None, Some(422)));
    }

    #[test]
    fn failover_classification_ignores_local_validation_errors_without_upstream_failure() {
        assert!(!classify_provider_side_failure(
            Some("invalid request payload"),
            None
        ));
        assert!(!classify_provider_side_failure(
            Some("unsupported model alias"),
            Some(200)
        ));
    }

    #[test]
    fn failover_classification_counts_transport_failures_even_after_non_error_status() {
        assert!(classify_provider_side_failure(
            Some("network failure"),
            None
        ));
        assert!(classify_provider_side_failure(Some("timeout"), None));
        assert!(classify_provider_side_failure(
            Some("Failed to read upstream response"),
            Some(200)
        ));
        assert!(!classify_provider_side_failure(None, Some(422)));
    }

    #[test]
    fn stream_failover_outcome_distinguishes_provider_failures_from_local_ones() {
        assert_eq!(
            classify_stream_failover_outcome(false, false, false, false),
            StreamFailoverOutcome::Success
        );
        assert_eq!(
            classify_stream_failover_outcome(true, false, false, true),
            StreamFailoverOutcome::ProviderFailure
        );
        assert_eq!(
            classify_stream_failover_outcome(true, true, false, true),
            StreamFailoverOutcome::ProviderFailure
        );
        assert_eq!(
            classify_stream_failover_outcome(true, false, false, false),
            StreamFailoverOutcome::Ignore
        );
        assert_eq!(
            classify_stream_failover_outcome(false, true, false, true),
            StreamFailoverOutcome::Ignore
        );
        assert_eq!(
            classify_stream_failover_outcome(false, false, true, true),
            StreamFailoverOutcome::Ignore
        );
    }

    #[tokio::test]
    async fn cooldown_expiry_retries_preferred_provider_on_next_live_request() {
        let primary = spawn_json_upstream(
            "/chat/completions",
            vec![
                (503, json!({"error": {"message": "service unavailable"}})),
                (200, openai_chat_completion_response("recovered")),
            ],
        )
        .await;
        let secondary = spawn_json_upstream(
            "/chat/completions",
            vec![(200, openai_chat_completion_response("backup"))],
        )
        .await;
        let service_state = headless_service_state_for_tests();
        install_failover_group_with_cooldown(
            &service_state,
            vec![
                test_rule(
                    "p1",
                    RuleProtocol::OpenaiCompletion,
                    primary.base_url.clone(),
                    "gpt-test",
                ),
                test_rule(
                    "p2",
                    RuleProtocol::OpenaiCompletion,
                    secondary.base_url.clone(),
                    "gpt-test",
                ),
            ],
            1,
            1,
        );

        let request = json!({
            "model": "claude-test",
            "max_tokens": 16,
            "messages": [{"role": "user", "content": "hello"}]
        });

        let (failure_status, _) = send_messages_request(&service_state, request.clone()).await;
        assert_eq!(failure_status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(primary.hits.load(Ordering::SeqCst), 1);
        assert_eq!(secondary.hits.load(Ordering::SeqCst), 0);
        assert_eq!(
            runtime_active_failover_provider(&service_state, "dev"),
            Some("p2".to_string())
        );

        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        let (success_status, success_payload) =
            send_messages_request(&service_state, request).await;
        assert_eq!(success_status, StatusCode::OK);
        assert_eq!(
            success_payload["content"][0]["text"].as_str(),
            Some("recovered")
        );
        assert_eq!(primary.hits.load(Ordering::SeqCst), 2);
        assert_eq!(secondary.hits.load(Ordering::SeqCst), 0);
        assert_eq!(
            runtime_active_failover_provider(&service_state, "dev"),
            None
        );
    }

    #[tokio::test]
    async fn local_errors_after_cooldown_do_not_clear_failover_before_preferred_provider_recovers()
    {
        let primary = spawn_json_upstream(
            "/chat/completions",
            vec![(200, openai_chat_completion_response("recovered"))],
        )
        .await;
        let secondary = spawn_json_upstream(
            "/chat/completions",
            vec![(200, openai_chat_completion_response("backup"))],
        )
        .await;
        let service_state = headless_service_state_for_tests();
        install_failover_group_with_cooldown(
            &service_state,
            vec![
                test_rule(
                    "p1",
                    RuleProtocol::OpenaiCompletion,
                    primary.base_url.clone(),
                    "gpt-test",
                ),
                test_rule(
                    "p2",
                    RuleProtocol::OpenaiCompletion,
                    secondary.base_url.clone(),
                    "gpt-test",
                ),
            ],
            1,
            1,
        );

        {
            let mut runtime = service_state
                .failover_state
                .write()
                .expect("failover state lock should be writable");
            crate::proxy::failover::record_provider_failure(
                &mut runtime,
                "dev",
                "p1",
                &["p1".to_string(), "p2".to_string()],
                &crate::proxy::failover::FailoverConfigSnapshot {
                    enabled: true,
                    failure_threshold: 1,
                    cooldown_seconds: 1,
                },
                chrono::Utc::now() - chrono::Duration::seconds(2),
            );
        }
        assert_eq!(
            runtime_active_failover_provider(&service_state, "dev"),
            Some("p2".to_string())
        );

        let (error_status, _) = send_messages_request(
            &service_state,
            json!({
                "model": "claude-test",
                "max_tokens": 16,
                "messages": "invalid"
            }),
        )
        .await;
        assert_eq!(error_status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(primary.hits.load(Ordering::SeqCst), 0);
        assert_eq!(secondary.hits.load(Ordering::SeqCst), 0);
        assert_eq!(
            runtime_active_failover_provider(&service_state, "dev"),
            Some("p2".to_string())
        );

        let (success_status, success_payload) = send_messages_request(
            &service_state,
            json!({
                "model": "claude-test",
                "max_tokens": 16,
                "messages": [{"role": "user", "content": "hello"}]
            }),
        )
        .await;
        assert_eq!(success_status, StatusCode::OK);
        assert_eq!(
            success_payload["content"][0]["text"].as_str(),
            Some("recovered")
        );
        assert_eq!(primary.hits.load(Ordering::SeqCst), 1);
        assert_eq!(secondary.hits.load(Ordering::SeqCst), 0);
        assert_eq!(
            runtime_active_failover_provider(&service_state, "dev"),
            None
        );
    }

    #[tokio::test]
    async fn provider_failure_after_cooldown_restarts_failover_to_secondary() {
        let primary = spawn_json_upstream(
            "/chat/completions",
            vec![(503, json!({"error": {"message": "still failing"}}))],
        )
        .await;
        let secondary = spawn_json_upstream(
            "/chat/completions",
            vec![(200, openai_chat_completion_response("backup"))],
        )
        .await;
        let service_state = headless_service_state_for_tests();
        install_failover_group_with_cooldown(
            &service_state,
            vec![
                test_rule(
                    "p1",
                    RuleProtocol::OpenaiCompletion,
                    primary.base_url.clone(),
                    "gpt-test",
                ),
                test_rule(
                    "p2",
                    RuleProtocol::OpenaiCompletion,
                    secondary.base_url.clone(),
                    "gpt-test",
                ),
            ],
            1,
            1,
        );

        {
            let mut runtime = service_state
                .failover_state
                .write()
                .expect("failover state lock should be writable");
            crate::proxy::failover::record_provider_failure(
                &mut runtime,
                "dev",
                "p1",
                &["p1".to_string(), "p2".to_string()],
                &crate::proxy::failover::FailoverConfigSnapshot {
                    enabled: true,
                    failure_threshold: 1,
                    cooldown_seconds: 1,
                },
                chrono::Utc::now() - chrono::Duration::seconds(2),
            );
        }

        let (failure_status, failure_payload) = send_messages_request(
            &service_state,
            json!({
                "model": "claude-test",
                "max_tokens": 16,
                "messages": [{"role": "user", "content": "hello"}]
            }),
        )
        .await;
        assert_eq!(failure_status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            failure_payload["error"]["message"].as_str(),
            Some("still failing")
        );
        assert_eq!(primary.hits.load(Ordering::SeqCst), 1);
        assert_eq!(secondary.hits.load(Ordering::SeqCst), 0);
        assert_eq!(
            runtime_active_failover_provider(&service_state, "dev"),
            Some("p2".to_string())
        );
    }

    #[tokio::test]
    async fn provider_side_failures_switch_live_requests_to_next_provider() {
        let primary = spawn_json_upstream(
            "/chat/completions",
            vec![(429, json!({"error": {"message": "rate limited"}}))],
        )
        .await;
        let secondary = spawn_json_upstream(
            "/chat/completions",
            vec![(200, openai_chat_completion_response("ok"))],
        )
        .await;
        let service_state = headless_service_state_for_tests();
        install_failover_group(
            &service_state,
            vec![
                test_rule(
                    "p1",
                    RuleProtocol::OpenaiCompletion,
                    primary.base_url.clone(),
                    "gpt-test",
                ),
                test_rule(
                    "p2",
                    RuleProtocol::OpenaiCompletion,
                    secondary.base_url.clone(),
                    "gpt-test",
                ),
            ],
            1,
        );

        let failure_request = json!({
            "model": "claude-test",
            "max_tokens": 16,
            "messages": [{"role": "user", "content": "hello"}]
        });
        let (failure_status, failure_payload) =
            send_messages_request(&service_state, failure_request.clone()).await;
        assert_eq!(failure_status, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(
            failure_payload["error"]["message"].as_str(),
            Some("rate limited")
        );

        let (success_status, success_payload) =
            send_messages_request(&service_state, failure_request).await;
        assert_eq!(success_status, StatusCode::OK);
        assert_eq!(success_payload["content"][0]["text"].as_str(), Some("ok"));
        assert_eq!(primary.hits.load(Ordering::SeqCst), 1);
        assert_eq!(secondary.hits.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn local_transform_errors_do_not_activate_failover_for_live_requests() {
        let primary = spawn_json_upstream(
            "/chat/completions",
            vec![(200, openai_chat_completion_response("ok"))],
        )
        .await;
        let secondary = spawn_json_upstream(
            "/chat/completions",
            vec![(200, openai_chat_completion_response("backup"))],
        )
        .await;
        let service_state = headless_service_state_for_tests();
        install_failover_group(
            &service_state,
            vec![
                test_rule(
                    "p1",
                    RuleProtocol::OpenaiCompletion,
                    primary.base_url.clone(),
                    "gpt-test",
                ),
                test_rule(
                    "p2",
                    RuleProtocol::OpenaiCompletion,
                    secondary.base_url.clone(),
                    "gpt-test",
                ),
            ],
            1,
        );

        let local_error_request = json!({
            "model": "claude-test",
            "max_tokens": 16,
            "messages": "invalid"
        });
        let (error_status, _) = send_messages_request(&service_state, local_error_request).await;
        assert_eq!(error_status, StatusCode::UNPROCESSABLE_ENTITY);

        let success_request = json!({
            "model": "claude-test",
            "max_tokens": 16,
            "messages": [{"role": "user", "content": "hello"}]
        });
        let (success_status, success_payload) =
            send_messages_request(&service_state, success_request).await;
        assert_eq!(success_status, StatusCode::OK);
        assert_eq!(success_payload["content"][0]["text"].as_str(), Some("ok"));
        assert_eq!(primary.hits.load(Ordering::SeqCst), 1);
        assert_eq!(secondary.hits.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn upstream_5xx_failures_switch_live_requests_to_next_provider() {
        let primary = spawn_json_upstream(
            "/chat/completions",
            vec![(503, json!({"error": {"message": "service unavailable"}}))],
        )
        .await;
        let secondary = spawn_json_upstream(
            "/chat/completions",
            vec![(200, openai_chat_completion_response("ok"))],
        )
        .await;
        let service_state = headless_service_state_for_tests();
        install_failover_group(
            &service_state,
            vec![
                test_rule(
                    "p1",
                    RuleProtocol::OpenaiCompletion,
                    primary.base_url.clone(),
                    "gpt-test",
                ),
                test_rule(
                    "p2",
                    RuleProtocol::OpenaiCompletion,
                    secondary.base_url.clone(),
                    "gpt-test",
                ),
            ],
            1,
        );

        let request = json!({
            "model": "claude-test",
            "max_tokens": 16,
            "messages": [{"role": "user", "content": "hello"}]
        });

        let (failure_status, failure_payload) =
            send_messages_request(&service_state, request.clone()).await;
        assert_eq!(failure_status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            failure_payload["error"]["message"].as_str(),
            Some("service unavailable")
        );
        assert_eq!(runtime_failure_count(&service_state, "dev", "p1"), 1);
        assert_eq!(
            runtime_active_failover_provider(&service_state, "dev"),
            Some("p2".to_string())
        );

        let (success_status, success_payload) =
            send_messages_request(&service_state, request).await;
        assert_eq!(success_status, StatusCode::OK);
        assert_eq!(success_payload["content"][0]["text"].as_str(), Some("ok"));
        assert_eq!(primary.hits.load(Ordering::SeqCst), 1);
        assert_eq!(secondary.hits.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn transport_failures_switch_live_requests_to_next_provider() {
        let service_state = headless_service_state_for_tests();
        let secondary = spawn_json_upstream(
            "/chat/completions",
            vec![(200, openai_chat_completion_response("ok"))],
        )
        .await;
        install_failover_group(
            &service_state,
            vec![
                test_rule(
                    "p1",
                    RuleProtocol::OpenaiCompletion,
                    "http://127.0.0.1:1".to_string(),
                    "gpt-test",
                ),
                test_rule(
                    "p2",
                    RuleProtocol::OpenaiCompletion,
                    secondary.base_url.clone(),
                    "gpt-test",
                ),
            ],
            1,
        );

        let request = json!({
            "model": "claude-test",
            "max_tokens": 16,
            "messages": [{"role": "user", "content": "hello"}]
        });

        let (failure_status, failure_payload) =
            send_messages_request(&service_state, request.clone()).await;
        assert_eq!(failure_status, StatusCode::BAD_GATEWAY);
        assert!(failure_payload["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("Upstream request failed"));
        assert_eq!(runtime_failure_count(&service_state, "dev", "p1"), 1);
        assert_eq!(
            runtime_active_failover_provider(&service_state, "dev"),
            Some("p2".to_string())
        );

        let (success_status, success_payload) =
            send_messages_request(&service_state, request).await;
        assert_eq!(success_status, StatusCode::OK);
        assert_eq!(success_payload["content"][0]["text"].as_str(), Some("ok"));
        assert_eq!(secondary.hits.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn stream_read_failures_activate_failover_for_next_live_request() {
        let primary = spawn_stream_upstream(
            "/chat/completions",
            vec![
                Ok(b"data: {\"id\":\"chatcmpl-test\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"gpt-test\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"he\"},\"finish_reason\":null}]}\n\n".to_vec()),
                Err("stream read failed".to_string()),
            ],
        )
        .await;
        let secondary = spawn_json_upstream(
            "/chat/completions",
            vec![(200, openai_chat_completion_response("ok"))],
        )
        .await;
        let service_state = headless_service_state_for_tests();
        install_failover_group(
            &service_state,
            vec![
                test_rule(
                    "p1",
                    RuleProtocol::OpenaiCompletion,
                    primary.base_url.clone(),
                    "gpt-test",
                ),
                test_rule(
                    "p2",
                    RuleProtocol::OpenaiCompletion,
                    secondary.base_url.clone(),
                    "gpt-test",
                ),
            ],
            1,
        );

        let request = json!({
            "model": "claude-test",
            "max_tokens": 16,
            "stream": true,
            "messages": [{"role": "user", "content": "hello"}]
        });

        let (stream_status, stream_body) =
            send_streaming_messages_request(&service_state, request.clone()).await;
        assert_eq!(
            stream_status,
            StatusCode::OK,
            "unexpected stream body: {stream_body}"
        );
        assert!(stream_body.contains("message_start"));
        assert_eq!(runtime_failure_count(&service_state, "dev", "p1"), 1);
        assert_eq!(
            runtime_active_failover_provider(&service_state, "dev"),
            Some("p2".to_string())
        );

        let (success_status, success_payload) = send_messages_request(
            &service_state,
            json!({
                "model": "claude-test",
                "max_tokens": 16,
                "messages": [{"role": "user", "content": "hello again"}]
            }),
        )
        .await;
        assert_eq!(success_status, StatusCode::OK);
        assert_eq!(success_payload["content"][0]["text"].as_str(), Some("ok"));
        assert_eq!(primary.hits.load(Ordering::SeqCst), 1);
        assert_eq!(secondary.hits.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn successful_stream_request_resets_failure_count_before_threshold_is_reached() {
        let primary = spawn_scripted_stream_upstream(
            "/chat/completions",
            vec![
                vec![
                    Ok(b"data: {\"id\":\"chatcmpl-test\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"gpt-test\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"he\"},\"finish_reason\":null}]}\n\n".to_vec()),
                    Err("stream read failed".to_string()),
                ],
                vec![
                    Ok(b"data: {\"id\":\"chatcmpl-test\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"gpt-test\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"ok\"},\"finish_reason\":null}]}\n\n".to_vec()),
                    Ok(b"data: [DONE]\n\n".to_vec()),
                ],
                vec![
                    Ok(b"data: {\"id\":\"chatcmpl-test\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"gpt-test\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"bye\"},\"finish_reason\":null}]}\n\n".to_vec()),
                    Err("stream read failed again".to_string()),
                ],
                vec![
                    Ok(b"data: {\"id\":\"chatcmpl-test\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"gpt-test\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"final\"},\"finish_reason\":null}]}\n\n".to_vec()),
                    Ok(b"data: [DONE]\n\n".to_vec()),
                ],
            ],
        )
        .await;
        let secondary = spawn_json_upstream(
            "/chat/completions",
            vec![(200, openai_chat_completion_response("backup"))],
        )
        .await;
        let service_state = headless_service_state_for_tests();
        install_failover_group(
            &service_state,
            vec![
                test_rule(
                    "p1",
                    RuleProtocol::OpenaiCompletion,
                    primary.base_url.clone(),
                    "gpt-test",
                ),
                test_rule(
                    "p2",
                    RuleProtocol::OpenaiCompletion,
                    secondary.base_url.clone(),
                    "gpt-test",
                ),
            ],
            2,
        );

        let request = json!({
            "model": "claude-test",
            "max_tokens": 16,
            "stream": true,
            "messages": [{"role": "user", "content": "hello"}]
        });

        let (first_status, first_body) =
            send_streaming_messages_request(&service_state, request.clone()).await;
        assert_eq!(
            first_status,
            StatusCode::OK,
            "unexpected stream body: {first_body}"
        );
        assert!(first_body.contains("message_start"));
        assert_eq!(runtime_failure_count(&service_state, "dev", "p1"), 1);
        assert_eq!(
            runtime_active_failover_provider(&service_state, "dev"),
            None
        );

        let (second_status, second_body) =
            send_streaming_messages_request(&service_state, request.clone()).await;
        assert_eq!(
            second_status,
            StatusCode::OK,
            "unexpected stream body: {second_body}"
        );
        assert!(second_body.contains("message_start"));
        assert_eq!(runtime_failure_count(&service_state, "dev", "p1"), 0);
        assert_eq!(
            runtime_active_failover_provider(&service_state, "dev"),
            None
        );

        let (third_status, third_body) =
            send_streaming_messages_request(&service_state, request.clone()).await;
        assert_eq!(
            third_status,
            StatusCode::OK,
            "unexpected stream body: {third_body}"
        );
        assert!(third_body.contains("message_start"));
        assert_eq!(runtime_failure_count(&service_state, "dev", "p1"), 1);
        assert_eq!(
            runtime_active_failover_provider(&service_state, "dev"),
            None
        );

        let (fourth_status, fourth_body) =
            send_streaming_messages_request(&service_state, request).await;
        assert_eq!(
            fourth_status,
            StatusCode::OK,
            "unexpected stream body: {fourth_body}"
        );
        assert!(fourth_body.contains("message_start"));
        assert_eq!(runtime_failure_count(&service_state, "dev", "p1"), 0);
        assert_eq!(
            runtime_active_failover_provider(&service_state, "dev"),
            None
        );
        assert_eq!(primary.hits.load(Ordering::SeqCst), 4);
        assert_eq!(secondary.hits.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn non_stream_body_read_failures_activate_failover_for_next_live_request() {
        let primary = spawn_raw_json_upstream(
            "/chat/completions",
            vec![vec![
                Ok(
                    serde_json::to_vec(&openai_chat_completion_response("partial"))
                        .expect("chat completion response should serialize"),
                ),
                Err("body read failed".to_string()),
            ]],
        )
        .await;
        let secondary = spawn_json_upstream(
            "/chat/completions",
            vec![(200, openai_chat_completion_response("ok"))],
        )
        .await;
        let service_state = headless_service_state_for_tests();
        install_failover_group(
            &service_state,
            vec![
                test_rule(
                    "p1",
                    RuleProtocol::OpenaiCompletion,
                    primary.base_url.clone(),
                    "gpt-test",
                ),
                test_rule(
                    "p2",
                    RuleProtocol::OpenaiCompletion,
                    secondary.base_url.clone(),
                    "gpt-test",
                ),
            ],
            1,
        );

        let request = json!({
            "model": "claude-test",
            "max_tokens": 16,
            "messages": [{"role": "user", "content": "hello"}]
        });

        let (failure_status, failure_payload) =
            send_messages_request(&service_state, request.clone()).await;
        assert_eq!(failure_status, StatusCode::BAD_GATEWAY);
        assert!(failure_payload["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("Failed to read upstream response"));
        assert_eq!(runtime_failure_count(&service_state, "dev", "p1"), 1);
        assert_eq!(
            runtime_active_failover_provider(&service_state, "dev"),
            Some("p2".to_string())
        );

        let (success_status, success_payload) =
            send_messages_request(&service_state, request).await;
        assert_eq!(success_status, StatusCode::OK);
        assert_eq!(success_payload["content"][0]["text"].as_str(), Some("ok"));
        assert_eq!(primary.hits.load(Ordering::SeqCst), 1);
        assert_eq!(secondary.hits.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn successful_live_request_resets_failure_count_before_threshold_is_reached() {
        let primary = spawn_json_upstream(
            "/chat/completions",
            vec![
                (429, json!({"error": {"message": "rate limited"}})),
                (200, openai_chat_completion_response("primary-ok")),
                (429, json!({"error": {"message": "rate limited again"}})),
                (200, openai_chat_completion_response("primary-final")),
            ],
        )
        .await;
        let secondary = spawn_json_upstream(
            "/chat/completions",
            vec![(200, openai_chat_completion_response("backup"))],
        )
        .await;
        let service_state = headless_service_state_for_tests();
        install_failover_group(
            &service_state,
            vec![
                test_rule(
                    "p1",
                    RuleProtocol::OpenaiCompletion,
                    primary.base_url.clone(),
                    "gpt-test",
                ),
                test_rule(
                    "p2",
                    RuleProtocol::OpenaiCompletion,
                    secondary.base_url.clone(),
                    "gpt-test",
                ),
            ],
            2,
        );

        let request = json!({
            "model": "claude-test",
            "max_tokens": 16,
            "messages": [{"role": "user", "content": "hello"}]
        });

        let (first_status, _) = send_messages_request(&service_state, request.clone()).await;
        assert_eq!(first_status, StatusCode::TOO_MANY_REQUESTS);

        let (second_status, second_payload) =
            send_messages_request(&service_state, request.clone()).await;
        assert_eq!(second_status, StatusCode::OK);
        assert_eq!(
            second_payload["content"][0]["text"].as_str(),
            Some("primary-ok")
        );

        let (third_status, _) = send_messages_request(&service_state, request.clone()).await;
        assert_eq!(third_status, StatusCode::TOO_MANY_REQUESTS);

        let (fourth_status, fourth_payload) = send_messages_request(&service_state, request).await;
        assert_eq!(fourth_status, StatusCode::OK);
        assert_eq!(
            fourth_payload["content"][0]["text"].as_str(),
            Some("primary-final")
        );
        assert_eq!(primary.hits.load(Ordering::SeqCst), 4);
        assert_eq!(secondary.hits.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn local_access_token_accepts_bearer_for_openai_entries() {
        let entry = PathEntry {
            protocol: EntryProtocol::Openai,
            endpoint: EntryEndpoint::Responses,
        };
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            HeaderValue::from_static("Bearer local-test-token"),
        );

        assert!(request_matches_local_access_token(
            &headers,
            &entry,
            "local-test-token"
        ));
    }

    #[test]
    fn local_access_token_accepts_x_api_key_for_anthropic_entries() {
        let entry = PathEntry {
            protocol: EntryProtocol::Anthropic,
            endpoint: EntryEndpoint::Messages,
        };
        let mut headers = HeaderMap::new();
        headers.insert("x-api-key", HeaderValue::from_static("local-test-token"));

        assert!(request_matches_local_access_token(
            &headers,
            &entry,
            "local-test-token"
        ));
    }

    #[test]
    fn local_access_token_allows_bearer_fallback_for_anthropic_entries() {
        let entry = PathEntry {
            protocol: EntryProtocol::Anthropic,
            endpoint: EntryEndpoint::Messages,
        };
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            HeaderValue::from_static("Bearer local-test-token"),
        );

        assert!(request_matches_local_access_token(
            &headers,
            &entry,
            "local-test-token"
        ));
    }

    #[test]
    fn local_access_token_rejects_x_api_key_for_openai_entries() {
        let entry = PathEntry {
            protocol: EntryProtocol::Openai,
            endpoint: EntryEndpoint::ChatCompletions,
        };
        let mut headers = HeaderMap::new();
        headers.insert("x-api-key", HeaderValue::from_static("local-test-token"));

        assert!(!request_matches_local_access_token(
            &headers,
            &entry,
            "local-test-token"
        ));
    }
}
