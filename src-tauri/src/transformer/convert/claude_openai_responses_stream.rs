//! Claude to OpenAI Responses streaming conversion

use super::claude_openai_responses::{
    build_anthropic_usage_from_responses, map_responses_stop_reason,
};
use super::common::{build_claude_event, parse_sse};
use crate::transformer::types::StreamContext;
use serde_json::{json, Value};

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

fn parse_sse_block(event: &[u8]) -> Option<(String, String)> {
    let text = String::from_utf8_lossy(event);
    let mut event_type = String::new();
    let mut data_parts = Vec::new();

    for raw_line in text.lines() {
        let line = raw_line.trim_end_matches('\r');
        if let Some(evt) = line.strip_prefix("event: ") {
            event_type = evt.trim().to_string();
        } else if let Some(data) = line.strip_prefix("data: ") {
            data_parts.push(data.to_string());
        } else if let Some(data) = line.strip_prefix("data:") {
            data_parts.push(data.trim_start().to_string());
        }
    }

    if data_parts.is_empty() {
        return None;
    }

    Some((event_type, data_parts.join("\n")))
}

#[inline]
fn response_object_from_event(data: &Value) -> &Value {
    data.get("response").unwrap_or(data)
}

#[inline]
fn content_part_key(data: &Value) -> Option<String> {
    if let (Some(item_id), Some(content_index)) = (
        data.get("item_id").and_then(|v| v.as_str()),
        data.get("content_index").and_then(|v| v.as_i64()),
    ) {
        return Some(format!("part:{item_id}:{content_index}"));
    }
    if let (Some(output_index), Some(content_index)) = (
        data.get("output_index").and_then(|v| v.as_i64()),
        data.get("content_index").and_then(|v| v.as_i64()),
    ) {
        return Some(format!("part:out:{output_index}:{content_index}"));
    }
    None
}

#[inline]
fn tool_item_key_from_added(data: &Value, item: &Value) -> Option<String> {
    if let Some(item_id) = item.get("id").and_then(|v| v.as_str()) {
        return Some(format!("tool:{item_id}"));
    }
    if let Some(item_id) = data.get("item_id").and_then(|v| v.as_str()) {
        return Some(format!("tool:{item_id}"));
    }
    if let Some(output_index) = data.get("output_index").and_then(|v| v.as_i64()) {
        return Some(format!("tool:out:{output_index}"));
    }
    None
}

#[inline]
fn tool_item_key_from_event(data: &Value) -> Option<String> {
    if let Some(item_id) = data.get("item_id").and_then(|v| v.as_str()) {
        return Some(format!("tool:{item_id}"));
    }
    if let Some(output_index) = data.get("output_index").and_then(|v| v.as_i64()) {
        return Some(format!("tool:out:{output_index}"));
    }
    None
}

#[inline]
fn next_index(ctx: &mut StreamContext) -> i32 {
    let idx = ctx.responses_next_content_index;
    ctx.responses_next_content_index += 1;
    idx
}

#[inline]
fn resolve_content_index(ctx: &mut StreamContext, data: &Value) -> i32 {
    if let Some(k) = content_part_key(data) {
        if let Some(existing) = ctx.responses_index_by_key.get(&k).copied() {
            existing
        } else {
            let assigned = next_index(ctx);
            ctx.responses_index_by_key.insert(k, assigned);
            assigned
        }
    } else if let Some(existing) = ctx.responses_fallback_open_index {
        existing
    } else {
        let assigned = next_index(ctx);
        ctx.responses_fallback_open_index = Some(assigned);
        assigned
    }
}

fn emit_message_start_if_needed(
    ctx: &mut StreamContext,
    result: &mut Vec<u8>,
    usage: Option<&Value>,
) {
    if ctx.message_start_sent {
        return;
    }
    if ctx.message_id.is_empty() {
        ctx.message_id = format!("msg_{}", uuid::Uuid::new_v4().simple());
    }

    let usage_json = build_anthropic_usage_from_responses(usage);
    if let Some(input_tokens) = usage_json.get("input_tokens").and_then(|v| v.as_i64()) {
        ctx.input_tokens = input_tokens as i32;
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
                "usage": usage_json
            }
        }),
    ));
    ctx.message_start_sent = true;
}

fn close_open_index(ctx: &mut StreamContext, result: &mut Vec<u8>, index: i32) {
    if !ctx.responses_open_indices.remove(&index) {
        return;
    }
    result.extend(build_claude_event(
        "content_block_stop",
        &json!({"index": index}),
    ));
    if ctx.responses_fallback_open_index == Some(index) {
        ctx.responses_fallback_open_index = None;
    }
}

fn close_all_open_indices(ctx: &mut StreamContext, result: &mut Vec<u8>) {
    if ctx.responses_open_indices.is_empty() {
        return;
    }
    let mut remaining: Vec<i32> = ctx.responses_open_indices.iter().copied().collect();
    remaining.sort_unstable();
    for index in remaining {
        close_open_index(ctx, result, index);
    }
}

fn finalize_openai_responses_stream_to_claude_done(ctx: &mut StreamContext) -> Vec<u8> {
    let mut result = Vec::new();
    close_all_open_indices(ctx, &mut result);
    ctx.responses_tool_index_by_item_id.clear();
    ctx.responses_last_tool_index = None;
    ctx.responses_fallback_open_index = None;

    if !ctx.finish_reason_sent && ctx.message_start_sent {
        result.extend(build_claude_event("message_stop", &json!({})));
        ctx.finish_reason_sent = true;
    }

    result
}

pub fn finalize_openai_responses_stream_to_claude(ctx: &mut StreamContext) -> Vec<u8> {
    if ctx.finish_reason_sent
        || (!ctx.message_start_sent
            && ctx.responses_open_indices.is_empty()
            && !ctx.tool_block_started
            && !ctx.content_block_started
            && !ctx.thinking_block_started)
    {
        return Vec::new();
    }
    finalize_openai_responses_stream_to_claude_done(ctx)
}

pub fn openai_responses_stream_to_claude(
    event: &[u8],
    ctx: &mut StreamContext,
) -> Result<Vec<u8>, String> {
    let Some((event_type, json_data)) = parse_sse_block(event) else {
        return Ok(Vec::new());
    };

    if json_data == "[DONE]" {
        return Ok(finalize_openai_responses_stream_to_claude_done(ctx));
    }

    let data: Value = match serde_json::from_str(&json_data) {
        Ok(data) => data,
        Err(_) => return Ok(Vec::new()),
    };

    let event_name = if event_type.is_empty() {
        data.get("type")
            .and_then(|t| t.as_str())
            .unwrap_or_default()
            .to_string()
    } else {
        event_type
    };

    let mut result = Vec::new();

    match event_name.as_str() {
        "response.created" => {
            let response_obj = response_object_from_event(&data);
            if let Some(id) = response_obj.get("id").and_then(|v| v.as_str()) {
                ctx.message_id = id.to_string();
            }
            if let Some(model) = response_obj.get("model").and_then(|v| v.as_str()) {
                ctx.model_name = model.to_string();
            }
            emit_message_start_if_needed(ctx, &mut result, response_obj.get("usage"));
        }

        "response.content_part.added" => {
            if let Some(part) = data.get("part") {
                let part_type = part.get("type").and_then(|t| t.as_str());
                if matches!(part_type, Some("output_text") | Some("refusal")) {
                    emit_message_start_if_needed(ctx, &mut result, None);
                    let index = resolve_content_index(ctx, &data);
                    if !ctx.responses_open_indices.contains(&index) {
                        result.extend(build_claude_event(
                            "content_block_start",
                            &json!({
                                "index": index,
                                "content_block": {
                                    "type": "text",
                                    "text": ""
                                }
                            }),
                        ));
                        ctx.responses_open_indices.insert(index);
                    }
                }
            }
        }

        "response.output_text.delta" | "response.refusal.delta" => {
            if let Some(delta) = data.get("delta").and_then(|d| d.as_str()) {
                emit_message_start_if_needed(ctx, &mut result, None);
                let index = resolve_content_index(ctx, &data);

                if !ctx.responses_open_indices.contains(&index) {
                    result.extend(build_claude_event(
                        "content_block_start",
                        &json!({
                            "index": index,
                            "content_block": {
                                "type": "text",
                                "text": ""
                            }
                        }),
                    ));
                    ctx.responses_open_indices.insert(index);
                }

                result.extend(build_claude_event(
                    "content_block_delta",
                    &json!({
                        "index": index,
                        "delta": {
                            "type": "text_delta",
                            "text": delta
                        }
                    }),
                ));
            }
        }

        "response.content_part.done" | "response.refusal.done" | "response.reasoning.done" => {
            let index = if let Some(k) = content_part_key(&data) {
                ctx.responses_index_by_key.get(&k).copied()
            } else {
                ctx.responses_fallback_open_index
            };
            if let Some(index) = index {
                close_open_index(ctx, &mut result, index);
            }
        }

        "response.output_item.added" => {
            if let Some(item) = data.get("item") {
                if item.get("type").and_then(|t| t.as_str()) == Some("function_call") {
                    ctx.responses_has_tool_use = true;
                    emit_message_start_if_needed(ctx, &mut result, None);

                    let index = if let Some(k) = tool_item_key_from_added(&data, item) {
                        if let Some(existing) = ctx.responses_index_by_key.get(&k).copied() {
                            existing
                        } else {
                            let assigned = next_index(ctx);
                            ctx.responses_index_by_key.insert(k, assigned);
                            assigned
                        }
                    } else {
                        next_index(ctx)
                    };

                    if let Some(item_id) = item
                        .get("id")
                        .and_then(|v| v.as_str())
                        .or_else(|| data.get("item_id").and_then(|v| v.as_str()))
                    {
                        ctx.responses_tool_index_by_item_id
                            .insert(item_id.to_string(), index);
                    }
                    ctx.responses_last_tool_index = Some(index);

                    if !ctx.responses_open_indices.contains(&index) {
                        let call_id = item
                            .get("call_id")
                            .or_else(|| item.get("id"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("");

                        result.extend(build_claude_event(
                            "content_block_start",
                            &json!({
                                "index": index,
                                "content_block": {
                                    "type": "tool_use",
                                    "id": call_id,
                                    "name": name
                                }
                            }),
                        ));
                        ctx.responses_open_indices.insert(index);
                    }
                }
            }
        }

        "response.function_call_arguments.delta" => {
            if let Some(delta) = data.get("delta").and_then(|d| d.as_str()) {
                emit_message_start_if_needed(ctx, &mut result, None);
                let item_id = data.get("item_id").and_then(|v| v.as_str());
                let index = item_id
                    .and_then(|id| ctx.responses_tool_index_by_item_id.get(id).copied())
                    .or_else(|| {
                        tool_item_key_from_event(&data)
                            .and_then(|k| ctx.responses_index_by_key.get(&k).copied())
                    })
                    .or(ctx.responses_last_tool_index)
                    .unwrap_or_else(|| next_index(ctx));

                ctx.responses_last_tool_index = Some(index);

                if !ctx.responses_open_indices.contains(&index) {
                    result.extend(build_claude_event(
                        "content_block_start",
                        &json!({
                            "index": index,
                            "content_block": {
                                "type": "tool_use",
                                "id": data
                                    .get("call_id")
                                    .and_then(|v| v.as_str())
                                    .or(item_id)
                                    .unwrap_or(""),
                                "name": data
                                    .get("name")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                            }
                        }),
                    ));
                    ctx.responses_open_indices.insert(index);
                }

                result.extend(build_claude_event(
                    "content_block_delta",
                    &json!({
                        "index": index,
                        "delta": {
                            "type": "input_json_delta",
                            "partial_json": delta
                        }
                    }),
                ));
            }
        }

        "response.function_call_arguments.done" => {
            let item_id = data.get("item_id").and_then(|v| v.as_str());
            let index = item_id
                .and_then(|id| ctx.responses_tool_index_by_item_id.get(id).copied())
                .or_else(|| {
                    tool_item_key_from_event(&data)
                        .and_then(|k| ctx.responses_index_by_key.get(&k).copied())
                })
                .or(ctx.responses_last_tool_index);
            if let Some(index) = index {
                close_open_index(ctx, &mut result, index);
                if let Some(item_id) = item_id {
                    ctx.responses_tool_index_by_item_id.remove(item_id);
                }
            }
        }

        "response.reasoning.delta" => {
            if let Some(delta) = data
                .get("delta")
                .or_else(|| data.get("text"))
                .and_then(|d| d.as_str())
            {
                emit_message_start_if_needed(ctx, &mut result, None);
                let index = resolve_content_index(ctx, &data);
                if !ctx.responses_open_indices.contains(&index) {
                    result.extend(build_claude_event(
                        "content_block_start",
                        &json!({
                            "index": index,
                            "content_block": {
                                "type": "thinking",
                                "thinking": ""
                            }
                        }),
                    ));
                    ctx.responses_open_indices.insert(index);
                }
                result.extend(build_claude_event(
                    "content_block_delta",
                    &json!({
                        "index": index,
                        "delta": {
                            "type": "thinking_delta",
                            "thinking": delta
                        }
                    }),
                ));
            }
        }

        "response.completed" => {
            if ctx.finish_reason_sent {
                return Ok(result);
            }

            let response_obj = response_object_from_event(&data);
            emit_message_start_if_needed(ctx, &mut result, response_obj.get("usage"));
            close_all_open_indices(ctx, &mut result);
            ctx.responses_fallback_open_index = None;

            let stop_reason = map_responses_stop_reason(
                response_obj.get("status").and_then(|s| s.as_str()),
                ctx.responses_has_tool_use,
                response_obj
                    .pointer("/incomplete_details/reason")
                    .and_then(|reason| reason.as_str()),
            );

            let usage_json = build_anthropic_usage_from_responses(response_obj.get("usage"));
            if let Some(output_tokens) = usage_json.get("output_tokens").and_then(|v| v.as_i64()) {
                ctx.output_tokens = output_tokens as i32;
            }

            result.extend(build_claude_event(
                "message_delta",
                &json!({
                    "delta": {
                        "stop_reason": stop_reason,
                        "stop_sequence": null
                    },
                    "usage": usage_json
                }),
            ));
            result.extend(build_claude_event("message_stop", &json!({})));
            ctx.finish_reason_sent = true;
        }

        // Explicitly accepted lifecycle events with no Anthropic counterpart.
        "response.output_text.done" | "response.output_item.done" | "response.in_progress" => {}

        _ => {}
    }

    Ok(result)
}
