//! Module Overview
//! Streaming bridge for protocol-specific SSE event conversion.
//! Currently supports OpenAI chat-completions SSE -> Anthropic messages SSE.

use crate::mappers::{MapperSurface, OpenaiChatToAnthropicStreamMapper};
use axum::body::Bytes;
use serde_json::Value;

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
    Box::new(OpenaiChatToAnthropicBridgeAdapter::new(request_model))
}

struct OpenaiChatToAnthropicBridgeAdapter {
    mapper: OpenaiChatToAnthropicStreamMapper,
}

impl OpenaiChatToAnthropicBridgeAdapter {
    fn new(request_model: &str) -> Self {
        Self {
            mapper: OpenaiChatToAnthropicStreamMapper::new(request_model),
        }
    }
}

impl BridgeAdapter for OpenaiChatToAnthropicBridgeAdapter {
    fn on_json_frame(&mut self, payload: &Value, out: &mut Vec<Bytes>) {
        for (event, payload) in self.mapper.on_stream_payload(payload) {
            push_sse_event(out, &event, &payload);
        }
    }

    fn on_done_frame(&mut self, out: &mut Vec<Bytes>) {
        for (event, payload) in self.mapper.on_done() {
            push_sse_event(out, &event, &payload);
        }
    }

    fn finish(&mut self, out: &mut Vec<Bytes>) {
        for (event, payload) in self.mapper.finish() {
            push_sse_event(out, &event, &payload);
        }
    }

    fn on_single_response_json(&mut self, payload: &Value, out: &mut Vec<Bytes>) {
        for (event, payload) in self.mapper.on_non_stream_payload(payload) {
            push_sse_event(out, &event, &payload);
        }
        for (event, payload) in self.mapper.finish() {
            push_sse_event(out, &event, &payload);
        }
    }

    fn final_response_json(&self) -> Option<Value> {
        self.mapper.final_message_json()
    }
}

fn push_sse_event(out: &mut Vec<Bytes>, event: &str, payload: &Value) {
    out.push(encode_sse_json_event(event, payload));
}

fn encode_sse_json_event(event: &str, payload: &Value) -> Bytes {
    Bytes::from(format!("event: {event}\ndata: {}\n\n", payload))
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
