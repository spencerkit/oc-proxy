//! OpenAI to Claude conversion

use crate::transformer::types::*;
use serde_json::{json, Value};

pub fn openai_resp_to_claude(openai_resp: &[u8]) -> Result<Vec<u8>, String> {
    let resp: OpenAIResponse = serde_json::from_slice(openai_resp)
        .map_err(|e| format!("parse openai response: {}", e))?;

    let choice = resp.choices.first()
        .ok_or_else(|| "no choices in response".to_string())?;

    let mut content = Vec::new();

    if let Some(Value::String(text)) = &choice.message.content {
        if !text.is_empty() {
            content.push(json!({"type": "text", "text": text}));
        }
    }

    if let Some(ref tool_calls) = choice.message.tool_calls {
        for tc in tool_calls {
            let input: Value = serde_json::from_str(&tc.function.arguments)
                .unwrap_or(json!({}));
            content.push(json!({
                "type": "tool_use",
                "id": tc.id,
                "name": tc.function.name,
                "input": input
            }));
        }
    }

    let stop_reason = match choice.finish_reason.as_str() {
        "tool_calls" => "tool_use",
        "stop" => "end_turn",
        "length" => "max_tokens",
        _ => "end_turn"
    };

    let claude_resp = json!({
        "id": resp.id,
        "type": "message",
        "role": "assistant",
        "content": content,
        "stop_reason": stop_reason,
        "usage": {
            "input_tokens": resp.usage.prompt_tokens,
            "output_tokens": resp.usage.completion_tokens
        }
    });

    serde_json::to_vec(&claude_resp).map_err(|e| format!("serialize: {}", e))
}
