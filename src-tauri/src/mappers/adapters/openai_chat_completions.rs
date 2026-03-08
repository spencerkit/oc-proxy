//! Module Overview
//! OpenAI Chat Completions adapter implementation.
//! Encodes/decodes chat-completions payloads to/from canonical structures.

use super::super::canonical::{
    CanonicalBlock, CanonicalFinishReason, CanonicalMessage, CanonicalRequest, CanonicalResponse,
    CanonicalRole, CanonicalTool, CanonicalToolCall, CanonicalToolChoice, CanonicalUsage,
    MapOptions,
};
use super::super::helpers::{
    as_array, extract_openai_usage_summary, parse_openai_finish_reason, str_or_empty,
    to_tool_result_content, OpenAIFinishReason,
};
use serde_json::{json, Value};
use std::collections::HashMap;
use uuid::Uuid;

/// Returns a cloned field only when it exists and is non-null.
fn non_null(body: &Value, key: &str) -> Option<Value> {
    body.get(key).filter(|v| !v.is_null()).cloned()
}

/// Removes any OpenAI `$schema` metadata before forwarding tool parameter unions.
/// Returns a default empty object schema if input is null.
pub fn strip_schema_field(schema: &Value) -> Value {
    if schema.is_null() {
        return json!({"type": "object", "properties": {}});
    }
    if let Some(mut map) = schema.as_object().cloned() {
        map.remove("$schema");
        Value::Object(map)
    } else {
        schema.clone()
    }
}

/// Normalizes mixed OpenAI content shapes into canonical text blocks.
fn parse_text_blocks(content: &Value) -> Vec<CanonicalBlock> {
    if let Some(arr) = content.as_array() {
        let mut out = vec![];
        for item in arr {
            if let Some(s) = item.as_str() {
                if !s.is_empty() {
                    out.push(CanonicalBlock::Text(s.to_string()));
                }
                continue;
            }

            if let Some(obj) = item.as_object() {
                let block_type = obj.get("type").and_then(|v| v.as_str()).unwrap_or_default();
                let text = if block_type == "text"
                    || block_type == "input_text"
                    || block_type == "output_text"
                {
                    obj.get("text").and_then(|v| v.as_str())
                } else {
                    obj.get("text")
                        .or_else(|| obj.get("input_text"))
                        .or_else(|| obj.get("output_text"))
                        .and_then(|v| v.as_str())
                };
                if let Some(s) = text {
                    if !s.is_empty() {
                        out.push(CanonicalBlock::Text(s.to_string()));
                    }
                } else {
                    out.push(CanonicalBlock::Text(item.to_string()));
                }
                continue;
            }

            out.push(CanonicalBlock::Text(item.to_string()));
        }
        return out;
    }

    if let Some(s) = content.as_str() {
        if !s.is_empty() {
            return vec![CanonicalBlock::Text(s.to_string())];
        }
        return vec![];
    }

    if content.is_null() {
        return vec![];
    }

    vec![CanonicalBlock::Text(content.to_string())]
}

/// Resolves effective model by prioritizing forced target model option.
fn resolve_model(body: &Value, options: &MapOptions) -> String {
    if options.target_model.is_empty() {
        str_or_empty(body.get("model"))
    } else {
        options.target_model.clone()
    }
}

/// Decodes an OpenAI chat-completions request into canonical request structure.
pub fn decode_request(body: &Value, options: &MapOptions) -> Result<CanonicalRequest, String> {
    let mut system_chunks: Vec<String> = vec![];
    let mut messages: Vec<CanonicalMessage> = vec![];

    for msg in as_array(body, "messages") {
        let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or_default();
        if role == "system" {
            if let Some(s) = msg.get("content").and_then(|v| v.as_str()) {
                system_chunks.push(s.to_string());
            }
            continue;
        }

        if role == "assistant" {
            let mut blocks = vec![];
            if let Some(content) = msg.get("content") {
                blocks.extend(parse_text_blocks(content));
            }

            if let Some(tool_calls) = msg.get("tool_calls").and_then(|v| v.as_array()) {
                for call in tool_calls {
                    let input = call
                        .get("function")
                        .and_then(|f| f.get("arguments"))
                        .and_then(|v| v.as_str())
                        .and_then(|s| serde_json::from_str::<Value>(s).ok())
                        .unwrap_or_else(|| {
                            json!({
                                "raw": str_or_empty(
                                    call.get("function").and_then(|f| f.get("arguments"))
                                )
                            })
                        });

                    blocks.push(CanonicalBlock::ToolUse {
                        id: str_or_empty(call.get("id")),
                        name: str_or_empty(call.get("function").and_then(|f| f.get("name"))),
                        input,
                    });
                }
            }

            messages.push(CanonicalMessage {
                role: CanonicalRole::Assistant,
                blocks,
            });
            continue;
        }

        if role == "tool" {
            messages.push(CanonicalMessage {
                role: CanonicalRole::Tool,
                blocks: vec![CanonicalBlock::ToolResult {
                    tool_use_id: msg
                        .get("tool_call_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("toolu_generated")
                        .to_string(),
                    content: to_tool_result_content(msg.get("content").unwrap_or(&Value::Null)),
                }],
            });
            continue;
        }

        messages.push(CanonicalMessage {
            role: CanonicalRole::from_str(role),
            blocks: parse_text_blocks(msg.get("content").unwrap_or(&Value::Null)),
        });
    }

    let tools = body.get("tools").and_then(|v| v.as_array()).map(|arr| {
        arr.iter()
            .map(|tool| {
                let function = tool.get("function").unwrap_or(tool);
                CanonicalTool {
                    name: str_or_empty(function.get("name")),
                    description: function
                        .get("description")
                        .filter(|v| !v.is_null())
                        .cloned(),
                    input_schema: function
                        .get("parameters")
                        .or_else(|| function.get("input_schema"))
                        .cloned()
                        .unwrap_or_else(|| json!({"type": "object", "properties": {}})),
                }
            })
            .collect::<Vec<_>>()
    });

    let tool_choice = body.get("tool_choice").and_then(|tc| {
        if tc.is_string() {
            return Some(CanonicalToolChoice {
                kind: tc.as_str().unwrap_or("auto").to_string(),
                name: None,
            });
        }
        if tc.is_object() {
            return Some(CanonicalToolChoice {
                kind: tc
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("auto")
                    .to_string(),
                name: tc
                    .get("function")
                    .and_then(|f| f.get("name"))
                    .or_else(|| tc.get("name"))
                    .and_then(|v| v.as_str())
                    .map(|v| v.to_string()),
            });
        }
        None
    });

    let system = if let Some(system) = non_null(body, "system") {
        Some(system)
    } else if !system_chunks.is_empty() {
        Some(json!(system_chunks.join("\n\n")))
    } else {
        None
    };

    Ok(CanonicalRequest {
        model: resolve_model(body, options),
        messages,
        max_tokens: body
            .get("max_tokens")
            .or_else(|| body.get("max_output_tokens"))
            .filter(|v| !v.is_null())
            .cloned(),
        temperature: non_null(body, "temperature"),
        top_p: non_null(body, "top_p"),
        stream: body
            .get("stream")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        system,
        tools,
        tool_choice,
        stop: non_null(body, "stop"),
        thinking: non_null(body, "thinking"),
        context_management: non_null(body, "context_management"),
    })
}

/// Merges all canonical text blocks into a single plain-text string.
fn merge_text(blocks: &[CanonicalBlock]) -> String {
    let mut out = String::new();
    for block in blocks {
        if let CanonicalBlock::Text(text) = block {
            out.push_str(text);
        }
    }
    out
}

/// Encodes canonical request into OpenAI chat-completions request JSON.
pub fn encode_request(request: &CanonicalRequest) -> Value {
    let mut messages: Vec<Value> = vec![];

    if let Some(system) = &request.system {
        messages.push(json!({ "role": "system", "content": system }));
    }

    for msg in &request.messages {
        match &msg.role {
            CanonicalRole::System => {
                if request.system.is_none() {
                    messages.push(json!({
                        "role": "system",
                        "content": merge_text(&msg.blocks),
                    }));
                }
            }
            CanonicalRole::Assistant => {
                let mut tool_calls = vec![];
                for block in &msg.blocks {
                    if let CanonicalBlock::ToolUse { id, name, input } = block {
                        tool_calls.push(json!({
                            "id": if id.is_empty() { "tool_generated" } else { id },
                            "type": "function",
                            "function": {
                                "name": name,
                                "arguments": serde_json::to_string(input)
                                    .unwrap_or_else(|_| "{}".to_string()),
                            }
                        }));
                    }
                }

                let mut assistant_msg = json!({
                    "role": "assistant",
                    "content": merge_text(&msg.blocks),
                });
                if !tool_calls.is_empty() {
                    assistant_msg["tool_calls"] = json!(tool_calls);
                }
                messages.push(assistant_msg);
            }
            CanonicalRole::User => {
                let mut user_text = String::new();
                for block in &msg.blocks {
                    match block {
                        CanonicalBlock::Text(text) => user_text.push_str(text),
                        CanonicalBlock::ToolResult {
                            tool_use_id,
                            content,
                        } => {
                            if !user_text.is_empty() {
                                messages.push(json!({ "role": "user", "content": user_text }));
                                user_text = String::new();
                            }
                            messages.push(json!({
                                "role": "tool",
                                "tool_call_id": if tool_use_id.is_empty() { "tool_generated" } else { tool_use_id },
                                "content": content,
                            }));
                        }
                        CanonicalBlock::ToolUse { .. } => {}
                    }
                }
                if !user_text.is_empty() {
                    messages.push(json!({ "role": "user", "content": user_text }));
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
                        messages.push(json!({
                            "role": "tool",
                            "tool_call_id": if tool_use_id.is_empty() { "tool_generated" } else { tool_use_id },
                            "content": content,
                        }));
                        emitted = true;
                    }
                }

                if !emitted {
                    messages.push(json!({
                        "role": "tool",
                        "tool_call_id": "tool_generated",
                        "content": merge_text(&msg.blocks),
                    }));
                }
            }
            CanonicalRole::Other(role) => {
                messages.push(json!({
                    "role": role,
                    "content": merge_text(&msg.blocks),
                }));
            }
        }
    }

    let mut req = json!({
        "model": request.model,
        "messages": messages,
        "max_tokens": request.max_tokens.clone().unwrap_or(Value::Null),
        "temperature": request.temperature.clone().unwrap_or(Value::Null),
        "top_p": request.top_p.clone().unwrap_or(Value::Null),
        "stream": request.stream,
    });

    if request.stream {
        req["stream_options"] = json!({ "include_usage": true });
    }

    if let Some(tools) = &request.tools {
        req["tools"] = json!(tools
            .iter()
            .map(|tool| {
                json!({
                    "type": "function",
                    "function": {
                        "name": tool.name,
                        "description": tool.description.clone().unwrap_or(Value::Null),
                        "parameters": strip_schema_field(&tool.input_schema),
                    }
                })
            })
            .collect::<Vec<_>>());
    }

    if let Some(choice) = &request.tool_choice {
        if let Some(name) = &choice.name {
            req["tool_choice"] = json!({
                "type": "function",
                "function": { "name": name }
            });
        } else {
            req["tool_choice"] = json!(choice.kind);
        }
    }

    if let Some(stop) = &request.stop {
        req["stop"] = stop.clone();
    }

    req
}

/// Decodes OpenAI chat-completions response JSON into canonical response structure.
pub fn decode_response(openai_response: &Value, request_model: &str) -> CanonicalResponse {
    let choice = openai_response
        .get("choices")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .cloned()
        .unwrap_or_else(|| json!({}));
    let message = choice.get("message").cloned().unwrap_or_else(|| json!({}));

    let mut tool_calls = vec![];
    if let Some(arr) = message.get("tool_calls").and_then(|v| v.as_array()) {
        for call in arr {
            tool_calls.push(CanonicalToolCall {
                id: call
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("tool_generated")
                    .to_string(),
                name: call
                    .get("function")
                    .and_then(|f| f.get("name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("tool")
                    .to_string(),
                arguments: call
                    .get("function")
                    .and_then(|f| f.get("arguments"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("{}")
                    .to_string(),
            });
        }
    }

    let finish_reason = match parse_openai_finish_reason(
        choice
            .get("finish_reason")
            .and_then(|v| v.as_str())
            .unwrap_or("stop"),
    ) {
        OpenAIFinishReason::ToolCalls => CanonicalFinishReason::ToolUse,
        OpenAIFinishReason::Length => CanonicalFinishReason::MaxTokens,
        OpenAIFinishReason::Stop => CanonicalFinishReason::Stop,
        OpenAIFinishReason::Other(other) => CanonicalFinishReason::Other(other.to_string()),
    };

    let usage = openai_response
        .get("usage")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let usage_summary = extract_openai_usage_summary(&usage).unwrap_or_default();

    CanonicalResponse {
        id: openai_response
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("chatcmpl_generated")
            .to_string(),
        created: openai_response
            .get("created")
            .and_then(|v| v.as_i64())
            .unwrap_or_else(|| chrono::Utc::now().timestamp()),
        model: if request_model.is_empty() {
            openai_response
                .get("model")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string()
        } else {
            request_model.to_string()
        },
        text: message
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        tool_calls,
        finish_reason,
        usage: CanonicalUsage {
            input_tokens: usage_summary.input_tokens,
            output_tokens: usage_summary.output_tokens,
            total_tokens: usage_summary.total_tokens,
        },
    }
}

/// Encodes canonical response into OpenAI chat-completions response JSON.
pub fn encode_response(response: &CanonicalResponse) -> Value {
    let tool_calls = response
        .tool_calls
        .iter()
        .map(|call| {
            json!({
                "id": if call.id.is_empty() { "tool_generated" } else { &call.id },
                "type": "function",
                "function": {
                    "name": call.name,
                    "arguments": call.arguments,
                }
            })
        })
        .collect::<Vec<_>>();

    let mut message = json!({
        "role": "assistant",
        "content": response.text,
    });
    if !tool_calls.is_empty() {
        message["tool_calls"] = json!(tool_calls);
    }

    let finish_reason = match &response.finish_reason {
        CanonicalFinishReason::ToolUse => "tool_calls",
        CanonicalFinishReason::MaxTokens => "length",
        CanonicalFinishReason::Stop => "stop",
        CanonicalFinishReason::Other(other) => other.as_str(),
    };

    let total_tokens = response
        .usage
        .total_tokens
        .unwrap_or(response.usage.input_tokens + response.usage.output_tokens);

    json!({
        "id": response.id,
        "object": "chat.completion",
        "created": response.created,
        "model": response.model,
        "choices": [
            {
                "index": 0,
                "message": message,
                "finish_reason": finish_reason,
            }
        ],
        "usage": {
            "prompt_tokens": response.usage.input_tokens,
            "completion_tokens": response.usage.output_tokens,
            "total_tokens": total_tokens,
        }
    })
}

#[derive(Clone)]
struct ResponsesToolState {
    index: usize,
    id: String,
    name: String,
    arguments: String,
}

pub(crate) struct OpenaiResponsesToChatStreamMapper {
    request_model: String,
    upstream_id: Option<String>,
    upstream_model: Option<String>,
    upstream_created: Option<i64>,
    final_usage: Option<Value>,
    final_finish_reason: Option<String>,
    done_emitted: bool,
    saw_tool_call: bool,
    tool_states: HashMap<String, ResponsesToolState>,
    output_index_to_tool_id: HashMap<usize, String>,
    next_tool_index: usize,
}

impl OpenaiResponsesToChatStreamMapper {
    /// Creates a stream mapper that converts `responses` SSE events into chat chunks.
    pub(crate) fn new(request_model: &str) -> Self {
        Self {
            request_model: request_model.to_string(),
            upstream_id: None,
            upstream_model: None,
            upstream_created: None,
            final_usage: None,
            final_finish_reason: None,
            done_emitted: false,
            saw_tool_call: false,
            tool_states: HashMap::new(),
            output_index_to_tool_id: HashMap::new(),
            next_tool_index: 0,
        }
    }

    /// Consumes one `responses` SSE JSON payload and emits chat-completions chunks.
    pub(crate) fn on_stream_payload(&mut self, event: Option<&str>, payload: &Value) -> Vec<Value> {
        let mut out = Vec::new();
        self.update_common_metadata(payload);
        let event_name = self.resolve_event_name(event, payload);

        if let Some(name) = event_name.as_deref() {
            match name {
                "response.output_text.delta" => {
                    if let Some(text) = payload
                        .get("delta")
                        .and_then(|v| v.as_str())
                        .or_else(|| payload.get("text").and_then(|v| v.as_str()))
                    {
                        self.emit_chat_delta(json!({ "content": text }), None, &mut out);
                    }
                }
                "response.output_item.added" => {
                    let item = Self::response_item(payload);
                    if item.and_then(|v| v.get("type")).and_then(|v| v.as_str())
                        == Some("function_call")
                    {
                        let output_index = Self::output_index_from_payload(payload, item);
                        let call_id = Self::call_id_from_payload(payload, item);
                        let name = item
                            .and_then(|v| v.get("name"))
                            .and_then(|v| v.as_str())
                            .map(|v| v.to_string());
                        self.ensure_tool_state(call_id, output_index, name);
                        self.saw_tool_call = true;
                    }
                }
                "response.function_call_arguments.delta" => {
                    let item = Self::response_item(payload);
                    let output_index = Self::output_index_from_payload(payload, item);
                    let call_id = Self::call_id_from_payload(payload, item);
                    let name = item
                        .and_then(|v| v.get("name"))
                        .and_then(|v| v.as_str())
                        .map(|v| v.to_string());
                    let key = self.ensure_tool_state(call_id, output_index, name);
                    if let Some(delta) = payload.get("delta").and_then(|v| v.as_str()) {
                        self.emit_tool_delta(&key, delta, &mut out);
                    }
                }
                "response.output_item.done" => {
                    let item = Self::response_item(payload);
                    if item.and_then(|v| v.get("type")).and_then(|v| v.as_str())
                        == Some("function_call")
                    {
                        let output_index = Self::output_index_from_payload(payload, item);
                        let call_id = Self::call_id_from_payload(payload, item);
                        let name = item
                            .and_then(|v| v.get("name"))
                            .and_then(|v| v.as_str())
                            .map(|v| v.to_string());
                        let key = self.ensure_tool_state(call_id, output_index, name);
                        if let Some(arguments) = item
                            .and_then(|v| v.get("arguments"))
                            .and_then(|v| v.as_str())
                        {
                            if let Some(state) = self.tool_states.get(&key) {
                                if state.arguments.is_empty() {
                                    self.emit_tool_delta(&key, arguments, &mut out);
                                }
                            }
                        }
                    }
                }
                "response.completed" => {
                    self.final_finish_reason = self.event_finish_reason_from_completed(payload);
                    out.extend(self.finish());
                }
                _ => {}
            }
        }

        out
    }

    /// Finalizes mapping when upstream emits `[DONE]`.
    pub(crate) fn on_done(&mut self) -> Vec<Value> {
        self.finish()
    }

    /// Flushes exactly one final chat chunk with finish reason/usage.
    pub(crate) fn finish(&mut self) -> Vec<Value> {
        if self.done_emitted {
            return Vec::new();
        }
        let finish_reason = self.final_finish_reason.clone().unwrap_or_else(|| {
            if self.saw_tool_call {
                "tool_calls".to_string()
            } else {
                "stop".to_string()
            }
        });
        let mut out = Vec::new();
        self.emit_chat_delta(json!({}), Some(&finish_reason), &mut out);
        self.done_emitted = true;
        out
    }

    /// Builds a fallback non-stream chat-completion JSON from accumulated metadata.
    pub(crate) fn final_chat_response_json(&self) -> Option<Value> {
        let id = self
            .upstream_id
            .clone()
            .unwrap_or_else(|| format!("chatcmpl_{}", Uuid::new_v4().simple()));
        let model = if !self.request_model.is_empty() {
            self.request_model.clone()
        } else {
            self.upstream_model
                .clone()
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| "unknown".to_string())
        };
        let usage = self.final_usage.clone().unwrap_or_else(|| {
            json!({
                "prompt_tokens": 0,
                "completion_tokens": 0,
                "total_tokens": 0,
            })
        });
        Some(json!({
            "id": id,
            "object": "chat.completion",
            "created": self.upstream_created.unwrap_or_else(|| chrono::Utc::now().timestamp()),
            "model": model,
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "",
                },
                "finish_reason": self.final_finish_reason.clone().unwrap_or_else(|| "stop".to_string()),
            }],
            "usage": usage,
        }))
    }

    /// Updates shared metadata fields (id/model/created/usage) from incoming payload.
    fn update_common_metadata(&mut self, payload: &Value) {
        if let Some(id) = payload.get("id").and_then(|v| v.as_str()).or_else(|| {
            payload
                .get("response")
                .and_then(|v| v.get("id"))
                .and_then(|v| v.as_str())
        }) {
            self.upstream_id = Some(id.to_string());
        }
        if let Some(model) = payload.get("model").and_then(|v| v.as_str()).or_else(|| {
            payload
                .get("response")
                .and_then(|v| v.get("model"))
                .and_then(|v| v.as_str())
        }) {
            self.upstream_model = Some(model.to_string());
        }
        if let Some(created) = payload
            .get("created")
            .and_then(|v| v.as_i64())
            .or_else(|| payload.get("created_at").and_then(|v| v.as_i64()))
            .or_else(|| {
                payload
                    .get("response")
                    .and_then(|v| v.get("created_at"))
                    .and_then(|v| v.as_i64())
            })
        {
            self.upstream_created = Some(created);
        }
        if let Some(usage) = payload
            .get("usage")
            .or_else(|| payload.get("response").and_then(|v| v.get("usage")))
        {
            if let Some(summary) = extract_openai_usage_summary(usage) {
                self.final_usage = Some(json!({
                    "prompt_tokens": summary.input_tokens,
                    "completion_tokens": summary.output_tokens,
                    "total_tokens": summary
                        .total_tokens
                        .unwrap_or(summary.input_tokens + summary.output_tokens),
                }));
            }
        }
    }

    /// Resolves event name from explicit SSE event header or payload `type`.
    fn resolve_event_name(&self, event: Option<&str>, payload: &Value) -> Option<String> {
        event.map(|v| v.to_string()).or_else(|| {
            payload
                .get("type")
                .and_then(|v| v.as_str())
                .map(|v| v.to_string())
        })
    }

    /// Appends one chat-completions chunk object to output.
    fn emit_chat_delta(&self, delta: Value, finish_reason: Option<&str>, out: &mut Vec<Value>) {
        let mut choice = json!({
            "index": 0,
            "delta": delta,
            "finish_reason": Value::Null,
        });
        if let Some(reason) = finish_reason {
            choice["finish_reason"] = json!(reason);
        }

        let mut chunk = json!({
            "id": self
                .upstream_id
                .clone()
                .unwrap_or_else(|| format!("chatcmpl_{}", Uuid::new_v4().simple())),
            "object": "chat.completion.chunk",
            "created": self
                .upstream_created
                .unwrap_or_else(|| chrono::Utc::now().timestamp()),
            "model": if !self.request_model.is_empty() {
                self.request_model.clone()
            } else {
                self.upstream_model
                    .clone()
                    .filter(|v| !v.is_empty())
                    .unwrap_or_else(|| "unknown".to_string())
            },
            "choices": [choice],
        });
        if finish_reason.is_some() {
            if let Some(usage) = self.final_usage.clone() {
                chunk["usage"] = usage;
            }
        }
        out.push(chunk);
    }

    /// Creates or updates tool-call state keyed by call id/output index.
    fn ensure_tool_state(
        &mut self,
        tool_id: Option<String>,
        output_index: Option<usize>,
        tool_name: Option<String>,
    ) -> String {
        let mut key = tool_id.unwrap_or_else(|| {
            if let Some(index) = output_index {
                if let Some(existing) = self.output_index_to_tool_id.get(&index) {
                    return existing.clone();
                }
                format!("call_{index}")
            } else {
                format!("call_{}", Uuid::new_v4().simple())
            }
        });
        if key.is_empty() {
            key = format!("call_{}", Uuid::new_v4().simple());
        }
        if let Some(index) = output_index {
            self.output_index_to_tool_id.insert(index, key.clone());
        }
        let index = if let Some(existing) = self.tool_states.get(&key) {
            existing.index
        } else {
            let idx = self.next_tool_index;
            self.next_tool_index += 1;
            idx
        };
        let entry = self
            .tool_states
            .entry(key.clone())
            .or_insert_with(|| ResponsesToolState {
                index,
                id: key.clone(),
                name: tool_name
                    .clone()
                    .filter(|v| !v.is_empty())
                    .unwrap_or_else(|| "tool".to_string()),
                arguments: String::new(),
            });
        if let Some(name) = tool_name {
            if !name.is_empty() {
                entry.name = name;
            }
        }
        key
    }

    /// Emits incremental tool-call argument delta as chat `tool_calls` chunk.
    fn emit_tool_delta(&mut self, tool_key: &str, args_delta: &str, out: &mut Vec<Value>) {
        if args_delta.is_empty() {
            return;
        }
        let Some(state) = self.tool_states.get_mut(tool_key) else {
            return;
        };
        state.arguments.push_str(args_delta);
        let tool_index = state.index;
        let tool_id = state.id.clone();
        let tool_name = state.name.clone();
        self.saw_tool_call = true;
        self.emit_chat_delta(
            json!({
                "tool_calls": [{
                    "index": tool_index,
                    "id": tool_id,
                    "type": "function",
                    "function": {
                        "name": tool_name,
                        "arguments": args_delta,
                    }
                }]
            }),
            None,
            out,
        );
    }

    /// Infers chat finish_reason from `response.completed` payload status/output.
    fn event_finish_reason_from_completed(&self, payload: &Value) -> Option<String> {
        let status = payload
            .get("response")
            .and_then(|v| v.get("status"))
            .and_then(|v| v.as_str())
            .or_else(|| payload.get("status").and_then(|v| v.as_str()));
        if status == Some("incomplete") {
            return Some("length".to_string());
        }
        let has_function_call = payload
            .get("response")
            .and_then(|v| v.get("output"))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .any(|item| item.get("type").and_then(|v| v.as_str()) == Some("function_call"))
            })
            .unwrap_or(false);
        if has_function_call || self.saw_tool_call {
            return Some("tool_calls".to_string());
        }
        Some("stop".to_string())
    }

    /// Extracts nested `item` object from responses event payload.
    fn response_item(payload: &Value) -> Option<&Value> {
        payload.get("item")
    }

    /// Resolves output index from top-level event or nested item object.
    fn output_index_from_payload(payload: &Value, item: Option<&Value>) -> Option<usize> {
        payload
            .get("output_index")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .or_else(|| {
                item.and_then(|v| v.get("output_index"))
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize)
            })
    }

    /// Resolves call id using top-level field first, then nested item fields.
    fn call_id_from_payload(payload: &Value, item: Option<&Value>) -> Option<String> {
        payload
            .get("call_id")
            .and_then(|v| v.as_str())
            .map(|v| v.to_string())
            .or_else(|| {
                item.and_then(|v| v.get("call_id"))
                    .and_then(|v| v.as_str())
                    .map(|v| v.to_string())
            })
            .or_else(|| {
                item.and_then(|v| v.get("id"))
                    .and_then(|v| v.as_str())
                    .map(|v| v.to_string())
            })
    }
}
