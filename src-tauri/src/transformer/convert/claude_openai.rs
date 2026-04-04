//! Claude <-> OpenAI Chat conversion
//! Reference: ccNexus/internal/transformer/convert/claude_openai.go

use super::common::{
    build_claude_event, claude_thinking_to_openai_reasoning_effort, extract_system_text,
    extract_tool_result_content, parse_sse,
};
use crate::transformer::types::*;
use serde_json::{json, Value};

const THINK_TAG_OPEN: &str = "<think>";
const THINK_TAG_CLOSE: &str = "</think>";

pub fn claude_req_to_openai(claude_req: &[u8], model: &str) -> Result<Vec<u8>, String> {
    let req: ClaudeRequest =
        serde_json::from_slice(claude_req).map_err(|e| format!("parse claude request: {}", e))?;

    let mut messages = Vec::new();

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

    for msg in &req.messages {
        match &msg.content {
            Value::String(text) => {
                messages.push(OpenAIMessage {
                    role: msg.role.clone(),
                    content: Some(Value::String(text.clone())),
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
                        }
                        "tool_use" => {
                            let id = block.get("id").and_then(|i| i.as_str()).unwrap_or("");
                            let name = block.get("name").and_then(|n| n.as_str()).unwrap_or("");
                            if !id.is_empty() && !name.is_empty() {
                                let args =
                                    serde_json::to_string(block.get("input").unwrap_or(&json!({})))
                                        .unwrap_or_else(|_| "{}".to_string());
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
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            if !call_id.is_empty() {
                                tool_results.push(OpenAIMessage {
                                    role: "tool".to_string(),
                                    content: Some(Value::String(extract_tool_result_content(
                                        block.get("content").unwrap_or(&Value::Null),
                                    ))),
                                    tool_calls: None,
                                    tool_call_id: Some(call_id.to_string()),
                                });
                            }
                        }
                        _ => {}
                    }
                }

                if !text_parts.is_empty() || !tool_calls.is_empty() {
                    messages.push(OpenAIMessage {
                        role: msg.role.clone(),
                        content: if text_parts.is_empty() {
                            None
                        } else {
                            Some(Value::String(text_parts.join("")))
                        },
                        tool_calls: if tool_calls.is_empty() {
                            None
                        } else {
                            Some(tool_calls)
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
        stream_options: req
            .stream
            .filter(|stream| *stream)
            .map(|_| OpenAIStreamOptions {
                include_usage: true,
            }),
        tools: None,
        tool_choice: None,
        reasoning_effort: claude_thinking_to_openai_reasoning_effort(req.thinking.as_ref(), model)
            .map(str::to_string),
    };

    if let Some(tools) = &req.tools {
        if !tools.is_empty() {
            openai_req.tools = Some(
                tools
                    .iter()
                    .map(|tool| OpenAITool {
                        tool_type: "function".to_string(),
                        function: OpenAIToolFunction {
                            name: tool.name.clone(),
                            description: tool.description.clone(),
                            parameters: tool.input_schema.clone(),
                        },
                    })
                    .collect(),
            );
        }
    }

    serde_json::to_vec(&openai_req).map_err(|e| format!("serialize: {}", e))
}

pub fn openai_req_to_claude(openai_req: &[u8], model: &str) -> Result<Vec<u8>, String> {
    let req: Value =
        serde_json::from_slice(openai_req).map_err(|e| format!("parse openai request: {}", e))?;

    let mut claude_req = json!({
        "model": model,
        "max_tokens": 8192,
        "stream": req.get("stream").cloned().unwrap_or(json!(false))
    });

    if let Some(max_tokens) = req
        .get("max_tokens")
        .or_else(|| req.get("max_completion_tokens"))
    {
        claude_req["max_tokens"] = max_tokens.clone();
    }
    if let Some(temperature) = req.get("temperature") {
        claude_req["temperature"] = temperature.clone();
    }
    if let Some(thinking) = req.get("thinking").filter(|v| !v.is_null()) {
        claude_req["thinking"] = thinking.clone();
    }

    let mut system_prompt = Vec::new();
    let mut messages = Vec::new();

    if let Some(openai_messages) = req.get("messages").and_then(|v| v.as_array()) {
        for msg in openai_messages {
            let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("user");

            if role == "system" {
                let text = extract_openai_text_content(msg.get("content"));
                if !text.trim().is_empty() {
                    system_prompt.push(text);
                }
                continue;
            }

            let mut claude_msg = json!({
                "role": role,
                "content": match msg.get("content") {
                    Some(Value::String(text)) => Value::String(text.clone()),
                    Some(Value::Array(_)) => Value::Array(convert_openai_content_to_claude_blocks(
                        msg.get("content").unwrap_or(&Value::Null),
                    )),
                    Some(other) => Value::String(json_value_to_string(Some(other))),
                    None => Value::String(String::new()),
                }
            });

            if let Some(tool_calls) = msg.get("tool_calls").and_then(|v| v.as_array()) {
                let mut blocks = Vec::new();
                match claude_msg.get("content") {
                    Some(Value::String(text)) if !text.is_empty() => {
                        blocks.push(json!({"type": "text", "text": text}));
                    }
                    Some(Value::Array(existing)) => {
                        blocks.extend(existing.iter().cloned());
                    }
                    _ => {}
                }

                for tool_call in tool_calls {
                    let id = tool_call.get("id").and_then(|v| v.as_str()).unwrap_or("");
                    let name = tool_call
                        .get("function")
                        .and_then(|f| f.get("name"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let arguments = tool_call
                        .get("function")
                        .and_then(|f| f.get("arguments"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("{}");

                    let input =
                        serde_json::from_str::<Value>(arguments).unwrap_or_else(|_| json!({}));
                    blocks.push(json!({
                        "type": "tool_use",
                        "id": id,
                        "name": name,
                        "input": input
                    }));
                }

                claude_msg["content"] = Value::Array(blocks);
            }

            if role == "tool" {
                claude_msg["role"] = json!("user");
                claude_msg["content"] = json!([{
                    "type": "tool_result",
                    "tool_use_id": msg
                        .get("tool_call_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or(""),
                    "content": extract_openai_tool_result_content(msg.get("content"))
                }]);
            }

            messages.push(claude_msg);
        }
    }

    if !system_prompt.is_empty() {
        claude_req["system"] = json!(system_prompt.join("\n").trim().to_string());
    }
    claude_req["messages"] = Value::Array(messages);

    if let Some(tools) = req.get("tools").and_then(|v| v.as_array()) {
        let claude_tools: Vec<Value> = tools
            .iter()
            .filter_map(|tool| {
                if tool.get("type").and_then(|v| v.as_str()) != Some("function") {
                    return None;
                }

                let function = tool.get("function")?;
                Some(json!({
                    "name": function.get("name")?,
                    "description": function.get("description"),
                    "input_schema": function.get("parameters")?
                }))
            })
            .collect();

        if !claude_tools.is_empty() {
            claude_req["tools"] = json!(claude_tools);
        }
    }

    serde_json::to_vec(&claude_req).map_err(|e| format!("serialize: {}", e))
}

pub fn claude_resp_to_openai(claude_resp: &[u8], model: &str) -> Result<Vec<u8>, String> {
    let resp: ClaudeResponse =
        serde_json::from_slice(claude_resp).map_err(|e| format!("parse claude response: {}", e))?;

    let mut text_content = String::new();
    let mut tool_calls = Vec::new();

    for block in &resp.content {
        let block_type = block.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match block_type {
            "text" => {
                if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                    text_content.push_str(text);
                }
            }
            "thinking" => {}
            "tool_use" => {
                let id = block.get("id").and_then(|v| v.as_str()).unwrap_or("");
                let name = block.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let arguments = serde_json::to_string(block.get("input").unwrap_or(&json!({})))
                    .unwrap_or_else(|_| "{}".to_string());
                tool_calls.push(json!({
                    "id": id,
                    "type": "function",
                    "function": {
                        "name": name,
                        "arguments": arguments
                    }
                }));
            }
            _ => {}
        }
    }

    let finish_reason = match resp.stop_reason.as_str() {
        "tool_use" => "tool_calls",
        "max_tokens" => "length",
        _ => "stop",
    };

    let mut message = json!({
        "role": "assistant",
        "content": text_content
    });
    if !tool_calls.is_empty() {
        message["tool_calls"] = Value::Array(tool_calls);
    }

    let openai_resp = json!({
        "id": resp.id,
        "object": "chat.completion",
        "model": model,
        "choices": [{
            "index": 0,
            "message": message,
            "finish_reason": finish_reason
        }],
        "usage": {
            "prompt_tokens": resp.usage.input_tokens,
            "completion_tokens": resp.usage.output_tokens,
            "total_tokens": resp.usage.input_tokens + resp.usage.output_tokens
        }
    });

    serde_json::to_vec(&openai_resp).map_err(|e| format!("serialize: {}", e))
}

pub fn openai_resp_to_claude(openai_resp: &[u8]) -> Result<Vec<u8>, String> {
    let resp: OpenAIResponse =
        serde_json::from_slice(openai_resp).map_err(|e| format!("parse openai response: {}", e))?;

    let choice = resp
        .choices
        .first()
        .ok_or_else(|| "no choices in response".to_string())?;

    let mut content = Vec::new();

    if let Some(message_content) = &choice.message.content {
        match message_content {
            Value::String(text) => content.extend(split_think_tagged_text(text)),
            Value::Array(parts) => {
                for part in parts {
                    let part_type = part.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    if matches!(part_type, "text" | "output_text" | "input_text") {
                        if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                            content.extend(split_think_tagged_text(text));
                        }
                    }
                }
            }
            _ => {}
        }
    }

    if let Some(tool_calls) = &choice.message.tool_calls {
        for tool_call in tool_calls {
            let input = serde_json::from_str::<Value>(&tool_call.function.arguments)
                .unwrap_or_else(|_| json!({}));
            content.push(json!({
                "type": "tool_use",
                "id": tool_call.id,
                "name": tool_call.function.name,
                "input": input
            }));
        }
    }

    let stop_reason = match choice.finish_reason.as_str() {
        "tool_calls" => "tool_use",
        "length" => "max_tokens",
        _ => "end_turn",
    };

    let claude_resp = json!({
        "id": resp.id,
        "type": "message",
        "role": "assistant",
        "content": content,
        "model": resp.model,
        "stop_reason": stop_reason,
        "usage": {
            "input_tokens": resp.usage.prompt_tokens,
            "output_tokens": resp.usage.completion_tokens
        }
    });

    serde_json::to_vec(&claude_resp).map_err(|e| format!("serialize: {}", e))
}

pub fn openai_stream_to_claude(event: &[u8], ctx: &mut StreamContext) -> Result<Vec<u8>, String> {
    let (_, json_data) = parse_sse(event);
    if json_data.is_empty() {
        return Ok(Vec::new());
    }
    if json_data == "[DONE]" {
        return Ok(finalize_openai_stream_to_claude(ctx));
    }

    let chunk: OpenAIStreamChunk = serde_json::from_str(&json_data)
        .map_err(|e| format!("parse openai stream chunk: {}", e))?;

    let mut result = Vec::new();
    let message_id = if chunk.id.is_empty() {
        if ctx.message_id.is_empty() {
            ctx.message_id = format!("msg_{}", uuid::Uuid::new_v4().simple());
        }
        ctx.message_id.clone()
    } else {
        chunk.id.clone()
    };

    if !ctx.message_start_sent {
        ctx.message_start_sent = true;
        ctx.message_id = message_id.clone();
        let model_name = if chunk.model.is_empty() {
            ctx.model_name.clone()
        } else {
            chunk.model.clone()
        };
        result.extend(build_claude_event(
            "message_start",
            &json!({
                "message": {
                    "id": message_id,
                    "type": "message",
                    "role": "assistant",
                    "content": [],
                    "model": model_name,
                    "stop_reason": null,
                    "stop_sequence": null,
                    "usage": {
                        "input_tokens": 0,
                        "output_tokens": 0
                    }
                }
            }),
        ));
    }

    if chunk.choices.is_empty() {
        if let Some(usage) = &chunk.usage {
            result.extend(build_usage_delta_event(
                usage.prompt_tokens,
                usage.completion_tokens,
            ));
        }
        return Ok(result);
    }

    let choice = &chunk.choices[0];

    if let Some(usage) = &chunk.usage {
        let delta = &choice.delta;
        let has_payload = delta.role.as_deref().unwrap_or_default().is_empty()
            && delta.content.as_deref().unwrap_or_default().is_empty()
            && delta
                .reasoning_content
                .as_deref()
                .unwrap_or_default()
                .is_empty()
            && delta
                .tool_calls
                .as_ref()
                .map(|v| v.is_empty())
                .unwrap_or(true)
            && choice.finish_reason.is_none();
        if has_payload {
            result.extend(build_usage_delta_event(
                usage.prompt_tokens,
                usage.completion_tokens,
            ));
            return Ok(result);
        }
    }

    if let Some(reasoning) = choice.delta.reasoning_content.as_deref() {
        emit_thinking_delta(ctx, &mut result, reasoning);
    }

    if let Some(text) = choice.delta.content.as_deref() {
        emit_text_delta(ctx, &mut result, text);
    }

    if let Some(tool_calls) = &choice.delta.tool_calls {
        for tool_call in tool_calls {
            if !tool_call.id.is_empty() {
                close_thinking_block(ctx, &mut result);
                if ctx.content_block_started {
                    close_text_block(ctx, &mut result);
                    ctx.content_index += 1;
                }
                if ctx.tool_block_started {
                    close_tool_block(ctx, &mut result);
                    ctx.content_index += 1;
                }

                ctx.tool_block_started = true;
                ctx.tool_index = ctx.content_index;
                ctx.current_tool_id = tool_call.id.clone();
                ctx.current_tool_name = tool_call.function.name.clone();
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

            if !tool_call.function.arguments.is_empty() {
                if !ctx.tool_block_started {
                    ctx.tool_block_started = true;
                    ctx.tool_index = ctx.content_index;
                    if ctx.current_tool_id.is_empty() {
                        ctx.current_tool_id = format!("tool_{}", ctx.tool_index);
                    }
                    if ctx.current_tool_name.is_empty() {
                        ctx.current_tool_name = if tool_call.function.name.is_empty() {
                            "unknown".to_string()
                        } else {
                            tool_call.function.name.clone()
                        };
                    }
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

                ctx.tool_arguments.push_str(&tool_call.function.arguments);
                result.extend(build_claude_event(
                    "content_block_delta",
                    &json!({
                        "index": ctx.tool_index,
                        "delta": {
                            "type": "input_json_delta",
                            "partial_json": tool_call.function.arguments
                        }
                    }),
                ));
            }
        }
    }

    if let Some(finish_reason) = choice.finish_reason.as_deref() {
        if !finish_reason.is_empty() && !ctx.finish_reason_sent {
            close_thinking_block(ctx, &mut result);
            close_text_block(ctx, &mut result);
            close_tool_block(ctx, &mut result);

            result.extend(build_claude_event(
                "message_delta",
                &json!({
                    "delta": {
                        "stop_reason": map_openai_finish_reason(finish_reason),
                        "stop_sequence": null
                    },
                    "usage": {
                        "output_tokens": 0
                    }
                }),
            ));
            ctx.finish_reason_sent = true;
        }
    }

    if let Some(usage) = &chunk.usage {
        result.extend(build_usage_delta_event(
            usage.prompt_tokens,
            usage.completion_tokens,
        ));
    }

    Ok(result)
}

pub fn finalize_openai_stream_to_claude(ctx: &mut StreamContext) -> Vec<u8> {
    let mut result = Vec::new();

    close_thinking_block(ctx, &mut result);
    close_text_block(ctx, &mut result);
    close_tool_block(ctx, &mut result);

    if ctx.message_start_sent && !ctx.finish_reason_sent {
        result.extend(build_claude_event(
            "message_delta",
            &json!({
                "delta": {
                    "stop_reason": "end_turn",
                    "stop_sequence": null
                },
                "usage": {
                    "output_tokens": 0
                }
            }),
        ));
        ctx.finish_reason_sent = true;
    }

    if ctx.message_start_sent {
        result.extend(build_claude_event("message_stop", &json!({})));
    }

    result
}

fn convert_openai_content_to_claude_blocks(content: &Value) -> Vec<Value> {
    let mut blocks = Vec::new();

    let Some(items) = content.as_array() else {
        return blocks;
    };

    for item in items {
        let item_type = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match item_type {
            "text" | "input_text" | "output_text" => {
                if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                    blocks.push(json!({
                        "type": "text",
                        "text": text
                    }));
                }
            }
            "image_url" => {
                if let Some(url) = item
                    .get("image_url")
                    .and_then(|v| v.get("url"))
                    .and_then(|v| v.as_str())
                {
                    if let Some(image_block) = convert_data_url_to_claude_image(url) {
                        blocks.push(image_block);
                    }
                }
            }
            _ => {}
        }
    }

    blocks
}

fn convert_data_url_to_claude_image(url: &str) -> Option<Value> {
    if !url.starts_with("data:") {
        return None;
    }

    let mut parts = url.splitn(2, ',');
    let header = parts.next()?;
    let data = parts.next()?;
    let media_type = header
        .strip_prefix("data:")
        .and_then(|v| v.split(';').next())
        .unwrap_or("");

    if media_type.is_empty() {
        return None;
    }

    Some(json!({
        "type": "image",
        "source": {
            "type": "base64",
            "media_type": media_type,
            "data": data
        }
    }))
}

fn extract_openai_text_content(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| match item.get("type").and_then(|v| v.as_str()) {
                Some("text" | "input_text" | "output_text") => item
                    .get("text")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(""),
        Some(other) => json_value_to_string(Some(other)),
        None => String::new(),
    }
}

fn extract_openai_tool_result_content(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| match item.get("type").and_then(|v| v.as_str()) {
                Some("text" | "input_text" | "output_text") => item
                    .get("text")
                    .and_then(|v| v.as_str())
                    .map(str::to_string),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Some(other) => json_value_to_string(Some(other)),
        None => String::new(),
    }
}

fn json_value_to_string(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) => text.clone(),
        Some(other) => serde_json::to_string(other).unwrap_or_default(),
        None => String::new(),
    }
}

fn split_think_tagged_text(text: &str) -> Vec<Value> {
    let mut remaining = text;
    let mut blocks = Vec::new();

    loop {
        let Some(open_idx) = remaining.find(THINK_TAG_OPEN) else {
            if !remaining.is_empty() {
                blocks.push(json!({
                    "type": "text",
                    "text": remaining
                }));
            }
            return blocks;
        };

        if open_idx > 0 {
            blocks.push(json!({
                "type": "text",
                "text": &remaining[..open_idx]
            }));
        }

        remaining = &remaining[open_idx + THINK_TAG_OPEN.len()..];
        let Some(close_idx) = remaining.find(THINK_TAG_CLOSE) else {
            if !remaining.is_empty() {
                blocks.push(json!({
                    "type": "text",
                    "text": remaining
                }));
            }
            return blocks;
        };

        if close_idx > 0 {
            blocks.push(json!({
                "type": "thinking",
                "thinking": &remaining[..close_idx]
            }));
        }

        remaining = &remaining[close_idx + THINK_TAG_CLOSE.len()..];
    }
}

fn emit_text_delta(ctx: &mut StreamContext, result: &mut Vec<u8>, text: &str) {
    if text.is_empty() {
        return;
    }

    close_thinking_block(ctx, result);
    if ctx.tool_block_started {
        close_tool_block(ctx, result);
        ctx.content_index += 1;
    }

    if !ctx.content_block_started {
        ctx.content_block_started = true;
        result.extend(build_claude_event(
            "content_block_start",
            &json!({
                "index": ctx.content_index,
                "content_block": {
                    "type": "text",
                    "text": ""
                }
            }),
        ));
    }

    result.extend(build_claude_event(
        "content_block_delta",
        &json!({
            "index": ctx.content_index,
            "delta": {
                "type": "text_delta",
                "text": text
            }
        }),
    ));
}

fn emit_thinking_delta(ctx: &mut StreamContext, result: &mut Vec<u8>, text: &str) {
    if text.is_empty() {
        return;
    }

    if ctx.content_block_started {
        close_text_block(ctx, result);
        ctx.content_index += 1;
    }
    if ctx.tool_block_started {
        close_tool_block(ctx, result);
        ctx.content_index += 1;
    }

    if !ctx.thinking_block_started {
        ctx.thinking_block_started = true;
        ctx.thinking_index = ctx.content_index;
        result.extend(build_claude_event(
            "content_block_start",
            &json!({
                "index": ctx.thinking_index,
                "content_block": {
                    "type": "thinking",
                    "thinking": ""
                }
            }),
        ));
    }

    result.extend(build_claude_event(
        "content_block_delta",
        &json!({
            "index": ctx.thinking_index,
            "delta": {
                "type": "thinking_delta",
                "thinking": text
            }
        }),
    ));
}

fn close_text_block(ctx: &mut StreamContext, result: &mut Vec<u8>) {
    if !ctx.content_block_started {
        return;
    }

    result.extend(build_claude_event(
        "content_block_stop",
        &json!({
            "index": ctx.content_index
        }),
    ));
    ctx.content_block_started = false;
}

fn close_thinking_block(ctx: &mut StreamContext, result: &mut Vec<u8>) {
    if !ctx.thinking_block_started {
        return;
    }

    result.extend(build_claude_event(
        "content_block_stop",
        &json!({
            "index": ctx.thinking_index
        }),
    ));
    ctx.thinking_block_started = false;
    ctx.content_index = ctx.thinking_index + 1;
}

fn close_tool_block(ctx: &mut StreamContext, result: &mut Vec<u8>) {
    if !ctx.tool_block_started {
        return;
    }

    result.extend(build_claude_event(
        "content_block_stop",
        &json!({
            "index": ctx.tool_index
        }),
    ));
    ctx.tool_block_started = false;
}

fn build_usage_delta_event(input_tokens: i32, output_tokens: i32) -> Vec<u8> {
    build_claude_event(
        "message_delta",
        &json!({
            "delta": {},
            "usage": {
                "input_tokens": input_tokens,
                "output_tokens": output_tokens
            }
        }),
    )
}

fn map_openai_finish_reason(reason: &str) -> &'static str {
    match reason {
        "tool_calls" => "tool_use",
        "length" => "max_tokens",
        _ => "end_turn",
    }
}
