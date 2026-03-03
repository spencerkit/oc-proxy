use super::helpers::{
    as_array, flatten_anthropic_text, str_or_empty, to_text_content, to_tool_result_content,
};
use serde_json::{json, Value};

pub fn map_openai_to_anthropic_request(
    body: &Value,
    strict_mode: bool,
    target_model: &str,
) -> Result<Value, String> {
    if strict_mode {
        let supported = [
            "model",
            "messages",
            "stream",
            "max_tokens",
            "max_output_tokens",
            "temperature",
            "top_p",
            "tools",
            "tool_choice",
            "parallel_tool_calls",
            "metadata",
            "stop",
            "input",
            "instructions",
            "reasoning",
            "truncation",
            "previous_response_id",
            "system",
            "thinking",
            "context_management",
        ];
        if let Some(obj) = body.as_object() {
            let unknown = obj
                .keys()
                .filter(|k| !supported.contains(&k.as_str()))
                .cloned()
                .collect::<Vec<_>>();
            if !unknown.is_empty() {
                return Err(format!(
                    "Unsupported OpenAI fields in strict mode: {}",
                    unknown.join(", ")
                ));
            }
        }
    }

    let mut system_chunks: Vec<String> = vec![];
    let mut messages: Vec<Value> = vec![];
    for msg in as_array(body, "messages") {
        let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or_default();
        if role == "system" {
            if let Some(s) = msg.get("content").and_then(|v| v.as_str()) {
                system_chunks.push(s.to_string());
            }
            continue;
        }

        if role == "assistant" {
            let mut content = vec![];
            if let Some(content_value) = msg.get("content") {
                let should_keep = match content_value {
                    Value::Null => false,
                    Value::String(s) => !s.is_empty(),
                    _ => true,
                };
                if should_keep {
                    content.extend(to_text_content(content_value));
                }
            }
            if let Some(tool_calls) = msg.get("tool_calls").and_then(|v| v.as_array()) {
                for call in tool_calls {
                    let input = call
                        .get("function")
                        .and_then(|f| f.get("arguments"))
                        .and_then(|v| v.as_str())
                        .and_then(|s| serde_json::from_str::<Value>(s).ok())
                        .unwrap_or_else(|| {
                            json!({"raw": str_or_empty(call.get("function").and_then(|f| f.get("arguments")))})
                        });
                    content.push(json!({
                        "type": "tool_use",
                        "id": str_or_empty(call.get("id")),
                        "name": str_or_empty(call.get("function").and_then(|f| f.get("name"))),
                        "input": input,
                    }));
                }
            }
            messages.push(json!({"role": "assistant", "content": content}));
            continue;
        }

        if role == "tool" {
            let tool_use_id = msg
                .get("tool_call_id")
                .and_then(|v| v.as_str())
                .unwrap_or("toolu_generated");
            messages.push(json!({
                "role": "user",
                "content": [
                    {
                        "type": "tool_result",
                        "tool_use_id": tool_use_id,
                        "content": to_tool_result_content(msg.get("content").unwrap_or(&Value::Null)),
                    }
                ],
            }));
            continue;
        }

        messages.push(json!({
            "role": role,
            "content": to_text_content(msg.get("content").unwrap_or(&Value::Null)),
        }));
    }

    let mut req = json!({
        "model": if target_model.is_empty() { str_or_empty(body.get("model")) } else { target_model.to_string() },
        "max_tokens": body.get("max_tokens").or_else(|| body.get("max_output_tokens")).cloned().unwrap_or(json!(1024)),
        "temperature": body.get("temperature").cloned().unwrap_or(Value::Null),
        "top_p": body.get("top_p").cloned().unwrap_or(Value::Null),
        "stop_sequences": body.get("stop").cloned().unwrap_or(Value::Null),
        "stream": body.get("stream").and_then(|v| v.as_bool()).unwrap_or(false),
        "messages": messages,
    });

    if let Some(system) = body.get("system") {
        req["system"] = system.clone();
    } else if !system_chunks.is_empty() {
        req["system"] = json!(system_chunks.join("\n\n"));
    }

    if let Some(thinking) = body.get("thinking") {
        req["thinking"] = thinking.clone();
    }

    if let Some(context_management) = body.get("context_management") {
        req["context_management"] = context_management.clone();
    }

    if let Some(tools) = body.get("tools").and_then(|v| v.as_array()) {
        req["tools"] = json!(tools
            .iter()
            .map(|tool| {
                json!({
                    "name": tool.get("function").and_then(|f| f.get("name")).or_else(|| tool.get("name")).cloned().unwrap_or(json!("")),
                    "description": tool.get("function").and_then(|f| f.get("description")).or_else(|| tool.get("description")).cloned().unwrap_or(Value::Null),
                    "input_schema": tool.get("function").and_then(|f| f.get("parameters")).or_else(|| tool.get("parameters")).or_else(|| tool.get("input_schema")).cloned().unwrap_or(json!({"type": "object", "properties": {}})),
                })
            })
            .collect::<Vec<_>>());
    }

    if let Some(tool_choice) = body.get("tool_choice") {
        if tool_choice.is_string() {
            req["tool_choice"] = json!({"type": tool_choice.as_str().unwrap_or("auto")});
        } else if tool_choice.is_object() {
            req["tool_choice"] = json!({
                "type": tool_choice.get("type").and_then(|v| v.as_str()).unwrap_or("auto"),
                "name": tool_choice
                    .get("function")
                    .and_then(|f| f.get("name"))
                    .or_else(|| tool_choice.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or(""),
            });
        }
    }

    Ok(req)
}

pub fn map_anthropic_to_openai_request(
    body: &Value,
    strict_mode: bool,
    target_model: &str,
) -> Result<Value, String> {
    if strict_mode {
        let supported = [
            "model",
            "messages",
            "max_tokens",
            "system",
            "temperature",
            "top_p",
            "stream",
            "tools",
            "tool_choice",
            "stop_sequences",
            "metadata",
            "thinking",
            "context_management",
        ];
        if let Some(obj) = body.as_object() {
            let unknown = obj
                .keys()
                .filter(|k| !supported.contains(&k.as_str()))
                .cloned()
                .collect::<Vec<_>>();
            if !unknown.is_empty() {
                return Err(format!(
                    "Unsupported Claude fields in strict mode: {}",
                    unknown.join(", ")
                ));
            }
        }
    }

    let mut messages: Vec<Value> = vec![];
    if let Some(system) = body.get("system") {
        messages.push(json!({"role": "system", "content": system.clone()}));
    }

    if let Some(in_messages) = body.get("messages").and_then(|v| v.as_array()) {
        for msg in in_messages {
            let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or_default();
            let content = msg.get("content").cloned().unwrap_or(Value::Null);

            if role == "assistant" {
                let text = flatten_anthropic_text(&content);
                let mut assistant_msg = json!({"role": "assistant", "content": text});
                if let Some(arr) = content.as_array() {
                    let tool_calls = arr
                        .iter()
                        .filter(|block| block.get("type").and_then(|v| v.as_str()) == Some("tool_use"))
                        .map(|block| {
                            json!({
                                "id": block.get("id").cloned().unwrap_or(json!("tool_generated")),
                                "type": "function",
                                "function": {
                                    "name": block.get("name").cloned().unwrap_or(json!("tool")),
                                    "arguments": serde_json::to_string(block.get("input").unwrap_or(&json!({}))).unwrap_or_else(|_| "{}".to_string()),
                                }
                            })
                        })
                        .collect::<Vec<_>>();
                    if !tool_calls.is_empty() {
                        assistant_msg["tool_calls"] = json!(tool_calls);
                    }
                }
                messages.push(assistant_msg);
                continue;
            }

            if role == "user" {
                if let Some(arr) = content.as_array() {
                    let mut user_text = String::new();
                    for block in arr {
                        let block_type = block
                            .get("type")
                            .and_then(|v| v.as_str())
                            .unwrap_or_default();
                        if block_type == "tool_result" {
                            if !user_text.is_empty() {
                                messages.push(json!({"role": "user", "content": user_text}));
                                user_text = String::new();
                            }
                            messages.push(json!({
                                "role": "tool",
                                "tool_call_id": block.get("tool_use_id").cloned().unwrap_or(json!("tool_generated")),
                                "content": to_tool_result_content(block.get("content").unwrap_or(&Value::Null)),
                            }));
                        } else if block_type == "text" {
                            user_text.push_str(
                                block
                                    .get("text")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or_default(),
                            );
                        }
                    }
                    if !user_text.is_empty() {
                        messages.push(json!({"role": "user", "content": user_text}));
                    }
                } else {
                    messages
                        .push(json!({"role": "user", "content": flatten_anthropic_text(&content)}));
                }
                continue;
            }

            messages.push(json!({"role": role, "content": flatten_anthropic_text(&content)}));
        }
    }

    let mut req = json!({
        "model": if target_model.is_empty() { str_or_empty(body.get("model")) } else { target_model.to_string() },
        "messages": messages,
        "max_tokens": body.get("max_tokens").cloned().unwrap_or(Value::Null),
        "temperature": body.get("temperature").cloned().unwrap_or(Value::Null),
        "top_p": body.get("top_p").cloned().unwrap_or(Value::Null),
        "stream": body.get("stream").and_then(|v| v.as_bool()).unwrap_or(false),
    });

    if let Some(tools) = body.get("tools").and_then(|v| v.as_array()) {
        req["tools"] = json!(tools
            .iter()
            .map(|tool| {
                json!({
                    "type": "function",
                    "function": {
                        "name": tool.get("name").cloned().unwrap_or(json!("")),
                        "description": tool.get("description").cloned().unwrap_or(Value::Null),
                        "parameters": tool.get("input_schema").cloned().unwrap_or(json!({"type": "object", "properties": {}})),
                    }
                })
            })
            .collect::<Vec<_>>());
    }

    if let Some(tool_choice_name) = body
        .get("tool_choice")
        .and_then(|tc| tc.get("name"))
        .and_then(|v| v.as_str())
    {
        req["tool_choice"] = json!({
            "type": "function",
            "function": { "name": tool_choice_name }
        });
    }

    if let Some(stop_sequences) = body.get("stop_sequences") {
        req["stop"] = stop_sequences.clone();
    }

    Ok(req)
}
