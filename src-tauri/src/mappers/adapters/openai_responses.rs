//! Module Overview
//! OpenAI Responses adapter implementation.
//! Bridges responses-specific input/output shapes with the canonical mapping model.

use super::super::canonical::{
    CanonicalBlock, CanonicalFinishReason, CanonicalRequest, CanonicalResponse, CanonicalRole,
    CanonicalToolChoice, MapOptions,
};
use super::super::normalize::normalize_openai_request;
use super::openai_chat_completions;
use serde_json::{json, Value};

pub fn decode_request(body: &Value, options: &MapOptions) -> Result<CanonicalRequest, String> {
    let normalized = normalize_openai_request("/v1/responses", body);
    openai_chat_completions::decode_request(&normalized, options)
}

fn merge_text(blocks: &[CanonicalBlock]) -> String {
    let mut out = String::new();
    for block in blocks {
        if let CanonicalBlock::Text(text) = block {
            out.push_str(text);
        }
    }
    out
}

fn push_user_message(input: &mut Vec<Value>, text: &str) {
    if text.is_empty() {
        return;
    }
    input.push(json!({
        "type": "message",
        "role": "user",
        "content": [{ "type": "input_text", "text": text }],
    }));
}

fn push_assistant_message(input: &mut Vec<Value>, text: &str) {
    if text.is_empty() {
        return;
    }
    input.push(json!({
        "type": "message",
        "role": "assistant",
        "content": [{ "type": "output_text", "text": text }],
    }));
}

pub fn encode_request(request: &CanonicalRequest) -> Value {
    let mut input = vec![];
    let mut system_chunks = vec![];

    for msg in &request.messages {
        match &msg.role {
            CanonicalRole::System => {
                let text = merge_text(&msg.blocks);
                if !text.is_empty() {
                    system_chunks.push(text);
                }
            }
            CanonicalRole::User => {
                let mut text = String::new();
                for block in &msg.blocks {
                    match block {
                        CanonicalBlock::Text(s) => text.push_str(s),
                        CanonicalBlock::ToolResult {
                            tool_use_id,
                            content,
                        } => {
                            push_user_message(&mut input, &text);
                            text = String::new();
                            input.push(json!({
                                "type": "function_call_output",
                                "id": if tool_use_id.is_empty() { "call_generated" } else { tool_use_id },
                                "call_id": if tool_use_id.is_empty() { "call_generated" } else { tool_use_id },
                                "output": content,
                            }));
                        }
                        CanonicalBlock::ToolUse { .. } => {}
                    }
                }
                push_user_message(&mut input, &text);
            }
            CanonicalRole::Assistant => {
                push_assistant_message(&mut input, &merge_text(&msg.blocks));
                for block in &msg.blocks {
                    if let CanonicalBlock::ToolUse {
                        id,
                        name,
                        input: args,
                    } = block
                    {
                        let call_id = if id.is_empty() { "call_generated" } else { id };
                        input.push(json!({
                            "type": "function_call",
                            "id": call_id,
                            "call_id": call_id,
                            "status": "completed",
                            "name": name,
                            "arguments": args,
                        }));
                    }
                }
            }
            CanonicalRole::Tool => {
                let mut emitted = false;
                for block in &msg.blocks {
                    if let CanonicalBlock::ToolResult {
                        tool_use_id,
                        content,
                    } = block
                    {
                        emitted = true;
                        input.push(json!({
                            "type": "function_call_output",
                            "id": if tool_use_id.is_empty() { "call_generated" } else { tool_use_id },
                            "call_id": if tool_use_id.is_empty() { "call_generated" } else { tool_use_id },
                            "output": content,
                        }));
                    }
                }

                if !emitted {
                    let text = merge_text(&msg.blocks);
                    if !text.is_empty() {
                        input.push(json!({
                            "type": "function_call_output",
                            "id": "call_generated",
                            "call_id": "call_generated",
                            "output": text,
                        }));
                    }
                }
            }
            CanonicalRole::Other(role) => {
                let text = merge_text(&msg.blocks);
                if !text.is_empty() {
                    input.push(json!({
                        "type": "message",
                        "role": role,
                        "content": [{ "type": "input_text", "text": text }],
                    }));
                }
            }
        }
    }

    let mut out = json!({
        "model": request.model,
        "input": input,
        "stream": request.stream,
        "max_output_tokens": request.max_tokens.clone().unwrap_or(Value::Null),
        "temperature": request.temperature.clone().unwrap_or(Value::Null),
        "top_p": request.top_p.clone().unwrap_or(Value::Null),
    });

    if let Some(tools) = &request.tools {
        out["tools"] = json!(tools
            .iter()
            .map(|tool| {
                json!({
                    "type": "function",
                    "function": {
                        "name": tool.name,
                        "description": tool.description.clone().unwrap_or(Value::Null),
                        "parameters": tool.input_schema,
                    }
                })
            })
            .collect::<Vec<_>>());
    }

    if let Some(CanonicalToolChoice { kind, name }) = &request.tool_choice {
        if let Some(name) = name {
            out["tool_choice"] = json!({
                "type": "function",
                "function": { "name": name }
            });
        } else {
            out["tool_choice"] = json!(kind);
        }
    }

    if let Some(stop) = &request.stop {
        out["stop"] = stop.clone();
    }

    if let Some(system) = &request.system {
        out["instructions"] = system.clone();
    } else if !system_chunks.is_empty() {
        out["instructions"] = json!(system_chunks.join("\n\n"));
    }

    if let Some(thinking) = &request.thinking {
        out["thinking"] = thinking.clone();
    }

    if let Some(context_management) = &request.context_management {
        out["context_management"] = context_management.clone();
    }

    out
}

pub fn decode_response(responses: &Value, request_model: &str) -> CanonicalResponse {
    let mut chat_like = json!({
        "id": responses.get("id").cloned().unwrap_or_else(|| json!("resp_generated")),
        "created": responses
            .get("created_at")
            .cloned()
            .unwrap_or_else(|| json!(chrono::Utc::now().timestamp())),
        "model": responses.get("model").cloned().unwrap_or_else(|| json!("")),
        "choices": [{ "message": { "role": "assistant", "content": "", "tool_calls": [] }, "finish_reason": "stop" }],
        "usage": {
            "input_tokens": responses.get("usage").and_then(|u| u.get("input_tokens")).cloned().unwrap_or_else(|| json!(0)),
            "output_tokens": responses.get("usage").and_then(|u| u.get("output_tokens")).cloned().unwrap_or_else(|| json!(0)),
            "total_tokens": responses.get("usage").and_then(|u| u.get("total_tokens")).cloned().unwrap_or_else(|| json!(0)),
        }
    });

    let mut text = String::new();
    let mut tool_calls = vec![];
    if let Some(arr) = responses.get("output").and_then(|v| v.as_array()) {
        for item in arr {
            let item_type = item
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if item_type == "message" {
                if let Some(parts) = item.get("content").and_then(|v| v.as_array()) {
                    for part in parts {
                        if part.get("type").and_then(|v| v.as_str()) == Some("output_text")
                            || part.get("type").and_then(|v| v.as_str()) == Some("input_text")
                            || part.get("type").and_then(|v| v.as_str()) == Some("text")
                        {
                            text.push_str(
                                part.get("text")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or_default(),
                            );
                        }
                    }
                }
                continue;
            }

            if item_type == "function_call" {
                tool_calls.push(json!({
                    "id": item.get("call_id").or_else(|| item.get("id")).cloned().unwrap_or_else(|| json!("call_generated")),
                    "type": "function",
                    "function": {
                        "name": item.get("name").cloned().unwrap_or_else(|| json!("tool")),
                        "arguments": item.get("arguments").cloned().unwrap_or_else(|| json!("{}")),
                    },
                }));
            }
        }
    }

    chat_like["choices"][0]["message"]["content"] = json!(text);
    chat_like["choices"][0]["message"]["tool_calls"] = json!(tool_calls);
    if !chat_like["choices"][0]["message"]["tool_calls"]
        .as_array()
        .is_some_and(|arr| !arr.is_empty())
    {
        chat_like["choices"][0]["message"]
            .as_object_mut()
            .map(|obj| obj.remove("tool_calls"));
    } else {
        chat_like["choices"][0]["finish_reason"] = json!("tool_calls");
    }

    let mut canonical = super::openai_chat_completions::decode_response(&chat_like, request_model);
    canonical.finish_reason = match responses
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("completed")
    {
        "incomplete" => CanonicalFinishReason::MaxTokens,
        _ => canonical.finish_reason,
    };
    canonical
}

pub fn encode_response(response: &CanonicalResponse) -> Value {
    let mut output = vec![];

    output.push(json!({
        "type": "message",
        "role": "assistant",
        "content": [{"type": "output_text", "text": response.text}],
    }));

    for call in &response.tool_calls {
        output.push(json!({
            "type": "function_call",
            "id": if call.id.is_empty() { "call_generated" } else { &call.id },
            "call_id": if call.id.is_empty() { "call_generated" } else { &call.id },
            "status": "completed",
            "name": call.name,
            "arguments": call.arguments,
        }));
    }

    json!({
        "id": response.id,
        "object": "response",
        "created_at": response.created,
        "model": response.model,
        "status": "completed",
        "output": output,
        "usage": {
            "input_tokens": response.usage.input_tokens,
            "output_tokens": response.usage.output_tokens,
            "total_tokens": response
                .usage
                .total_tokens
                .unwrap_or(response.usage.input_tokens + response.usage.output_tokens),
        },
    })
}
