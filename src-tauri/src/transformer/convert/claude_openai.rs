//! Claude to OpenAI conversion
//! Reference: ccNexus/internal/transformer/convert/claude_openai.go

use super::common::*;
use crate::transformer::types::*;
use serde_json::{json, Value};

pub fn claude_req_to_openai(claude_req: &[u8], model: &str) -> Result<Vec<u8>, String> {
    let req: ClaudeRequest =
        serde_json::from_slice(claude_req).map_err(|e| format!("parse claude request: {}", e))?;

    let mut messages = Vec::new();

    // Convert system prompt
    if let Some(system) = &req.system {
        let system_text = extract_system_text(system);
        if !system_text.is_empty() {
            messages.push(OpenAIMessage {
                role: "system".to_string(),
                content: Some(Value::String(system_text)),
                tool_calls: None,
                tool_call_id: None,
            });
        }
    }

    // Convert messages
    for msg in &req.messages {
        match &msg.content {
            Value::String(s) => {
                messages.push(OpenAIMessage {
                    role: msg.role.clone(),
                    content: Some(Value::String(s.clone())),
                    tool_calls: None,
                    tool_call_id: None,
                });
            }
            Value::Array(blocks) => {
                let mut text_parts = Vec::new();
                let mut tool_calls = Vec::new();
                let mut tool_results = Vec::new();
                let mut has_thinking = false;

                for block in blocks {
                    let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");

                    match block_type {
                        "text" => {
                            if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                                text_parts.push(text.to_string());
                            }
                        }
                        "thinking" => {
                            has_thinking = true;
                            continue;
                        }
                        "tool_use" => {
                            let id = block.get("id").and_then(|i| i.as_str()).unwrap_or("");
                            let name = block.get("name").and_then(|n| n.as_str()).unwrap_or("");
                            if !id.is_empty() && !name.is_empty() {
                                let args =
                                    serde_json::to_string(block.get("input").unwrap_or(&json!({})))
                                        .unwrap_or_default();
                                tool_calls.push(OpenAIToolCall {
                                    index: None,
                                    id: id.to_string(),
                                    call_type: "function".to_string(),
                                    function: OpenAIFunction {
                                        name: name.to_string(),
                                        arguments: args,
                                    },
                                });
                            }
                        }
                        "tool_result" => {
                            let call_id = block
                                .get("tool_use_id")
                                .and_then(|i| i.as_str())
                                .unwrap_or("");
                            if !call_id.is_empty() {
                                let content = extract_tool_result_content(
                                    block.get("content").unwrap_or(&Value::Null),
                                );
                                tool_results.push(OpenAIMessage {
                                    role: "tool".to_string(),
                                    content: Some(Value::String(content)),
                                    tool_calls: None,
                                    tool_call_id: Some(call_id.to_string()),
                                });
                            }
                        }
                        _ => {} // Skip unknown types including tool_reference
                    }
                }

                if !text_parts.is_empty() || !tool_calls.is_empty() {
                    let content = if !text_parts.is_empty() {
                        Some(Value::String(text_parts.join("")))
                    } else {
                        None
                    };
                    messages.push(OpenAIMessage {
                        role: msg.role.clone(),
                        content,
                        tool_calls: if !tool_calls.is_empty() {
                            Some(tool_calls)
                        } else {
                            None
                        },
                        tool_call_id: None,
                    });
                } else if has_thinking && msg.role == "assistant" {
                    messages.push(OpenAIMessage {
                        role: "assistant".to_string(),
                        content: Some(Value::String("(thinking...)".to_string())),
                        tool_calls: None,
                        tool_call_id: None,
                    });
                }

                messages.extend(tool_results);
            }
            _ => {}
        }
    }

    let mut openai_req = OpenAIRequest {
        model: model.to_string(),
        messages,
        max_tokens: None,
        max_completion_tokens: req.max_tokens,
        temperature: req.temperature,
        stream: req.stream,
        tools: None,
        tool_choice: None,
    };

    // Convert tools
    if let Some(tools) = &req.tools {
        if !tools.is_empty() {
            openai_req.tools = Some(
                tools
                    .iter()
                    .map(|t| OpenAITool {
                        tool_type: "function".to_string(),
                        function: OpenAIToolFunction {
                            name: t.name.clone(),
                            description: t.description.clone(),
                            parameters: t.input_schema.clone(),
                        },
                    })
                    .collect(),
            );
        }
    }

    serde_json::to_vec(&openai_req).map_err(|e| format!("serialize: {}", e))
}
