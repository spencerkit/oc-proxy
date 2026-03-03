//! Module Overview
//! Request mapping engine built around decode-to-canonical then encode-to-target flow.
//! Centralizes adapter dispatch and strict-field policy enforcement.

use super::adapters::{anthropic_messages, openai_chat_completions, openai_responses};
use super::canonical::{CanonicalRequest, MapOptions, MapperSurface};
use super::policy::validate_request_fields;
use serde_json::Value;

/// Decode source request payload into canonical request representation.
fn decode_request(
    source: MapperSurface,
    body: &Value,
    options: &MapOptions,
) -> Result<CanonicalRequest, String> {
    match source {
        MapperSurface::AnthropicMessages => anthropic_messages::decode_request(body, options),
        MapperSurface::OpenaiChatCompletions => {
            openai_chat_completions::decode_request(body, options)
        }
        MapperSurface::OpenaiResponses => openai_responses::decode_request(body, options),
    }
}

/// Encode canonical request into concrete target protocol surface payload.
fn encode_request(target: MapperSurface, request: &CanonicalRequest) -> Value {
    match target {
        MapperSurface::AnthropicMessages => anthropic_messages::encode_request(request),
        MapperSurface::OpenaiChatCompletions => openai_chat_completions::encode_request(request),
        MapperSurface::OpenaiResponses => openai_responses::encode_request(request),
    }
}

/// Generic request mapping pipeline:
/// - validate source fields (optional strict mode),
/// - decode source payload into canonical model,
/// - encode canonical model into target payload shape.
pub fn map_request(
    source: MapperSurface,
    target: MapperSurface,
    body: &Value,
    options: &MapOptions,
) -> Result<Value, String> {
    validate_request_fields(source, body, options.strict_mode)?;
    let canonical = decode_request(source, body, options)?;
    Ok(encode_request(target, &canonical))
}
