//! Module Overview
//! Request processing pipeline for /oc endpoints.
//! Handles auth, routing, request/response mapping, upstream I/O, streaming, metrics, and final logging.

use super::observability::{
    apply_headers, extract_token_usage, finalize_log, log_simple, plain_headers,
    proxy_error_response, response_headers_json, response_headers_sse, StreamTokenAccumulator,
};
use super::routing::{
    assert_rule_ready, build_rule_headers, detect_entry_protocol, refresh_route_index_if_needed,
    resolve_target_model, resolve_upstream_path, resolve_upstream_url, EntryEndpoint, ParsedPath,
    PathEntry, RouteResolution,
};
use super::stream_bridge::{create_stream_bridge, map_non_stream_response_via_bridge};
use super::{
    ServiceState, MAX_REQUEST_BODY_BYTES, MAX_STREAM_LOG_BODY_BYTES,
    MESSAGES_TO_RESPONSES_NON_STREAM_REQUEST_TIMEOUT_MS, NON_STREAM_REQUEST_TIMEOUT_MS,
    STREAM_REQUEST_TIMEOUT_MS,
};
use crate::mappers::{map_request_by_surface, map_response_by_surface, MapperSurface};
use crate::models::RuleProtocol;
use axum::body::{to_bytes, Body, Bytes};
use axum::extract::{Path, State};
use axum::http::{HeaderMap, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use futures_util::TryStreamExt;
use serde_json::{json, Value};
use tokio::sync::mpsc;
use uuid::Uuid;

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

    if method != Method::POST {
        let payload = json!({"error": {"code": "not_found", "message": "Use POST /oc/:groupId/:endpoint (messages/chat/completions/responses)"}});
        return reject_and_log(&state, trace_id, method, &parsed_path, 404, payload).await;
    }

    if let Err(msg) = refresh_route_index_if_needed(&state) {
        state.metrics.increment_error();
        return proxy_error_response(500, "proxy_error", &msg, None, "proxy", &trace_id);
    }

    let (auth_enabled, expected_auth, capture_body, strict_mode) = match state.config.read() {
        Ok(cfg) => {
            let expected = format!("Bearer {}", cfg.server.local_bearer_token);
            (
                cfg.server.auth_enabled,
                expected,
                cfg.logging.capture_body,
                cfg.compat.strict_mode,
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
    let upstream_path = resolve_upstream_path(&target_protocol);
    let upstream_url = match resolve_upstream_url(&active_route.rule.api_address, upstream_path) {
        Ok(v) => v,
        Err(msg) => {
            state.metrics.increment_error();
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
                None,
                Some(request_body.clone()),
                None,
                Some(json!({
                    "error": {
                        "code": "proxy_error",
                        "message": msg.clone(),
                    }
                })),
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
    ) {
        Ok(v) => v,
        Err(msg) => {
            state.metrics.increment_error();
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
                Some(request_body.clone()),
                None,
                Some(json!({
                    "error": {
                        "code": "proxy_error",
                        "message": msg.clone(),
                    }
                })),
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
    let request_timeout_ms = resolve_request_timeout_ms(stream, &entry, &target_protocol);

    let upstream_resp = match state
        .client
        .post(upstream_url.clone())
        .timeout(std::time::Duration::from_millis(request_timeout_ms))
        .headers(reqwest::header::HeaderMap::from_iter(
            upstream_headers.iter().filter_map(|(k, v)| {
                let name = reqwest::header::HeaderName::from_bytes(k.as_bytes()).ok()?;
                let value = reqwest::header::HeaderValue::from_str(v).ok()?;
                Some((name, value))
            }),
        ))
        .json(&upstream_body)
        .send()
        .await
    {
        Ok(r) => r,
        Err(err) => {
            let err_msg = format!("Upstream request failed: {err}");
            state.metrics.increment_error();
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
                Some(request_body.clone()),
                Some(upstream_body.clone()),
                Some(json!({
                    "error": {
                        "code": "upstream_error",
                        "message": err_msg.clone(),
                    }
                })),
                Some(502),
                None,
                None,
                Some(response_headers_json(&trace_id)),
                None,
                started.elapsed().as_millis() as u64,
                "error",
                capture_body,
            );
            return proxy_error_response(502, "upstream_error", &err_msg, None, "proxy", &trace_id);
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

    if stream && upstream_ct.contains("text/event-stream") {
        let source_surface = surface_from_rule_protocol(&target_protocol);
        let target_surface = surface_from_entry(&entry);
        let mut stream_bridge = if upstream_is_error {
            None
        } else {
            create_stream_bridge(source_surface, target_surface, &requested_model)
        };
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
        let stream_upstream_is_error = upstream_is_error;
        let stream_started = started;

        tokio::spawn(async move {
            let mut bytes_stream = upstream_resp.bytes_stream();
            let mut usage_acc = StreamTokenAccumulator::default();
            let mut stream_failed = false;
            let mut downstream_closed = false;
            let mut stream_body = Vec::<u8>::new();
            let mut stream_body_truncated = false;

            loop {
                match bytes_stream.try_next().await {
                    Ok(Some(bytes)) => {
                        usage_acc.consume_chunk(bytes.as_ref());
                        let outgoing_chunks = if let Some(bridge) = stream_bridge.as_mut() {
                            bridge.consume_chunk(bytes.as_ref())
                        } else {
                            vec![bytes]
                        };

                        for outgoing in outgoing_chunks {
                            if stream_capture_body && !stream_body_truncated {
                                let remaining =
                                    MAX_STREAM_LOG_BODY_BYTES.saturating_sub(stream_body.len());
                                if remaining == 0 {
                                    stream_body_truncated = true;
                                } else if outgoing.len() <= remaining {
                                    stream_body.extend_from_slice(outgoing.as_ref());
                                } else {
                                    stream_body.extend_from_slice(&outgoing.as_ref()[..remaining]);
                                    stream_body_truncated = true;
                                }
                            }
                            if tx.send(Ok(outgoing)).await.is_err() {
                                downstream_closed = true;
                                break;
                            }
                        }
                        if downstream_closed {
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

            if !stream_failed && !downstream_closed {
                if let Some(bridge) = stream_bridge.as_mut() {
                    let trailing = bridge.finish();
                    for outgoing in trailing {
                        if stream_capture_body && !stream_body_truncated {
                            let remaining =
                                MAX_STREAM_LOG_BODY_BYTES.saturating_sub(stream_body.len());
                            if remaining == 0 {
                                stream_body_truncated = true;
                            } else if outgoing.len() <= remaining {
                                stream_body.extend_from_slice(outgoing.as_ref());
                            } else {
                                stream_body.extend_from_slice(&outgoing.as_ref()[..remaining]);
                                stream_body_truncated = true;
                            }
                        }
                        if tx.send(Ok(outgoing)).await.is_err() {
                            break;
                        }
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
                &trace_id,
                &method,
                &parsed_path,
                &active_route.group_name,
                &active_route.rule,
                &entry,
                Some(&requested_model),
                Some(&target_model),
                Some(&upstream_url),
                Some(request_body.clone()),
                Some(upstream_body.clone()),
                Some(json!({
                    "error": {
                        "code": "upstream_error",
                        "message": err_msg.clone(),
                    }
                })),
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
                &trace_id,
                &method,
                &parsed_path,
                &active_route.group_name,
                &active_route.rule,
                &entry,
                Some(&requested_model),
                Some(&target_model),
                Some(&upstream_url),
                Some(request_body.clone()),
                Some(upstream_body.clone()),
                Some(json!({
                    "error": {
                        "code": "upstream_error",
                        "message": err_msg.clone(),
                    },
                    "upstream_raw": upstream_text.chars().take(200).collect::<String>(),
                })),
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
            &trace_id,
            &method,
            &parsed_path,
            &active_route.group_name,
            &active_route.rule,
            &entry,
            Some(&requested_model),
            Some(&target_model),
            Some(&upstream_url),
            Some(request_body.clone()),
            Some(upstream_body.clone()),
            Some(upstream_json.clone()),
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

    let output_body = map_response_body(&entry, &target_protocol, &upstream_json, &requested_model);

    let token_usage =
        extract_token_usage(&upstream_json).or_else(|| extract_token_usage(&output_body));
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

/// Build upstream payload for the selected target protocol surface.
///
/// - Same-surface forwarding is pass-through plus resolved target model override.
/// - Cross-surface forwarding uses canonical mapper conversion.
/// - Cross-surface streaming keeps mapper output (`stream`), while SSE bytes are
///   forwarded directly from upstream.
/// - For `messages -> responses`, stream is forced to false for compatibility.
pub(super) fn build_upstream_body(
    entry: &PathEntry,
    target_protocol: &RuleProtocol,
    request_body: &Value,
    strict_mode: bool,
    target_model: &str,
) -> Result<Value, String> {
    let source_surface = surface_from_entry(entry);
    let target_surface = surface_from_rule_protocol(target_protocol);

    if source_surface == target_surface {
        return Ok(passthrough_with_model(request_body, target_model));
    }

    let mut mapped = map_request_by_surface(
        source_surface,
        target_surface,
        request_body,
        strict_mode,
        target_model,
    )?;
    if mapped.is_object() {
        mapped["model"] = json!(target_model);
        if source_surface == MapperSurface::AnthropicMessages
            && target_surface == MapperSurface::OpenaiResponses
        {
            mapped["stream"] = json!(false);
            let has_tools = mapped
                .get("tools")
                .and_then(|v| v.as_array())
                .map(|arr| !arr.is_empty())
                .unwrap_or(false);
            let tool_choice_missing = mapped
                .get("tool_choice")
                .map(|v| v.is_null())
                .unwrap_or(true);
            if has_tools && tool_choice_missing {
                mapped["tool_choice"] = json!("auto");
            }
            let parallel_tool_calls_missing = mapped
                .get("parallel_tool_calls")
                .map(|v| v.is_null())
                .unwrap_or(true);
            if has_tools && parallel_tool_calls_missing {
                mapped["parallel_tool_calls"] = json!(true);
            }
            // TODO(protocol-compat): negotiate upstream capability and restore
            // max_output_tokens forwarding when the responses endpoint supports it.
            if let Some(obj) = mapped.as_object_mut() {
                obj.remove("max_output_tokens");
            }
        }
    }
    Ok(mapped)
}

/// Map upstream response body back to the downstream entry surface.
fn map_response_body(
    entry: &PathEntry,
    target_protocol: &RuleProtocol,
    upstream_json: &Value,
    request_model: &str,
) -> Value {
    let source_surface = surface_from_rule_protocol(target_protocol);
    let target_surface = surface_from_entry(entry);

    if source_surface == target_surface {
        return upstream_json.clone();
    }

    if let Some(mapped) = map_non_stream_response_via_bridge(
        source_surface,
        target_surface,
        upstream_json,
        request_model,
    ) {
        return mapped;
    }

    map_response_by_surface(source_surface, target_surface, upstream_json, request_model)
}

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

/// Convert parsed downstream endpoint into mapper surface enum.
fn surface_from_entry(entry: &PathEntry) -> MapperSurface {
    match entry.endpoint {
        EntryEndpoint::Messages => MapperSurface::AnthropicMessages,
        EntryEndpoint::ChatCompletions => MapperSurface::OpenaiChatCompletions,
        EntryEndpoint::Responses => MapperSurface::OpenaiResponses,
    }
}

/// Convert active rule protocol into mapper surface enum.
fn surface_from_rule_protocol(protocol: &RuleProtocol) -> MapperSurface {
    match protocol {
        RuleProtocol::Anthropic => MapperSurface::AnthropicMessages,
        RuleProtocol::Openai => MapperSurface::OpenaiResponses,
        RuleProtocol::OpenaiCompletion => MapperSurface::OpenaiChatCompletions,
    }
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
