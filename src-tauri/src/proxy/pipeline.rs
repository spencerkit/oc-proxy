use super::observability::{
    apply_headers, extract_token_usage, finalize_log, log_simple, plain_headers,
    proxy_error_response, response_headers_json, response_headers_sse, StreamTokenAccumulator,
};
use super::routing::{
    assert_rule_ready, build_rule_headers, detect_entry_protocol, protocol_from_entry,
    refresh_route_index_if_needed, resolve_target_model, resolve_upstream_path,
    resolve_upstream_url, ParsedPath, PathEntry, RouteResolution,
};
use super::{
    ServiceState, MAX_REQUEST_BODY_BYTES, MAX_STREAM_LOG_BODY_BYTES, NON_STREAM_REQUEST_TIMEOUT_MS,
    STREAM_REQUEST_TIMEOUT_MS,
};
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
                            let remaining =
                                MAX_STREAM_LOG_BODY_BYTES.saturating_sub(stream_body.len());
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
                axum::http::HeaderValue::from_str(v)
                    .unwrap_or_else(|_| axum::http::HeaderValue::from_static("")),
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

pub(super) fn build_upstream_body(
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
    _downstream: &crate::models::RuleProtocol,
    upstream_json: &Value,
    _request_model: &str,
) -> Value {
    upstream_json.clone()
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
