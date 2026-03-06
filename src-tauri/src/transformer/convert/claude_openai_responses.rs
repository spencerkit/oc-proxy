//! Claude Messages to OpenAI Responses conversion

use crate::transformer::types::*;
use super::common::*;
use serde_json::{json, Value};

pub fn claude_req_to_openai_responses(claude_req: &[u8], model: &str) -> Result<Vec<u8>, String> {
    let req: ClaudeRequest = serde_json::from_slice(claude_req)
        .map_err(|e| format!("parse: {}", e))?;

    let mut openai_req = json!({
        "model": model,
        "stream": req.stream
    });

    if let Some(system) = &req.system {
        openai_req["instructions"] = json!(extract_system_text(system));
    }

    let mut input = Vec::new();
    for msg in &req.messages {
        let mut item = json!({"type": "message", "role": msg.role});

        let mut content_parts = Vec::new();
        match &msg.content {
            Value::String(s) => {
                content_parts.push(json!({"type": "input_text", "text": s}));
            }
            Value::Array(blocks) => {
                for block in blocks {
                    if let Some(block_type) = block.get("type").and_then(|t| t.as_str()) {
                        match block_type {
                            "text" => {
                                if let Some(text) = block.get("text") {
                                    content_parts.push(json!({"type": "input_text", "text": text}));
                                }
                            }
                            "tool_result" => {
                                if let Some(call_id) = block.get("tool_use_id") {
                                    let content = extract_tool_result_content(
                                        block.get("content").unwrap_or(&Value::Null)
                                    );
                                    content_parts.push(json!({
                                        "type": "function_call_output",
                                        "call_id": call_id,
                                        "output": content
                                    }));
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }
        item["content"] = json!(content_parts);
        input.push(item);
    }
    openai_req["input"] = json!(input);

    if let Some(tools) = &req.tools {
        let openai_tools: Vec<Value> = tools.iter().map(|t| {
            json!({
                "type": "function",
                "name": t.name,
                "description": t.description,
                "parameters": t.input_schema
            })
        }).collect();
        openai_req["tools"] = json!(openai_tools);
    }

    serde_json::to_vec(&openai_req).map_err(|e| format!("serialize: {}", e))
}

pub fn openai_responses_req_to_claude(openai_req: &[u8], model: &str) -> Result<Vec<u8>, String> {
    let req: Value = serde_json::from_slice(openai_req)
        .map_err(|e| format!("parse: {}", e))?;

    let mut claude_req = json!({
        "model": model,
        "max_tokens": 8192,
        "stream": req.get("stream").unwrap_or(&json!(false))
    });

    if let Some(instructions) = req.get("instructions").and_then(|i| i.as_str()) {
        claude_req["system"] = json!(instructions);
    }

    let mut messages = Vec::new();
    let mut pending_tool_uses = Vec::new();
    let mut pending_tool_results = Vec::new();

    if let Some(input) = req.get("input").and_then(|i| i.as_array()) {
        for item in input {
            match item.get("type").and_then(|t| t.as_str()) {
                Some("message") => {
                    if !pending_tool_uses.is_empty() {
                        messages.push(json!({"role": "assistant", "content": pending_tool_uses}));
                        pending_tool_uses = Vec::new();
                    }
                    if !pending_tool_results.is_empty() {
                        messages.push(json!({"role": "user", "content": pending_tool_results}));
                        pending_tool_results = Vec::new();
                    }

                    let role = item.get("role").and_then(|r| r.as_str()).unwrap_or("user");
                    // Claude only supports "user" and "assistant" roles
                    let claude_role = match role {
                        "assistant" => "assistant",
                        _ => "user" // Map developer, system, user all to user
                    };
                    let mut content = Vec::new();

                    if let Some(parts) = item.get("content").and_then(|c| c.as_array()) {
                        for part in parts {
                            if part.get("type").and_then(|t| t.as_str()) == Some("input_text") {
                                if let Some(text) = part.get("text") {
                                    content.push(json!({"type": "text", "text": text}));
                                }
                            }
                        }
                    }

                    // Only add message if content is not empty
                    if !content.is_empty() {
                        messages.push(json!({"role": claude_role, "content": content}));
                    }
                }
                Some("function_call") => {
                    let call_id = item.get("call_id").and_then(|c| c.as_str()).unwrap_or("");
                    let name = item.get("name").and_then(|n| n.as_str()).unwrap_or("");
                    let args_str = item.get("arguments").and_then(|a| a.as_str()).unwrap_or("{}");
                    let input: Value = serde_json::from_str(args_str).unwrap_or(json!({}));

                    pending_tool_uses.push(json!({
                        "type": "tool_use",
                        "id": call_id,
                        "name": name,
                        "input": input
                    }));
                }
                Some("function_call_output") => {
                    // Flush pending tool uses first
                    if !pending_tool_uses.is_empty() {
                        messages.push(json!({"role": "assistant", "content": pending_tool_uses}));
                        pending_tool_uses = Vec::new();
                    }

                    let call_id = item.get("call_id").and_then(|c| c.as_str()).unwrap_or("");
                    let output = item.get("output").and_then(|o| o.as_str()).unwrap_or("");

                    pending_tool_results.push(json!({
                        "type": "tool_result",
                        "tool_use_id": call_id,
                        "content": output
                    }));

                    // Immediately flush tool results after adding
                    messages.push(json!({"role": "user", "content": pending_tool_results}));
                    pending_tool_results = Vec::new();
                }
                _ => {}
            }
        }
    }

    if !pending_tool_uses.is_empty() {
        messages.push(json!({"role": "assistant", "content": pending_tool_uses}));
    }
    if !pending_tool_results.is_empty() {
        messages.push(json!({"role": "user", "content": pending_tool_results}));
    }

    claude_req["messages"] = json!(messages);

    if let Some(tools) = req.get("tools").and_then(|t| t.as_array()) {
        let claude_tools: Vec<Value> = tools.iter().filter_map(|t| {
            if t.get("type").and_then(|ty| ty.as_str()) == Some("function") {
                Some(json!({
                    "name": t.get("name")?,
                    "description": t.get("description")?,
                    "input_schema": t.get("parameters")?
                }))
            } else {
                None
            }
        }).collect();
        if !claude_tools.is_empty() {
            claude_req["tools"] = json!(claude_tools);
        }
    }

    serde_json::to_vec(&claude_req).map_err(|e| format!("serialize: {}", e))
}

pub fn claude_resp_to_openai_responses(claude_resp: &[u8]) -> Result<Vec<u8>, String> {
    let resp: Value = serde_json::from_slice(claude_resp)
        .map_err(|e| format!("parse: {}", e))?;

    let mut content = Vec::new();
    let mut stop_reason = "end_turn";

    if let Some(output) = resp.get("output").and_then(|o| o.as_array()) {
        for item in output {
            match item.get("type").and_then(|t| t.as_str()) {
                Some("message") => {
                    if let Some(parts) = item.get("content").and_then(|c| c.as_array()) {
                        for part in parts {
                            if part.get("type").and_then(|t| t.as_str()) == Some("output_text") {
                                if let Some(text) = part.get("text") {
                                    content.push(json!({"type": "text", "text": text}));
                                }
                            }
                        }
                    }
                }
                Some("function_call") => {
                    if let Some(call_id) = item.get("call_id") {
                        if let Some(name) = item.get("name") {
                            let args_str = item.get("arguments")
                                .and_then(|a| a.as_str())
                                .unwrap_or("{}");
                            let input: Value = serde_json::from_str(args_str).unwrap_or(json!({}));
                            content.push(json!({
                                "type": "tool_use",
                                "id": call_id,
                                "name": name,
                                "input": input
                            }));
                            stop_reason = "tool_use";
                        }
                    }
                }
                _ => {}
            }
        }
    }

    let claude_resp = json!({
        "id": resp.get("id").unwrap_or(&json!("resp-id")),
        "type": "message",
        "role": "assistant",
        "content": content,
        "stop_reason": stop_reason,
        "usage": {
            "input_tokens": resp.get("usage").and_then(|u| u.get("input_tokens")).unwrap_or(&json!(0)),
            "output_tokens": resp.get("usage").and_then(|u| u.get("output_tokens")).unwrap_or(&json!(0))
        }
    });

    serde_json::to_vec(&claude_resp).map_err(|e| format!("serialize: {}", e))
}

pub fn openai_responses_resp_to_claude(openai_resp: &[u8]) -> Result<Vec<u8>, String> {
    let resp: Value = serde_json::from_slice(openai_resp)
        .map_err(|e| format!("parse: {}", e))?;

    let mut content = Vec::new();
    let mut stop_reason = "end_turn";

    if let Some(output) = resp.get("output").and_then(|o| o.as_array()) {
        for item in output {
            match item.get("type").and_then(|t| t.as_str()) {
                Some("message") => {
                    if let Some(parts) = item.get("content").and_then(|c| c.as_array()) {
                        for part in parts {
                            if part.get("type").and_then(|t| t.as_str()) == Some("output_text") {
                                if let Some(text) = part.get("text") {
                                    content.push(json!({"type": "text", "text": text}));
                                }
                            }
                        }
                    }
                }
                Some("function_call") => {
                    if let Some(call_id) = item.get("call_id") {
                        if let Some(name) = item.get("name") {
                            let args_str = item.get("arguments").and_then(|a| a.as_str()).unwrap_or("{}");
                            let input: Value = serde_json::from_str(args_str).unwrap_or(json!({}));
                            content.push(json!({
                                "type": "tool_use",
                                "id": call_id,
                                "name": name,
                                "input": input
                            }));
                            stop_reason = "tool_use";
                        }
                    }
                }
                _ => {}
            }
        }
    }

    let claude_resp = json!({
        "id": resp.get("id").unwrap_or(&json!("resp-id")),
        "type": "message",
        "role": "assistant",
        "content": content,
        "stop_reason": stop_reason,
        "usage": {
            "input_tokens": resp.get("usage").and_then(|u| u.get("input_tokens")).unwrap_or(&json!(0)),
            "output_tokens": resp.get("usage").and_then(|u| u.get("output_tokens")).unwrap_or(&json!(0))
        }
    });

    serde_json::to_vec(&claude_resp).map_err(|e| format!("serialize: {}", e))
}
