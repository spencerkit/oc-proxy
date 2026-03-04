//! Module Overview
//! Anthropic Messages adapter implementation.
//! Encodes/decodes Anthropic request and response payloads to/from canonical structures.

use super::super::canonical::{
    CanonicalBlock, CanonicalFinishReason, CanonicalMessage, CanonicalRequest, CanonicalResponse,
    CanonicalRole, CanonicalTool, CanonicalToolCall, CanonicalToolChoice, CanonicalUsage,
    MapOptions,
};
use super::super::helpers::{
    extract_openai_usage_summary, flatten_anthropic_text, map_openai_finish_reason_to_anthropic_stop,
    str_or_empty, to_tool_result_content,
};
use serde_json::{json, Value};
use std::collections::HashMap;
use uuid::Uuid;

/// Performs non null.
fn non_null(body: &Value, key: &str) -> Option<Value> {
    body.get(key).filter(|v| !v.is_null()).cloned()
}

/// Parses blocks.
fn parse_blocks(role: &CanonicalRole, content: &Value) -> Vec<CanonicalBlock> {
    if let Some(arr) = content.as_array() {
        let mut blocks = Vec::with_capacity(arr.len());
        for block in arr {
            let block_type = block
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            match block_type {
                "text" => {
                    let text = block
                        .get("text")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default();
                    if !text.is_empty() {
                        blocks.push(CanonicalBlock::Text(text.to_string()));
                    }
                }
                "tool_use" => {
                    blocks.push(CanonicalBlock::ToolUse {
                        id: str_or_empty(block.get("id")),
                        name: str_or_empty(block.get("name")),
                        input: block.get("input").cloned().unwrap_or_else(|| json!({})),
                    });
                }
                "tool_result" => {
                    blocks.push(CanonicalBlock::ToolResult {
                        tool_use_id: str_or_empty(block.get("tool_use_id")),
                        content: to_tool_result_content(
                            block.get("content").unwrap_or(&Value::Null),
                        ),
                    });
                }
                _ => {
                    if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                        if !text.is_empty() {
                            blocks.push(CanonicalBlock::Text(text.to_string()));
                        }
                    }
                }
            }
        }
        return blocks;
    }

    if let Some(s) = content.as_str() {
        if !s.is_empty() {
            return vec![CanonicalBlock::Text(s.to_string())];
        }
        return Vec::new();
    }

    // Anthropic payloads should normally be string/array for message content;
    // keep unknown content as text to avoid dropping user input.
    if !content.is_null() {
        return vec![CanonicalBlock::Text(content.to_string())];
    }

    if *role == CanonicalRole::Assistant || *role == CanonicalRole::User {
        return Vec::new();
    }

    let flattened = flatten_anthropic_text(content);
    if flattened.is_empty() {
        Vec::new()
    } else {
        vec![CanonicalBlock::Text(flattened)]
    }
}

/// Decodes request for this module's workflow.
pub fn decode_request(body: &Value, options: &MapOptions) -> Result<CanonicalRequest, String> {
    let model = if options.target_model.is_empty() {
        str_or_empty(body.get("model"))
    } else {
        options.target_model.clone()
    };

    let mut messages = vec![];
    if let Some(in_messages) = body.get("messages").and_then(|v| v.as_array()) {
        for msg in in_messages {
            let role = CanonicalRole::from_str(
                msg.get("role").and_then(|v| v.as_str()).unwrap_or_default(),
            );
            let content = msg.get("content").cloned().unwrap_or(Value::Null);
            messages.push(CanonicalMessage {
                blocks: parse_blocks(&role, &content),
                role,
            });
        }
    }

    let tools = body.get("tools").and_then(|v| v.as_array()).map(|arr| {
        arr.iter()
            .map(|tool| CanonicalTool {
                name: str_or_empty(tool.get("name")),
                description: tool.get("description").filter(|v| !v.is_null()).cloned(),
                input_schema: tool
                    .get("input_schema")
                    .cloned()
                    .unwrap_or_else(|| json!({"type": "object", "properties": {}})),
            })
            .collect::<Vec<_>>()
    });

    let tool_choice = body.get("tool_choice").and_then(|tc| {
        tc.as_object().map(|_| CanonicalToolChoice {
            kind: tc
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("auto")
                .to_string(),
            name: tc
                .get("name")
                .and_then(|v| v.as_str())
                .map(|v| v.to_string()),
        })
    });

    Ok(CanonicalRequest {
        model,
        messages,
        max_tokens: non_null(body, "max_tokens"),
        temperature: non_null(body, "temperature"),
        top_p: non_null(body, "top_p"),
        stream: body.get("stream").and_then(|v| v.as_bool()).unwrap_or(true),
        system: non_null(body, "system"),
        tools,
        tool_choice,
        stop: non_null(body, "stop_sequences"),
        thinking: non_null(body, "thinking"),
        context_management: non_null(body, "context_management"),
    })
}

/// Pushs text block for this module's workflow.
fn push_text_block(out: &mut Vec<Value>, text: &str) {
    if !text.is_empty() {
        out.push(json!({ "type": "text", "text": text }));
    }
}

#[derive(Clone)]
enum ActiveBlock {
    Text { block_index: usize },
    Tool { tool_index: usize, block_index: usize },
}

#[derive(Clone)]
struct ToolState {
    block_index: usize,
    id: String,
    name: String,
    started: bool,
}

enum AccumulatedContentBlock {
    Text { text: String },
    ToolUse {
        id: String,
        name: String,
        partial_input: String,
    },
}

pub(crate) struct OpenaiChatToAnthropicStreamMapper {
    request_model: String,
    upstream_model: Option<String>,
    upstream_id: Option<String>,
    resolved_message_id: Option<String>,
    resolved_model: Option<String>,
    message_started: bool,
    message_stopped: bool,
    next_block_index: usize,
    active_block: Option<ActiveBlock>,
    tool_states: HashMap<usize, ToolState>,
    accumulated_content: HashMap<usize, AccumulatedContentBlock>,
    final_stop_reason: Option<String>,
    final_usage: Option<Value>,
}

impl OpenaiChatToAnthropicStreamMapper {
    /// Performs new.
    pub(crate) fn new(request_model: &str) -> Self {
        Self {
            request_model: request_model.to_string(),
            upstream_model: None,
            upstream_id: None,
            resolved_message_id: None,
            resolved_model: None,
            message_started: false,
            message_stopped: false,
            next_block_index: 0,
            active_block: None,
            tool_states: HashMap::new(),
            accumulated_content: HashMap::new(),
            final_stop_reason: None,
            final_usage: None,
        }
    }

    /// Handles stream payload in this processing pipeline.
    pub(crate) fn on_stream_payload(&mut self, parsed: &Value) -> Vec<(String, Value)> {
        let mut out = Vec::new();
        self.update_common_metadata(parsed);
        if let Some(choices) = parsed.get("choices").and_then(|v| v.as_array()) {
            for (i, choice) in choices.iter().enumerate() {
                self.handle_choice_delta(choice, i, &mut out);
            }
        }
        out
    }

    /// Handles non stream payload in this processing pipeline.
    pub(crate) fn on_non_stream_payload(&mut self, parsed: &Value) -> Vec<(String, Value)> {
        let mut out = Vec::new();
        self.update_common_metadata(parsed);
        self.ensure_message_start(&mut out);

        if let Some(choices) = parsed.get("choices").and_then(|v| v.as_array()) {
            for (i, choice) in choices.iter().enumerate() {
                let choice_index = choice
                    .get("index")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(i as u64) as usize;
                if choice_index != 0 {
                    continue;
                }

                if let Some(message) = choice.get("message").and_then(|v| v.as_object()) {
                    if let Some(content) = message.get("content") {
                        if let Some(text) = extract_openai_message_content_text(content) {
                            self.emit_text_delta(&text, &mut out);
                        }
                    }
                    if let Some(tool_calls) = message.get("tool_calls").and_then(|v| v.as_array()) {
                        for (tc_i, tc) in tool_calls.iter().enumerate() {
                            let normalized = normalize_openai_tool_call_arguments(tc);
                            let tool_index = normalized
                                .get("index")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(tc_i as u64)
                                as usize;
                            self.emit_tool_delta(tool_index, &normalized, &mut out);
                        }
                    }
                }

                self.handle_choice_delta(choice, i, &mut out);
            }
        }
        out
    }

    /// Handles done in this processing pipeline.
    pub(crate) fn on_done(&mut self) -> Vec<(String, Value)> {
        self.emit_final_events()
    }

    /// Performs finish.
    pub(crate) fn finish(&mut self) -> Vec<(String, Value)> {
        self.emit_final_events()
    }

    /// Performs final message JSON.
    pub(crate) fn final_message_json(&self) -> Option<Value> {
        self.build_final_message_json()
    }

    /// Updates common metadata for this module's workflow.
    fn update_common_metadata(&mut self, parsed: &Value) {
        if let Some(id) = parsed.get("id").and_then(|v| v.as_str()) {
            self.upstream_id = Some(id.to_string());
        }
        if let Some(model) = parsed.get("model").and_then(|v| v.as_str()) {
            self.upstream_model = Some(model.to_string());
        }
        if let Some(usage) = parsed.get("usage") {
            if let Some(summary) = extract_openai_usage_summary(usage) {
                self.final_usage = Some(json!({
                    "input_tokens": summary.input_tokens,
                    "output_tokens": summary.output_tokens,
                    "cache_read_input_tokens": summary.cache_read_tokens,
                    "cache_creation_input_tokens": summary.cache_write_tokens,
                }));
            }
        }
    }

    /// Performs handle choice delta.
    fn handle_choice_delta(
        &mut self,
        choice: &Value,
        default_index: usize,
        out: &mut Vec<(String, Value)>,
    ) {
        let choice_index = choice
            .get("index")
            .and_then(|v| v.as_u64())
            .unwrap_or(default_index as u64) as usize;
        if choice_index != 0 {
            return;
        }

        if let Some(delta) = choice.get("delta").and_then(|v| v.as_object()) {
            if let Some(content) = delta.get("content").and_then(|v| v.as_str()) {
                self.emit_text_delta(content, out);
            }
            if let Some(tool_calls) = delta.get("tool_calls").and_then(|v| v.as_array()) {
                for (tc_i, tc) in tool_calls.iter().enumerate() {
                    let tool_index = tc
                        .get("index")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(tc_i as u64) as usize;
                    self.emit_tool_delta(tool_index, tc, out);
                }
            }
        }

        if let Some(finish_reason) = choice.get("finish_reason").and_then(|v| v.as_str()) {
            if !finish_reason.is_empty() {
                self.final_stop_reason =
                    Some(map_openai_finish_reason_to_anthropic_stop(finish_reason).to_string());
                self.ensure_message_start(out);
                self.close_active_block(out);
            }
        }
    }

    /// Emits text delta for this module's workflow.
    fn emit_text_delta(&mut self, content: &str, out: &mut Vec<(String, Value)>) {
        if content.is_empty() {
            return;
        }

        self.ensure_message_start(out);

        if matches!(self.active_block, Some(ActiveBlock::Tool { .. })) {
            self.close_active_block(out);
        }

        let block_index = match self.active_block.clone() {
            Some(ActiveBlock::Text { block_index }) => block_index,
            _ => {
                let index = self.next_block_index;
                self.next_block_index += 1;
                self.accumulated_content
                    .entry(index)
                    .or_insert_with(|| AccumulatedContentBlock::Text {
                        text: String::new(),
                    });
                out.push((
                    "content_block_start".to_string(),
                    json!({
                        "type": "content_block_start",
                        "index": index,
                        "content_block": {
                            "type": "text",
                            "text": "",
                        }
                    }),
                ));
                self.active_block = Some(ActiveBlock::Text { block_index: index });
                index
            }
        };
        if let Some(AccumulatedContentBlock::Text { text }) =
            self.accumulated_content.get_mut(&block_index)
        {
            text.push_str(content);
        }

        out.push((
            "content_block_delta".to_string(),
            json!({
                "type": "content_block_delta",
                "index": block_index,
                "delta": {
                    "type": "text_delta",
                    "text": content,
                }
            }),
        ));
    }

    /// Emits tool delta for this module's workflow.
    fn emit_tool_delta(&mut self, tool_index: usize, chunk: &Value, out: &mut Vec<(String, Value)>) {
        self.ensure_message_start(out);

        if matches!(self.active_block, Some(ActiveBlock::Text { .. })) {
            self.close_active_block(out);
        }

        if !self.tool_states.contains_key(&tool_index) {
            let block_index = self.next_block_index;
            self.next_block_index += 1;
            self.tool_states.insert(
                tool_index,
                ToolState {
                    block_index,
                    id: format!("toolu_{}", Uuid::new_v4().simple()),
                    name: "tool".to_string(),
                    started: false,
                },
            );
        }

        {
            let state = self
                .tool_states
                .get_mut(&tool_index)
                .expect("tool state must exist");
            if let Some(id) = chunk.get("id").and_then(|v| v.as_str()) {
                if !id.is_empty() {
                    state.id = id.to_string();
                }
            }
            if let Some(name) = chunk
                .get("function")
                .and_then(|v| v.get("name"))
                .and_then(|v| v.as_str())
            {
                if !name.is_empty() {
                    state.name = name.to_string();
                }
            }
        }

        let (block_index, tool_id, tool_name, started) = {
            let state = self
                .tool_states
                .get(&tool_index)
                .expect("tool state must exist");
            (
                state.block_index,
                state.id.clone(),
                state.name.clone(),
                state.started,
            )
        };
        self.upsert_tool_content_block(block_index, &tool_id, &tool_name);

        if !started {
            if matches!(
                self.active_block,
                Some(ActiveBlock::Tool {
                    tool_index: active_idx,
                    ..
                }) if active_idx != tool_index
            ) {
                self.close_active_block(out);
            }

            out.push((
                "content_block_start".to_string(),
                json!({
                    "type": "content_block_start",
                    "index": block_index,
                    "content_block": {
                        "type": "tool_use",
                        "id": tool_id,
                        "name": tool_name,
                        "input": {},
                    }
                }),
            ));
            if let Some(state) = self.tool_states.get_mut(&tool_index) {
                state.started = true;
            }
        }

        self.active_block = Some(ActiveBlock::Tool {
            tool_index,
            block_index,
        });

        if let Some(arguments) = chunk
            .get("function")
            .and_then(|v| v.get("arguments"))
            .and_then(|v| v.as_str())
        {
            if !arguments.is_empty() {
                if let Some(AccumulatedContentBlock::ToolUse { partial_input, .. }) =
                    self.accumulated_content.get_mut(&block_index)
                {
                    partial_input.push_str(arguments);
                }
                out.push((
                    "content_block_delta".to_string(),
                    json!({
                        "type": "content_block_delta",
                        "index": block_index,
                        "delta": {
                            "type": "input_json_delta",
                            "partial_json": arguments,
                        }
                    }),
                ));
            }
        }
    }

    /// Performs upsert tool content block.
    fn upsert_tool_content_block(&mut self, block_index: usize, id: &str, name: &str) {
        match self.accumulated_content.get_mut(&block_index) {
            Some(AccumulatedContentBlock::ToolUse {
                id: tool_id,
                name: tool_name,
                ..
            }) => {
                *tool_id = id.to_string();
                *tool_name = name.to_string();
            }
            Some(AccumulatedContentBlock::Text { .. }) | None => {
                self.accumulated_content.insert(
                    block_index,
                    AccumulatedContentBlock::ToolUse {
                        id: id.to_string(),
                        name: name.to_string(),
                        partial_input: String::new(),
                    },
                );
            }
        }
    }

    /// Performs ensure message start.
    fn ensure_message_start(&mut self, out: &mut Vec<(String, Value)>) {
        if self.message_started {
            return;
        }

        let model = if self.request_model.is_empty() {
            self.upstream_model
                .clone()
                .unwrap_or_else(|| "unknown".to_string())
        } else {
            self.request_model.clone()
        };
        self.resolved_model = Some(model.clone());
        let id = self
            .upstream_id
            .clone()
            .unwrap_or_else(|| format!("msg_{}", Uuid::new_v4().simple()));
        self.resolved_message_id = Some(id.clone());

        out.push((
            "message_start".to_string(),
            json!({
                "type": "message_start",
                "message": {
                    "id": id,
                    "type": "message",
                    "role": "assistant",
                    "model": model,
                    "content": [],
                    "stop_reason": Value::Null,
                    "stop_sequence": Value::Null,
                    "usage": {
                        "input_tokens": 0,
                        "output_tokens": 0,
                    }
                }
            }),
        ));
        self.message_started = true;
    }

    /// Performs close active block.
    fn close_active_block(&mut self, out: &mut Vec<(String, Value)>) {
        let Some(active) = self.active_block.clone() else {
            return;
        };
        let index = match active {
            ActiveBlock::Text { block_index } => block_index,
            ActiveBlock::Tool { block_index, .. } => block_index,
        };

        out.push((
            "content_block_stop".to_string(),
            json!({
                "type": "content_block_stop",
                "index": index,
            }),
        ));
        self.active_block = None;
    }

    /// Emits final events for this module's workflow.
    fn emit_final_events(&mut self) -> Vec<(String, Value)> {
        let mut out = Vec::new();
        if self.message_stopped || !self.message_started {
            return out;
        }

        self.close_active_block(&mut out);

        let stop_reason = self
            .final_stop_reason
            .clone()
            .unwrap_or_else(|| "end_turn".to_string());
        let usage = self.final_usage.clone().unwrap_or_else(|| {
            json!({
                "input_tokens": 0,
                "output_tokens": 0,
            })
        });

        out.push((
            "message_delta".to_string(),
            json!({
                "type": "message_delta",
                "delta": {
                    "stop_reason": stop_reason,
                    "stop_sequence": Value::Null,
                },
                "usage": usage,
            }),
        ));
        out.push(("message_stop".to_string(), json!({ "type": "message_stop" })));
        self.message_stopped = true;
        out
    }

    /// Builds final message JSON.
    fn build_final_message_json(&self) -> Option<Value> {
        let id = self.resolved_message_id.as_ref()?;
        let model = self.resolved_model.as_ref()?;
        let mut sorted = self.accumulated_content.iter().collect::<Vec<_>>();
        sorted.sort_by_key(|(index, _)| *index);

        let content = sorted
            .into_iter()
            .filter_map(|(_, block)| match block {
                AccumulatedContentBlock::Text { text } => (!text.is_empty()).then(|| {
                    json!({
                        "type": "text",
                        "text": text,
                    })
                }),
                AccumulatedContentBlock::ToolUse {
                    id,
                    name,
                    partial_input,
                } => {
                    let input = if partial_input.is_empty() {
                        json!({})
                    } else {
                        serde_json::from_str::<Value>(partial_input)
                            .unwrap_or_else(|_| json!({ "raw": partial_input }))
                    };
                    Some(json!({
                        "type": "tool_use",
                        "id": id,
                        "name": name,
                        "input": input,
                    }))
                }
            })
            .collect::<Vec<_>>();

        Some(json!({
            "id": id,
            "type": "message",
            "role": "assistant",
            "model": model,
            "content": content,
            "stop_reason": self
                .final_stop_reason
                .clone()
                .unwrap_or_else(|| "end_turn".to_string()),
            "stop_sequence": Value::Null,
            "usage": self.final_usage.clone().unwrap_or_else(|| {
                json!({
                    "input_tokens": 0,
                    "output_tokens": 0,
                })
            }),
        }))
    }
}

/// Extracts OpenAI message content text for this module's workflow.
fn extract_openai_message_content_text(content: &Value) -> Option<String> {
    if let Some(text) = content.as_str() {
        return (!text.is_empty()).then_some(text.to_string());
    }

    let Some(items) = content.as_array() else {
        return None;
    };

    let mut merged = String::new();
    for item in items {
        if let Some(text) = item.as_str() {
            merged.push_str(text);
            continue;
        }
        if let Some(obj) = item.as_object() {
            let text = obj
                .get("text")
                .or_else(|| obj.get("output_text"))
                .or_else(|| obj.get("input_text"))
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            merged.push_str(text);
        }
    }

    (!merged.is_empty()).then_some(merged)
}

/// Normalizes OpenAI tool call arguments for this module's workflow.
fn normalize_openai_tool_call_arguments(tool_call: &Value) -> Value {
    let mut normalized = tool_call.clone();
    let args = normalized
        .get("function")
        .and_then(|f| f.get("arguments"))
        .cloned();

    if let Some(arguments) = args {
        let as_string = arguments
            .as_str()
            .map(|v| v.to_string())
            .or_else(|| serde_json::to_string(&arguments).ok())
            .unwrap_or_default();
        if normalized.get("function").is_none() || !normalized["function"].is_object() {
            normalized["function"] = json!({});
        }
        normalized["function"]["arguments"] = json!(as_string);
    }

    normalized
}

/// Encodes request for this module's workflow.
pub fn encode_request(request: &CanonicalRequest) -> Value {
    let mut system_chunks = vec![];
    let mut messages = vec![];

    for msg in &request.messages {
        if msg.role == CanonicalRole::System {
            for block in &msg.blocks {
                if let CanonicalBlock::Text(text) = block {
                    if !text.is_empty() {
                        system_chunks.push(text.clone());
                    }
                }
            }
            continue;
        }

        let out_role = match msg.role {
            CanonicalRole::Tool => "user",
            _ => msg.role.as_str(),
        };

        let mut content = vec![];
        for block in &msg.blocks {
            match block {
                CanonicalBlock::Text(text) => push_text_block(&mut content, text),
                CanonicalBlock::ToolUse { id, name, input } => content.push(json!({
                    "type": "tool_use",
                    "id": if id.is_empty() { "toolu_generated" } else { id },
                    "name": name,
                    "input": input,
                })),
                CanonicalBlock::ToolResult {
                    tool_use_id,
                    content: result,
                } => content.push(json!({
                    "type": "tool_result",
                    "tool_use_id": if tool_use_id.is_empty() { "toolu_generated" } else { tool_use_id },
                    "content": result,
                })),
            }
        }

        messages.push(json!({
            "role": out_role,
            "content": content,
        }));
    }

    let mut out = json!({
        "model": request.model,
        "max_tokens": request.max_tokens.clone().unwrap_or_else(|| json!(1024)),
        "temperature": request.temperature.clone().unwrap_or(Value::Null),
        "top_p": request.top_p.clone().unwrap_or(Value::Null),
        "stop_sequences": request.stop.clone().unwrap_or(Value::Null),
        "stream": request.stream,
        "messages": messages,
    });

    if let Some(system) = &request.system {
        out["system"] = system.clone();
    } else if !system_chunks.is_empty() {
        out["system"] = json!(system_chunks.join("\n\n"));
    }

    if let Some(tools) = &request.tools {
        out["tools"] = json!(tools
            .iter()
            .map(|tool| {
                json!({
                    "name": tool.name,
                    "description": tool.description.clone().unwrap_or(Value::Null),
                    "input_schema": tool.input_schema,
                })
            })
            .collect::<Vec<_>>());
    }

    if let Some(choice) = &request.tool_choice {
        let mut out_choice = json!({ "type": choice.kind });
        if let Some(name) = &choice.name {
            out_choice["name"] = json!(name);
        }
        out["tool_choice"] = out_choice;
    }

    if let Some(thinking) = &request.thinking {
        out["thinking"] = thinking.clone();
    }

    if let Some(context_management) = &request.context_management {
        out["context_management"] = context_management.clone();
    }

    out
}

/// Decodes response for this module's workflow.
pub fn decode_response(anthropic_response: &Value, request_model: &str) -> CanonicalResponse {
    let mut text_parts = vec![];
    let mut tool_calls = vec![];
    if let Some(arr) = anthropic_response.get("content").and_then(|v| v.as_array()) {
        for block in arr {
            let block_type = block
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if block_type == "text" {
                text_parts.push(
                    block
                        .get("text")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                );
            }
            if block_type == "tool_use" {
                tool_calls.push(CanonicalToolCall {
                    id: block
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("tool_generated")
                        .to_string(),
                    name: block
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("tool")
                        .to_string(),
                    arguments: serde_json::to_string(block.get("input").unwrap_or(&json!({})))
                        .unwrap_or_else(|_| "{}".to_string()),
                });
            }
        }
    }

    CanonicalResponse {
        id: anthropic_response
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("chatcmpl_generated")
            .to_string(),
        created: chrono::Utc::now().timestamp(),
        model: if request_model.is_empty() {
            anthropic_response
                .get("model")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string()
        } else {
            request_model.to_string()
        },
        text: text_parts.join(""),
        finish_reason: if tool_calls.is_empty() {
            CanonicalFinishReason::Stop
        } else {
            CanonicalFinishReason::ToolUse
        },
        usage: CanonicalUsage {
            input_tokens: anthropic_response
                .get("usage")
                .and_then(|u| u.get("input_tokens"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            output_tokens: anthropic_response
                .get("usage")
                .and_then(|u| u.get("output_tokens"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0),
            total_tokens: None,
        },
        tool_calls,
    }
}

/// Encodes response for this module's workflow.
pub fn encode_response(response: &CanonicalResponse) -> Value {
    let mut content = vec![];
    if !response.text.is_empty() {
        content.push(json!({
            "type": "text",
            "text": response.text,
        }));
    }

    for call in &response.tool_calls {
        content.push(json!({
            "type": "tool_use",
            "id": if call.id.is_empty() { "tool_generated" } else { &call.id },
            "name": call.name,
            "input": serde_json::from_str::<Value>(&call.arguments).unwrap_or_else(|_| json!({})),
        }));
    }

    let stop_reason = match &response.finish_reason {
        CanonicalFinishReason::ToolUse => "tool_use",
        CanonicalFinishReason::MaxTokens => "max_tokens",
        CanonicalFinishReason::Stop => "end_turn",
        CanonicalFinishReason::Other(other) => other.as_str(),
    };

    json!({
        "id": response.id,
        "type": "message",
        "role": "assistant",
        "model": response.model,
        "content": content,
        "stop_reason": stop_reason,
        "usage": {
            "input_tokens": response.usage.input_tokens,
            "output_tokens": response.usage.output_tokens,
        }
    })
}
