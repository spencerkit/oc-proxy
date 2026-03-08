//! Module Overview
//! OpenAI Responses adapter implementation.
//! Bridges responses-specific input/output shapes with the canonical mapping model.

use super::super::canonical::{
    CanonicalBlock, CanonicalFinishReason, CanonicalRequest, CanonicalResponse, CanonicalRole,
    CanonicalToolChoice, MapOptions,
};
use super::super::helpers::extract_openai_usage_summary;
use super::super::normalize::normalize_openai_request;
use super::openai_chat_completions;
use serde_json::{json, Value};
use std::collections::HashMap;
use uuid::Uuid;

/// Decodes OpenAI responses request JSON into canonical request structure.
pub fn decode_request(body: &Value, options: &MapOptions) -> Result<CanonicalRequest, String> {
    let normalized = normalize_openai_request("/v1/responses", body);
    openai_chat_completions::decode_request(&normalized, options)
}

/// Merges canonical text blocks into a single string payload.
fn merge_text(blocks: &[CanonicalBlock]) -> String {
    let mut out = String::new();
    for block in blocks {
        if let CanonicalBlock::Text(text) = block {
            out.push_str(text);
        }
    }
    out
}

/// Pushes a user text item into OpenAI responses input array.
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

/// Pushes an assistant text item into OpenAI responses input array.
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

/// Sanitizes a function-call id fragment to a safe alphanumeric token.
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

/// Normalizes function-call ids and maintains stable id remapping for references.
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

/// Flattens `system` field into Responses API `instructions` string.
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

/// Canonicalizes a schema key for tolerant alias matching.
fn canonicalize_schema_key(key: &str) -> String {
    key.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect::<String>()
}

/// Collects schema object properties recursively into a flattened key/value map.
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

/// Returns flattened schema property map for argument alias normalization.
fn schema_properties(schema: &Value) -> serde_json::Map<String, Value> {
    let mut out = serde_json::Map::new();
    collect_schema_properties(schema, &mut out);
    out
}

/// Builds alias lookup index from schema property names.
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

/// Normalizes function arguments payload using schema alias hints when possible.
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

/// Encodes canonical request into OpenAI responses request JSON.
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
                    "parameters": openai_chat_completions::strip_schema_field(&tool.input_schema),
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

    // Map Anthropic thinking to OpenAI reasoning
    if let Some(thinking) = &request.thinking {
        if let Some(thinking_obj) = thinking.as_object() {
            if let Some(thinking_type) = thinking_obj.get("type").and_then(|v| v.as_str()) {
                let effort = match thinking_type {
                    "enabled" => "medium",
                    "adaptive" => "high",
                    _ => "medium", // fallback
                };
                out["reasoning"] = json!({
                    "effort": effort
                });
            }
        }
    }

    // Note: context_management is not forwarded as OpenAI API currently rejects it
    // with "Unsupported parameter" error despite being in the SDK types.
    // if let Some(context_management) = &request.context_management {
    //     out["context_management"] = context_management.clone();
    // }

    out
}

/// Decodes OpenAI responses response JSON into canonical response structure.
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
    let mut reasoning_text = String::new();
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

            // Extract reasoning content and map to thinking
            if item_type == "reasoning" {
                if let Some(content_arr) = item.get("content").and_then(|v| v.as_array()) {
                    for content_item in content_arr {
                        if content_item.get("type").and_then(|v| v.as_str())
                            == Some("reasoning_text")
                        {
                            reasoning_text.push_str(
                                content_item
                                    .get("reasoning")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or_default(),
                            );
                        }
                    }
                }
            }
        }
    }

    // Prepend reasoning as thinking block if present
    if !reasoning_text.is_empty() {
        text = format!("<thinking>\n{}\n</thinking>\n\n{}", reasoning_text, text);
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

/// Encodes canonical response into OpenAI responses response JSON.
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

#[derive(Clone)]
struct ChatToolState {
    id: String,
    name: String,
    arguments: String,
    output_index: usize,
    added_sent: bool,
}

pub(crate) struct OpenaiChatToResponsesStreamMapper {
    request_model: String,
    upstream_id: Option<String>,
    upstream_model: Option<String>,
    upstream_created: Option<i64>,
    output_text: String,
    tool_states: HashMap<usize, ChatToolState>,
    next_output_index: usize,
    final_usage: Option<Value>,
    final_finish_reason: Option<String>,
    completed: bool,
}

impl OpenaiChatToResponsesStreamMapper {
    /// Creates a stream mapper that converts chat-completions chunks into responses events.
    pub(crate) fn new(request_model: &str) -> Self {
        Self {
            request_model: request_model.to_string(),
            upstream_id: None,
            upstream_model: None,
            upstream_created: None,
            output_text: String::new(),
            tool_states: HashMap::new(),
            next_output_index: 1,
            final_usage: None,
            final_finish_reason: None,
            completed: false,
        }
    }

    /// Consumes one chat-completions stream payload and emits responses event tuples.
    pub(crate) fn on_stream_payload(&mut self, payload: &Value) -> Vec<(String, Value)> {
        let mut out = Vec::new();
        self.update_common_metadata(payload);

        if let Some(choices) = payload.get("choices").and_then(|v| v.as_array()) {
            for (i, choice) in choices.iter().enumerate() {
                let choice_index = choice
                    .get("index")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(i as u64) as usize;
                if choice_index != 0 {
                    continue;
                }

                if let Some(delta) = choice.get("delta") {
                    if let Some(content) = delta.get("content").and_then(|v| v.as_str()) {
                        self.emit_text_delta(content, &mut out);
                    }
                    if let Some(tool_calls) = delta.get("tool_calls").and_then(|v| v.as_array()) {
                        for (tc_i, tc) in tool_calls.iter().enumerate() {
                            let tool_index = tc
                                .get("index")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(tc_i as u64)
                                as usize;
                            self.emit_tool_delta(tool_index, tc, &mut out);
                        }
                    }
                }

                if let Some(reason) = choice.get("finish_reason").and_then(|v| v.as_str()) {
                    if !reason.is_empty() {
                        self.final_finish_reason = Some(reason.to_string());
                    }
                }
            }
        }
        out
    }

    /// Finalizes mapping when upstream emits `[DONE]`.
    pub(crate) fn on_done(&mut self) -> Vec<(String, Value)> {
        self.finish()
    }

    /// Flushes final responses event sequence exactly once.
    pub(crate) fn finish(&mut self) -> Vec<(String, Value)> {
        if self.completed {
            return Vec::new();
        }
        let mut out = Vec::new();
        self.emit_completed(&mut out);
        out
    }

    /// Builds a final non-stream Responses JSON if mapper has completed.
    pub(crate) fn final_responses_json(&self) -> Option<Value> {
        if !self.completed {
            return None;
        }
        Some(json!({
            "id": self.response_id(),
            "object": "response",
            "created_at": self.response_created(),
            "model": self.response_model(),
            "status": "completed",
            "output": [{
                "type": "message",
                "role": "assistant",
                "content": [{"type":"output_text","text": self.output_text}],
            }],
            "usage": self.final_usage.clone().unwrap_or_else(|| json!({
                "input_tokens": 0,
                "output_tokens": 0,
                "total_tokens": 0,
            }))
        }))
    }

    /// Updates shared metadata fields (id/model/created/usage) from stream payload.
    fn update_common_metadata(&mut self, payload: &Value) {
        if let Some(id) = payload.get("id").and_then(|v| v.as_str()) {
            self.upstream_id = Some(id.to_string());
        }
        if let Some(model) = payload.get("model").and_then(|v| v.as_str()) {
            self.upstream_model = Some(model.to_string());
        }
        if let Some(created) = payload.get("created").and_then(|v| v.as_i64()) {
            self.upstream_created = Some(created);
        }
        if let Some(usage) = payload.get("usage") {
            if let Some(summary) = extract_openai_usage_summary(usage) {
                self.final_usage = Some(json!({
                    "input_tokens": summary.input_tokens,
                    "output_tokens": summary.output_tokens,
                    "total_tokens": summary
                        .total_tokens
                        .unwrap_or(summary.input_tokens + summary.output_tokens),
                }));
            }
        }
    }

    /// Produces stable Responses `id` from upstream id or generated fallback.
    fn response_id(&self) -> String {
        self.upstream_id
            .as_ref()
            .map(|id| format!("resp_{id}"))
            .unwrap_or_else(|| format!("resp_{}", Uuid::new_v4().simple()))
    }

    /// Resolves output model with request override precedence.
    fn response_model(&self) -> String {
        if !self.request_model.is_empty() {
            return self.request_model.clone();
        }
        self.upstream_model
            .clone()
            .unwrap_or_else(|| "unknown".to_string())
    }

    /// Resolves output created timestamp with current-time fallback.
    fn response_created(&self) -> i64 {
        self.upstream_created
            .unwrap_or_else(|| chrono::Utc::now().timestamp())
    }

    /// Creates or updates tool state for a tool-call index.
    fn ensure_tool_state(&mut self, tool_index: usize, tool_call: &Value) -> &mut ChatToolState {
        self.tool_states.entry(tool_index).or_insert_with(|| {
            let output_index = self.next_output_index;
            self.next_output_index += 1;
            let id = tool_call
                .get("id")
                .and_then(|v| v.as_str())
                .filter(|v| !v.is_empty())
                .map(|v| v.to_string())
                .unwrap_or_else(|| format!("call_{}", Uuid::new_v4().simple()));
            let name = tool_call
                .get("function")
                .and_then(|v| v.get("name"))
                .and_then(|v| v.as_str())
                .filter(|v| !v.is_empty())
                .map(|v| v.to_string())
                .unwrap_or_else(|| "tool".to_string());
            ChatToolState {
                id,
                name,
                arguments: String::new(),
                output_index,
                added_sent: false,
            }
        })
    }

    /// Emits one `response.output_text.delta` event and accumulates full text.
    fn emit_text_delta(&mut self, text: &str, out: &mut Vec<(String, Value)>) {
        if text.is_empty() {
            return;
        }
        self.output_text.push_str(text);
        out.push((
            "response.output_text.delta".to_string(),
            json!({
                "type": "response.output_text.delta",
                "response_id": self.response_id(),
                "output_index": 0,
                "content_index": 0,
                "delta": text,
            }),
        ));
    }

    /// Emits tool-call added/arguments-delta events from chat tool call chunk.
    fn emit_tool_delta(
        &mut self,
        tool_index: usize,
        tool_call: &Value,
        out: &mut Vec<(String, Value)>,
    ) {
        let state = self.ensure_tool_state(tool_index, tool_call).clone();

        if !state.added_sent {
            out.push((
                "response.output_item.added".to_string(),
                json!({
                    "type": "response.output_item.added",
                    "response_id": self.response_id(),
                    "output_index": state.output_index,
                    "item": {
                        "type": "function_call",
                        "id": state.id,
                        "call_id": state.id,
                        "name": state.name,
                        "arguments": "",
                        "status": "in_progress",
                    }
                }),
            ));
            if let Some(existing) = self.tool_states.get_mut(&tool_index) {
                existing.added_sent = true;
            }
        }

        if let Some(name) = tool_call
            .get("function")
            .and_then(|v| v.get("name"))
            .and_then(|v| v.as_str())
            .filter(|v| !v.is_empty())
        {
            if let Some(existing) = self.tool_states.get_mut(&tool_index) {
                existing.name = name.to_string();
            }
        }

        if let Some(arguments) = tool_call
            .get("function")
            .and_then(|v| v.get("arguments"))
            .and_then(|v| v.as_str())
        {
            if !arguments.is_empty() {
                let response_id = self.response_id();
                if let Some(existing) = self.tool_states.get_mut(&tool_index) {
                    existing.arguments.push_str(arguments);
                    let output_index = existing.output_index;
                    let item_id = existing.id.clone();
                    out.push((
                        "response.function_call_arguments.delta".to_string(),
                        json!({
                            "type": "response.function_call_arguments.delta",
                            "response_id": response_id,
                            "output_index": output_index,
                            "item_id": item_id,
                            "call_id": item_id,
                            "delta": arguments,
                        }),
                    ));
                }
            }
        }
    }

    /// Maps chat finish_reason into Responses status string.
    fn finish_reason_to_status(reason: &str) -> &'static str {
        match reason {
            "length" => "incomplete",
            _ => "completed",
        }
    }

    /// Emits trailing item/text/completed events and seals mapper state.
    fn emit_completed(&mut self, out: &mut Vec<(String, Value)>) {
        if self.completed {
            return;
        }

        let mut sorted_tools = self.tool_states.iter().collect::<Vec<_>>();
        sorted_tools.sort_by_key(|(_, state)| state.output_index);

        for (_, state) in &sorted_tools {
            out.push((
                "response.output_item.done".to_string(),
                json!({
                    "type": "response.output_item.done",
                    "response_id": self.response_id(),
                    "output_index": state.output_index,
                    "item": {
                        "type": "function_call",
                        "id": state.id,
                        "call_id": state.id,
                        "name": state.name,
                        "arguments": state.arguments,
                        "status": "completed",
                    }
                }),
            ));
        }

        if !self.output_text.is_empty() {
            out.push((
                "response.output_text.done".to_string(),
                json!({
                    "type": "response.output_text.done",
                    "response_id": self.response_id(),
                    "output_index": 0,
                    "content_index": 0,
                    "text": self.output_text,
                }),
            ));
        }

        let mut output = vec![json!({
            "type": "message",
            "role": "assistant",
            "content": [{"type": "output_text", "text": self.output_text}],
        })];
        for (_, state) in sorted_tools {
            output.push(json!({
                "type": "function_call",
                "id": state.id,
                "call_id": state.id,
                "name": state.name,
                "arguments": state.arguments,
                "status": "completed",
            }));
        }

        let finish_reason = self.final_finish_reason.clone().unwrap_or_else(|| {
            if self.tool_states.is_empty() {
                "stop".to_string()
            } else {
                "tool_calls".to_string()
            }
        });
        let status = Self::finish_reason_to_status(&finish_reason);
        out.push((
            "response.completed".to_string(),
            json!({
                "type": "response.completed",
                "response": {
                    "id": self.response_id(),
                    "object": "response",
                    "created_at": self.response_created(),
                    "model": self.response_model(),
                    "status": status,
                    "output": output,
                    "usage": self.final_usage.clone().unwrap_or_else(|| json!({
                        "input_tokens": 0,
                        "output_tokens": 0,
                        "total_tokens": 0,
                    })),
                }
            }),
        ));

        self.completed = true;
    }
}

/// Stream mapper that converts OpenAI Responses SSE events into Anthropic Messages SSE.
pub(crate) struct OpenaiResponsesToAnthropicStreamMapper {
    request_model: String,
    upstream_id: Option<String>,
    upstream_model: Option<String>,
    message_started: bool,
    message_stopped: bool,
    next_block_index: usize,
    accumulated_text: String,
    tool_states: HashMap<String, ResponsesToolState>,
    output_index_to_tool_id: HashMap<usize, String>,
    final_stop_reason: Option<String>,
    final_usage: Option<Value>,
}

#[derive(Clone)]
struct ResponsesToolState {
    id: String,
    name: String,
    arguments: String,
}

impl OpenaiResponsesToAnthropicStreamMapper {
    /// Creates a stream mapper that converts responses SSE events into Anthropic messages SSE.
    pub(crate) fn new(request_model: &str) -> Self {
        Self {
            request_model: request_model.to_string(),
            upstream_id: None,
            upstream_model: None,
            message_started: false,
            message_stopped: false,
            next_block_index: 0,
            accumulated_text: String::new(),
            tool_states: HashMap::new(),
            output_index_to_tool_id: HashMap::new(),
            final_stop_reason: None,
            final_usage: None,
        }
    }

    /// Consumes one responses SSE JSON payload and emits Anthropic messages SSE events.
    pub(crate) fn on_stream_payload(
        &mut self,
        event: Option<&str>,
        payload: &Value,
    ) -> Vec<(String, Value)> {
        let mut out = Vec::new();
        self.update_common_metadata(payload);
        let event_name = self.resolve_event_name(event, payload);

        if let Some(name) = event_name.as_deref() {
            match name {
                "response.created" => {
                    self.ensure_message_start(&mut out);
                }
                "response.output_text.delta" => {
                    self.ensure_message_start(&mut out);
                    if let Some(text) = payload
                        .get("delta")
                        .and_then(|v| v.as_str())
                        .or_else(|| payload.get("text").and_then(|v| v.as_str()))
                    {
                        self.emit_text_delta(text, &mut out);
                    }
                }
                "response.output_item.added" => {
                    self.ensure_message_start(&mut out);
                    if let Some(item) = payload.get("item") {
                        if item.get("type").and_then(|v| v.as_str()) == Some("function_call") {
                            let output_index = payload
                                .get("output_index")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0)
                                as usize;
                            let call_id = item
                                .get("call_id")
                                .or_else(|| item.get("id"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("call_unknown")
                                .to_string();
                            let name = item
                                .get("name")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();

                            let block_index = self.next_block_index;
                            self.next_block_index += 1;

                            self.output_index_to_tool_id
                                .insert(output_index, call_id.clone());
                            self.tool_states.insert(
                                call_id.clone(),
                                ResponsesToolState {
                                    id: call_id.clone(),
                                    name: name.clone(),
                                    arguments: String::new(),
                                },
                            );

                            out.push((
                                "content_block_start".to_string(),
                                json!({
                                    "type": "content_block_start",
                                    "index": block_index,
                                    "content_block": {
                                        "type": "tool_use",
                                        "id": call_id,
                                        "name": name,
                                        "input": {},
                                    }
                                }),
                            ));
                        }
                    }
                }
                "response.function_call_arguments.delta" => {
                    self.ensure_message_start(&mut out);
                    if let Some(_item) = payload.get("item") {
                        let output_index = payload
                            .get("output_index")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0) as usize;
                        if let Some(call_id) = self.output_index_to_tool_id.get(&output_index) {
                            if let Some(delta) = payload.get("delta").and_then(|v| v.as_str()) {
                                if let Some(state) = self.tool_states.get_mut(call_id) {
                                    state.arguments.push_str(delta);

                                    let block_index = output_index + 1;
                                    out.push((
                                        "content_block_delta".to_string(),
                                        json!({
                                            "type": "content_block_delta",
                                            "index": block_index,
                                            "delta": {
                                                "type": "input_json_delta",
                                                "partial_json": delta,
                                            }
                                        }),
                                    ));
                                }
                            }
                        }
                    }
                }
                "response.output_item.done" | "response.done" | "response.completed" => {
                    if let Some(response) = payload.get("response") {
                        if let Some(usage) = response.get("usage") {
                            self.final_usage = Some(usage.clone());
                        }
                        if let Some(status) = response.get("status").and_then(|v| v.as_str()) {
                            self.final_stop_reason = Some(
                                match status {
                                    "completed" => "end_turn",
                                    "incomplete" => "max_tokens",
                                    "failed" => "error",
                                    _ => "end_turn",
                                }
                                .to_string(),
                            );
                        }
                    }
                }
                _ => {}
            }
        }
        out
    }

    /// Handles done event.
    pub(crate) fn on_done(&mut self) -> Vec<(String, Value)> {
        self.emit_final_events()
    }

    /// Performs finish.
    pub(crate) fn finish(&mut self) -> Vec<(String, Value)> {
        self.emit_final_events()
    }

    /// Builds final Anthropic message JSON.
    pub(crate) fn final_message_json(&self) -> Option<Value> {
        if !self.message_stopped {
            return None;
        }
        let mut content = Vec::new();
        if !self.accumulated_text.is_empty() {
            content.push(json!({
                "type": "text",
                "text": self.accumulated_text,
            }));
        }
        for (_, state) in &self.tool_states {
            content.push(json!({
                "type": "tool_use",
                "id": state.id,
                "name": state.name,
                "input": serde_json::from_str::<Value>(&state.arguments).unwrap_or(json!({})),
            }));
        }
        Some(json!({
            "id": self.message_id(),
            "type": "message",
            "role": "assistant",
            "content": content,
            "model": self.message_model(),
            "stop_reason": self.final_stop_reason.clone().unwrap_or_else(|| "end_turn".to_string()),
            "usage": self.anthropic_usage(),
        }))
    }

    fn update_common_metadata(&mut self, payload: &Value) {
        if let Some(response) = payload.get("response") {
            if self.upstream_id.is_none() {
                self.upstream_id = response
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(String::from);
            }
            if self.upstream_model.is_none() {
                self.upstream_model = response
                    .get("model")
                    .and_then(|v| v.as_str())
                    .map(String::from);
            }
        }
    }

    fn resolve_event_name(&self, event: Option<&str>, payload: &Value) -> Option<String> {
        if let Some(e) = event {
            return Some(e.to_string());
        }
        payload
            .get("type")
            .and_then(|v| v.as_str())
            .map(String::from)
    }

    fn ensure_message_start(&mut self, out: &mut Vec<(String, Value)>) {
        if self.message_started {
            return;
        }
        out.push((
            "message_start".to_string(),
            json!({
                "type": "message_start",
                "message": {
                    "id": self.message_id(),
                    "type": "message",
                    "role": "assistant",
                    "content": [],
                    "model": self.message_model(),
                    "usage": { "input_tokens": 0, "output_tokens": 0 },
                }
            }),
        ));
        self.message_started = true;
    }

    fn emit_text_delta(&mut self, text: &str, out: &mut Vec<(String, Value)>) {
        if text.is_empty() {
            return;
        }
        self.accumulated_text.push_str(text);
        out.push((
            "content_block_delta".to_string(),
            json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": { "type": "text_delta", "text": text },
            }),
        ));
    }

    fn emit_final_events(&mut self) -> Vec<(String, Value)> {
        if self.message_stopped {
            return Vec::new();
        }
        let mut out = Vec::new();
        if !self.accumulated_text.is_empty() {
            out.push((
                "content_block_stop".to_string(),
                json!({
                    "type": "content_block_stop",
                    "index": 0,
                }),
            ));
        }
        for (idx, (_, state)) in self.tool_states.iter().enumerate() {
            out.push((
                "content_block_start".to_string(),
                json!({
                    "type": "content_block_start",
                    "index": idx + 1,
                    "content_block": {
                        "type": "tool_use",
                        "id": state.id,
                        "name": state.name,
                    },
                }),
            ));
            out.push((
                "content_block_stop".to_string(),
                json!({
                    "type": "content_block_stop",
                    "index": idx + 1,
                }),
            ));
        }
        out.push((
            "message_delta".to_string(),
            json!({
                "type": "message_delta",
                "delta": {
                    "stop_reason": self.final_stop_reason.clone().unwrap_or_else(|| "end_turn".to_string()),
                },
                "usage": self.anthropic_usage(),
            }),
        ));
        out.push((
            "message_stop".to_string(),
            json!({ "type": "message_stop" }),
        ));
        self.message_stopped = true;
        out
    }

    fn message_id(&self) -> String {
        self.upstream_id
            .clone()
            .unwrap_or_else(|| format!("msg_{}", Uuid::new_v4().simple()))
    }

    fn message_model(&self) -> String {
        self.upstream_model
            .clone()
            .or_else(|| Some(self.request_model.clone()))
            .unwrap_or_else(|| "unknown".to_string())
    }

    fn anthropic_usage(&self) -> Value {
        if let Some(usage) = &self.final_usage {
            return json!({
                "input_tokens": usage.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
                "output_tokens": usage.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
            });
        }
        json!({
            "input_tokens": 0,
            "output_tokens": 0,
        })
    }
}
