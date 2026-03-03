//! Module Overview
//! Public response mapping entrypoints and convenience adapters.
//! Supports protocol conversions and OpenAI chat/responses compatibility transformations.

use super::canonical::MapperSurface;
use super::response_engine::map_response;
use serde_json::Value;

/// Map response payload between two explicit protocol surfaces.
pub fn map_response_by_surface(
    source: MapperSurface,
    target: MapperSurface,
    body: &Value,
    request_model: &str,
) -> Value {
    map_response(source, target, body, request_model)
}

/// Convert Anthropic messages response into OpenAI chat-completions response.
pub fn map_anthropic_to_openai_response(anthropic_response: &Value, request_model: &str) -> Value {
    map_response_by_surface(
        MapperSurface::AnthropicMessages,
        MapperSurface::OpenaiChatCompletions,
        anthropic_response,
        request_model,
    )
}

/// Convert OpenAI chat-completions response into Anthropic messages response.
pub fn map_openai_to_anthropic_response(openai_response: &Value, request_model: &str) -> Value {
    map_response_by_surface(
        MapperSurface::OpenaiChatCompletions,
        MapperSurface::AnthropicMessages,
        openai_response,
        request_model,
    )
}

/// Convert OpenAI chat-completions response into OpenAI responses output shape.
pub fn map_openai_chat_to_responses(chat_response: &Value) -> Value {
    map_response_by_surface(
        MapperSurface::OpenaiChatCompletions,
        MapperSurface::OpenaiResponses,
        chat_response,
        "",
    )
}
