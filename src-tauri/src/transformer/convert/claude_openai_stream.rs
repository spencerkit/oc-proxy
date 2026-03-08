//! Claude to OpenAI streaming conversion

use crate::transformer::types::StreamContext;
use super::common::parse_sse;
use serde_json::{json, Value};

pub fn claude_stream_to_openai(event: &[u8], ctx: &mut StreamContext) -> Result<Vec<u8>, String> {
    let (event_type, json_data) = parse_sse(event);
    if json_data.is_empty() {
        return Ok(Vec::new());
    }

    let data: Value = serde_json::from_str(&json_data).map_err(|e| format!("parse: {}", e))?;

    let mut result = String::new();

    match event_type.as_str() {
        "message_start" => {
            if let Some(msg) = data.get("message") {
                if let Some(id) = msg.get("id").and_then(|v| v.as_str()) {
                    ctx.message_id = id.to_string();
                }
            }
        }
        "content_block_delta" => {
            if let Some(delta) = data.get("delta") {
                if delta.get("type").and_then(|v| v.as_str()) == Some("text_delta") {
                    if let Some(text) = delta.get("text").and_then(|v| v.as_str()) {
                        let chunk = json!({
                            "id": ctx.message_id,
                            "object": "chat.completion.chunk",
                            "model": ctx.model_name,
                            "choices": [{
                                "index": 0,
                                "delta": {"content": text},
                                "finish_reason": null
                            }]
                        });
                        result.push_str(&format!("data: {}\n\n", serde_json::to_string(&chunk).unwrap()));
                    }
                }
            }
        }
        "message_delta" => {
            if let Some(delta) = data.get("delta") {
                if let Some(reason) = delta.get("stop_reason").and_then(|v| v.as_str()) {
                    let finish_reason = match reason {
                        "end_turn" => "stop",
                        "max_tokens" => "length",
                        "tool_use" => "tool_calls",
                        _ => "stop"
                    };
                    let chunk = json!({
                        "id": ctx.message_id,
                        "object": "chat.completion.chunk",
                        "model": ctx.model_name,
                        "choices": [{
                            "index": 0,
                            "delta": {},
                            "finish_reason": finish_reason
                        }]
                    });
                    result.push_str(&format!("data: {}\n\n", serde_json::to_string(&chunk).unwrap()));
                }
            }
        }
        "message_stop" => {
            result.push_str("data: [DONE]\n\n");
        }
        _ => {}
    }

    Ok(result.into_bytes())
}
