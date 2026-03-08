//! OpenAI Chat Completions to OpenAI Responses conversion

use serde_json::{json, Value};

/// Convert Chat Completions request to Responses request
pub fn openai_chat_to_responses(chat_req: &[u8], model: &str) -> Result<Vec<u8>, String> {
    let req: Value = serde_json::from_slice(chat_req).map_err(|e| format!("parse: {}", e))?;

    let mut input = Vec::new();
    let mut instructions = None;

    if let Some(messages) = req.get("messages").and_then(|m| m.as_array()) {
        for msg in messages {
            let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("user");

            // Extract system message as instructions
            if role == "system" {
                if let Some(content) = msg.get("content") {
                    instructions = Some(match content {
                        Value::String(s) => s.clone(),
                        _ => serde_json::to_string(content).unwrap_or_default(),
                    });
                }
                continue; // Skip adding system message to input
            }

            // Handle assistant messages with tool_calls
            if role == "assistant" {
                if let Some(tool_calls) = msg.get("tool_calls").and_then(|t| t.as_array()) {
                    for tc in tool_calls {
                        let id = tc.get("id").and_then(|i| i.as_str()).unwrap_or("");
                        let name = tc
                            .get("function")
                            .and_then(|f| f.get("name"))
                            .and_then(|n| n.as_str())
                            .unwrap_or("");
                        let args = tc
                            .get("function")
                            .and_then(|f| f.get("arguments"))
                            .and_then(|a| a.as_str())
                            .unwrap_or("{}");

                        input.push(json!({
                            "type": "function_call",
                            "id": id,
                            "call_id": id,
                            "name": name,
                            "arguments": args,
                            "status": "completed"
                        }));
                    }
                    continue; // Skip adding as message item
                }
            }

            // Handle tool messages
            if role == "tool" {
                let call_id = msg
                    .get("tool_call_id")
                    .and_then(|c| c.as_str())
                    .unwrap_or("");
                let output = msg
                    .get("content")
                    .and_then(|c| match c {
                        Value::String(s) => Some(s.clone()),
                        _ => Some(serde_json::to_string(c).unwrap_or_default()),
                    })
                    .unwrap_or_default();

                input.push(json!({
                    "type": "function_call_output",
                    "call_id": call_id,
                    "output": output
                }));
                continue;
            }

            // Regular user/assistant message
            let mut item = json!({"type": "message", "role": role});

            let mut content_parts = Vec::new();
            if let Some(content) = msg.get("content") {
                match content {
                    Value::String(s) => {
                        content_parts.push(json!({"type": "input_text", "text": s}));
                    }
                    Value::Array(arr) => {
                        for part in arr {
                            if let Some(part_type) = part.get("type").and_then(|t| t.as_str()) {
                                if part_type == "text" {
                                    if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                                        content_parts
                                            .push(json!({"type": "input_text", "text": text}));
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            item["content"] = json!(content_parts);
            input.push(item);
        }
    }

    let mut resp_req = json!({
        "model": model,
        "input": input,
        "stream": req.get("stream").unwrap_or(&json!(false))
    });

    // Add instructions if present
    if let Some(instructions) = instructions {
        resp_req["instructions"] = json!(instructions);
    }

    // Convert tools
    if let Some(tools) = req.get("tools").and_then(|t| t.as_array()) {
        let responses_tools: Vec<Value> = tools
            .iter()
            .filter_map(|t| {
                if t.get("type").and_then(|ty| ty.as_str()) == Some("function") {
                    if let Some(function) = t.get("function") {
                        Some(json!({
                            "type": "function",
                            "name": function.get("name")?,
                            "description": function.get("description"),
                            "parameters": function.get("parameters")?
                        }))
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();

        if !responses_tools.is_empty() {
            resp_req["tools"] = json!(responses_tools);
        }
    }

    // Map max_completion_tokens to max_output_tokens
    if let Some(max_tokens) = req
        .get("max_completion_tokens")
        .or_else(|| req.get("max_tokens"))
    {
        resp_req["max_output_tokens"] = max_tokens.clone();
    }

    serde_json::to_vec(&resp_req).map_err(|e| format!("serialize: {}", e))
}

/// Convert Responses request to Chat Completions request
pub fn openai_responses_req_to_chat(resp_req: &[u8], model: &str) -> Result<Vec<u8>, String> {
    let req: Value = serde_json::from_slice(resp_req).map_err(|e| format!("parse: {}", e))?;

    let mut messages = Vec::new();
    let mut pending_tool_calls = Vec::new();

    // Convert instructions to system message
    if let Some(instructions) = req.get("instructions").and_then(|i| i.as_str()) {
        messages.push(json!({
            "role": "system",
            "content": instructions
        }));
    }

    match req.get("input") {
        Some(Value::String(input_text)) => {
            messages.push(json!({
                "role": "user",
                "content": input_text
            }));
        }
        Some(Value::Array(input)) => {
            for item in input {
                match item.get("type").and_then(|t| t.as_str()) {
                    Some("message") => {
                        flush_pending_tool_calls(&mut messages, &mut pending_tool_calls);
                        let role = item.get("role").and_then(|r| r.as_str()).unwrap_or("user");
                        messages.push(json!({
                            "role": role,
                            "content": extract_openai2_text(item.get("content"))
                        }));
                    }
                    Some("function_call") => {
                        let call_id = item
                            .get("call_id")
                            .or_else(|| item.get("id"))
                            .and_then(|c| c.as_str())
                            .unwrap_or("");
                        let name = item.get("name").and_then(|n| n.as_str()).unwrap_or("");
                        let args = item
                            .get("arguments")
                            .and_then(|a| a.as_str())
                            .unwrap_or("{}");
                        pending_tool_calls.push(json!({
                            "id": call_id,
                            "type": "function",
                            "function": {
                                "name": name,
                                "arguments": args
                            }
                        }));
                    }
                    Some("function_call_output") => {
                        flush_pending_tool_calls(&mut messages, &mut pending_tool_calls);
                        let call_id = item.get("call_id").and_then(|c| c.as_str()).unwrap_or("");
                        messages.push(json!({
                            "role": "tool",
                            "tool_call_id": call_id,
                            "content": json_value_to_string(item.get("output"))
                        }));
                    }
                    _ => {}
                }
            }
            flush_pending_tool_calls(&mut messages, &mut pending_tool_calls);
        }
        _ => {}
    }

    let mut chat_req = json!({
        "model": model,
        "messages": messages,
        "stream": req.get("stream").unwrap_or(&json!(false))
    });

    // Convert tools
    if let Some(tools) = req.get("tools").and_then(|t| t.as_array()) {
        let chat_tools: Vec<Value> = tools
            .iter()
            .filter_map(|t| {
                let tool_type = t.get("type").and_then(|ty| ty.as_str())?;
                let name = t.get("name").and_then(|n| n.as_str())?;
                let description = t.get("description").cloned().unwrap_or(Value::Null);
                let parameters = match tool_type {
                    "function" => t.get("parameters").cloned().unwrap_or_else(|| json!({})),
                    "custom" => json!({
                        "type": "object",
                        "properties": {
                            "input": {
                                "type": "string",
                                "description": "The input for this tool"
                            }
                        },
                        "required": ["input"]
                    }),
                    _ => return None,
                };

                Some(json!({
                    "type": "function",
                    "function": {
                        "name": name,
                        "description": description,
                        "parameters": parameters
                    }
                }))
            })
            .collect();

        if !chat_tools.is_empty() {
            chat_req["tools"] = json!(chat_tools);
        }
    }

    // Map max_output_tokens to max_completion_tokens
    if let Some(max_tokens) = req.get("max_output_tokens") {
        let should_set = max_tokens
            .as_i64()
            .map(|v| v > 0)
            .or_else(|| max_tokens.as_u64().map(|v| v > 0))
            .unwrap_or(false);
        if should_set {
            chat_req["max_completion_tokens"] = max_tokens.clone();
        }
    }

    serde_json::to_vec(&chat_req).map_err(|e| format!("serialize: {}", e))
}

fn flush_pending_tool_calls(messages: &mut Vec<Value>, pending_tool_calls: &mut Vec<Value>) {
    if pending_tool_calls.is_empty() {
        return;
    }
    let tool_calls = std::mem::take(pending_tool_calls);
    messages.push(json!({
        "role": "assistant",
        "content": Value::Null,
        "tool_calls": tool_calls
    }));
}

fn extract_openai2_text(content: Option<&Value>) -> String {
    let Some(content) = content else {
        return String::new();
    };
    match content {
        Value::String(s) => s.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|p| {
                let part_type = p.get("type").and_then(|t| t.as_str());
                if matches!(part_type, Some("input_text" | "output_text" | "text")) {
                    p.get("text")
                        .and_then(|t| t.as_str())
                        .map(|t| t.to_string())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

fn json_value_to_string(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(s)) => s.clone(),
        Some(v) => serde_json::to_string(v).unwrap_or_default(),
        None => String::new(),
    }
}

/// Convert Responses response to Chat Completions response
/// Convert Responses response to Chat Completions response
pub fn openai_responses_to_chat(resp: &[u8]) -> Result<Vec<u8>, String> {
    let resp: Value = serde_json::from_slice(resp).map_err(|e| format!("parse: {}", e))?;

    let mut text = String::new();
    let mut tool_calls = Vec::new();
    let mut finish_reason = "stop";

    if let Some(output) = resp.get("output").and_then(|o| o.as_array()) {
        for item in output {
            match item.get("type").and_then(|t| t.as_str()) {
                Some("message") => {
                    if let Some(parts) = item.get("content").and_then(|c| c.as_array()) {
                        for part in parts {
                            if part.get("type").and_then(|t| t.as_str()) == Some("output_text") {
                                if let Some(t) = part.get("text").and_then(|t| t.as_str()) {
                                    text.push_str(t);
                                }
                            }
                        }
                    }
                }
                Some("function_call") => {
                    let id = item.get("id").and_then(|i| i.as_str()).unwrap_or("");
                    let name = item.get("name").and_then(|n| n.as_str()).unwrap_or("");
                    let args = item
                        .get("arguments")
                        .and_then(|a| a.as_str())
                        .unwrap_or("{}");

                    tool_calls.push(json!({
                        "id": id,
                        "type": "function",
                        "function": {
                            "name": name,
                            "arguments": args
                        }
                    }));
                    finish_reason = "tool_calls";
                }
                _ => {}
            }
        }
    }

    let message = if !tool_calls.is_empty() {
        json!({
            "role": "assistant",
            "content": if text.is_empty() { Value::Null } else { Value::String(text) },
            "tool_calls": tool_calls
        })
    } else {
        json!({
            "role": "assistant",
            "content": text
        })
    };

    let input_tokens = resp
        .get("usage")
        .and_then(|u| u.get("input_tokens"))
        .and_then(|t| t.as_i64())
        .unwrap_or(0) as i32;
    let output_tokens = resp
        .get("usage")
        .and_then(|u| u.get("output_tokens"))
        .and_then(|t| t.as_i64())
        .unwrap_or(0) as i32;

    let chat_resp = json!({
        "id": resp.get("id").unwrap_or(&json!("chatcmpl-id")),
        "object": "chat.completion",
        "created": 1234567890,
        "model": resp.get("model").unwrap_or(&json!("gpt-4")),
        "choices": [{
            "index": 0,
            "message": message,
            "finish_reason": finish_reason
        }],
        "usage": {
            "prompt_tokens": input_tokens,
            "completion_tokens": output_tokens,
            "total_tokens": input_tokens + output_tokens
        }
    });

    serde_json::to_vec(&chat_resp).map_err(|e| format!("serialize: {}", e))
}

/// Convert Chat Completions response to Responses response
pub fn openai_chat_resp_to_responses(chat_resp: &[u8]) -> Result<Vec<u8>, String> {
    let resp: Value = serde_json::from_slice(chat_resp).map_err(|e| format!("parse: {}", e))?;

    let mut output = Vec::new();

    if let Some(choices) = resp.get("choices").and_then(|c| c.as_array()) {
        for choice in choices {
            if let Some(message) = choice.get("message") {
                let role = message
                    .get("role")
                    .and_then(|r| r.as_str())
                    .unwrap_or("assistant");

                // Handle text content first
                if let Some(content) = message.get("content") {
                    if let Some(text) = content.as_str() {
                        if !text.is_empty() {
                            output.push(json!({
                                "type": "message",
                                "id": format!("msg_{}", resp.get("id").and_then(|i| i.as_str()).unwrap_or("id")),
                                "role": role,
                                "status": "completed",
                                "content": [{
                                    "type": "output_text",
                                    "text": text
                                }]
                            }));
                        }
                    }
                }

                // Handle tool calls after text
                if let Some(tool_calls) = message.get("tool_calls").and_then(|t| t.as_array()) {
                    for tc in tool_calls {
                        let id = tc.get("id").and_then(|i| i.as_str()).unwrap_or("");
                        let name = tc
                            .get("function")
                            .and_then(|f| f.get("name"))
                            .and_then(|n| n.as_str())
                            .unwrap_or("");
                        let args = tc
                            .get("function")
                            .and_then(|f| f.get("arguments"))
                            .and_then(|a| a.as_str())
                            .unwrap_or("{}");

                        output.push(json!({
                            "type": "function_call",
                            "id": id,
                            "call_id": id,
                            "name": name,
                            "arguments": args,
                            "status": "completed"
                        }));
                    }
                }
            }
        }
    }

    let status = match resp
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|a| a.first())
    {
        Some(choice) => match choice.get("finish_reason").and_then(|f| f.as_str()) {
            Some("stop") | Some("tool_calls") => "completed",
            _ => "completed",
        },
        None => "completed",
    };

    let input_tokens = resp
        .get("usage")
        .and_then(|u| u.get("prompt_tokens"))
        .and_then(|t| t.as_i64())
        .unwrap_or(0) as i32;
    let output_tokens = resp
        .get("usage")
        .and_then(|u| u.get("completion_tokens"))
        .and_then(|t| t.as_i64())
        .unwrap_or(0) as i32;

    let responses_resp = json!({
        "id": resp.get("id").unwrap_or(&json!("resp-id")),
        "object": "response",
        "created_at": 1234567890,
        "status": status,
        "model": resp.get("model").unwrap_or(&json!("gpt-4")),
        "output": output,
        "usage": {
            "input_tokens": input_tokens,
            "output_tokens": output_tokens,
            "total_tokens": input_tokens + output_tokens
        }
    });

    serde_json::to_vec(&responses_resp).map_err(|e| format!("serialize: {}", e))
}
