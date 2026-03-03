//! Module Overview
//! Streaming bridge for protocol-specific SSE event conversion.
//! Currently supports OpenAI chat-completions SSE -> Anthropic messages SSE.

use crate::mappers::helpers::{
    extract_openai_usage_summary, map_openai_finish_reason_to_anthropic_stop,
};
use crate::mappers::MapperSurface;
use axum::body::Bytes;
use serde_json::{json, Value};
use std::collections::HashMap;
use uuid::Uuid;

type DynBridgeAdapter = dyn BridgeAdapter + Send;
type BridgeBuilder = fn(&str) -> Box<DynBridgeAdapter>;

const BRIDGE_REGISTRY: &[(MapperSurface, MapperSurface, BridgeBuilder)] = &[(
    MapperSurface::OpenaiChatCompletions,
    MapperSurface::AnthropicMessages,
    build_openai_chat_to_anthropic_bridge,
)];

pub(super) struct StreamBridge {
    parser: SseDataParser,
    adapter: Box<DynBridgeAdapter>,
}

pub(super) fn create_stream_bridge(
    source: MapperSurface,
    target: MapperSurface,
    request_model: &str,
) -> Option<StreamBridge> {
    let builder = BRIDGE_REGISTRY
        .iter()
        .find_map(|(src, tgt, build)| ((*src == source) && (*tgt == target)).then_some(*build))?;

    Some(StreamBridge {
        parser: SseDataParser::default(),
        adapter: builder(request_model),
    })
}

pub(super) fn map_non_stream_response_via_bridge(
    source: MapperSurface,
    target: MapperSurface,
    payload: &Value,
    request_model: &str,
) -> Option<Value> {
    let builder = BRIDGE_REGISTRY
        .iter()
        .find_map(|(src, tgt, build)| ((*src == source) && (*tgt == target)).then_some(*build))?;

    let mut adapter = builder(request_model);
    let mut sink = Vec::new();
    adapter.on_single_response_json(payload, &mut sink);
    adapter.finish(&mut sink);
    adapter.final_response_json()
}

impl StreamBridge {
    pub(super) fn consume_chunk(&mut self, chunk: &[u8]) -> Vec<Bytes> {
        let frames = self.parser.consume_chunk(chunk);
        let mut out = Vec::new();
        for frame in frames {
            match frame {
                SseDataFrame::Json(payload) => self.adapter.on_json_frame(&payload, &mut out),
                SseDataFrame::Done => self.adapter.on_done_frame(&mut out),
            }
        }
        out
    }

    pub(super) fn finish(&mut self) -> Vec<Bytes> {
        let mut out = Vec::new();

        // Handle potential last data line without trailing newline.
        for frame in self.parser.drain_remainder() {
            match frame {
                SseDataFrame::Json(payload) => self.adapter.on_json_frame(&payload, &mut out),
                SseDataFrame::Done => self.adapter.on_done_frame(&mut out),
            }
        }

        self.adapter.finish(&mut out);
        out
    }
}

trait BridgeAdapter {
    fn on_json_frame(&mut self, payload: &Value, out: &mut Vec<Bytes>);
    fn on_done_frame(&mut self, out: &mut Vec<Bytes>);
    fn finish(&mut self, out: &mut Vec<Bytes>);
    fn on_single_response_json(&mut self, _payload: &Value, _out: &mut Vec<Bytes>) {}
    fn final_response_json(&self) -> Option<Value> {
        None
    }
}

#[derive(Default)]
struct SseDataParser {
    line_buffer: String,
}

enum SseDataFrame {
    Json(Value),
    Done,
}

impl SseDataParser {
    fn consume_chunk(&mut self, chunk: &[u8]) -> Vec<SseDataFrame> {
        self.line_buffer.push_str(&String::from_utf8_lossy(chunk));
        let mut out = Vec::new();

        while let Some(newline_idx) = self.line_buffer.find('\n') {
            let mut line = self.line_buffer[..newline_idx].to_string();
            if line.ends_with('\r') {
                let _ = line.pop();
            }
            if let Some(frame) = Self::parse_line(&line) {
                out.push(frame);
            }
            self.line_buffer.drain(..=newline_idx);
        }

        out
    }

    fn drain_remainder(&mut self) -> Vec<SseDataFrame> {
        if self.line_buffer.is_empty() {
            return Vec::new();
        }

        let mut line = std::mem::take(&mut self.line_buffer);
        if line.ends_with('\r') {
            let _ = line.pop();
        }

        match Self::parse_line(&line) {
            Some(frame) => vec![frame],
            None => Vec::new(),
        }
    }

    fn parse_line(line: &str) -> Option<SseDataFrame> {
        let rest = line.strip_prefix("data:")?;
        let payload = rest.trim_start();

        if payload.is_empty() {
            return None;
        }
        if payload == "[DONE]" {
            return Some(SseDataFrame::Done);
        }

        serde_json::from_str::<Value>(payload)
            .ok()
            .map(SseDataFrame::Json)
    }
}

fn build_openai_chat_to_anthropic_bridge(request_model: &str) -> Box<DynBridgeAdapter> {
    Box::new(OpenaiChatToAnthropicBridge::new(request_model))
}

#[derive(Clone)]
enum ActiveBlock {
    Text {
        block_index: usize,
    },
    Tool {
        tool_index: usize,
        block_index: usize,
    },
}

#[derive(Clone)]
struct ToolState {
    block_index: usize,
    id: String,
    name: String,
    started: bool,
}

enum AccumulatedContentBlock {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        partial_input: String,
    },
}

struct OpenaiChatToAnthropicBridge {
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

impl OpenaiChatToAnthropicBridge {
    fn new(request_model: &str) -> Self {
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

    fn update_common_metadata(&mut self, parsed: &Value) {
        if let Some(id) = parsed.get("id").and_then(|v| v.as_str()) {
            self.upstream_id = Some(id.to_string());
        }
        if let Some(model) = parsed.get("model").and_then(|v| v.as_str()) {
            self.upstream_model = Some(model.to_string());
        }
        if let Some(usage) = parsed.get("usage") {
            if let Some(norm) = normalize_openai_usage_to_anthropic(usage) {
                self.final_usage = Some(norm);
            }
        }
    }

    fn handle_choice_delta(&mut self, choice: &Value, default_index: usize, out: &mut Vec<Bytes>) {
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

    fn handle_json_payload(&mut self, parsed: &Value, out: &mut Vec<Bytes>) {
        self.update_common_metadata(parsed);

        if let Some(choices) = parsed.get("choices").and_then(|v| v.as_array()) {
            for (i, choice) in choices.iter().enumerate() {
                self.handle_choice_delta(choice, i, out);
            }
        }
    }

    fn handle_non_stream_payload(&mut self, parsed: &Value, out: &mut Vec<Bytes>) {
        self.update_common_metadata(parsed);
        self.ensure_message_start(out);

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
                            self.emit_text_delta(&text, out);
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
                            self.emit_tool_delta(tool_index, &normalized, out);
                        }
                    }
                }

                // Keep compatibility if upstream accidentally returns streaming-style delta object.
                self.handle_choice_delta(choice, i, out);
            }
        }
    }

    fn emit_text_delta(&mut self, content: &str, out: &mut Vec<Bytes>) {
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
                self.accumulated_content.entry(index).or_insert_with(|| {
                    AccumulatedContentBlock::Text {
                        text: String::new(),
                    }
                });
                push_sse_event(
                    out,
                    "content_block_start",
                    &json!({
                        "type": "content_block_start",
                        "index": index,
                        "content_block": {
                            "type": "text",
                            "text": "",
                        }
                    }),
                );
                self.active_block = Some(ActiveBlock::Text { block_index: index });
                index
            }
        };
        if let Some(AccumulatedContentBlock::Text { text }) =
            self.accumulated_content.get_mut(&block_index)
        {
            text.push_str(content);
        }

        push_sse_event(
            out,
            "content_block_delta",
            &json!({
                "type": "content_block_delta",
                "index": block_index,
                "delta": {
                    "type": "text_delta",
                    "text": content,
                }
            }),
        );
    }

    fn emit_tool_delta(&mut self, tool_index: usize, chunk: &Value, out: &mut Vec<Bytes>) {
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

            push_sse_event(
                out,
                "content_block_start",
                &json!({
                    "type": "content_block_start",
                    "index": block_index,
                    "content_block": {
                        "type": "tool_use",
                        "id": tool_id,
                        "name": tool_name,
                        "input": {},
                    }
                }),
            );
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
                push_sse_event(
                    out,
                    "content_block_delta",
                    &json!({
                        "type": "content_block_delta",
                        "index": block_index,
                        "delta": {
                            "type": "input_json_delta",
                            "partial_json": arguments,
                        }
                    }),
                );
            }
        }
    }

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

    fn ensure_message_start(&mut self, out: &mut Vec<Bytes>) {
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

        push_sse_event(
            out,
            "message_start",
            &json!({
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
        );
        self.message_started = true;
    }

    fn close_active_block(&mut self, out: &mut Vec<Bytes>) {
        let Some(active) = self.active_block.clone() else {
            return;
        };
        let index = match active {
            ActiveBlock::Text { block_index } => block_index,
            ActiveBlock::Tool { block_index, .. } => block_index,
        };

        push_sse_event(
            out,
            "content_block_stop",
            &json!({
                "type": "content_block_stop",
                "index": index,
            }),
        );
        self.active_block = None;
    }

    fn emit_final_events(&mut self, out: &mut Vec<Bytes>) {
        if self.message_stopped {
            return;
        }
        if !self.message_started {
            return;
        }

        self.close_active_block(out);

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

        push_sse_event(
            out,
            "message_delta",
            &json!({
                "type": "message_delta",
                "delta": {
                    "stop_reason": stop_reason,
                    "stop_sequence": Value::Null,
                },
                "usage": usage,
            }),
        );
        push_sse_event(out, "message_stop", &json!({ "type": "message_stop" }));
        self.message_stopped = true;
    }

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

impl BridgeAdapter for OpenaiChatToAnthropicBridge {
    fn on_json_frame(&mut self, payload: &Value, out: &mut Vec<Bytes>) {
        self.handle_json_payload(payload, out);
    }

    fn on_done_frame(&mut self, out: &mut Vec<Bytes>) {
        self.emit_final_events(out);
    }

    fn finish(&mut self, out: &mut Vec<Bytes>) {
        self.emit_final_events(out);
    }

    fn on_single_response_json(&mut self, payload: &Value, out: &mut Vec<Bytes>) {
        self.handle_non_stream_payload(payload, out);
        self.emit_final_events(out);
    }

    fn final_response_json(&self) -> Option<Value> {
        self.build_final_message_json()
    }
}

fn push_sse_event(out: &mut Vec<Bytes>, event: &str, payload: &Value) {
    out.push(encode_sse_json_event(event, payload));
}

fn encode_sse_json_event(event: &str, payload: &Value) -> Bytes {
    Bytes::from(format!("event: {event}\ndata: {}\n\n", payload))
}

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

fn normalize_openai_usage_to_anthropic(usage: &Value) -> Option<Value> {
    let summary = extract_openai_usage_summary(usage)?;
    Some(json!({
        "input_tokens": summary.input_tokens,
        "output_tokens": summary.output_tokens,
        "cache_read_input_tokens": summary.cache_read_tokens,
        "cache_creation_input_tokens": summary.cache_write_tokens,
    }))
}

#[cfg(test)]
mod tests {
    use super::{create_stream_bridge, map_non_stream_response_via_bridge};
    use crate::mappers::MapperSurface;
    use serde_json::json;

    #[test]
    fn bridge_openai_chat_stream_to_anthropic_events() {
        let mut bridge = create_stream_bridge(
            MapperSurface::OpenaiChatCompletions,
            MapperSurface::AnthropicMessages,
            "claude-target",
        )
        .expect("bridge should exist");
        let input = concat!(
            "data: {\"id\":\"chatcmpl_1\",\"model\":\"gpt-x\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"hel\"},\"finish_reason\":null}],\"usage\":null}\n\n",
            "data: {\"id\":\"chatcmpl_1\",\"model\":\"gpt-x\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"lo\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":2}}\n\n",
            "data: [DONE]\n\n"
        );

        let mut out = bridge.consume_chunk(input.as_bytes());
        out.extend(bridge.finish());
        let combined = out
            .iter()
            .map(|b| String::from_utf8_lossy(b.as_ref()).to_string())
            .collect::<Vec<_>>()
            .join("");

        assert!(combined.contains("event: message_start"));
        assert!(combined.contains("text_delta"), "{combined}");
        assert!(combined.contains("hel"), "{combined}");
        assert!(combined.contains("lo"), "{combined}");
        assert!(combined.contains("event: message_delta"));
        assert!(combined.contains("\"stop_reason\":\"end_turn\""));
        assert!(combined.contains("\"input_tokens\":10"));
        assert!(combined.contains("\"output_tokens\":2"));
        assert!(combined.contains("event: message_stop"));
    }

    #[test]
    fn bridge_openai_tool_calls_to_anthropic_tool_use_events() {
        let mut bridge = create_stream_bridge(
            MapperSurface::OpenaiChatCompletions,
            MapperSurface::AnthropicMessages,
            "claude-target",
        )
        .expect("bridge should exist");
        let input = concat!(
            "data: {\"id\":\"chatcmpl_2\",\"model\":\"gpt-x\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"type\":\"function\",\"function\":{\"name\":\"lookup\",\"arguments\":\"{\\\"city\\\":\"}}]},\"finish_reason\":null}],\"usage\":null}\n\n",
            "data: {\"id\":\"chatcmpl_2\",\"model\":\"gpt-x\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\"sf\\\"}\"}}]},\"finish_reason\":\"tool_calls\"}],\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":1}}\n\n",
            "data: [DONE]\n\n"
        );

        let out = bridge.consume_chunk(input.as_bytes());
        let combined = out
            .iter()
            .map(|b| String::from_utf8_lossy(b.as_ref()).to_string())
            .collect::<Vec<_>>()
            .join("");

        assert!(combined.contains("event: content_block_start"));
        assert!(combined.contains("\"type\":\"tool_use\""));
        assert!(combined.contains("\"name\":\"lookup\""));
        assert!(combined.contains("\"type\":\"input_json_delta\""));
        assert!(combined.contains("\"stop_reason\":\"tool_use\""));
    }

    #[test]
    fn bridge_parser_handles_split_lines_without_newline() {
        let mut bridge = create_stream_bridge(
            MapperSurface::OpenaiChatCompletions,
            MapperSurface::AnthropicMessages,
            "claude-target",
        )
        .expect("bridge should exist");

        let part1 = "data: {\"id\":\"chatcmpl_split\",\"model\":\"gpt-x\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"he";
        let part2 = "llo\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":1,\"completion_tokens\":1}}";

        let out1 = bridge.consume_chunk(part1.as_bytes());
        assert!(out1.is_empty());
        let out2 = bridge.consume_chunk(part2.as_bytes());
        assert!(out2.is_empty());

        let out3 = bridge.finish();
        let combined = out3
            .iter()
            .map(|b| String::from_utf8_lossy(b.as_ref()).to_string())
            .collect::<Vec<_>>()
            .join("");

        assert!(combined.contains("hello"), "{combined}");
        assert!(combined.contains("event: message_stop"), "{combined}");
    }

    #[test]
    fn non_stream_bridge_maps_openai_chat_text_response() {
        let out = map_non_stream_response_via_bridge(
            MapperSurface::OpenaiChatCompletions,
            MapperSurface::AnthropicMessages,
            &json!({
                "id": "chatcmpl_non_stream_1",
                "model": "gpt-upstream",
                "choices": [
                    {
                        "index": 0,
                        "message": {
                            "role": "assistant",
                            "content": "hello"
                        },
                        "finish_reason": "stop"
                    }
                ],
                "usage": {
                    "prompt_tokens": 9,
                    "completion_tokens": 4
                }
            }),
            "claude-target",
        )
        .expect("non-stream bridge should produce output");

        assert_eq!(out["id"], "chatcmpl_non_stream_1");
        assert_eq!(out["model"], "claude-target");
        assert_eq!(out["content"][0]["type"], "text");
        assert_eq!(out["content"][0]["text"], "hello");
        assert_eq!(out["stop_reason"], "end_turn");
        assert_eq!(out["usage"]["input_tokens"], 9);
        assert_eq!(out["usage"]["output_tokens"], 4);
    }

    #[test]
    fn non_stream_bridge_maps_openai_chat_tool_calls_response() {
        let out = map_non_stream_response_via_bridge(
            MapperSurface::OpenaiChatCompletions,
            MapperSurface::AnthropicMessages,
            &json!({
                "id": "chatcmpl_non_stream_2",
                "model": "gpt-upstream",
                "choices": [
                    {
                        "index": 0,
                        "message": {
                            "role": "assistant",
                            "tool_calls": [
                                {
                                    "id": "call_1",
                                    "index": 0,
                                    "type": "function",
                                    "function": {
                                        "name": "lookup",
                                        "arguments": "{\"city\":\"sf\"}"
                                    }
                                }
                            ]
                        },
                        "finish_reason": "tool_calls"
                    }
                ],
                "usage": {
                    "prompt_tokens": 3,
                    "completion_tokens": 1
                }
            }),
            "claude-target",
        )
        .expect("non-stream bridge should produce output");

        assert_eq!(out["content"][0]["type"], "tool_use");
        assert_eq!(out["content"][0]["id"], "call_1");
        assert_eq!(out["content"][0]["name"], "lookup");
        assert_eq!(out["content"][0]["input"]["city"], "sf");
        assert_eq!(out["stop_reason"], "tool_use");
        assert_eq!(out["usage"]["input_tokens"], 3);
        assert_eq!(out["usage"]["output_tokens"], 1);
    }
}
