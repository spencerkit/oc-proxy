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
use std::collections::HashMap;

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

fn sanitize_call_id_fragment(raw: &str) -> String {
    raw.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
}

fn normalize_function_call_id(raw: &str, id_map: &mut HashMap<String, String>) -> String {
    let normalized_raw = raw.trim();
    if normalized_raw.is_empty() {
        return "fc_generated".to_string();
    }
    if let Some(existing) = id_map.get(normalized_raw) {
        return existing.clone();
    }

    let normalized = if normalized_raw.starts_with("fc") {
        normalized_raw.to_string()
    } else {
        let suffix = sanitize_call_id_fragment(normalized_raw);
        if suffix.is_empty() {
            "fc_generated".to_string()
        } else {
            format!("fc_{suffix}")
        }
    };

    id_map.insert(normalized_raw.to_string(), normalized.clone());
    normalized
}

fn normalize_system_to_instructions(system: &Value) -> Option<String> {
    if system.is_null() {
        return None;
    }

    if let Some(text) = system.as_str() {
        if text.trim().is_empty() {
            return None;
        }
        return Some(text.to_string());
    }

    if let Some(arr) = system.as_array() {
        let mut texts = Vec::with_capacity(arr.len());
        for block in arr {
            if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                if !text.is_empty() {
                    texts.push(text.to_string());
                }
                continue;
            }

            if let Some(text) = block.as_str() {
                if !text.is_empty() {
                    texts.push(text.to_string());
                }
            }
        }

        if !texts.is_empty() {
            return Some(texts.join("\n\n"));
        }
    }

    Some(system.to_string())
}

fn canonicalize_schema_key(key: &str) -> String {
    key.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect::<String>()
}

fn collect_schema_properties(schema: &Value, out: &mut serde_json::Map<String, Value>) {
    if let Some(properties) = schema.get("properties").and_then(|v| v.as_object()) {
        for (key, value) in properties {
            out.entry(key.clone()).or_insert_with(|| value.clone());
        }
    }

    for composite in ["allOf", "anyOf", "oneOf"] {
        if let Some(parts) = schema.get(composite).and_then(|v| v.as_array()) {
            for part in parts {
                collect_schema_properties(part, out);
            }
        }
    }
}

fn schema_properties(schema: &Value) -> serde_json::Map<String, Value> {
    let mut out = serde_json::Map::new();
    collect_schema_properties(schema, &mut out);
    out
}

fn schema_alias_index(
    properties: &serde_json::Map<String, Value>,
) -> HashMap<String, Option<String>> {
    let mut index = HashMap::<String, Option<String>>::new();
    for key in properties.keys() {
        let alias = canonicalize_schema_key(key);
        if alias.is_empty() {
            continue;
        }

        match index.get_mut(&alias) {
            Some(slot) => {
                if slot.as_deref() != Some(key.as_str()) {
                    *slot = None;
                }
            }
            None => {
                index.insert(alias, Some(key.clone()));
            }
        }
    }
    index
}

fn normalize_arguments_with_schema(arguments: &Value, schema: Option<&Value>) -> Value {
    let Some(schema) = schema else {
        return arguments.clone();
    };

    match arguments {
        Value::Object(obj) => {
            let properties = schema_properties(schema);
            if properties.is_empty() {
                return arguments.clone();
            }

            let alias_index = schema_alias_index(&properties);
            let mut normalized = serde_json::Map::new();
            for (key, value) in obj {
                let resolved_key = if properties.contains_key(key) {
                    key.clone()
                } else {
                    let alias = canonicalize_schema_key(key);
                    match alias_index.get(&alias) {
                        Some(Some(mapped))
                            if !mapped.is_empty() && !obj.contains_key(mapped.as_str()) =>
                        {
                            mapped.clone()
                        }
                        _ => key.clone(),
                    }
                };

                let child_schema = properties.get(&resolved_key);
                normalized.insert(
                    resolved_key,
                    normalize_arguments_with_schema(value, child_schema),
                );
            }
            Value::Object(normalized)
        }
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| normalize_arguments_with_schema(item, schema.get("items")))
                .collect::<Vec<_>>(),
        ),
        _ => arguments.clone(),
    }
}

pub fn encode_request(request: &CanonicalRequest) -> Value {
    let mut input = vec![];
    let mut system_chunks = vec![];
    let mut function_call_id_map = HashMap::<String, String>::new();
    let tool_schemas = request
        .tools
        .as_ref()
        .map(|tools| {
            tools
                .iter()
                .map(|tool| (tool.name.clone(), tool.input_schema.clone()))
                .collect::<HashMap<_, _>>()
        })
        .unwrap_or_default();

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
                            let call_id =
                                normalize_function_call_id(tool_use_id, &mut function_call_id_map);
                            input.push(json!({
                                "type": "function_call_output",
                                "id": call_id.clone(),
                                "call_id": call_id,
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
                        let normalized_args =
                            normalize_arguments_with_schema(args, tool_schemas.get(name));
                        let call_id = normalize_function_call_id(id, &mut function_call_id_map);
                        input.push(json!({
                            "type": "function_call",
                            "id": call_id.clone(),
                            "call_id": call_id,
                            "status": "completed",
                            "name": name,
                            "arguments": serde_json::to_string(&normalized_args)
                                .unwrap_or_else(|_| "{}".to_string()),
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
                        let call_id =
                            normalize_function_call_id(tool_use_id, &mut function_call_id_map);
                        input.push(json!({
                            "type": "function_call_output",
                            "id": call_id.clone(),
                            "call_id": call_id,
                            "output": content,
                        }));
                    }
                }

                if !emitted {
                    let text = merge_text(&msg.blocks);
                    if !text.is_empty() {
                        let call_id =
                            normalize_function_call_id("call_generated", &mut function_call_id_map);
                        input.push(json!({
                            "type": "function_call_output",
                            "id": call_id.clone(),
                            "call_id": call_id,
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
                    "name": tool.name,
                    "description": tool.description.clone().unwrap_or(Value::Null),
                    "parameters": tool.input_schema,
                })
            })
            .collect::<Vec<_>>());
    }

    if let Some(CanonicalToolChoice { kind, name }) = &request.tool_choice {
        if let Some(name) = name {
            out["tool_choice"] = json!({
                "type": "function",
                "name": name
            });
        } else {
            out["tool_choice"] = json!(kind);
        }
    }

    if let Some(stop) = &request.stop {
        out["stop"] = stop.clone();
    }

    if let Some(system) = &request.system {
        if let Some(instructions) = normalize_system_to_instructions(system) {
            out["instructions"] = json!(instructions);
        }
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
