use super::super::canonical::{
    CanonicalBlock, CanonicalFinishReason, CanonicalMessage, CanonicalRequest, CanonicalResponse,
    CanonicalRole, CanonicalTool, CanonicalToolCall, CanonicalToolChoice, CanonicalUsage,
    MapOptions,
};
use super::super::helpers::{flatten_anthropic_text, str_or_empty, to_tool_result_content};
use serde_json::{json, Value};

fn non_null(body: &Value, key: &str) -> Option<Value> {
    body.get(key).filter(|v| !v.is_null()).cloned()
}

fn parse_blocks(role: &CanonicalRole, content: &Value) -> Vec<CanonicalBlock> {
    if let Some(arr) = content.as_array() {
        let mut blocks = Vec::with_capacity(arr.len());
        for block in arr {
            let block_type = block
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            match block_type {
                "text" => {
                    let text = block
                        .get("text")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default();
                    if !text.is_empty() {
                        blocks.push(CanonicalBlock::Text(text.to_string()));
                    }
                }
                "tool_use" => {
                    blocks.push(CanonicalBlock::ToolUse {
                        id: str_or_empty(block.get("id")),
                        name: str_or_empty(block.get("name")),
                        input: block.get("input").cloned().unwrap_or_else(|| json!({})),
                    });
                }
                "tool_result" => {
                    blocks.push(CanonicalBlock::ToolResult {
                        tool_use_id: str_or_empty(block.get("tool_use_id")),
                        content: to_tool_result_content(
                            block.get("content").unwrap_or(&Value::Null),
                        ),
                    });
                }
                _ => {
                    if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                        if !text.is_empty() {
                            blocks.push(CanonicalBlock::Text(text.to_string()));
                        }
                    }
                }
            }
        }
        return blocks;
    }

    if let Some(s) = content.as_str() {
        if !s.is_empty() {
            return vec![CanonicalBlock::Text(s.to_string())];
        }
        return Vec::new();
    }

    // Anthropic payloads should normally be string/array for message content;
    // keep unknown content as text to avoid dropping user input.
    if !content.is_null() {
        return vec![CanonicalBlock::Text(content.to_string())];
    }

    if *role == CanonicalRole::Assistant || *role == CanonicalRole::User {
        return Vec::new();
    }

    let flattened = flatten_anthropic_text(content);
    if flattened.is_empty() {
        Vec::new()
    } else {
        vec![CanonicalBlock::Text(flattened)]
    }
}

pub fn decode_request(body: &Value, options: &MapOptions) -> Result<CanonicalRequest, String> {
    let model = if options.target_model.is_empty() {
        str_or_empty(body.get("model"))
    } else {
        options.target_model.clone()
    };

    let mut messages = vec![];
    if let Some(in_messages) = body.get("messages").and_then(|v| v.as_array()) {
        for msg in in_messages {
            let role = CanonicalRole::from_str(
                msg.get("role").and_then(|v| v.as_str()).unwrap_or_default(),
            );
            let content = msg.get("content").cloned().unwrap_or(Value::Null);
            messages.push(CanonicalMessage {
                blocks: parse_blocks(&role, &content),
                role,
            });
        }
    }

    let tools = body.get("tools").and_then(|v| v.as_array()).map(|arr| {
        arr.iter()
            .map(|tool| CanonicalTool {
                name: str_or_empty(tool.get("name")),
                description: tool.get("description").filter(|v| !v.is_null()).cloned(),
                input_schema: tool
                    .get("input_schema")
                    .cloned()
                    .unwrap_or_else(|| json!({"type": "object", "properties": {}})),
            })
            .collect::<Vec<_>>()
    });

    let tool_choice = body.get("tool_choice").and_then(|tc| {
        tc.as_object().map(|_| CanonicalToolChoice {
            kind: tc
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("auto")
                .to_string(),
            name: tc
                .get("name")
                .and_then(|v| v.as_str())
                .map(|v| v.to_string()),
        })
    });

    Ok(CanonicalRequest {
        model,
        messages,
        max_tokens: non_null(body, "max_tokens"),
        temperature: non_null(body, "temperature"),
        top_p: non_null(body, "top_p"),
        stream: body
            .get("stream")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        system: non_null(body, "system"),
        tools,
        tool_choice,
        stop: non_null(body, "stop_sequences"),
        thinking: non_null(body, "thinking"),
        context_management: non_null(body, "context_management"),
    })
}

fn push_text_block(out: &mut Vec<Value>, text: &str) {
    if !text.is_empty() {
        out.push(json!({ "type": "text", "text": text }));
    }
}

pub fn encode_request(request: &CanonicalRequest) -> Value {
    let mut system_chunks = vec![];
    let mut messages = vec![];

    for msg in &request.messages {
        if msg.role == CanonicalRole::System {
            for block in &msg.blocks {
                if let CanonicalBlock::Text(text) = block {
                    if !text.is_empty() {
                        system_chunks.push(text.clone());
                    }
                }
            }
            continue;
        }

        let out_role = match msg.role {
            CanonicalRole::Tool => "user",
            _ => msg.role.as_str(),
        };

        let mut content = vec![];
        for block in &msg.blocks {
            match block {
                CanonicalBlock::Text(text) => push_text_block(&mut content, text),
                CanonicalBlock::ToolUse { id, name, input } => content.push(json!({
                    "type": "tool_use",
                    "id": if id.is_empty() { "toolu_generated" } else { id },
                    "name": name,
                    "input": input,
                })),
                CanonicalBlock::ToolResult {
                    tool_use_id,
                    content: result,
                } => content.push(json!({
                    "type": "tool_result",
                    "tool_use_id": if tool_use_id.is_empty() { "toolu_generated" } else { tool_use_id },
                    "content": result,
                })),
            }
        }

        messages.push(json!({
            "role": out_role,
            "content": content,
        }));
    }

    let mut out = json!({
        "model": request.model,
        "max_tokens": request.max_tokens.clone().unwrap_or_else(|| json!(1024)),
        "temperature": request.temperature.clone().unwrap_or(Value::Null),
        "top_p": request.top_p.clone().unwrap_or(Value::Null),
        "stop_sequences": request.stop.clone().unwrap_or(Value::Null),
        "stream": request.stream,
        "messages": messages,
    });

    if let Some(system) = &request.system {
        out["system"] = system.clone();
    } else if !system_chunks.is_empty() {
        out["system"] = json!(system_chunks.join("\n\n"));
    }

    if let Some(tools) = &request.tools {
        out["tools"] = json!(tools
            .iter()
            .map(|tool| {
                json!({
                    "name": tool.name,
                    "description": tool.description.clone().unwrap_or(Value::Null),
                    "input_schema": tool.input_schema,
                })
            })
            .collect::<Vec<_>>());
    }

    if let Some(choice) = &request.tool_choice {
        let mut out_choice = json!({ "type": choice.kind });
        if let Some(name) = &choice.name {
            out_choice["name"] = json!(name);
        }
        out["tool_choice"] = out_choice;
    }

    if let Some(thinking) = &request.thinking {
        out["thinking"] = thinking.clone();
    }

    if let Some(context_management) = &request.context_management {
        out["context_management"] = context_management.clone();
    }

    out
}

pub fn decode_response(anthropic_response: &Value, request_model: &str) -> CanonicalResponse {
    let mut text_parts = vec![];
    let mut tool_calls = vec![];
    if let Some(arr) = anthropic_response.get("content").and_then(|v| v.as_array()) {
        for block in arr {
            let block_type = block
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if block_type == "text" {
                text_parts.push(
                    block
                        .get("text")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                );
            }
            if block_type == "tool_use" {
                tool_calls.push(CanonicalToolCall {
                    id: block
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("tool_generated")
                        .to_string(),
                    name: block
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("tool")
                        .to_string(),
                    arguments: serde_json::to_string(block.get("input").unwrap_or(&json!({})))
                        .unwrap_or_else(|_| "{}".to_string()),
                });
            }
        }
    }

    CanonicalResponse {
        id: anthropic_response
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("chatcmpl_generated")
            .to_string(),
        created: chrono::Utc::now().timestamp(),
        model: if request_model.is_empty() {
            anthropic_response
                .get("model")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string()
        } else {
            request_model.to_string()
        },
        text: text_parts.join(""),
        finish_reason: if tool_calls.is_empty() {
            CanonicalFinishReason::Stop
        } else {
            CanonicalFinishReason::ToolUse
        },
        usage: CanonicalUsage {
            input_tokens: anthropic_response
                .get("usage")
                .and_then(|u| u.get("input_tokens"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            output_tokens: anthropic_response
                .get("usage")
                .and_then(|u| u.get("output_tokens"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            total_tokens: None,
        },
        tool_calls,
    }
}

pub fn encode_response(response: &CanonicalResponse) -> Value {
    let mut content = vec![];
    if !response.text.is_empty() {
        content.push(json!({
            "type": "text",
            "text": response.text,
        }));
    }

    for call in &response.tool_calls {
        content.push(json!({
            "type": "tool_use",
            "id": if call.id.is_empty() { "tool_generated" } else { &call.id },
            "name": call.name,
            "input": serde_json::from_str::<Value>(&call.arguments).unwrap_or_else(|_| json!({})),
        }));
    }

    let stop_reason = match &response.finish_reason {
        CanonicalFinishReason::ToolUse => "tool_use",
        CanonicalFinishReason::MaxTokens => "max_tokens",
        CanonicalFinishReason::Stop => "end_turn",
        CanonicalFinishReason::Other(other) => other.as_str(),
    };

    json!({
        "id": response.id,
        "type": "message",
        "role": "assistant",
        "model": response.model,
        "content": content,
        "stop_reason": stop_reason,
        "usage": {
            "input_tokens": response.usage.input_tokens,
            "output_tokens": response.usage.output_tokens,
        }
    })
}
