use super::adapters::{anthropic_messages, openai_chat_completions, openai_responses};
use super::canonical::{CanonicalResponse, MapperSurface};
use serde_json::Value;

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

fn encode_response(target: MapperSurface, response: &CanonicalResponse) -> Value {
    match target {
        MapperSurface::AnthropicMessages => anthropic_messages::encode_response(response),
        MapperSurface::OpenaiChatCompletions => openai_chat_completions::encode_response(response),
        MapperSurface::OpenaiResponses => openai_responses::encode_response(response),
    }
}

pub fn map_response(
    source: MapperSurface,
    target: MapperSurface,
    body: &Value,
    request_model: &str,
) -> Value {
    let canonical = decode_response(source, body, request_model);
    encode_response(target, &canonical)
}
