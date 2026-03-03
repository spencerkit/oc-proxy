//! Module Overview
//! OpenAI Chat Completions adapter implementation.
//! Encodes/decodes chat-completions payloads to/from canonical structures.

use super::super::canonical::{
    CanonicalBlock, CanonicalFinishReason, CanonicalMessage, CanonicalRequest, CanonicalResponse,
    CanonicalRole, CanonicalTool, CanonicalToolCall, CanonicalToolChoice, CanonicalUsage,
    MapOptions,
};
use super::super::helpers::{
    as_array, extract_openai_usage_summary, parse_openai_finish_reason, str_or_empty,
    to_tool_result_content, OpenAIFinishReason,
};
use serde_json::{json, Value};

fn non_null(body: &Value, key: &str) -> Option<Value> {
    body.get(key).filter(|v| !v.is_null()).cloned()
}

fn parse_text_blocks(content: &Value) -> Vec<CanonicalBlock> {
    if let Some(arr) = content.as_array() {
        let mut out = vec![];
        for item in arr {
            if let Some(s) = item.as_str() {
                if !s.is_empty() {
                    out.push(CanonicalBlock::Text(s.to_string()));
                }
                continue;
            }

            if let Some(obj) = item.as_object() {
                let block_type = obj.get("type").and_then(|v| v.as_str()).unwrap_or_default();
                let text = if block_type == "text"
                    || block_type == "input_text"
                    || block_type == "output_text"
                {
                    obj.get("text").and_then(|v| v.as_str())
                } else {
                    obj.get("text")
                        .or_else(|| obj.get("input_text"))
                        .or_else(|| obj.get("output_text"))
                        .and_then(|v| v.as_str())
                };
                if let Some(s) = text {
                    if !s.is_empty() {
                        out.push(CanonicalBlock::Text(s.to_string()));
                    }
                } else {
                    out.push(CanonicalBlock::Text(item.to_string()));
                }
                continue;
            }

            out.push(CanonicalBlock::Text(item.to_string()));
        }
        return out;
    }

    if let Some(s) = content.as_str() {
        if !s.is_empty() {
            return vec![CanonicalBlock::Text(s.to_string())];
        }
        return vec![];
    }

    if content.is_null() {
        return vec![];
    }

    vec![CanonicalBlock::Text(content.to_string())]
}

fn resolve_model(body: &Value, options: &MapOptions) -> String {
    if options.target_model.is_empty() {
        str_or_empty(body.get("model"))
    } else {
        options.target_model.clone()
    }
}

pub fn decode_request(body: &Value, options: &MapOptions) -> Result<CanonicalRequest, String> {
    let mut system_chunks: Vec<String> = vec![];
    let mut messages: Vec<CanonicalMessage> = vec![];

    for msg in as_array(body, "messages") {
        let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or_default();
        if role == "system" {
            if let Some(s) = msg.get("content").and_then(|v| v.as_str()) {
                system_chunks.push(s.to_string());
            }
            continue;
        }

        if role == "assistant" {
            let mut blocks = vec![];
            if let Some(content) = msg.get("content") {
                blocks.extend(parse_text_blocks(content));
            }

            if let Some(tool_calls) = msg.get("tool_calls").and_then(|v| v.as_array()) {
                for call in tool_calls {
                    let input = call
                        .get("function")
                        .and_then(|f| f.get("arguments"))
                        .and_then(|v| v.as_str())
                        .and_then(|s| serde_json::from_str::<Value>(s).ok())
                        .unwrap_or_else(|| {
                            json!({
                                "raw": str_or_empty(
                                    call.get("function").and_then(|f| f.get("arguments"))
                                )
                            })
                        });

                    blocks.push(CanonicalBlock::ToolUse {
                        id: str_or_empty(call.get("id")),
                        name: str_or_empty(call.get("function").and_then(|f| f.get("name"))),
                        input,
                    });
                }
            }

            messages.push(CanonicalMessage {
                role: CanonicalRole::Assistant,
                blocks,
            });
            continue;
        }

        if role == "tool" {
            messages.push(CanonicalMessage {
                role: CanonicalRole::Tool,
                blocks: vec![CanonicalBlock::ToolResult {
                    tool_use_id: msg
                        .get("tool_call_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("toolu_generated")
                        .to_string(),
                    content: to_tool_result_content(msg.get("content").unwrap_or(&Value::Null)),
                }],
            });
            continue;
        }

        messages.push(CanonicalMessage {
            role: CanonicalRole::from_str(role),
            blocks: parse_text_blocks(msg.get("content").unwrap_or(&Value::Null)),
        });
    }

    let tools = body.get("tools").and_then(|v| v.as_array()).map(|arr| {
        arr.iter()
            .map(|tool| {
                let function = tool.get("function").unwrap_or(tool);
                CanonicalTool {
                    name: str_or_empty(function.get("name")),
                    description: function
                        .get("description")
                        .filter(|v| !v.is_null())
                        .cloned(),
                    input_schema: function
                        .get("parameters")
                        .or_else(|| function.get("input_schema"))
                        .cloned()
                        .unwrap_or_else(|| json!({"type": "object", "properties": {}})),
                }
            })
            .collect::<Vec<_>>()
    });

    let tool_choice = body.get("tool_choice").and_then(|tc| {
        if tc.is_string() {
            return Some(CanonicalToolChoice {
                kind: tc.as_str().unwrap_or("auto").to_string(),
                name: None,
            });
        }
        if tc.is_object() {
            return Some(CanonicalToolChoice {
                kind: tc
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("auto")
                    .to_string(),
                name: tc
                    .get("function")
                    .and_then(|f| f.get("name"))
                    .or_else(|| tc.get("name"))
                    .and_then(|v| v.as_str())
                    .map(|v| v.to_string()),
            });
        }
        None
    });

    let system = if let Some(system) = non_null(body, "system") {
        Some(system)
    } else if !system_chunks.is_empty() {
        Some(json!(system_chunks.join("\n\n")))
    } else {
        None
    };

    Ok(CanonicalRequest {
        model: resolve_model(body, options),
        messages,
        max_tokens: body
            .get("max_tokens")
            .or_else(|| body.get("max_output_tokens"))
            .filter(|v| !v.is_null())
            .cloned(),
        temperature: non_null(body, "temperature"),
        top_p: non_null(body, "top_p"),
        stream: body
            .get("stream")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        system,
        tools,
        tool_choice,
        stop: non_null(body, "stop"),
        thinking: non_null(body, "thinking"),
        context_management: non_null(body, "context_management"),
    })
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

pub fn encode_request(request: &CanonicalRequest) -> Value {
    let mut messages: Vec<Value> = vec![];

    if let Some(system) = &request.system {
        messages.push(json!({ "role": "system", "content": system }));
    }

    for msg in &request.messages {
        match &msg.role {
            CanonicalRole::System => {
                if request.system.is_none() {
                    messages.push(json!({
                        "role": "system",
                        "content": merge_text(&msg.blocks),
                    }));
                }
            }
            CanonicalRole::Assistant => {
                let mut tool_calls = vec![];
                for block in &msg.blocks {
                    if let CanonicalBlock::ToolUse { id, name, input } = block {
                        tool_calls.push(json!({
                            "id": if id.is_empty() { "tool_generated" } else { id },
                            "type": "function",
                            "function": {
                                "name": name,
                                "arguments": serde_json::to_string(input)
                                    .unwrap_or_else(|_| "{}".to_string()),
                            }
                        }));
                    }
                }

                let mut assistant_msg = json!({
                    "role": "assistant",
                    "content": merge_text(&msg.blocks),
                });
                if !tool_calls.is_empty() {
                    assistant_msg["tool_calls"] = json!(tool_calls);
                }
                messages.push(assistant_msg);
            }
            CanonicalRole::User => {
                let mut user_text = String::new();
                for block in &msg.blocks {
                    match block {
                        CanonicalBlock::Text(text) => user_text.push_str(text),
                        CanonicalBlock::ToolResult {
                            tool_use_id,
                            content,
                        } => {
                            if !user_text.is_empty() {
                                messages.push(json!({ "role": "user", "content": user_text }));
                                user_text = String::new();
                            }
                            messages.push(json!({
                                "role": "tool",
                                "tool_call_id": if tool_use_id.is_empty() { "tool_generated" } else { tool_use_id },
                                "content": content,
                            }));
                        }
                        CanonicalBlock::ToolUse { .. } => {}
                    }
                }
                if !user_text.is_empty() {
                    messages.push(json!({ "role": "user", "content": user_text }));
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
                        messages.push(json!({
                            "role": "tool",
                            "tool_call_id": if tool_use_id.is_empty() { "tool_generated" } else { tool_use_id },
                            "content": content,
                        }));
                        emitted = true;
                    }
                }

                if !emitted {
                    messages.push(json!({
                        "role": "tool",
                        "tool_call_id": "tool_generated",
                        "content": merge_text(&msg.blocks),
                    }));
                }
            }
            CanonicalRole::Other(role) => {
                messages.push(json!({
                    "role": role,
                    "content": merge_text(&msg.blocks),
                }));
            }
        }
    }

    let mut req = json!({
        "model": request.model,
        "messages": messages,
        "max_tokens": request.max_tokens.clone().unwrap_or(Value::Null),
        "temperature": request.temperature.clone().unwrap_or(Value::Null),
        "top_p": request.top_p.clone().unwrap_or(Value::Null),
        "stream": request.stream,
    });

    if request.stream {
        req["stream_options"] = json!({ "include_usage": true });
    }

    if let Some(tools) = &request.tools {
        req["tools"] = json!(tools
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

    if let Some(choice) = &request.tool_choice {
        if let Some(name) = &choice.name {
            req["tool_choice"] = json!({
                "type": "function",
                "function": { "name": name }
            });
        } else {
            req["tool_choice"] = json!(choice.kind);
        }
    }

    if let Some(stop) = &request.stop {
        req["stop"] = stop.clone();
    }

    req
}

pub fn decode_response(openai_response: &Value, request_model: &str) -> CanonicalResponse {
    let choice = openai_response
        .get("choices")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .cloned()
        .unwrap_or_else(|| json!({}));
    let message = choice.get("message").cloned().unwrap_or_else(|| json!({}));

    let mut tool_calls = vec![];
    if let Some(arr) = message.get("tool_calls").and_then(|v| v.as_array()) {
        for call in arr {
            tool_calls.push(CanonicalToolCall {
                id: call
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("tool_generated")
                    .to_string(),
                name: call
                    .get("function")
                    .and_then(|f| f.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("tool")
                    .to_string(),
                arguments: call
                    .get("function")
                    .and_then(|f| f.get("arguments"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("{}")
                    .to_string(),
            });
        }
    }

    let finish_reason = match parse_openai_finish_reason(
        choice
            .get("finish_reason")
            .and_then(|v| v.as_str())
            .unwrap_or("stop"),
    ) {
        OpenAIFinishReason::ToolCalls => CanonicalFinishReason::ToolUse,
        OpenAIFinishReason::Length => CanonicalFinishReason::MaxTokens,
        OpenAIFinishReason::Stop => CanonicalFinishReason::Stop,
        OpenAIFinishReason::Other(other) => CanonicalFinishReason::Other(other.to_string()),
    };

    let usage = openai_response
        .get("usage")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let usage_summary = extract_openai_usage_summary(&usage).unwrap_or_default();

    CanonicalResponse {
        id: openai_response
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("chatcmpl_generated")
            .to_string(),
        created: openai_response
            .get("created")
            .and_then(|v| v.as_i64())
            .unwrap_or_else(|| chrono::Utc::now().timestamp()),
        model: if request_model.is_empty() {
            openai_response
                .get("model")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string()
        } else {
            request_model.to_string()
        },
        text: message
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        tool_calls,
        finish_reason,
        usage: CanonicalUsage {
            input_tokens: usage_summary.input_tokens,
            output_tokens: usage_summary.output_tokens,
            total_tokens: usage_summary.total_tokens,
        },
    }
}

pub fn encode_response(response: &CanonicalResponse) -> Value {
    let tool_calls = response
        .tool_calls
        .iter()
        .map(|call| {
            json!({
                "id": if call.id.is_empty() { "tool_generated" } else { &call.id },
                "type": "function",
                "function": {
                    "name": call.name,
                    "arguments": call.arguments,
                }
            })
        })
        .collect::<Vec<_>>();

    let mut message = json!({
        "role": "assistant",
        "content": response.text,
    });
    if !tool_calls.is_empty() {
        message["tool_calls"] = json!(tool_calls);
    }

    let finish_reason = match &response.finish_reason {
        CanonicalFinishReason::ToolUse => "tool_calls",
        CanonicalFinishReason::MaxTokens => "length",
        CanonicalFinishReason::Stop => "stop",
        CanonicalFinishReason::Other(other) => other.as_str(),
    };

    let total_tokens = response
        .usage
        .total_tokens
        .unwrap_or(response.usage.input_tokens + response.usage.output_tokens);

    json!({
        "id": response.id,
        "object": "chat.completion",
        "created": response.created,
        "model": response.model,
        "choices": [
            {
                "index": 0,
                "message": message,
                "finish_reason": finish_reason,
            }
        ],
        "usage": {
            "prompt_tokens": response.usage.input_tokens,
            "completion_tokens": response.usage.output_tokens,
            "total_tokens": total_tokens,
        }
    })
}
