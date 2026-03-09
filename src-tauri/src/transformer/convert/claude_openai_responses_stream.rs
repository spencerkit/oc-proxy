//! Claude to OpenAI Responses streaming conversion

use super::common::{build_claude_event, parse_sse};
use crate::transformer::types::StreamContext;
use serde_json::{json, Value};

const THINK_TAG_OPEN: &str = "<think>";
const THINK_TAG_CLOSE: &str = "</think>";

pub fn claude_stream_to_openai_responses(
    event: &[u8],
    ctx: &mut StreamContext,
) -> Result<Vec<u8>, String> {
    let (event_type, json_data) = parse_sse(event);
    if json_data.is_empty() {
        return Ok(Vec::new());
    }

    let data: Value = serde_json::from_str(&json_data).map_err(|e| format!("parse: {}", e))?;

    // Claude may send structured error events over SSE.
    if data.get("type").and_then(|t| t.as_str()) == Some("error") {
        if let Some(message) = data
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(|m| m.as_str())
        {
            return Err(format!("upstream error: {}", message));
        }
        return Err("upstream error".to_string());
    }

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
            result.push_str(&format!(
                "data: {}\n\n",
                serde_json::to_string(&evt).unwrap()
            ));
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
                        result.push_str(&format!(
                            "data: {}\n\n",
                            serde_json::to_string(&evt1).unwrap()
                        ));

                        let evt2 = json!({
                            "type": "response.content_part.added",
                            "output_index": idx,
                            "content_index": 0,
                            "part": {"type": "output_text", "text": ""}
                        });
                        result.push_str(&format!(
                            "data: {}\n\n",
                            serde_json::to_string(&evt2).unwrap()
                        ));
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
                        result.push_str(&format!(
                            "data: {}\n\n",
                            serde_json::to_string(&evt).unwrap()
                        ));
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
                    result.push_str(&format!(
                        "data: {}\n\n",
                        serde_json::to_string(&evt).unwrap()
                    ));
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
                            result.push_str(&format!(
                                "data: {}\n\n",
                                serde_json::to_string(&evt1).unwrap()
                            ));

                            let evt2 = json!({
                                "type": "response.content_part.added",
                                "output_index": idx,
                                "content_index": 0,
                                "part": {"type": "output_text", "text": ""}
                            });
                            result.push_str(&format!(
                                "data: {}\n\n",
                                serde_json::to_string(&evt2).unwrap()
                            ));
                        }

                        if let Some(text) = delta.get("text").and_then(|t| t.as_str()) {
                            let evt = json!({
                                "type": "response.output_text.delta",
                                "output_index": ctx.content_index,
                                "content_index": 0,
                                "delta": text
                            });
                            result.push_str(&format!(
                                "data: {}\n\n",
                                serde_json::to_string(&evt).unwrap()
                            ));
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
                            result.push_str(&format!(
                                "data: {}\n\n",
                                serde_json::to_string(&start_evt).unwrap()
                            ));

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
                            result.push_str(&format!(
                                "data: {}\n\n",
                                serde_json::to_string(&evt).unwrap()
                            ));
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
                result.push_str(&format!(
                    "data: {}\n\n",
                    serde_json::to_string(&evt1).unwrap()
                ));

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
                result.push_str(&format!(
                    "data: {}\n\n",
                    serde_json::to_string(&evt2).unwrap()
                ));
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
                result.push_str(&format!(
                    "data: {}\n\n",
                    serde_json::to_string(&evt1).unwrap()
                ));

                let evt2 = json!({
                    "type": "response.content_part.done",
                    "output_index": idx,
                    "content_index": 0,
                    "part": {"type": "output_text"}
                });
                result.push_str(&format!(
                    "data: {}\n\n",
                    serde_json::to_string(&evt2).unwrap()
                ));

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
                result.push_str(&format!(
                    "data: {}\n\n",
                    serde_json::to_string(&evt3).unwrap()
                ));
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
                result.push_str(&format!(
                    "data: {}\n\n",
                    serde_json::to_string(&evt1).unwrap()
                ));

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
                result.push_str(&format!(
                    "data: {}\n\n",
                    serde_json::to_string(&evt2).unwrap()
                ));
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
            result.push_str(&format!(
                "data: {}\n\n",
                serde_json::to_string(&evt).unwrap()
            ));
            result.push_str("data: [DONE]\n\n");
        }
        _ => {}
    }

    Ok(result.into_bytes())
}

fn split_trailing_partial_tag(s: &str, tag: &str) -> (String, String) {
    if s.is_empty() || tag.is_empty() {
        return (s.to_string(), String::new());
    }

    let max = (tag.len() - 1).min(s.len());
    for (start, _) in s.char_indices().rev() {
        let suffix_len = s.len() - start;
        if suffix_len == 0 || suffix_len > max {
            continue;
        }
        let suffix = &s[start..];
        if tag.starts_with(suffix) {
            return (s[..start].to_string(), suffix.to_string());
        }
    }

    (s.to_string(), String::new())
}

fn close_thinking_block(ctx: &mut StreamContext, result: &mut Vec<u8>) {
    if !ctx.thinking_block_started {
        return;
    }

    result.extend(build_claude_event(
        "content_block_stop",
        &json!({"index": ctx.thinking_index}),
    ));
    ctx.thinking_block_started = false;
}

fn emit_text_delta(ctx: &mut StreamContext, result: &mut Vec<u8>, text: &str) {
    if text.is_empty() {
        return;
    }

    if !ctx.content_block_started {
        ctx.content_block_started = true;
        result.extend(build_claude_event(
            "content_block_start",
            &json!({
                "index": ctx.content_index,
                "content_block": {"type": "text", "text": ""}
            }),
        ));
    }

    result.extend(build_claude_event(
        "content_block_delta",
        &json!({
            "index": ctx.content_index,
            "delta": {"type": "text_delta", "text": text}
        }),
    ));
}

fn emit_thinking_delta(ctx: &mut StreamContext, result: &mut Vec<u8>, text: &str) {
    if text.is_empty() {
        return;
    }

    if !ctx.thinking_block_started {
        if ctx.content_block_started {
            result.extend(build_claude_event(
                "content_block_stop",
                &json!({"index": ctx.content_index}),
            ));
            ctx.content_block_started = false;
            ctx.content_index += 1;
        }

        ctx.thinking_block_started = true;
        ctx.thinking_index = ctx.content_index;
        ctx.content_index += 1;
        result.extend(build_claude_event(
            "content_block_start",
            &json!({
                "index": ctx.thinking_index,
                "content_block": {"type": "thinking", "thinking": ""}
            }),
        ));
    }

    result.extend(build_claude_event(
        "content_block_delta",
        &json!({
            "index": ctx.thinking_index,
            "delta": {"type": "thinking_delta", "thinking": text}
        }),
    ));
}

fn emit_text_with_close(ctx: &mut StreamContext, result: &mut Vec<u8>, text: &str) {
    if text.is_empty() {
        return;
    }

    if ctx.thinking_block_started && !ctx.content_block_started && !ctx.in_thinking_tag {
        close_thinking_block(ctx, result);
    }
    emit_text_delta(ctx, result, text);
}

fn emit_thinking_with_close(ctx: &mut StreamContext, result: &mut Vec<u8>, text: &str) {
    if text.is_empty() {
        return;
    }

    emit_thinking_delta(ctx, result, text);
    if ctx.thinking_block_started {
        close_thinking_block(ctx, result);
    }
}

fn consume_think_tagged_stream(
    mut content: String,
    ctx: &mut StreamContext,
    result: &mut Vec<u8>,
    mut emit_text: impl FnMut(&mut StreamContext, &mut Vec<u8>, &str),
    mut emit_thinking: impl FnMut(&mut StreamContext, &mut Vec<u8>, &str),
) {
    while !content.is_empty() {
        if ctx.in_thinking_tag {
            if let Some(close_idx) = content.find(THINK_TAG_CLOSE) {
                if close_idx > 0 {
                    emit_thinking(ctx, result, &content[..close_idx]);
                }
                ctx.in_thinking_tag = false;
                content = content[close_idx + THINK_TAG_CLOSE.len()..].to_string();
                continue;
            }

            let (text, buffer) = split_trailing_partial_tag(&content, THINK_TAG_CLOSE);
            if !text.is_empty() {
                emit_thinking(ctx, result, &text);
            }
            ctx.thinking_buffer = buffer;
            return;
        }

        if let Some(open_idx) = content.find(THINK_TAG_OPEN) {
            if open_idx > 0 {
                emit_text(ctx, result, &content[..open_idx]);
            }
            ctx.in_thinking_tag = true;
            content = content[open_idx + THINK_TAG_OPEN.len()..].to_string();
            continue;
        }

        let (text, buffer) = split_trailing_partial_tag(&content, THINK_TAG_OPEN);
        if !text.is_empty() {
            emit_text(ctx, result, &text);
        }
        ctx.thinking_buffer = buffer;
        return;
    }
}

fn flush_think_tagged_stream(
    ctx: &mut StreamContext,
    result: &mut Vec<u8>,
    mut emit_text: impl FnMut(&mut StreamContext, &mut Vec<u8>, &str),
    mut emit_thinking: impl FnMut(&mut StreamContext, &mut Vec<u8>, &str),
) {
    if ctx.in_thinking_tag {
        if !ctx.thinking_buffer.is_empty() {
            let buffered = std::mem::take(&mut ctx.thinking_buffer);
            emit_thinking(ctx, result, &buffered);
        }
    } else if !ctx.thinking_buffer.is_empty() {
        let buffered = std::mem::take(&mut ctx.thinking_buffer);
        emit_text(ctx, result, &buffered);
    }

    ctx.in_thinking_tag = false;
}

fn finalize_openai_responses_stream_to_claude_done(ctx: &mut StreamContext) -> Vec<u8> {
    let mut result = Vec::new();
    flush_think_tagged_stream(
        ctx,
        &mut result,
        |ctx, result, text| emit_text_delta(ctx, result, text),
        |ctx, result, text| emit_thinking_delta(ctx, result, text),
    );

    if ctx.thinking_block_started {
        close_thinking_block(ctx, &mut result);
    }
    if ctx.content_block_started {
        result.extend(build_claude_event(
            "content_block_stop",
            &json!({"index": ctx.content_index}),
        ));
        ctx.content_block_started = false;
    }
    if ctx.tool_block_started {
        result.extend(build_claude_event(
            "content_block_stop",
            &json!({"index": ctx.tool_index}),
        ));
        ctx.tool_block_started = false;
    }
    if !ctx.finish_reason_sent {
        result.extend(build_claude_event("message_stop", &json!({})));
        ctx.finish_reason_sent = true;
    }

    result
}

pub fn finalize_openai_responses_stream_to_claude(ctx: &mut StreamContext) -> Vec<u8> {
    if ctx.finish_reason_sent
        || (!ctx.content_block_started
            && !ctx.thinking_block_started
            && !ctx.tool_block_started
            && ctx.thinking_buffer.is_empty())
    {
        return Vec::new();
    }

    finalize_openai_responses_stream_to_claude_done(ctx)
}

pub fn openai_responses_stream_to_claude(
    event: &[u8],
    ctx: &mut StreamContext,
) -> Result<Vec<u8>, String> {
    let (_, json_data) = parse_sse(event);
    if json_data.is_empty() {
        return Ok(Vec::new());
    }

    if json_data == "[DONE]" {
        return Ok(finalize_openai_responses_stream_to_claude_done(ctx));
    }

    let data: Value = match serde_json::from_str(&json_data) {
        Ok(data) => data,
        Err(_) => return Ok(Vec::new()),
    };

    let mut result = Vec::new();

    match data
        .get("type")
        .and_then(|t| t.as_str())
        .unwrap_or_default()
    {
        "response.created" => {
            if let Some(id) = data
                .get("response")
                .and_then(|r| r.get("id"))
                .and_then(|v| v.as_str())
            {
                ctx.message_id = id.to_string();
            }
            result.extend(build_claude_event(
                "message_start",
                &json!({
                    "message": {
                        "id": ctx.message_id,
                        "type": "message",
                        "role": "assistant",
                        "content": [],
                        "model": ctx.model_name,
                        "stop_reason": null,
                        "stop_sequence": null,
                        "usage": {"input_tokens": 0, "output_tokens": 0}
                    }
                }),
            ));
        }
        "response.output_text.delta" => {
            if let Some(delta) = data.get("delta").and_then(|d| d.as_str()) {
                let mut content = ctx.thinking_buffer.clone();
                content.push_str(delta);
                ctx.thinking_buffer.clear();
                consume_think_tagged_stream(
                    content,
                    ctx,
                    &mut result,
                    |ctx, result, text| emit_text_with_close(ctx, result, text),
                    |ctx, result, text| emit_thinking_with_close(ctx, result, text),
                );
            }
        }
        "response.output_item.added" => {
            if let Some(item) = data.get("item") {
                if item.get("type").and_then(|t| t.as_str()) == Some("function_call") {
                    if ctx.thinking_block_started {
                        close_thinking_block(ctx, &mut result);
                    }
                    if ctx.content_block_started {
                        result.extend(build_claude_event(
                            "content_block_stop",
                            &json!({"index": ctx.content_index}),
                        ));
                        ctx.content_block_started = false;
                        ctx.content_index += 1;
                    }

                    ctx.tool_block_started = true;
                    ctx.tool_index = ctx.content_index;
                    ctx.current_tool_id = item
                        .get("call_id")
                        .or_else(|| item.get("id"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    ctx.current_tool_name = item
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    ctx.tool_arguments.clear();

                    result.extend(build_claude_event(
                        "content_block_start",
                        &json!({
                            "index": ctx.tool_index,
                            "content_block": {
                                "type": "tool_use",
                                "id": ctx.current_tool_id,
                                "name": ctx.current_tool_name,
                                "input": {}
                            }
                        }),
                    ));
                }
            }
        }
        "response.function_call_arguments.delta" => {
            if !ctx.tool_block_started {
                return Ok(result);
            }
            if let Some(delta) = data.get("delta").and_then(|d| d.as_str()) {
                ctx.tool_arguments.push_str(delta);
                result.extend(build_claude_event(
                    "content_block_delta",
                    &json!({
                        "index": ctx.tool_index,
                        "delta": {"type": "input_json_delta", "partial_json": delta}
                    }),
                ));
            }
        }
        "response.output_item.done" => {
            if data
                .get("item")
                .and_then(|i| i.get("type"))
                .and_then(|t| t.as_str())
                == Some("function_call")
                && ctx.tool_block_started
            {
                result.extend(build_claude_event(
                    "content_block_stop",
                    &json!({"index": ctx.tool_index}),
                ));
                ctx.tool_block_started = false;
                ctx.content_index += 1;
            }
        }
        "response.completed" => {
            if ctx.finish_reason_sent {
                return Ok(result);
            }
            flush_think_tagged_stream(
                ctx,
                &mut result,
                |ctx, result, text| emit_text_delta(ctx, result, text),
                |ctx, result, text| emit_thinking_delta(ctx, result, text),
            );

            if ctx.thinking_block_started {
                close_thinking_block(ctx, &mut result);
            }
            if ctx.content_block_started {
                result.extend(build_claude_event(
                    "content_block_stop",
                    &json!({"index": ctx.content_index}),
                ));
                ctx.content_block_started = false;
            }

            let stop_reason = if ctx.tool_index > 0 || !ctx.current_tool_id.is_empty() {
                "tool_use"
            } else {
                "end_turn"
            };
            result.extend(build_claude_event(
                "message_delta",
                &json!({
                    "delta": {"stop_reason": stop_reason, "stop_sequence": null},
                    "usage": {"output_tokens": 0}
                }),
            ));
            result.extend(build_claude_event("message_stop", &json!({})));
            ctx.finish_reason_sent = true;
        }
        _ => {}
    }

    Ok(result)
}
