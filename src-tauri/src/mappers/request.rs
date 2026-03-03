use super::canonical::{MapOptions, MapperSurface};
use super::engine::map_request;
use serde_json::Value;

fn detect_openai_surface(body: &Value) -> MapperSurface {
    if body.get("input").is_some() && body.get("messages").is_none() {
        MapperSurface::OpenaiResponses
    } else {
        MapperSurface::OpenaiChatCompletions
    }
}

pub fn map_request_by_surface(
    source: MapperSurface,
    target: MapperSurface,
    body: &Value,
    strict_mode: bool,
    target_model: &str,
) -> Result<Value, String> {
    map_request(
        source,
        target,
        body,
        &MapOptions::new(strict_mode, target_model),
    )
}

pub fn map_openai_to_anthropic_request(
    body: &Value,
    strict_mode: bool,
    target_model: &str,
) -> Result<Value, String> {
    map_request_by_surface(
        detect_openai_surface(body),
        MapperSurface::AnthropicMessages,
        body,
        strict_mode,
        target_model,
    )
}

pub fn map_anthropic_to_openai_completions_request(
    body: &Value,
    strict_mode: bool,
    target_model: &str,
) -> Result<Value, String> {
    map_request_by_surface(
        MapperSurface::AnthropicMessages,
        MapperSurface::OpenaiChatCompletions,
        body,
        strict_mode,
        target_model,
    )
}

pub fn map_anthropic_to_openai_responses_request(
    body: &Value,
    strict_mode: bool,
    target_model: &str,
) -> Result<Value, String> {
    map_request_by_surface(
        MapperSurface::AnthropicMessages,
        MapperSurface::OpenaiResponses,
        body,
        strict_mode,
        target_model,
    )
}

// Backward-compatible alias: existing callers expect this to produce
// OpenAI chat/completions request payload.
pub fn map_anthropic_to_openai_request(
    body: &Value,
    strict_mode: bool,
    target_model: &str,
) -> Result<Value, String> {
    map_anthropic_to_openai_completions_request(body, strict_mode, target_model)
}
