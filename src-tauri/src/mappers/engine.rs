use super::adapters::{anthropic_messages, openai_chat_completions, openai_responses};
use super::canonical::{CanonicalRequest, MapOptions, MapperSurface};
use super::policy::validate_request_fields;
use serde_json::Value;

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

fn encode_request(target: MapperSurface, request: &CanonicalRequest) -> Value {
    match target {
        MapperSurface::AnthropicMessages => anthropic_messages::encode_request(request),
        MapperSurface::OpenaiChatCompletions => openai_chat_completions::encode_request(request),
        MapperSurface::OpenaiResponses => openai_responses::encode_request(request),
    }
}

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
