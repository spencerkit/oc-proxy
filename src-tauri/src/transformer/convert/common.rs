//! Common utility functions for protocol conversion
//! Reference: ccNexus/internal/transformer/convert/common.go

use serde_json::Value;

/// Extract system text from Claude system prompt
pub fn extract_system_text(system: &Value) -> String {
    match system {
        Value::String(s) => s.clone(),
        Value::Array(arr) => {
            let parts: Vec<String> = arr
                .iter()
                .filter_map(|block| {
                    block.get("text").and_then(|t| t.as_str()).map(|s| s.to_string())
                })
                .collect();
            parts.join("\n")
        }
        _ => String::new(),
    }
}

/// Extract tool result content
pub fn extract_tool_result_content(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(arr) => {
            let parts: Vec<String> = arr
                .iter()
                .filter_map(|block| {
                    if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                        Some(text.to_string())
                    } else {
                        None
                    }
                })
                .collect();
            parts.join("\n")
        }
        _ => serde_json::to_string(content).unwrap_or_default(),
    }
}

/// Parse SSE event data
pub fn parse_sse(data: &[u8]) -> (String, String) {
    let text = String::from_utf8_lossy(data);
    let mut event_type = String::new();
    let mut json_data = String::new();

    for line in text.lines() {
        let line = line.trim();
        if let Some(evt) = line.strip_prefix("event: ") {
            event_type = evt.to_string();
        } else if let Some(data) = line.strip_prefix("data: ") {
            json_data = data.to_string();
        }
    }

    (event_type, json_data)
}

/// Build Claude SSE event
pub fn build_claude_event(event_type: &str, data: &Value) -> Vec<u8> {
    let json_str = serde_json::to_string(data).unwrap_or_default();
    format!("event: {}\ndata: {}\n\n", event_type, json_str).into_bytes()
}

/// Build OpenAI streaming chunk
pub fn build_openai_chunk(
    id: &str,
    model: &str,
    content: Option<&str>,
    finish_reason: Option<&str>,
) -> Vec<u8> {
    let mut delta = serde_json::Map::new();
    if let Some(c) = content {
        delta.insert("content".to_string(), Value::String(c.to_string()));
    }

    let chunk = serde_json::json!({
        "id": id,
        "object": "chat.completion.chunk",
        "model": model,
        "choices": [{
            "index": 0,
            "delta": delta,
            "finish_reason": finish_reason
        }]
    });

    format!("data: {}\n\n", serde_json::to_string(&chunk).unwrap_or_default()).into_bytes()
}
