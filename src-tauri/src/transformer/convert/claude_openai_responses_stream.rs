//! Claude to OpenAI Responses streaming conversion

use crate::transformer::types::StreamContext;
use super::common::parse_sse;
use serde_json::{json, Value};

pub fn claude_stream_to_openai_responses(event: &[u8], ctx: &mut StreamContext) -> Result<Vec<u8>, String> {
    let (event_type, json_data) = parse_sse(event);
    if json_data.is_empty() {
        return Ok(Vec::new());
    }

    let data: Value = serde_json::from_str(&json_data).map_err(|e| format!("parse: {}", e))?;

    let mut result = String::new();

    match event_type.as_str() {
        "message_start" => {
            if let Some(msg) = data.get("message") {
                if let Some(id) = msg.get("id").and_then(|v| v.as_str()) {
                    ctx.message_id = id.to_string();
                }
                if let Some(usage) = msg.get("usage") {
                    if let Some(input) = usage.get("input_tokens").and_then(|v| v.as_i64()) {
                        ctx.input_tokens = input as i32;
                    }
                }
            }
            let evt = json!({
                "type": "response.created",
                "response": {
                    "id": ctx.message_id,
                    "object": "response",
                    "status": "in_progress"
                }
            });
            result.push_str(&format!("data: {}\n\n", serde_json::to_string(&evt).unwrap()));
        }
        "content_block_start" => {
            if let Some(block) = data.get("content_block") {
                let idx = data.get("index").and_then(|i| i.as_i64()).unwrap_or(0);
                match block.get("type").and_then(|t| t.as_str()) {
                    Some("text") => {
                        ctx.content_block_started = true;
                        ctx.content_index = idx as i32;

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
                    Some("tool_use") => {
                        ctx.tool_block_started = true;
                        ctx.tool_index = idx as i32;
                        if let Some(id) = block.get("id").and_then(|v| v.as_str()) {
                            ctx.current_tool_id = id.to_string();
                        }
                        if let Some(name) = block.get("name").and_then(|v| v.as_str()) {
                            ctx.current_tool_name = name.to_string();
                        }

                        let evt = json!({
                            "type": "response.output_item.added",
                            "output_index": idx,
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
                    _ => {}
                }
            }
        }
        "content_block_delta" => {
            if let Some(delta) = data.get("delta") {
                let idx = data.get("index").and_then(|i| i.as_i64()).unwrap_or(0) as i32;

                // Auto-generate message_start if not sent
                if ctx.message_id.is_empty() {
                    ctx.message_id = format!("msg_{}", uuid::Uuid::new_v4().to_string());
                    let evt = json!({
                        "type": "response.created",
                        "response": {
                            "id": ctx.message_id,
                            "object": "response",
                            "status": "in_progress"
                        }
                    });
                    result.push_str(&format!("data: {}\n\n", serde_json::to_string(&evt).unwrap()));
                }

                match delta.get("type").and_then(|t| t.as_str()) {
                    Some("text_delta") => {
                        // Auto-generate content_block_start if not started
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

                        if let Some(text) = delta.get("text").and_then(|t| t.as_str()) {
                            let evt = json!({
                                "type": "response.output_text.delta",
                                "output_index": ctx.content_index,
                                "content_index": 0,
                                "delta": text
                            });
                            result.push_str(&format!("data: {}\n\n", serde_json::to_string(&evt).unwrap()));
                        }
                    }
                    Some("input_json_delta") => {
                        let idx = data.get("index").and_then(|i| i.as_i64()).unwrap_or(0) as i32;

                        // If tool block hasn't started, emit start event first
                        if !ctx.tool_block_started {
                            ctx.tool_block_started = true;
                            ctx.tool_index = idx;

                            let start_evt = json!({
                                "type": "response.output_item.added",
                                "output_index": idx,
                                "item": {
                                    "type": "function_call",
                                    "id": format!("tool_{}", idx),
                                    "call_id": format!("tool_{}", idx),
                                    "name": "unknown",
                                    "arguments": "",
                                    "status": "in_progress"
                                }
                            });
                            result.push_str(&format!("data: {}\n\n", serde_json::to_string(&start_evt).unwrap()));

                            // Initialize tool fields if not set
                            if ctx.current_tool_id.is_empty() {
                                ctx.current_tool_id = format!("tool_{}", idx);
                            }
                        }

                        if let Some(partial) = delta.get("partial_json").and_then(|p| p.as_str()) {
                            ctx.tool_arguments.push_str(partial);
                            let evt = json!({
                                "type": "response.function_call_arguments.delta",
                                "output_index": ctx.tool_index,
                                "delta": partial
                            });
                            result.push_str(&format!("data: {}\n\n", serde_json::to_string(&evt).unwrap()));
                        }
                    }
                    _ => {}
                }
            }
        }
        "content_block_stop" => {
            let idx = data.get("index").and_then(|i| i.as_i64()).unwrap_or(0);

            // Handle tool block stop
            if ctx.tool_block_started && idx == ctx.tool_index as i64 {
                let evt1 = json!({
                    "type": "response.function_call_arguments.done",
                    "output_index": idx,
                    "arguments": ctx.tool_arguments
                });
                result.push_str(&format!("data: {}\n\n", serde_json::to_string(&evt1).unwrap()));

                let evt2 = json!({
                    "type": "response.output_item.done",
                    "output_index": idx,
                    "item": {
                        "type": "function_call",
                        "id": if ctx.current_tool_id.is_empty() { format!("tool_{}", idx) } else { ctx.current_tool_id.clone() },
                        "call_id": if ctx.current_tool_id.is_empty() { format!("tool_{}", idx) } else { ctx.current_tool_id.clone() },
                        "name": if ctx.current_tool_name.is_empty() { "unknown".to_string() } else { ctx.current_tool_name.clone() },
                        "arguments": ctx.tool_arguments,
                        "status": "completed"
                    }
                });
                result.push_str(&format!("data: {}\n\n", serde_json::to_string(&evt2).unwrap()));
                ctx.tool_block_started = false;
                ctx.tool_arguments = String::new();
            }
            // Handle content block stop
            else if ctx.content_block_started && idx == ctx.content_index as i64 {
                let evt1 = json!({
                    "type": "response.output_text.done",
                    "output_index": idx,
                    "content_index": 0
                });
                result.push_str(&format!("data: {}\n\n", serde_json::to_string(&evt1).unwrap()));

                let evt2 = json!({
                    "type": "response.content_part.done",
                    "output_index": idx,
                    "content_index": 0,
                    "part": {"type": "output_text"}
                });
                result.push_str(&format!("data: {}\n\n", serde_json::to_string(&evt2).unwrap()));

                let evt3 = json!({
                    "type": "response.output_item.done",
                    "output_index": idx,
                    "item": {
                        "type": "message",
                        "id": format!("msg_{}_{}", ctx.message_id, idx),
                        "role": "assistant",
                        "status": "completed"
                    }
                });
                result.push_str(&format!("data: {}\n\n", serde_json::to_string(&evt3).unwrap()));
                ctx.content_block_started = false;
            }
        }
        "message_delta" => {
            if let Some(usage) = data.get("usage") {
                if let Some(output) = usage.get("output_tokens").and_then(|v| v.as_i64()) {
                    ctx.output_tokens = output as i32;
                }
            }
        }
        "message_stop" => {
            // Close any unclosed tool block
            if ctx.tool_block_started {
                let evt1 = json!({
                    "type": "response.function_call_arguments.done",
                    "output_index": ctx.tool_index,
                    "arguments": ctx.tool_arguments
                });
                result.push_str(&format!("data: {}\n\n", serde_json::to_string(&evt1).unwrap()));

                let evt2 = json!({
                    "type": "response.output_item.done",
                    "output_index": ctx.tool_index,
                    "item": {
                        "type": "function_call",
                        "id": if ctx.current_tool_id.is_empty() { format!("tool_{}", ctx.tool_index) } else { ctx.current_tool_id.clone() },
                        "call_id": if ctx.current_tool_id.is_empty() { format!("tool_{}", ctx.tool_index) } else { ctx.current_tool_id.clone() },
                        "name": if ctx.current_tool_name.is_empty() { "unknown".to_string() } else { ctx.current_tool_name.clone() },
                        "arguments": ctx.tool_arguments,
                        "status": "completed"
                    }
                });
                result.push_str(&format!("data: {}\n\n", serde_json::to_string(&evt2).unwrap()));
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
        }
        _ => {}
    }

    Ok(result.into_bytes())
}
