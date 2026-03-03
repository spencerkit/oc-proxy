//! Module Overview
//! Response mapping engine using canonical response as intermediate representation.
//! Decodes source protocol payloads and re-encodes into the requested target surface.

use super::adapters::{anthropic_messages, openai_chat_completions, openai_responses};
use super::canonical::{CanonicalResponse, MapperSurface};
use serde_json::Value;

/// Decode source protocol response payload into canonical response.
fn decode_response(source: MapperSurface, body: &Value, request_model: &str) -> CanonicalResponse {
    match source {
        MapperSurface::AnthropicMessages => {
            anthropic_messages::decode_response(body, request_model)
        }
        MapperSurface::OpenaiChatCompletions => {
            openai_chat_completions::decode_response(body, request_model)
        }
        MapperSurface::OpenaiResponses => openai_responses::decode_response(body, request_model),
    }
}

/// Encode canonical response into target protocol response payload.
fn encode_response(target: MapperSurface, response: &CanonicalResponse) -> Value {
    match target {
        MapperSurface::AnthropicMessages => anthropic_messages::encode_response(response),
        MapperSurface::OpenaiChatCompletions => openai_chat_completions::encode_response(response),
        MapperSurface::OpenaiResponses => openai_responses::encode_response(response),
    }
}

/// Generic response mapping pipeline using canonical representation as intermediate format.
pub fn map_response(
    source: MapperSurface,
    target: MapperSurface,
    body: &Value,
    request_model: &str,
) -> Value {
    let canonical = decode_response(source, body, request_model);
    encode_response(target, &canonical)
}
