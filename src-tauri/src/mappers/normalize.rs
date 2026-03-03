use super::helpers::{input_item_function_arguments, input_item_to_text};
use serde_json::{json, Value};

fn push_responses_input_item_as_message(messages: &mut Vec<Value>, item: &Value) {
    if item.is_null() {
        return;
    }

    let item_type = item
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or_default();

    if item_type == "function_call" {
        messages.push(json!({
            "role": "assistant",
            "content": "",
            "tool_calls": [
                {
                    "id": item
                        .get("call_id")
                        .or_else(|| item.get("id"))
                        .cloned()
                        .unwrap_or_else(|| json!("call_generated")),
                    "type": "function",
                    "function": {
                        "name": item
                            .get("name")
                            .or_else(|| item.get("function").and_then(|f| f.get("name")))
                            .and_then(|v| v.as_str())
                            .unwrap_or("tool"),
                        "arguments": input_item_function_arguments(
                            item.get("arguments")
                                .or_else(|| item.get("function").and_then(|f| f.get("arguments"))),
                        ),
                    },
                }
            ],
        }));
        return;
    }

    if item_type == "function_call_output" {
        messages.push(json!({
            "role": "tool",
            "tool_call_id": item
                .get("call_id")
                .or_else(|| item.get("id"))
                .cloned()
                .unwrap_or_else(|| json!("call_generated")),
            "content": input_item_to_text(item.get("output").or_else(|| item.get("content")).unwrap_or(&Value::Null)),
        }));
        return;
    }

    let role = item.get("role").and_then(|v| v.as_str()).or_else(|| {
        if item_type == "message" {
            Some("user")
        } else {
            None
        }
    });

    if let Some(role_value) = role {
        messages.push(json!({
            "role": role_value,
            "content": item.get("content").cloned().unwrap_or_else(|| json!("")),
        }));
        return;
    }

    if item_type == "input_text" {
        if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
            messages.push(json!({ "role": "user", "content": text }));
        }
    }
}

pub fn normalize_openai_request(path: &str, body: &Value) -> Value {
    if path != "/v1/responses" {
        return body.clone();
    }

    let mut messages: Vec<Value> = vec![];
    let input = body.get("input").unwrap_or(&Value::Null);

    if let Some(s) = input.as_str() {
        messages.push(json!({ "role": "user", "content": s }));
    } else if let Some(arr) = input.as_array() {
        for item in arr {
            push_responses_input_item_as_message(&mut messages, item);
        }
    } else if input.is_object() {
        push_responses_input_item_as_message(&mut messages, input);
    }

    json!({
        "model": body.get("model").cloned().unwrap_or(Value::Null),
        "messages": messages,
        "stream": body.get("stream").cloned().unwrap_or(Value::Null),
        "max_tokens": body
            .get("max_tokens")
            .or_else(|| body.get("max_output_tokens"))
            .cloned()
            .unwrap_or(Value::Null),
        "temperature": body.get("temperature").cloned().unwrap_or(Value::Null),
        "top_p": body.get("top_p").cloned().unwrap_or(Value::Null),
        "tools": body.get("tools").cloned().unwrap_or(Value::Null),
        "tool_choice": body.get("tool_choice").cloned().unwrap_or(Value::Null),
        "metadata": body.get("metadata").cloned().unwrap_or(Value::Null),
        "stop": body.get("stop").cloned().unwrap_or(Value::Null),
        "system": body
            .get("system")
            .or_else(|| body.get("instructions"))
            .cloned()
            .unwrap_or(Value::Null),
        "thinking": body.get("thinking").cloned().unwrap_or(Value::Null),
        "context_management": body
            .get("context_management")
            .cloned()
            .unwrap_or(Value::Null),
    })
}
