//! Module Overview
//! Request processing pipeline for /oc endpoints.
//! Handles auth, routing, request/response mapping, upstream I/O, streaming, metrics, and final logging.

use super::observability::{
    append_processing_log, apply_headers, extract_token_usage, finalize_log, log_simple,
    plain_downstream_headers, plain_headers, proxy_error_response, response_headers_json,
    response_headers_sse, StreamTokenAccumulator,
};
use super::routing::{
    assert_rule_ready, build_rule_headers, detect_entry_protocol, refresh_route_index_if_needed,
    resolve_target_model, resolve_upstream_path, resolve_upstream_url, EntryEndpoint, ParsedPath,
    PathEntry, RouteResolution,
};
use super::{
    ServiceState, MAX_REQUEST_BODY_BYTES, MAX_STREAM_LOG_BODY_BYTES,
    MESSAGES_TO_RESPONSES_NON_STREAM_REQUEST_TIMEOUT_MS, NON_STREAM_REQUEST_TIMEOUT_MS,
    STREAM_REQUEST_TIMEOUT_MS,
};
use crate::models::{RuleProtocol, TokenUsage};
use crate::transformer::convert::{
    claude_openai_responses_stream, claude_openai_stream, openai_chat_responses_stream,
};
use crate::transformer::StreamContext;
use axum::body::{to_bytes, Body, Bytes};
use axum::extract::{Path, State};
use axum::http::{HeaderMap, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::Utc;
use futures_util::TryStreamExt;
use serde_json::{json, Value};
use std::collections::HashSet;
use tokio::sync::mpsc;
use uuid::Uuid;

const MAX_SSE_PENDING_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Copy)]
enum StreamTransform {
    None,
    ClaudeToOpenAIChat,
    ClaudeToOpenAIResponses,
    ChatCompletionsToResponses,
    ResponsesToChatCompletions,
    ResponsesToClaudeMessages,
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

    if let Err(msg) = refresh_route_index_if_needed(&state) {
        state.metrics.increment_error();
        return proxy_error_response(500, "proxy_error", &msg, None, "proxy", &trace_id);
    }

    let (auth_enabled, expected_auth, capture_body, strict_mode, text_tool_call_fallback_enabled) =
        match state.config.read() {
            Ok(cfg) => {
                let expected = format!("Bearer {}", cfg.server.local_bearer_token);
                (
                    cfg.server.auth_enabled,
                    expected,
                    cfg.logging.capture_body,
                    cfg.compat.strict_mode,
                    cfg.compat.text_tool_call_fallback_enabled,
                )
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

    let upstream_body = match build_upstream_body(
        &entry,
        &target_protocol,
        &request_body,
        strict_mode,
        &target_model,
        &state,
    ) {
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
    let request_timeout_ms = resolve_request_timeout_ms(stream, &entry, &target_protocol);

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
        let stream_target_protocol = target_protocol.clone();
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
        let stream_started = started;
        let stream_transform =
            select_stream_transform(stream_entry.endpoint, &stream_target_protocol);
        let mut stream_ctx_moved = stream_ctx;
        let mut stream_probe_sse = sse_fallback_probe_enabled;

        tokio::spawn(async move {
            let mut bytes_stream = upstream_resp.bytes_stream();
            let mut usage_acc = StreamTokenAccumulator::default();
            let mut stream_failed = false;
            let mut downstream_closed = false;
            let mut stream_body = Vec::<u8>::new();
            let mut stream_body_truncated = false;
            let mut stream_debug_body = Vec::<u8>::new();
            let mut sse_pending = Vec::<u8>::new();

            loop {
                let next_chunk = tokio::select! {
                    _ = tx.closed() => {
                        downstream_closed = true;
                        break;
                    }
                    result = bytes_stream.try_next() => result,
                };

                match next_chunk {
                    Ok(Some(bytes)) => {
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
                        let outgoing_chunks = match stream_transform {
                            StreamTransform::None => vec![bytes],
                            _ => {
                                sse_pending.extend_from_slice(bytes.as_ref());
                                if sse_pending.len() > MAX_SSE_PENDING_BYTES {
                                    stream_failed = true;
                                    let finalizer = finalize_stream_transform(
                                        stream_transform,
                                        &mut stream_ctx_moved,
                                    );
                                    if !finalizer.is_empty() {
                                        capture_stream_chunk(
                                            finalizer.as_slice(),
                                            stream_capture_body,
                                            &mut stream_body,
                                            &mut stream_body_truncated,
                                            stream_debug_capture_body,
                                            &mut stream_debug_body,
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
                                    match transform_sse_event(
                                        stream_transform,
                                        event.as_ref(),
                                        &mut stream_ctx_moved,
                                    ) {
                                        Ok(converted) => {
                                            if !converted.is_empty() {
                                                transformed_chunks.push(Bytes::from(converted));
                                            }
                                        }
                                        Err(_) => {
                                            stream_failed = true;
                                            let finalizer = finalize_stream_transform(
                                                stream_transform,
                                                &mut stream_ctx_moved,
                                            );
                                            if !finalizer.is_empty() {
                                                capture_stream_chunk(
                                                    finalizer.as_slice(),
                                                    stream_capture_body,
                                                    &mut stream_body,
                                                    &mut stream_body_truncated,
                                                    stream_debug_capture_body,
                                                    &mut stream_debug_body,
                                                );
                                                if tx
                                                    .send(Ok(Bytes::from(finalizer)))
                                                    .await
                                                    .is_err()
                                                {
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
                            }
                        };

                        for outgoing in outgoing_chunks {
                            capture_stream_chunk(
                                outgoing.as_ref(),
                                stream_capture_body,
                                &mut stream_body,
                                &mut stream_body_truncated,
                                stream_debug_capture_body,
                                &mut stream_debug_body,
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
                        if !matches!(stream_transform, StreamTransform::None)
                            && !sse_pending.is_empty()
                        {
                            let tail_event = std::mem::take(&mut sse_pending);
                            match transform_sse_event(
                                stream_transform,
                                tail_event.as_ref(),
                                &mut stream_ctx_moved,
                            ) {
                                Ok(converted) => {
                                    if !converted.is_empty() {
                                        capture_stream_chunk(
                                            converted.as_slice(),
                                            stream_capture_body,
                                            &mut stream_body,
                                            &mut stream_body_truncated,
                                            stream_debug_capture_body,
                                            &mut stream_debug_body,
                                        );
                                        if tx.send(Ok(Bytes::from(converted))).await.is_err() {
                                            downstream_closed = true;
                                        }
                                    }
                                }
                                Err(_) => {
                                    stream_failed = true;
                                    let finalizer = finalize_stream_transform(
                                        stream_transform,
                                        &mut stream_ctx_moved,
                                    );
                                    if !finalizer.is_empty() {
                                        capture_stream_chunk(
                                            finalizer.as_slice(),
                                            stream_capture_body,
                                            &mut stream_body,
                                            &mut stream_body_truncated,
                                            stream_debug_capture_body,
                                            &mut stream_debug_body,
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
                            let finalizer =
                                finalize_stream_transform(stream_transform, &mut stream_ctx_moved);
                            if !finalizer.is_empty() {
                                capture_stream_chunk(
                                    finalizer.as_slice(),
                                    stream_capture_body,
                                    &mut stream_body,
                                    &mut stream_body_truncated,
                                    stream_debug_capture_body,
                                    &mut stream_debug_body,
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
                        let finalizer =
                            finalize_stream_transform(stream_transform, &mut stream_ctx_moved);
                        if !finalizer.is_empty() {
                            capture_stream_chunk(
                                finalizer.as_slice(),
                                stream_capture_body,
                                &mut stream_body,
                                &mut stream_body_truncated,
                                stream_debug_capture_body,
                                &mut stream_debug_body,
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

            let stream_response_body = if stream_capture_body {
                Some(json!({
                    "stream": true,
                    "payload": String::from_utf8_lossy(&stream_body).to_string(),
                    "truncated": stream_body_truncated,
                }))
            } else {
                Some(json!({"stream": true}))
            };
            let stream_debug_response_body = if stream_debug_capture_body {
                Some(json!({
                    "stream": true,
                    "payload": String::from_utf8_lossy(&stream_debug_body).to_string(),
                    "truncated": false,
                }))
            } else {
                None
            };
            let mut response_headers = response_headers_sse(&stream_trace_id);
            finalize_log(
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

    let output_body = map_response_body(
        &entry,
        &target_protocol,
        &upstream_json,
        &requested_model,
        &state,
        &declared_tool_names,
        enable_text_tool_call_fallback,
    );

    let token_usage = merge_token_usage(
        extract_token_usage(&upstream_json),
        extract_token_usage(&output_body),
    );
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

/// Build upstream payload for the selected target protocol surface.
///
/// - Same-surface forwarding is pass-through plus resolved target model override.
/// - Cross-surface forwarding uses canonical mapper conversion.
/// - Cross-surface streaming keeps mapper output (`stream`), while SSE bytes are
///   forwarded directly from upstream and normalized for the downstream surface.
pub(super) fn build_upstream_body(
    entry: &PathEntry,
    target_protocol: &RuleProtocol,
    request_body: &Value,
    _strict_mode: bool,
    target_model: &str,
    _state: &ServiceState,
) -> Result<Value, String> {
    use crate::transformer::convert::*;

    let request_bytes =
        serde_json::to_vec(request_body).map_err(|e| format!("serialize request: {}", e))?;

    // Determine conversion based on entry and target
    let converted = match (entry.endpoint, target_protocol) {
        // OpenAI Responses -> Claude Messages
        (EntryEndpoint::Responses, RuleProtocol::Anthropic) => {
            claude_openai_responses::openai_responses_req_to_claude(&request_bytes, target_model)?
        }
        // Claude Messages -> OpenAI Responses
        (EntryEndpoint::Messages, RuleProtocol::Openai) => {
            claude_openai_responses::claude_req_to_openai_responses(&request_bytes, target_model)?
        }
        // Claude Messages -> OpenAI Chat Completions
        (EntryEndpoint::Messages, RuleProtocol::OpenaiCompletion) => {
            claude_openai::claude_req_to_openai(&request_bytes, target_model)?
        }
        // OpenAI Chat -> Claude Messages
        (EntryEndpoint::ChatCompletions, RuleProtocol::Anthropic) => {
            // First convert to Claude, then serialize
            let claude_bytes = openai_claude::openai_resp_to_claude(&request_bytes)?;
            claude_bytes
        }
        // OpenAI Responses -> OpenAI Chat Completions
        (EntryEndpoint::Responses, RuleProtocol::OpenaiCompletion) => {
            openai_chat_responses::openai_responses_req_to_chat(&request_bytes, target_model)?
        }
        // Same protocol, just update model
        _ => {
            return Ok(passthrough_with_model(request_body, target_model));
        }
    };

    let mut result: Value =
        serde_json::from_slice(&converted).map_err(|e| format!("parse converted: {}", e))?;

    // Ensure model is set
    if result.is_object() {
        result["model"] = json!(target_model);
    }

    Ok(result)
}

/// Map upstream response body back to the downstream entry surface.
fn map_response_body(
    entry: &PathEntry,
    target_protocol: &RuleProtocol,
    upstream_json: &Value,
    _request_model: &str,
    _state: &ServiceState,
    declared_tool_names: &HashSet<String>,
    text_tool_call_fallback_enabled: bool,
) -> Value {
    use crate::transformer::convert::*;

    let response_bytes = match serde_json::to_vec(upstream_json) {
        Ok(b) => b,
        Err(_) => return upstream_json.clone(),
    };

    // Determine conversion based on entry and target
    let converted = match (entry.endpoint, target_protocol) {
        // Claude Messages -> OpenAI Responses (response)
        (EntryEndpoint::Responses, RuleProtocol::Anthropic) => {
            claude_openai_responses::claude_resp_to_openai_responses(&response_bytes)
        }
        // OpenAI Responses -> Claude Messages (response)
        (EntryEndpoint::Messages, RuleProtocol::Openai) => {
            claude_openai_responses::openai_responses_resp_to_claude_with_options(
                &response_bytes,
                &claude_openai_responses::ResponsesToClaudeOptions {
                    text_tool_call_fallback_enabled,
                    allowed_tool_names: declared_tool_names.clone(),
                },
            )
        }
        // OpenAI Chat -> Claude Messages (response)
        (EntryEndpoint::Messages, RuleProtocol::OpenaiCompletion) => {
            openai_claude::openai_resp_to_claude(&response_bytes)
        }
        // Claude Messages -> OpenAI Chat (response)
        (EntryEndpoint::ChatCompletions, RuleProtocol::Anthropic) => {
            claude_openai_responses::claude_resp_to_openai_responses(&response_bytes)
        }
        // OpenAI Chat Completions -> OpenAI Responses (response)
        (EntryEndpoint::Responses, RuleProtocol::OpenaiCompletion) => {
            openai_chat_responses::openai_chat_resp_to_responses(&response_bytes)
        }
        _ => return upstream_json.clone(),
    };

    match converted {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|_| upstream_json.clone()),
        Err(_) => upstream_json.clone(),
    }
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
pub(super) fn resolve_request_timeout_ms(
    stream: bool,
    entry: &PathEntry,
    target_protocol: &RuleProtocol,
) -> u64 {
    if stream {
        return STREAM_REQUEST_TIMEOUT_MS;
    }

    if matches!(entry.endpoint, EntryEndpoint::Messages)
        && matches!(target_protocol, RuleProtocol::Openai)
    {
        return MESSAGES_TO_RESPONSES_NON_STREAM_REQUEST_TIMEOUT_MS;
    }

    NON_STREAM_REQUEST_TIMEOUT_MS
}

/// Preserve request object shape while enforcing resolved forwarded model.
fn passthrough_with_model(request_body: &Value, target_model: &str) -> Value {
    let mut with_model = if request_body.is_object() {
        request_body.clone()
    } else {
        json!({})
    };
    with_model["model"] = json!(target_model);
    with_model
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

fn select_stream_transform(
    entry_endpoint: EntryEndpoint,
    target_protocol: &RuleProtocol,
) -> StreamTransform {
    match (entry_endpoint, target_protocol) {
        (EntryEndpoint::Responses, RuleProtocol::Anthropic) => {
            StreamTransform::ClaudeToOpenAIResponses
        }
        (EntryEndpoint::ChatCompletions, RuleProtocol::Anthropic) => {
            StreamTransform::ClaudeToOpenAIChat
        }
        (EntryEndpoint::ChatCompletions, RuleProtocol::Openai) => {
            StreamTransform::ResponsesToChatCompletions
        }
        (EntryEndpoint::Messages, RuleProtocol::Openai) => {
            StreamTransform::ResponsesToClaudeMessages
        }
        (EntryEndpoint::Responses, RuleProtocol::OpenaiCompletion) => {
            StreamTransform::ChatCompletionsToResponses
        }
        _ => StreamTransform::None,
    }
}

fn transform_sse_event(
    transform: StreamTransform,
    event: &[u8],
    ctx: &mut StreamContext,
) -> Result<Vec<u8>, String> {
    match transform {
        StreamTransform::None => Ok(event.to_vec()),
        StreamTransform::ClaudeToOpenAIChat => {
            claude_openai_stream::claude_stream_to_openai(event, ctx)
        }
        StreamTransform::ClaudeToOpenAIResponses => {
            claude_openai_responses_stream::claude_stream_to_openai_responses(event, ctx)
        }
        StreamTransform::ChatCompletionsToResponses => {
            openai_chat_responses_stream::openai_chat_stream_to_responses(event, ctx)
        }
        StreamTransform::ResponsesToChatCompletions => {
            openai_chat_responses_stream::openai_responses_stream_to_chat(event, ctx)
        }
        StreamTransform::ResponsesToClaudeMessages => {
            claude_openai_responses_stream::openai_responses_stream_to_claude(event, ctx)
        }
    }
}

fn finalize_stream_transform(transform: StreamTransform, ctx: &mut StreamContext) -> Vec<u8> {
    match transform {
        StreamTransform::ResponsesToClaudeMessages => {
            claude_openai_responses_stream::finalize_openai_responses_stream_to_claude(ctx)
        }
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
        find_sse_delimiter, looks_like_sse_prelude, merge_token_usage, pop_sse_event,
        select_stream_transform, EntryEndpoint, StreamTransform,
    };
    use crate::models::{RuleProtocol, TokenUsage};

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
    fn select_stream_transform_maps_openai_cross_surface_correctly() {
        assert!(matches!(
            select_stream_transform(EntryEndpoint::Responses, &RuleProtocol::OpenaiCompletion),
            StreamTransform::ChatCompletionsToResponses
        ));
        assert!(matches!(
            select_stream_transform(EntryEndpoint::ChatCompletions, &RuleProtocol::Openai),
            StreamTransform::ResponsesToChatCompletions
        ));
        assert!(matches!(
            select_stream_transform(EntryEndpoint::Messages, &RuleProtocol::Openai),
            StreamTransform::ResponsesToClaudeMessages
        ));
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
}
