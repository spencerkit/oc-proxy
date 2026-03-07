use axum::body::Bytes;
use serde_json::Value;

/// Appends one SSE frame with explicit `event:` and JSON `data:` payload.
pub(super) fn push_sse_event(out: &mut Vec<Bytes>, event: &str, payload: &Value) {
    out.push(encode_sse_json_event(event, payload));
}

/// Appends one SSE frame with JSON payload in `data:` field only.
pub(super) fn push_sse_data_json(out: &mut Vec<Bytes>, payload: &Value) {
    out.push(encode_sse_data_json(payload));
}

/// Appends the SSE terminal sentinel frame.
pub(super) fn push_sse_done(out: &mut Vec<Bytes>) {
    out.push(Bytes::from("data: [DONE]\n\n"));
}

/// Encodes an `event + data` SSE frame into bytes.
fn encode_sse_json_event(event: &str, payload: &Value) -> Bytes {
    Bytes::from(format!("event: {event}\ndata: {}\n\n", payload))
}

/// Encodes a `data`-only SSE frame into bytes.
fn encode_sse_data_json(payload: &Value) -> Bytes {
    Bytes::from(format!("data: {}\n\n", payload))
}
