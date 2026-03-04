//! Module Overview
//! Public request mapping entrypoints and compatibility aliases.
//! Provides surface-aware mapping APIs for OpenAI/Anthropic request transformations.

use super::canonical::{MapOptions, MapperSurface};
use super::engine::map_request;
use serde_json::Value;

/// Heuristically classify OpenAI request payload as `responses` or `chat/completions`.
///
/// The signal is intentionally simple and backward-compatible:
/// - if `input` exists and `messages` is absent => responses surface,
/// - otherwise => chat-completions surface.
fn detect_openai_surface(body: &Value) -> MapperSurface {
    if body.get("input").is_some() && body.get("messages").is_none() {
        MapperSurface::OpenaiResponses
    } else {
        MapperSurface::OpenaiChatCompletions
    }
}

/// Map request body between two explicit protocol surfaces.
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

/// Map OpenAI (chat/responses auto-detected) request into Anthropic messages request.
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

/// Map Anthropic messages request into OpenAI chat-completions request.
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

/// Map Anthropic messages request into OpenAI responses request.
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
/// Maps Anthropic to OpenAI request for this module's workflow.
pub fn map_anthropic_to_openai_request(
    body: &Value,
    strict_mode: bool,
    target_model: &str,
) -> Result<Value, String> {
    map_anthropic_to_openai_completions_request(body, strict_mode, target_model)
}
