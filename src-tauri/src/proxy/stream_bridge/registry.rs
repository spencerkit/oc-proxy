use super::emit::{push_sse_data_json, push_sse_done, push_sse_event};
use crate::mappers::{
    map_response_by_surface, MapperSurface, OpenaiChatToAnthropicStreamMapper,
    OpenaiChatToResponsesStreamMapper, OpenaiResponsesToAnthropicStreamMapper,
    OpenaiResponsesToChatStreamMapper,
};
use axum::body::Bytes;
use serde_json::Value;

pub(super) type DynBridgeAdapter = dyn BridgeAdapter + Send;
type BridgeBuilder = fn(&str) -> Box<DynBridgeAdapter>;

const BRIDGE_REGISTRY: &[(MapperSurface, MapperSurface, BridgeBuilder)] = &[
    (
        MapperSurface::OpenaiChatCompletions,
        MapperSurface::AnthropicMessages,
        build_openai_chat_to_anthropic_bridge,
    ),
    (
        MapperSurface::OpenaiResponses,
        MapperSurface::OpenaiChatCompletions,
        build_openai_responses_to_chat_bridge,
    ),
    (
        MapperSurface::OpenaiChatCompletions,
        MapperSurface::OpenaiResponses,
        build_openai_chat_to_responses_bridge,
    ),
    (
        MapperSurface::OpenaiResponses,
        MapperSurface::AnthropicMessages,
        build_openai_responses_to_anthropic_bridge,
    ),
];

/// Builds a bridge adapter for a specific source/target surface pair.
pub(super) fn build_bridge(
    source: MapperSurface,
    target: MapperSurface,
    request_model: &str,
) -> Option<Box<DynBridgeAdapter>> {
    let builder = BRIDGE_REGISTRY
        .iter()
        .find_map(|(src, tgt, build)| ((*src == source) && (*tgt == target)).then_some(*build))?;
    Some(builder(request_model))
}

/// Maps a non-stream payload via bridge adapter when the pair is registered.
pub(super) fn map_non_stream_via_bridge(
    source: MapperSurface,
    target: MapperSurface,
    payload: &Value,
    request_model: &str,
) -> Option<Value> {
    let mut adapter = build_bridge(source, target, request_model)?;
    let mut sink = Vec::new();
    adapter.on_single_response_json(payload, &mut sink);
    adapter.finish(&mut sink);
    adapter.final_response_json()
}

/// Common adapter contract for streaming and non-stream bridge conversions.
pub(super) trait BridgeAdapter {
    /// Handles one parsed JSON SSE frame.
    fn on_json_frame(&mut self, event: Option<&str>, payload: &Value, out: &mut Vec<Bytes>);
    /// Handles one parsed `[DONE]` SSE frame.
    fn on_done_frame(&mut self, out: &mut Vec<Bytes>);
    /// Emits any final trailing frames.
    fn finish(&mut self, out: &mut Vec<Bytes>);
    /// Handles a full non-stream JSON payload.
    fn on_single_response_json(&mut self, _payload: &Value, _out: &mut Vec<Bytes>) {}
    /// Returns final non-stream JSON output when the adapter supports it.
    fn final_response_json(&self) -> Option<Value> {
        None
    }
}

/// Constructs `OpenAI Chat -> Anthropic Messages` adapter.
fn build_openai_chat_to_anthropic_bridge(request_model: &str) -> Box<DynBridgeAdapter> {
    Box::new(OpenaiChatToAnthropicBridgeAdapter::new(request_model))
}

/// Constructs `OpenAI Responses -> OpenAI Chat` adapter.
fn build_openai_responses_to_chat_bridge(request_model: &str) -> Box<DynBridgeAdapter> {
    Box::new(OpenaiResponsesToChatBridgeAdapter::new(request_model))
}

/// Constructs `OpenAI Chat -> OpenAI Responses` adapter.
fn build_openai_chat_to_responses_bridge(request_model: &str) -> Box<DynBridgeAdapter> {
    Box::new(OpenaiChatToResponsesBridgeAdapter::new(request_model))
}

/// Constructs `OpenAI Responses -> Anthropic Messages` adapter.
fn build_openai_responses_to_anthropic_bridge(request_model: &str) -> Box<DynBridgeAdapter> {
    Box::new(OpenaiResponsesToAnthropicBridgeAdapter::new(request_model))
}

struct OpenaiChatToAnthropicBridgeAdapter {
    mapper: OpenaiChatToAnthropicStreamMapper,
}

impl OpenaiChatToAnthropicBridgeAdapter {
    /// Creates adapter and initializes its dedicated stream mapper.
    fn new(request_model: &str) -> Self {
        Self {
            mapper: OpenaiChatToAnthropicStreamMapper::new(request_model),
        }
    }
}

impl BridgeAdapter for OpenaiChatToAnthropicBridgeAdapter {
    /// Converts one chat-completions SSE JSON frame into Anthropic message events.
    fn on_json_frame(&mut self, _event: Option<&str>, payload: &Value, out: &mut Vec<Bytes>) {
        for (event, payload) in self.mapper.on_stream_payload(payload) {
            push_sse_event(out, &event, &payload);
        }
    }

    /// Converts upstream stream completion marker into Anthropic terminal events.
    fn on_done_frame(&mut self, out: &mut Vec<Bytes>) {
        for (event, payload) in self.mapper.on_done() {
            push_sse_event(out, &event, &payload);
        }
    }

    /// Flushes final Anthropic events on stream teardown.
    fn finish(&mut self, out: &mut Vec<Bytes>) {
        for (event, payload) in self.mapper.finish() {
            push_sse_event(out, &event, &payload);
        }
    }

    /// Converts a full non-stream chat response into Anthropic event sequence.
    fn on_single_response_json(&mut self, payload: &Value, out: &mut Vec<Bytes>) {
        for (event, payload) in self.mapper.on_non_stream_payload(payload) {
            push_sse_event(out, &event, &payload);
        }
        for (event, payload) in self.mapper.finish() {
            push_sse_event(out, &event, &payload);
        }
    }

    /// Returns final Anthropic non-stream message JSON.
    fn final_response_json(&self) -> Option<Value> {
        self.mapper.final_message_json()
    }
}

struct OpenaiResponsesToChatBridgeAdapter {
    mapper: OpenaiResponsesToChatStreamMapper,
    done_sent: bool,
    request_model: String,
    non_stream_output: Option<Value>,
}

impl OpenaiResponsesToChatBridgeAdapter {
    /// Creates adapter and tracks stream completion state.
    fn new(request_model: &str) -> Self {
        Self {
            mapper: OpenaiResponsesToChatStreamMapper::new(request_model),
            done_sent: false,
            request_model: request_model.to_string(),
            non_stream_output: None,
        }
    }
}

impl BridgeAdapter for OpenaiResponsesToChatBridgeAdapter {
    /// Converts one responses SSE JSON frame into chat-completion chunk JSON.
    fn on_json_frame(&mut self, event: Option<&str>, payload: &Value, out: &mut Vec<Bytes>) {
        for chunk in self.mapper.on_stream_payload(event, payload) {
            push_sse_data_json(out, &chunk);
        }
    }

    /// Flushes mapper output when upstream sends `[DONE]`.
    fn on_done_frame(&mut self, out: &mut Vec<Bytes>) {
        for chunk in self.mapper.on_done() {
            push_sse_data_json(out, &chunk);
        }
    }

    /// Emits final chunk(s) and a single terminal `[DONE]` frame.
    fn finish(&mut self, out: &mut Vec<Bytes>) {
        for chunk in self.mapper.finish() {
            push_sse_data_json(out, &chunk);
        }
        if !self.done_sent {
            push_sse_done(out);
            self.done_sent = true;
        }
    }

    /// Maps non-stream responses payload through canonical mapper path.
    fn on_single_response_json(&mut self, payload: &Value, _out: &mut Vec<Bytes>) {
        self.non_stream_output = Some(map_response_by_surface(
            MapperSurface::OpenaiResponses,
            MapperSurface::OpenaiChatCompletions,
            payload,
            &self.request_model,
        ));
        self.done_sent = true;
    }

    /// Returns cached non-stream mapped output.
    fn final_response_json(&self) -> Option<Value> {
        self.non_stream_output.clone()
    }
}

struct OpenaiChatToResponsesBridgeAdapter {
    mapper: OpenaiChatToResponsesStreamMapper,
    done_sent: bool,
    request_model: String,
    non_stream_output: Option<Value>,
}

impl OpenaiChatToResponsesBridgeAdapter {
    /// Creates adapter and tracks stream completion state.
    fn new(request_model: &str) -> Self {
        Self {
            mapper: OpenaiChatToResponsesStreamMapper::new(request_model),
            done_sent: false,
            request_model: request_model.to_string(),
            non_stream_output: None,
        }
    }
}

impl BridgeAdapter for OpenaiChatToResponsesBridgeAdapter {
    /// Converts one chat-completions SSE JSON frame into responses events.
    fn on_json_frame(&mut self, _event: Option<&str>, payload: &Value, out: &mut Vec<Bytes>) {
        for (event, payload) in self.mapper.on_stream_payload(payload) {
            push_sse_event(out, &event, &payload);
        }
    }

    /// Flushes mapper output when upstream sends `[DONE]`.
    fn on_done_frame(&mut self, out: &mut Vec<Bytes>) {
        for (event, payload) in self.mapper.on_done() {
            push_sse_event(out, &event, &payload);
        }
    }

    /// Emits final event(s) and a single terminal `[DONE]` frame.
    fn finish(&mut self, out: &mut Vec<Bytes>) {
        for (event, payload) in self.mapper.finish() {
            push_sse_event(out, &event, &payload);
        }
        if !self.done_sent {
            push_sse_done(out);
            self.done_sent = true;
        }
    }

    /// Maps non-stream chat payload through canonical mapper path.
    fn on_single_response_json(&mut self, payload: &Value, _out: &mut Vec<Bytes>) {
        self.non_stream_output = Some(map_response_by_surface(
            MapperSurface::OpenaiChatCompletions,
            MapperSurface::OpenaiResponses,
            payload,
            &self.request_model,
        ));
        self.done_sent = true;
    }

    /// Returns cached non-stream mapped output.
    fn final_response_json(&self) -> Option<Value> {
        self.non_stream_output.clone()
    }
}

struct OpenaiResponsesToAnthropicBridgeAdapter {
    mapper: OpenaiResponsesToAnthropicStreamMapper,
}

impl OpenaiResponsesToAnthropicBridgeAdapter {
    /// Creates adapter and initializes its dedicated stream mapper.
    fn new(request_model: &str) -> Self {
        Self {
            mapper: OpenaiResponsesToAnthropicStreamMapper::new(request_model),
        }
    }
}

impl BridgeAdapter for OpenaiResponsesToAnthropicBridgeAdapter {
    /// Converts one responses SSE JSON frame into Anthropic message events.
    fn on_json_frame(&mut self, event: Option<&str>, payload: &Value, out: &mut Vec<Bytes>) {
        for (event, payload) in self.mapper.on_stream_payload(event, payload) {
            push_sse_event(out, &event, &payload);
        }
    }

    /// Converts upstream stream completion marker into Anthropic terminal events.
    fn on_done_frame(&mut self, out: &mut Vec<Bytes>) {
        for (event, payload) in self.mapper.on_done() {
            push_sse_event(out, &event, &payload);
        }
    }

    /// Flushes final Anthropic events on stream teardown.
    fn finish(&mut self, out: &mut Vec<Bytes>) {
        for (event, payload) in self.mapper.finish() {
            push_sse_event(out, &event, &payload);
        }
    }

    /// Returns final Anthropic non-stream message JSON.
    fn final_response_json(&self) -> Option<Value> {
        self.mapper.final_message_json()
    }
}
