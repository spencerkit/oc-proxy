use serde_json::{json, Value};

pub fn map_anthropic_to_openai_response(anthropic_response: &Value, request_model: &str) -> Value {
    let mut content_parts = vec![];
    let mut tool_calls = vec![];
    if let Some(arr) = anthropic_response.get("content").and_then(|v| v.as_array()) {
        for block in arr {
            let block_type = block
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if block_type == "text" {
                content_parts.push(
                    block
                        .get("text")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                );
            }
            if block_type == "tool_use" {
                tool_calls.push(json!({
                    "id": block.get("id").cloned().unwrap_or(json!("tool_generated")),
                    "type": "function",
                    "function": {
                        "name": block.get("name").cloned().unwrap_or(json!("tool")),
                        "arguments": serde_json::to_string(block.get("input").unwrap_or(&json!({}))).unwrap_or_else(|_| "{}".to_string()),
                    },
                }));
            }
        }
    }

    let mut message = json!({
        "role": "assistant",
        "content": content_parts.join(""),
    });
    if !tool_calls.is_empty() {
        message["tool_calls"] = json!(tool_calls.clone());
    }

    json!({
        "id": anthropic_response.get("id").cloned().unwrap_or(json!("chatcmpl_generated")),
        "object": "chat.completion",
        "created": chrono::Utc::now().timestamp(),
        "model": if request_model.is_empty() { anthropic_response.get("model").cloned().unwrap_or(json!("")) } else { json!(request_model) },
        "choices": [
            {
                "index": 0,
                "message": message,
                "finish_reason": if tool_calls.is_empty() { "stop" } else { "tool_calls" },
            }
        ],
        "usage": {
            "prompt_tokens": anthropic_response.get("usage").and_then(|u| u.get("input_tokens")).and_then(|v| v.as_u64()).unwrap_or(0),
            "completion_tokens": anthropic_response.get("usage").and_then(|u| u.get("output_tokens")).and_then(|v| v.as_u64()).unwrap_or(0),
            "total_tokens": anthropic_response.get("usage").and_then(|u| u.get("input_tokens")).and_then(|v| v.as_u64()).unwrap_or(0)
                + anthropic_response.get("usage").and_then(|u| u.get("output_tokens")).and_then(|v| v.as_u64()).unwrap_or(0),
        }
    })
}

pub fn map_openai_to_anthropic_response(openai_response: &Value, request_model: &str) -> Value {
    let choice = openai_response
        .get("choices")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .cloned()
        .unwrap_or_else(|| json!({}));
    let message = choice.get("message").cloned().unwrap_or_else(|| json!({}));

    let mut content = vec![];
    if let Some(text) = message.get("content").and_then(|v| v.as_str()) {
        if !text.is_empty() {
            content.push(json!({"type": "text", "text": text}));
        }
    }

    if let Some(tool_calls) = message.get("tool_calls").and_then(|v| v.as_array()) {
        for call in tool_calls {
            content.push(json!({
                "type": "tool_use",
                "id": call.get("id").cloned().unwrap_or(json!("tool_generated")),
                "name": call.get("function").and_then(|f| f.get("name")).cloned().unwrap_or(json!("tool")),
                "input": call
                    .get("function")
                    .and_then(|f| f.get("arguments"))
                    .and_then(|v| v.as_str())
                    .and_then(|s| serde_json::from_str::<Value>(s).ok())
                    .unwrap_or_else(|| json!({})),
            }));
        }
    }

    let finish_reason = choice
        .get("finish_reason")
        .and_then(|v| v.as_str())
        .unwrap_or("stop");

    let stop_reason = match finish_reason {
        "tool_calls" => "tool_use",
        "length" => "max_tokens",
        _ => "end_turn",
    };

    json!({
        "id": openai_response.get("id").cloned().unwrap_or(json!("msg_generated")),
        "type": "message",
        "role": "assistant",
        "model": if request_model.is_empty() { openai_response.get("model").cloned().unwrap_or(json!("")) } else { json!(request_model) },
        "content": content,
        "stop_reason": stop_reason,
        "usage": {
            "input_tokens": openai_response.get("usage").and_then(|u| u.get("prompt_tokens")).and_then(|v| v.as_u64()).unwrap_or(0),
            "output_tokens": openai_response.get("usage").and_then(|u| u.get("completion_tokens")).and_then(|v| v.as_u64()).unwrap_or(0),
        }
    })
}

pub fn map_openai_chat_to_responses(chat_response: &Value) -> Value {
    let choice = chat_response
        .get("choices")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .cloned()
        .unwrap_or_else(|| json!({}));
    let message = choice.get("message").cloned().unwrap_or_else(|| json!({}));
    let text = message
        .get("content")
        .and_then(|v| v.as_str())
        .unwrap_or_default();

    let mut output = vec![json!({
        "type": "message",
        "role": "assistant",
        "content": [{"type": "output_text", "text": text}],
    })];

    if let Some(tool_calls) = message.get("tool_calls").and_then(|v| v.as_array()) {
        for tool_call in tool_calls {
            output.push(json!({
                "type": "function_call",
                "id": tool_call.get("id").cloned().unwrap_or(json!("call_generated")),
                "call_id": tool_call.get("id").cloned().unwrap_or(json!("call_generated")),
                "status": "completed",
                "name": tool_call.get("function").and_then(|f| f.get("name")).cloned().unwrap_or(json!("tool")),
                "arguments": tool_call.get("function").and_then(|f| f.get("arguments")).cloned().unwrap_or(json!("{}")),
            }));
        }
    }

    let usage = chat_response
        .get("usage")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let input_tokens = usage
        .get("input_tokens")
        .and_then(|v| v.as_u64())
        .or_else(|| usage.get("prompt_tokens").and_then(|v| v.as_u64()))
        .unwrap_or(0);
    let output_tokens = usage
        .get("output_tokens")
        .and_then(|v| v.as_u64())
        .or_else(|| usage.get("completion_tokens").and_then(|v| v.as_u64()))
        .unwrap_or(0);

    json!({
        "id": chat_response.get("id").cloned().unwrap_or(json!("resp_generated")),
        "object": "response",
        "created_at": chat_response.get("created").cloned().unwrap_or(json!(chrono::Utc::now().timestamp())),
        "model": chat_response.get("model").cloned().unwrap_or(json!("")),
        "status": "completed",
        "output": output,
        "usage": {
            "input_tokens": input_tokens,
            "output_tokens": output_tokens,
            "total_tokens": usage
                .get("total_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(input_tokens + output_tokens),
        },
    })
}
