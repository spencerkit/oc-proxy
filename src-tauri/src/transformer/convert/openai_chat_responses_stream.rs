//! OpenAI Chat Completions to OpenAI Responses streaming conversion

use crate::transformer::types::StreamContext;
use super::common::parse_sse;
use serde_json::{json, Value};

/// Convert Chat Completions SSE stream to Responses SSE format
pub fn openai_chat_stream_to_responses(event: &[u8], ctx: &mut StreamContext) -> Result<Vec<u8>, String> {
    let (_, json_data) = parse_sse(event);

    // Handle [DONE] marker
    if json_data == "[DONE]" {
        // Close any unclosed blocks
        let mut result = String::new();

        if ctx.tool_block_started {
            let evt = json!({
                "type": "response.function_call_arguments.done",
                "output_index": ctx.tool_index,
                "arguments": ctx.tool_arguments
            });
            result.push_str(&format!("data: {}\n\n", serde_json::to_string(&evt).unwrap()));

            let evt2 = json!({
                "type": "response.output_item.done",
                "output_index": ctx.tool_index,
                "item": {
                    "type": "function_call",
                    "id": if ctx.current_tool_id.is_empty() { format!("call_{}", ctx.tool_index) } else { ctx.current_tool_id.clone() },
                    "call_id": if ctx.current_tool_id.is_empty() { format!("call_{}", ctx.tool_index) } else { ctx.current_tool_id.clone() },
                    "name": if ctx.current_tool_name.is_empty() { "unknown".to_string() } else { ctx.current_tool_name.clone() },
                    "arguments": ctx.tool_arguments,
                    "status": "completed"
                }
            });
            result.push_str(&format!("data: {}\n\n", serde_json::to_string(&evt2).unwrap()));
        }

        if ctx.content_block_started {
            let evt = json!({
                "type": "response.output_text.done",
                "output_index": ctx.content_index,
                "content_index": 0
            });
            result.push_str(&format!("data: {}\n\n", serde_json::to_string(&evt).unwrap()));

            let evt2 = json!({
                "type": "response.content_part.done",
                "output_index": ctx.content_index,
                "content_index": 0,
                "part": {"type": "output_text"}
            });
            result.push_str(&format!("data: {}\n\n", serde_json::to_string(&evt2).unwrap()));

            let evt3 = json!({
                "type": "response.output_item.done",
                "output_index": ctx.content_index,
                "item": {
                    "type": "message",
                    "id": format!("msg_{}_{}", ctx.message_id, ctx.content_index),
                    "role": "assistant",
                    "status": "completed"
                }
            });
            result.push_str(&format!("data: {}\n\n", serde_json::to_string(&evt3).unwrap()));
        }

        let evt = json!({
            "type": "response.completed",
            "response": {
                "id": ctx.message_id,
                "object": "response",
                "status": "completed",
                "usage": {
                    "input_tokens": ctx.input_tokens,
                    "output_tokens": ctx.output_tokens,
                    "total_tokens": ctx.input_tokens + ctx.output_tokens
                }
            }
        });
        result.push_str(&format!("data: {}\n\n", serde_json::to_string(&evt).unwrap()));
        result.push_str("data: [DONE]\n\n");

        return Ok(result.into_bytes());
    }

    if json_data.is_empty() {
        return Ok(Vec::new());
    }

    let data: Value = serde_json::from_str(&json_data).map_err(|e| format!("parse: {}", e))?;
    let mut result = String::new();

    // Extract message ID from chunk ID
    if ctx.message_id.is_empty() {
        if let Some(id) = data.get("id").and_then(|v| v.as_str()) {
            ctx.message_id = id.to_string();
        } else {
            ctx.message_id = format!("chatcmpl_{}", uuid::Uuid::new_v4());
        }
    }

    // Handle usage if present
    if let Some(usage) = data.get("usage") {
        if let Some(prompt) = usage.get("prompt_tokens").and_then(|v| v.as_i64()) {
            ctx.input_tokens = prompt as i32;
        }
        if let Some(completion) = usage.get("completion_tokens").and_then(|v| v.as_i64()) {
            ctx.output_tokens = completion as i32;
        }
    }

    // Process choices
    if let Some(choices) = data.get("choices").and_then(|c| c.as_array()) {
        for choice in choices {
            let idx = choice.get("index").and_then(|i| i.as_i64()).unwrap_or(0) as i32;

            if let Some(delta) = choice.get("delta") {
                // Auto-generate response.created event on first delta
                if !ctx.message_start_sent {
                    let evt = json!({
                        "type": "response.created",
                        "response": {
                            "id": ctx.message_id,
                            "object": "response",
                            "status": "in_progress"
                        }
                    });
                    result.push_str(&format!("data: {}\n\n", serde_json::to_string(&evt).unwrap()));
                    ctx.message_start_sent = true;
                }

                // Handle role delta (usually first chunk)
                if let Some(_role) = delta.get("role") {
                    // Role is always assistant for responses
                }

                // Handle content delta
                if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                    // Auto-generate message item added if not started
                    if !ctx.content_block_started {
                        ctx.content_block_started = true;
                        ctx.content_index = idx;

                        let evt1 = json!({
                            "type": "response.output_item.added",
                            "output_index": idx,
                            "item": {
                                "type": "message",
                                "id": format!("msg_{}_{}", ctx.message_id, idx),
                                "role": "assistant",
                                "status": "in_progress",
                                "content": []
                            }
                        });
                        result.push_str(&format!("data: {}\n\n", serde_json::to_string(&evt1).unwrap()));

                        let evt2 = json!({
                            "type": "response.content_part.added",
                            "output_index": idx,
                            "content_index": 0,
                            "part": {"type": "output_text", "text": ""}
                        });
                        result.push_str(&format!("data: {}\n\n", serde_json::to_string(&evt2).unwrap()));
                    }

                    let evt = json!({
                        "type": "response.output_text.delta",
                        "output_index": ctx.content_index,
                        "content_index": 0,
                        "delta": content
                    });
                    result.push_str(&format!("data: {}\n\n", serde_json::to_string(&evt).unwrap()));
                }

                // Handle tool_calls delta
                if let Some(tool_calls) = delta.get("tool_calls").and_then(|t| t.as_array()) {
                    for tc in tool_calls {
                        let tool_idx = tc.get("index").and_then(|i| i.as_i64()).unwrap_or(0) as i32;

                        // First chunk for this tool call - has id and function.name
                        if let (Some(id), Some(function)) = (tc.get("id"), tc.get("function")) {
                            // Auto-close content block if open
                            if ctx.content_block_started {
                                let evt = json!({
                                    "type": "response.output_text.done",
                                    "output_index": ctx.content_index,
                                    "content_index": 0
                                });
                                result.push_str(&format!("data: {}\n\n", serde_json::to_string(&evt).unwrap()));

                                let evt2 = json!({
                                    "type": "response.output_item.done",
                                    "output_index": ctx.content_index,
                                    "item": {
                                        "type": "message",
                                        "id": format!("msg_{}_{}", ctx.message_id, ctx.content_index),
                                        "role": "assistant",
                                        "status": "completed"
                                    }
                                });
                                result.push_str(&format!("data: {}\n\n", serde_json::to_string(&evt2).unwrap()));
                                ctx.content_block_started = false;
                            }

                            // Start new tool block
                            if !ctx.tool_block_started || ctx.tool_index != tool_idx {
                                ctx.tool_block_started = true;
                                ctx.tool_index = tool_idx;
                                ctx.tool_arguments = String::new();

                                if let (Some(id_str), Some(name)) = (id.as_str(), function.get("name").and_then(|n| n.as_str())) {
                                    ctx.current_tool_id = id_str.to_string();
                                    ctx.current_tool_name = name.to_string();
                                }

                                let evt = json!({
                                    "type": "response.output_item.added",
                                    "output_index": tool_idx,
                                    "item": {
                                        "type": "function_call",
                                        "id": ctx.current_tool_id,
                                        "call_id": ctx.current_tool_id,
                                        "name": ctx.current_tool_name,
                                        "arguments": "",
                                        "status": "in_progress"
                                    }
                                });
                                result.push_str(&format!("data: {}\n\n", serde_json::to_string(&evt).unwrap()));
                            }
                        }

                        // Subsequent chunks - has function.arguments
                        if let Some(function) = tc.get("function") {
                            if let Some(args) = function.get("arguments").and_then(|a| a.as_str()) {
                                ctx.tool_arguments.push_str(args);

                                let evt = json!({
                                    "type": "response.function_call_arguments.delta",
                                    "output_index": ctx.tool_index,
                                    "delta": args
                                });
                                result.push_str(&format!("data: {}\n\n", serde_json::to_string(&evt).unwrap()));
                            }
                        }
                    }
                }
            }

            // Handle finish_reason
            if let Some(finish_reason) = choice.get("finish_reason").and_then(|f| f.as_str()) {
                // Close tool block if open
                if ctx.tool_block_started {
                    let evt = json!({
                        "type": "response.function_call_arguments.done",
                        "output_index": ctx.tool_index,
                        "arguments": ctx.tool_arguments
                    });
                    result.push_str(&format!("data: {}\n\n", serde_json::to_string(&evt).unwrap()));

                    let evt2 = json!({
                        "type": "response.output_item.done",
                        "output_index": ctx.tool_index,
                        "item": {
                            "type": "function_call",
                            "id": if ctx.current_tool_id.is_empty() { format!("call_{}", ctx.tool_index) } else { ctx.current_tool_id.clone() },
                            "call_id": if ctx.current_tool_id.is_empty() { format!("call_{}", ctx.tool_index) } else { ctx.current_tool_id.clone() },
                            "name": if ctx.current_tool_name.is_empty() { "unknown".to_string() } else { ctx.current_tool_name.clone() },
                            "arguments": ctx.tool_arguments,
                            "status": "completed"
                        }
                    });
                    result.push_str(&format!("data: {}\n\n", serde_json::to_string(&evt2).unwrap()));
                    ctx.tool_block_started = false;
                }

                // Close content block if open
                if ctx.content_block_started {
                    let evt = json!({
                        "type": "response.output_text.done",
                        "output_index": ctx.content_index,
                        "content_index": 0
                    });
                    result.push_str(&format!("data: {}\n\n", serde_json::to_string(&evt).unwrap()));

                    let evt2 = json!({
                        "type": "response.content_part.done",
                        "output_index": ctx.content_index,
                        "content_index": 0,
                        "part": {"type": "output_text"}
                    });
                    result.push_str(&format!("data: {}\n\n", serde_json::to_string(&evt2).unwrap()));

                    let evt3 = json!({
                        "type": "response.output_item.done",
                        "output_index": ctx.content_index,
                        "item": {
                            "type": "message",
                            "id": format!("msg_{}_{}", ctx.message_id, ctx.content_index),
                            "role": "assistant",
                            "status": "completed"
                        }
                    });
                    result.push_str(&format!("data: {}\n\n", serde_json::to_string(&evt3).unwrap()));
                    ctx.content_block_started = false;
                }

                // Send completion event
                let status = match finish_reason {
                    "stop" | "tool_calls" => "completed",
                    _ => "completed"
                };

                let evt = json!({
                    "type": "response.completed",
                    "response": {
                        "id": ctx.message_id,
                        "object": "response",
                        "status": status,
                        "usage": {
                            "input_tokens": ctx.input_tokens,
                            "output_tokens": ctx.output_tokens,
                            "total_tokens": ctx.input_tokens + ctx.output_tokens
                        }
                    }
                });
                result.push_str(&format!("data: {}\n\n", serde_json::to_string(&evt).unwrap()));
                result.push_str("data: [DONE]\n\n");
            }
        }
    }

    Ok(result.into_bytes())
}

/// Convert Responses SSE stream to Chat Completions SSE format
pub fn openai_responses_stream_to_chat(event: &[u8], ctx: &mut StreamContext) -> Result<Vec<u8>, String> {
    let (_, json_data) = parse_sse(event);

    // Handle [DONE] marker
    if json_data == "[DONE]" {
        let mut result = String::new();
        result.push_str("data: [DONE]\n\n");
        return Ok(result.into_bytes());
    }

    if json_data.is_empty() {
        return Ok(Vec::new());
    }

    let data: Value = serde_json::from_str(&json_data).map_err(|e| format!("parse: {}", e))?;
    let mut result = String::new();

    let event_type = data.get("type").and_then(|t| t.as_str()).unwrap_or("");

    match event_type {
        "response.created" => {
            if let Some(response) = data.get("response") {
                if let Some(id) = response.get("id").and_then(|v| v.as_str()) {
                    ctx.message_id = id.to_string();
                }
            }
        }

        "response.output_text.delta" => {
            let output_index = data.get("output_index").and_then(|i| i.as_i64()).unwrap_or(0) as i32;
            let delta = data.get("delta").and_then(|d| d.as_str()).unwrap_or("");

            // Auto-generate role chunk on first content
            if !ctx.message_start_sent {
                let chunk = json!({
                    "id": ctx.message_id,
                    "object": "chat.completion.chunk",
                    "created": 1234567890,
                    "model": ctx.model_name,
                    "choices": [{
                        "index": 0,
                        "delta": {"role": "assistant"},
                        "finish_reason": null
                    }]
                });
                result.push_str(&format!("data: {}\n\n", serde_json::to_string(&chunk).unwrap()));
                ctx.message_start_sent = true;
            }

            let chunk = json!({
                "id": ctx.message_id,
                "object": "chat.completion.chunk",
                "created": 1234567890,
                "model": ctx.model_name,
                "choices": [{
                    "index": output_index,
                    "delta": {"content": delta},
                    "finish_reason": null
                }]
            });
            result.push_str(&format!("data: {}\n\n", serde_json::to_string(&chunk).unwrap()));
        }

        "response.output_item.added" => {
            if let Some(item) = data.get("item") {
                if item.get("type").and_then(|t| t.as_str()) == Some("function_call") {
                    let output_index = data.get("output_index").and_then(|i| i.as_i64()).unwrap_or(0) as i32;

                    if let (Some(id), Some(name)) = (
                        item.get("id").and_then(|i| i.as_str()),
                        item.get("name").and_then(|n| n.as_str())
                    ) {
                        ctx.current_tool_id = id.to_string();
                        ctx.current_tool_name = name.to_string();
                        ctx.tool_index = output_index;

                        let chunk = json!({
                            "id": ctx.message_id,
                            "object": "chat.completion.chunk",
                            "created": 1234567890,
                            "model": ctx.model_name,
                            "choices": [{
                                "index": 0,
                                "delta": {
                                    "tool_calls": [{
                                        "index": output_index,
                                        "id": id,
                                        "type": "function",
                                        "function": {
                                            "name": name,
                                            "arguments": ""
                                        }
                                    }]
                                },
                                "finish_reason": null
                            }]
                        });
                        result.push_str(&format!("data: {}\n\n", serde_json::to_string(&chunk).unwrap()));
                    }
                }
            }
        }

        "response.function_call_arguments.delta" => {
            let output_index = data.get("output_index").and_then(|i| i.as_i64()).unwrap_or(0) as i32;
            let delta = data.get("delta").and_then(|d| d.as_str()).unwrap_or("");

            ctx.tool_arguments.push_str(delta);

            let chunk = json!({
                "id": ctx.message_id,
                "object": "chat.completion.chunk",
                "created": 1234567890,
                "model": ctx.model_name,
                "choices": [{
                    "index": 0,
                    "delta": {
                        "tool_calls": [{
                            "index": output_index,
                            "function": {
                                "arguments": delta
                            }
                        }]
                    },
                    "finish_reason": null
                }]
            });
            result.push_str(&format!("data: {}\n\n", serde_json::to_string(&chunk).unwrap()));
        }

        "response.completed" => {
            let status = data.get("response")
                .and_then(|r| r.get("status"))
                .and_then(|s| s.as_str())
                .unwrap_or("completed");

            let finish_reason = match status {
                "completed" => {
                    // Check if we had tool calls
                    if !ctx.current_tool_id.is_empty() {
                        "tool_calls"
                    } else {
                        "stop"
                    }
                }
                _ => "stop"
            };

            let chunk = json!({
                "id": ctx.message_id,
                "object": "chat.completion.chunk",
                "created": 1234567890,
                "model": ctx.model_name,
                "choices": [{
                    "index": 0,
                    "delta": {},
                    "finish_reason": finish_reason
                }],
                "usage": {
                    "prompt_tokens": ctx.input_tokens,
                    "completion_tokens": ctx.output_tokens,
                    "total_tokens": ctx.input_tokens + ctx.output_tokens
                }
            });
            result.push_str(&format!("data: {}\n\n", serde_json::to_string(&chunk).unwrap()));
        }

        _ => {}
    }

    Ok(result.into_bytes())
}
