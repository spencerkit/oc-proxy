//! Module Overview
//! Streaming bridge for protocol-specific SSE event conversion.
//! Supports:
//! - OpenAI chat-completions SSE -> Anthropic messages SSE.
//! - OpenAI responses SSE -> OpenAI chat-completions SSE.
//! - OpenAI chat-completions SSE -> OpenAI responses SSE.

mod emit;
mod parser;
mod registry;

use self::parser::{SseDataParser, SseFramePayload};
use self::registry::{build_bridge, map_non_stream_via_bridge, DynBridgeAdapter};
use crate::mappers::MapperSurface;
use axum::body::Bytes;
use serde_json::Value;

pub(super) struct StreamBridge {
    parser: SseDataParser,
    adapter: Box<DynBridgeAdapter>,
}

/// Creates a stream bridge instance for a specific source/target protocol pair.
pub(super) fn create_stream_bridge(
    source: MapperSurface,
    target: MapperSurface,
    request_model: &str,
) -> Option<StreamBridge> {
    Some(StreamBridge {
        parser: SseDataParser::default(),
        adapter: build_bridge(source, target, request_model)?,
    })
}

/// Maps a non-stream response through the same bridge registry when supported.
pub(super) fn map_non_stream_response_via_bridge(
    source: MapperSurface,
    target: MapperSurface,
    payload: &Value,
    request_model: &str,
) -> Option<Value> {
    map_non_stream_via_bridge(source, target, payload, request_model)
}

impl StreamBridge {
    /// Consumes one upstream byte chunk, parses SSE frames, and emits mapped downstream frames.
    pub(super) fn consume_chunk(&mut self, chunk: &[u8]) -> Vec<Bytes> {
        let frames = self.parser.consume_chunk(chunk);
        let mut out = Vec::new();
        for frame in frames {
            match &frame.payload {
                SseFramePayload::Json(payload) => {
                    self.adapter
                        .on_json_frame(frame.event.as_deref(), payload, &mut out)
                }
                SseFramePayload::Done => self.adapter.on_done_frame(&mut out),
            }
        }
        out
    }

    /// Flushes parser remainder and adapter finalization output at end of stream.
    pub(super) fn finish(&mut self) -> Vec<Bytes> {
        let mut out = Vec::new();

        for frame in self.parser.drain_remainder() {
            match &frame.payload {
                SseFramePayload::Json(payload) => {
                    self.adapter
                        .on_json_frame(frame.event.as_deref(), payload, &mut out)
                }
                SseFramePayload::Done => self.adapter.on_done_frame(&mut out),
            }
        }

        self.adapter.finish(&mut out);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::{create_stream_bridge, map_non_stream_response_via_bridge};
    use crate::mappers::MapperSurface;
    use serde_json::json;

    #[test]
    /// Verifies chat-completions stream can be converted to Anthropic SSE events.
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
    /// Verifies tool-call deltas are mapped to Anthropic tool_use event sequence.
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
    /// Verifies parser handles split lines and trailing remainder correctly.
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
    /// Verifies Responses SSE events are transformed into chat chunk stream.
    fn bridge_responses_stream_to_chat_chunks() {
        let mut bridge = create_stream_bridge(
            MapperSurface::OpenaiResponses,
            MapperSurface::OpenaiChatCompletions,
            "gpt-target",
        )
        .expect("bridge should exist");

        let input = concat!(
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"hel\",\"response\":{\"id\":\"resp_1\",\"model\":\"gpt-up\",\"created_at\":123}}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"lo\"}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"status\":\"completed\",\"usage\":{\"input_tokens\":10,\"output_tokens\":2}}}\n\n"
        );

        let mut out = bridge.consume_chunk(input.as_bytes());
        out.extend(bridge.finish());
        let combined = out
            .iter()
            .map(|b| String::from_utf8_lossy(b.as_ref()).to_string())
            .collect::<Vec<_>>()
            .join("");

        assert!(combined.contains("chat.completion.chunk"), "{combined}");
        assert!(combined.contains("\"content\":\"hel\""), "{combined}");
        assert!(combined.contains("\"content\":\"lo\""), "{combined}");
        assert!(combined.contains("\"finish_reason\":\"stop\""), "{combined}");
        assert!(combined.contains("\"prompt_tokens\":10"), "{combined}");
        assert!(combined.contains("data: [DONE]"), "{combined}");
    }

    #[test]
    /// Verifies chat chunk stream can be transformed into Responses event stream.
    fn bridge_chat_stream_to_responses_events() {
        let mut bridge = create_stream_bridge(
            MapperSurface::OpenaiChatCompletions,
            MapperSurface::OpenaiResponses,
            "gpt-target",
        )
        .expect("bridge should exist");

        let input = concat!(
            "data: {\"id\":\"chatcmpl_9\",\"model\":\"gpt-up\",\"created\":456,\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hello\"},\"finish_reason\":null}],\"usage\":null}\n\n",
            "data: {\"id\":\"chatcmpl_9\",\"model\":\"gpt-up\",\"created\":456,\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":1}}\n\n",
            "data: [DONE]\n\n"
        );

        let mut out = bridge.consume_chunk(input.as_bytes());
        out.extend(bridge.finish());
        let combined = out
            .iter()
            .map(|b| String::from_utf8_lossy(b.as_ref()).to_string())
            .collect::<Vec<_>>()
            .join("");

        assert!(combined.contains("event: response.output_text.delta"), "{combined}");
        assert!(combined.contains("\"delta\":\"hello\""), "{combined}");
        assert!(combined.contains("event: response.completed"), "{combined}");
        assert!(combined.contains("\"object\":\"response\""), "{combined}");
        assert!(combined.contains("\"input_tokens\":5"), "{combined}");
        assert!(combined.contains("data: [DONE]"), "{combined}");
    }

    #[test]
    /// Verifies non-stream chat response can map to Anthropic message JSON.
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
    /// Verifies non-stream chat tool-calls can map to Anthropic tool_use JSON.
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

    #[test]
    /// Verifies non-stream Responses payload can map to chat completion JSON.
    fn non_stream_bridge_maps_responses_to_chat() {
        let out = map_non_stream_response_via_bridge(
            MapperSurface::OpenaiResponses,
            MapperSurface::OpenaiChatCompletions,
            &json!({
                "id": "resp_1",
                "object": "response",
                "model": "gpt-upstream",
                "status": "completed",
                "output": [{
                    "type": "message",
                    "role": "assistant",
                    "content": [{"type":"output_text","text":"hello"}]
                }],
                "usage": {
                    "input_tokens": 10,
                    "output_tokens": 2,
                    "total_tokens": 12
                }
            }),
            "gpt-target",
        );
        let out = out.expect("bridge should map non-stream responses to chat");
        assert_eq!(out["object"], "chat.completion");
        assert_eq!(out["model"], "gpt-target");
    }

    #[test]
    /// Verifies stream bridge emits exactly one terminal `[DONE]` marker.
    fn stream_bridge_emits_done_once_for_chat_to_responses() {
        let mut bridge = create_stream_bridge(
            MapperSurface::OpenaiChatCompletions,
            MapperSurface::OpenaiResponses,
            "gpt-target",
        )
        .expect("bridge should exist");

        let input = concat!(
            "data: {\"id\":\"chatcmpl_9\",\"model\":\"gpt-up\",\"created\":456,\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hello\"},\"finish_reason\":null}],\"usage\":null}\n\n",
            "data: [DONE]\n\n"
        );

        let mut out = bridge.consume_chunk(input.as_bytes());
        out.extend(bridge.finish());
        let combined = out
            .iter()
            .map(|b| String::from_utf8_lossy(b.as_ref()).to_string())
            .collect::<Vec<_>>()
            .join("");

        let done_count = combined.matches("data: [DONE]").count();
        assert_eq!(done_count, 1, "{combined}");
    }
}
