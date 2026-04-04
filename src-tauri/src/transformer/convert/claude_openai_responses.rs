//! Claude Messages to OpenAI Responses conversion

use super::common::claude_thinking_to_openai_reasoning;
use super::common::*;
use crate::transformer::types::*;
use serde_json::{json, Value};
use std::collections::HashSet;

const THINK_TAG_OPEN: &str = "<think>";
const THINK_TAG_CLOSE: &str = "</think>";

#[derive(Debug, Clone, Default)]
pub struct ResponsesToClaudeOptions {
    pub text_tool_call_fallback_enabled: bool,
    pub allowed_tool_names: HashSet<String>,
}

pub(crate) fn map_responses_stop_reason(
    status: Option<&str>,
    has_tool_use: bool,
    incomplete_reason: Option<&str>,
) -> &'static str {
    match status.unwrap_or("completed") {
        "completed" => {
            if has_tool_use {
                "tool_use"
            } else {
                "end_turn"
            }
        }
        "incomplete" => {
            if matches!(
                incomplete_reason,
                Some("max_output_tokens") | Some("max_tokens")
            ) || incomplete_reason.is_none()
            {
                "max_tokens"
            } else {
                "end_turn"
            }
        }
        _ => "end_turn",
    }
}

pub(crate) fn build_anthropic_usage_from_responses(usage: Option<&Value>) -> Value {
    let Some(u) = usage.filter(|v| !v.is_null()) else {
        return json!({
            "input_tokens": 0,
            "output_tokens": 0
        });
    };

    let input_tokens = u.get("input_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
    let output_tokens = u.get("output_tokens").and_then(|v| v.as_i64()).unwrap_or(0);

    let mut usage_json = json!({
        "input_tokens": input_tokens,
        "output_tokens": output_tokens
    });

    if let Some(cached) = u
        .pointer("/input_tokens_details/cached_tokens")
        .and_then(|v| v.as_i64())
    {
        usage_json["cache_read_input_tokens"] = json!(cached);
    } else if let Some(cached) = u
        .pointer("/prompt_tokens_details/cached_tokens")
        .and_then(|v| v.as_i64())
    {
        usage_json["cache_read_input_tokens"] = json!(cached);
    }

    if let Some(v) = u.get("cache_read_input_tokens") {
        usage_json["cache_read_input_tokens"] = v.clone();
    }
    if let Some(v) = u.get("cache_creation_input_tokens") {
        usage_json["cache_creation_input_tokens"] = v.clone();
    }

    usage_json
}

pub fn claude_req_to_openai_responses(claude_req: &[u8], model: &str) -> Result<Vec<u8>, String> {
    let req: ClaudeRequest =
        serde_json::from_slice(claude_req).map_err(|e| format!("parse: {}", e))?;

    let mut openai_req = json!({
        "model": model,
        "stream": req.stream.unwrap_or(true)
    });

    if let Some(system) = &req.system {
        let instructions = extract_system_text(system);
        if !instructions.is_empty() {
            openai_req["instructions"] = json!(instructions);
        }
    }
    if let Some(max_tokens) = req.max_tokens {
        if max_tokens > 0 {
            openai_req["max_output_tokens"] = json!(max_tokens);
        }
    }
    if let Some(temperature) = req.temperature {
        openai_req["temperature"] = json!(temperature);
    }
    if let Some(reasoning) = claude_thinking_to_openai_reasoning(req.thinking.as_ref(), model) {
        openai_req["reasoning"] = reasoning;
    }

    let mut input = Vec::new();
    for msg in &req.messages {
        match &msg.content {
            Value::String(s) => {
                let text_type = if msg.role == "assistant" {
                    "output_text"
                } else {
                    "input_text"
                };
                input.push(json!({
                    "type": "message",
                    "role": msg.role,
                    "content": [{
                        "type": text_type,
                        "text": s
                    }]
                }));
            }
            Value::Array(blocks) => {
                input.extend(convert_claude_message_to_openai_responses_items(
                    blocks, &msg.role,
                ));
            }
            Value::Null => input.push(json!({"role": msg.role})),
            _ => {}
        }
    }
    openai_req["input"] = json!(input);

    if let Some(tools) = &req.tools {
        let openai_tools: Vec<Value> = tools
            .iter()
            .map(|t| {
                json!({
                    "type": "function",
                    "name": t.name,
                    "description": t.description,
                    "parameters": t.input_schema
                })
            })
            .collect();
        if !openai_tools.is_empty() {
            openai_req["tools"] = json!(openai_tools);

            if let Some(mapped) =
                map_claude_tool_choice_to_openai_responses(req.tool_choice.as_ref())
            {
                openai_req["tool_choice"] = mapped;
            } else if has_claude_tool_result(&req.messages) {
                openai_req["tool_choice"] = json!("auto");
            } else {
                openai_req["tool_choice"] = json!("required");
            }
        }
    }

    serde_json::to_vec(&openai_req).map_err(|e| format!("serialize: {}", e))
}

pub fn openai_responses_req_to_claude(openai_req: &[u8], model: &str) -> Result<Vec<u8>, String> {
    let req: Value = serde_json::from_slice(openai_req).map_err(|e| format!("parse: {}", e))?;

    let mut claude_req = json!({
        "model": model,
        "max_tokens": 8192,
        "stream": req.get("stream").and_then(|v| v.as_bool()).unwrap_or(false)
    });

    let mut system_chunks = Vec::new();
    if let Some(instructions) = req.get("instructions").and_then(|i| i.as_str()) {
        if !instructions.is_empty() {
            system_chunks.push(instructions.to_string());
        }
    }
    if let Some(max_output_tokens) = req.get("max_output_tokens").and_then(|v| v.as_i64()) {
        if max_output_tokens > 0 {
            claude_req["max_tokens"] = json!(max_output_tokens);
        }
    }
    if let Some(temperature) = req.get("temperature") {
        claude_req["temperature"] = temperature.clone();
    }
    if let Some(thinking) = req.get("thinking").filter(|v| !v.is_null()) {
        claude_req["thinking"] = thinking.clone();
    }

    let mut messages = Vec::new();
    let mut pending_tool_uses = Vec::new();
    let mut pending_tool_results = Vec::new();

    match req.get("input") {
        Some(Value::String(text)) => {
            messages.push(json!({"role": "user", "content": text}));
        }
        Some(Value::Array(input)) => {
            for item in input {
                let item_type = item.get("type").and_then(|t| t.as_str());
                let is_message_item = matches!(item_type, Some("message"))
                    || (item_type.is_none() && item.get("role").is_some());

                if is_message_item {
                    if !pending_tool_uses.is_empty() {
                        messages.push(json!({"role": "assistant", "content": pending_tool_uses}));
                        pending_tool_uses = Vec::new();
                    }
                    if !pending_tool_results.is_empty() {
                        messages.push(json!({"role": "user", "content": pending_tool_results}));
                        pending_tool_results = Vec::new();
                    }

                    let role = item.get("role").and_then(|r| r.as_str()).unwrap_or("user");
                    if matches!(role, "developer" | "system") {
                        let system_text =
                            extract_openai_responses_message_text(item.get("content"));
                        if !system_text.is_empty() {
                            system_chunks.push(system_text);
                        }
                        continue;
                    }

                    let content = convert_openai_responses_content_to_claude(item.get("content"));
                    let normalized_role = if role == "assistant" {
                        "assistant"
                    } else {
                        "user"
                    };
                    messages.push(json!({"role": normalized_role, "content": content}));
                    continue;
                }

                match item_type {
                    Some("function_call") => {
                        if !pending_tool_results.is_empty() {
                            messages.push(json!({"role": "user", "content": pending_tool_results}));
                            pending_tool_results = Vec::new();
                        }

                        let call_id = item
                            .get("call_id")
                            .or_else(|| item.get("id"))
                            .and_then(|c| c.as_str())
                            .unwrap_or("");
                        let name = item.get("name").and_then(|n| n.as_str()).unwrap_or("");
                        let args_str = item
                            .get("arguments")
                            .and_then(|a| a.as_str())
                            .unwrap_or("{}");
                        let input: Value = serde_json::from_str(args_str).unwrap_or(json!({}));

                        pending_tool_uses.push(json!({
                            "type": "tool_use",
                            "id": call_id,
                            "name": name,
                            "input": input
                        }));
                    }
                    Some("function_call_output") => {
                        if !pending_tool_uses.is_empty() {
                            messages
                                .push(json!({"role": "assistant", "content": pending_tool_uses}));
                            pending_tool_uses = Vec::new();
                        }

                        let call_id = item.get("call_id").and_then(|c| c.as_str()).unwrap_or("");
                        let output = tool_result_to_string(item.get("output"));

                        pending_tool_results.push(json!({
                            "type": "tool_result",
                            "tool_use_id": call_id,
                            "content": output
                        }));
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }

    if !pending_tool_uses.is_empty() {
        messages.push(json!({"role": "assistant", "content": pending_tool_uses}));
    }
    if !pending_tool_results.is_empty() {
        messages.push(json!({"role": "user", "content": pending_tool_results}));
    }

    claude_req["messages"] = json!(messages);
    if !system_chunks.is_empty() {
        claude_req["system"] = json!(system_chunks.join("\n\n"));
    }

    if let Some(tools) = req.get("tools").and_then(|t| t.as_array()) {
        let claude_tools: Vec<Value> = tools
            .iter()
            .filter_map(|t| {
                let name = t.get("name")?.clone();
                let description = t.get("description").cloned().unwrap_or_else(|| json!(""));
                match t.get("type").and_then(|ty| ty.as_str()) {
                    Some("function") => Some(json!({
                        "name": name,
                        "description": description,
                        "input_schema": t
                            .get("parameters")
                            .cloned()
                            .unwrap_or_else(|| json!({"type": "object", "properties": {}}))
                    })),
                    Some("custom") => Some(json!({
                        "name": name,
                        "description": description,
                        "input_schema": {
                            "type": "object",
                            "properties": {
                                "input": {
                                    "type": "string",
                                    "description": "The input for this tool"
                                }
                            },
                            "required": ["input"]
                        }
                    })),
                    _ => None,
                }
            })
            .collect();
        if !claude_tools.is_empty() {
            claude_req["tools"] = json!(claude_tools);
        }
    }

    serde_json::to_vec(&claude_req).map_err(|e| format!("serialize: {}", e))
}

pub fn claude_resp_to_openai_responses(claude_resp: &[u8]) -> Result<Vec<u8>, String> {
    let resp: ClaudeResponse =
        serde_json::from_slice(claude_resp).map_err(|e| format!("parse: {}", e))?;

    let mut output_content = Vec::new();
    let mut function_calls = Vec::new();

    for block in &resp.content {
        match block.get("type").and_then(|t| t.as_str()) {
            Some("text") => {
                if let Some(text) = block.get("text") {
                    output_content.push(json!({"type": "output_text", "text": text}));
                }
            }
            Some("tool_use") => {
                let call_id = block.get("id").cloned().unwrap_or(json!(""));
                let name = block.get("name").cloned().unwrap_or(json!(""));
                let input = block.get("input").cloned().unwrap_or(json!({}));
                let args = serde_json::to_string(&input).unwrap_or_else(|_| "{}".to_string());
                function_calls.push(json!({
                    "type": "function_call",
                    "id": call_id,
                    "call_id": call_id,
                    "name": name,
                    "arguments": args
                }));
            }
            _ => {}
        }
    }

    let mut output = Vec::new();
    if !output_content.is_empty() {
        output.push(json!({
            "type": "message",
            "role": "assistant",
            "content": output_content
        }));
    }
    output.extend(function_calls);

    let openai_resp = json!({
        "id": resp.id,
        "object": "response",
        "status": "completed",
        "output": output,
        "usage": {
            "input_tokens": resp.usage.input_tokens,
            "output_tokens": resp.usage.output_tokens,
            "total_tokens": resp.usage.input_tokens + resp.usage.output_tokens
        }
    });

    serde_json::to_vec(&openai_resp).map_err(|e| format!("serialize: {}", e))
}

pub fn openai_responses_resp_to_claude_with_options(
    openai_resp: &[u8],
    options: &ResponsesToClaudeOptions,
) -> Result<Vec<u8>, String> {
    let resp: Value = serde_json::from_slice(openai_resp).map_err(|e| format!("parse: {}", e))?;

    let mut content = Vec::new();
    let mut has_tool_use = false;

    if let Some(output) = resp.get("output").and_then(|o| o.as_array()) {
        for item in output {
            match item.get("type").and_then(|t| t.as_str()) {
                Some("message") => {
                    if let Some(parts) = item.get("content").and_then(|c| c.as_array()) {
                        for part in parts {
                            match part.get("type").and_then(|t| t.as_str()) {
                                Some("output_text") => {
                                    if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                                        if options.text_tool_call_fallback_enabled {
                                            if let Some(parsed) = parse_text_tool_call_fallback(
                                                text,
                                                &options.allowed_tool_names,
                                            ) {
                                                let call_id = format!(
                                                    "fallback_call_{}",
                                                    uuid::Uuid::new_v4().simple()
                                                );
                                                content.push(json!({
                                                    "type": "tool_use",
                                                    "id": call_id,
                                                    "name": parsed.name,
                                                    "input": parsed.arguments,
                                                }));
                                                has_tool_use = true;
                                                continue;
                                            }
                                        }

                                        content.extend(split_think_tagged_text(text));
                                    }
                                }
                                Some("refusal") => {
                                    if let Some(refusal) =
                                        part.get("refusal").and_then(|r| r.as_str())
                                    {
                                        if !refusal.is_empty() {
                                            content.push(json!({"type": "text", "text": refusal}));
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
                Some("function_call") => {
                    if let Some(name) = item.get("name") {
                        let args_str = item
                            .get("arguments")
                            .and_then(|a| a.as_str())
                            .unwrap_or("{}");
                        let input: Value = serde_json::from_str(args_str).unwrap_or(json!({}));
                        let call_id = item.get("call_id").or_else(|| item.get("id"));
                        content.push(json!({
                            "type": "tool_use",
                            "id": call_id.cloned().unwrap_or(json!("")),
                            "name": name,
                            "input": input
                        }));
                        has_tool_use = true;
                    }
                }
                Some("reasoning") => {
                    if let Some(summary) = item.get("summary").and_then(|s| s.as_array()) {
                        let thinking_text = summary
                            .iter()
                            .filter_map(|entry| {
                                if entry.get("type").and_then(|t| t.as_str())
                                    == Some("summary_text")
                                {
                                    entry.get("text").and_then(|t| t.as_str())
                                } else {
                                    None
                                }
                            })
                            .collect::<Vec<_>>()
                            .join("");

                        if !thinking_text.is_empty() {
                            content.push(json!({
                                "type": "thinking",
                                "thinking": thinking_text
                            }));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    let stop_reason = map_responses_stop_reason(
        resp.get("status").and_then(|s| s.as_str()),
        has_tool_use,
        resp.pointer("/incomplete_details/reason")
            .and_then(|reason| reason.as_str()),
    );

    let usage_json = build_anthropic_usage_from_responses(resp.get("usage"));

    let claude_resp = json!({
        "id": resp.get("id").unwrap_or(&json!("resp-id")),
        "type": "message",
        "role": "assistant",
        "content": content,
        "stop_reason": stop_reason,
        "stop_sequence": null,
        "usage": usage_json
    });

    serde_json::to_vec(&claude_resp).map_err(|e| format!("serialize: {}", e))
}

pub fn openai_responses_resp_to_claude(openai_resp: &[u8]) -> Result<Vec<u8>, String> {
    openai_responses_resp_to_claude_with_options(openai_resp, &ResponsesToClaudeOptions::default())
}

fn map_claude_tool_choice_to_openai_responses(tool_choice: Option<&Value>) -> Option<Value> {
    let tool_choice = tool_choice?;

    match tool_choice {
        Value::Object(tc) => match tc.get("type").and_then(|v| v.as_str()) {
            Some("tool") => tc
                .get("name")
                .and_then(|v| v.as_str())
                .filter(|name| !name.is_empty())
                .map(|name| json!({"type": "function", "name": name})),
            Some("any") => Some(json!("required")),
            Some("auto") => Some(json!("auto")),
            Some("none") => Some(json!("none")),
            _ => None,
        },
        Value::String(tc) => match tc.as_str() {
            "any" => Some(json!("required")),
            "auto" => Some(json!("auto")),
            "none" => Some(json!("none")),
            other if !other.is_empty() => Some(json!(other)),
            _ => None,
        },
        _ => None,
    }
}

fn has_claude_tool_result(messages: &[ClaudeMessage]) -> bool {
    messages.iter().any(|msg| match &msg.content {
        Value::Array(blocks) => blocks
            .iter()
            .any(|block| block.get("type").and_then(|v| v.as_str()) == Some("tool_result")),
        _ => false,
    })
}

fn convert_claude_message_to_openai_responses_items(blocks: &[Value], role: &str) -> Vec<Value> {
    let mut items = Vec::new();
    let mut message_parts = Vec::new();
    let text_type = if role == "assistant" {
        "output_text"
    } else {
        "input_text"
    };

    let flush_message = |items: &mut Vec<Value>, message_parts: &mut Vec<Value>| {
        if message_parts.is_empty() {
            return;
        }
        items.push(json!({
            "type": "message",
            "role": role,
            "content": message_parts.clone()
        }));
        message_parts.clear();
    };

    for block in blocks {
        match block.get("type").and_then(|v| v.as_str()) {
            Some("text") => {
                if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                    message_parts.push(json!({
                        "type": text_type,
                        "text": text
                    }));
                }
            }
            Some("image") => {
                if let Some(source) = block.get("source") {
                    let media_type = source
                        .get("media_type")
                        .and_then(|m| m.as_str())
                        .unwrap_or("image/png");
                    let data = source.get("data").and_then(|d| d.as_str()).unwrap_or("");
                    message_parts.push(json!({
                        "type": "input_image",
                        "image_url": format!("data:{media_type};base64,{data}")
                    }));
                }
            }
            Some("thinking") => {}
            Some("tool_use") => {
                flush_message(&mut items, &mut message_parts);
                let call_id = block.get("id").and_then(|v| v.as_str()).unwrap_or("");
                let name = block.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let args = serde_json::to_string(block.get("input").unwrap_or(&json!({})))
                    .unwrap_or_else(|_| "{}".to_string());
                items.push(json!({
                    "type": "function_call",
                    "call_id": call_id,
                    "name": name,
                    "arguments": args
                }));
            }
            Some("tool_result") => {
                flush_message(&mut items, &mut message_parts);
                let call_id = block
                    .get("tool_use_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                items.push(json!({
                    "type": "function_call_output",
                    "call_id": call_id,
                    "output": tool_result_to_string(block.get("content"))
                }));
            }
            _ => {}
        }
    }

    flush_message(&mut items, &mut message_parts);
    items
}

fn tool_result_to_string(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(text)) => text.clone(),
        Some(other) => serde_json::to_string(other).unwrap_or_else(|_| format!("{other}")),
        None => String::new(),
    }
}

fn extract_openai_responses_message_text(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(parts)) => {
            let mut text_chunks = Vec::new();
            for part in parts {
                match part.get("type").and_then(|v| v.as_str()) {
                    Some("input_text" | "output_text") => {
                        if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                            if !text.is_empty() {
                                text_chunks.push(text.to_string());
                            }
                        }
                    }
                    Some("refusal") => {
                        if let Some(text) = part.get("refusal").and_then(|v| v.as_str()) {
                            if !text.is_empty() {
                                text_chunks.push(text.to_string());
                            }
                        }
                    }
                    _ => {}
                }
            }
            text_chunks.join("\n")
        }
        _ => String::new(),
    }
}

fn convert_openai_responses_content_to_claude(content: Option<&Value>) -> Value {
    let Some(items) = content.and_then(|v| v.as_array()) else {
        return content.cloned().unwrap_or_else(|| json!(""));
    };

    let mut result = Vec::new();
    for part in items {
        match part.get("type").and_then(|v| v.as_str()) {
            Some("input_text" | "output_text") => {
                result.push(json!({
                    "type": "text",
                    "text": part.get("text").cloned().unwrap_or(json!(""))
                }));
            }
            Some("refusal") => {
                if let Some(text) = part.get("refusal").and_then(|v| v.as_str()) {
                    result.push(json!({
                        "type": "text",
                        "text": text
                    }));
                }
            }
            _ => {}
        }
    }

    if result.len() == 1 {
        if let Some(text) = result[0].get("text").and_then(|v| v.as_str()) {
            return json!(text);
        }
    }

    Value::Array(result)
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
