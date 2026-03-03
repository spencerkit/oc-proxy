//! Module Overview
//! Strict-mode compatibility policy for mapper request fields.
//! Validates supported keys per source surface and rejects unknown fields when requested.

use super::canonical::MapperSurface;
use serde_json::Value;

const SUPPORTED_OPENAI_CHAT_KEYS: &[&str] = &[
    "model",
    "messages",
    "stream",
    "max_tokens",
    "max_output_tokens",
    "temperature",
    "top_p",
    "tools",
    "tool_choice",
    "parallel_tool_calls",
    "metadata",
    "stop",
    "system",
    "thinking",
    "context_management",
];

const SUPPORTED_OPENAI_RESPONSES_KEYS: &[&str] = &[
    "model",
    "input",
    "stream",
    "max_tokens",
    "max_output_tokens",
    "temperature",
    "top_p",
    "tools",
    "tool_choice",
    "metadata",
    "stop",
    "instructions",
    "reasoning",
    "truncation",
    "previous_response_id",
    "system",
    "thinking",
    "context_management",
];

const SUPPORTED_ANTHROPIC_KEYS: &[&str] = &[
    "model",
    "messages",
    "max_tokens",
    "system",
    "temperature",
    "top_p",
    "stream",
    "tools",
    "tool_choice",
    "stop_sequences",
    "metadata",
    "thinking",
    "context_management",
];

pub fn validate_request_fields(
    source: MapperSurface,
    body: &Value,
    strict_mode: bool,
) -> Result<(), String> {
    if !strict_mode {
        return Ok(());
    }

    let supported = match source {
        MapperSurface::AnthropicMessages => SUPPORTED_ANTHROPIC_KEYS,
        MapperSurface::OpenaiChatCompletions => SUPPORTED_OPENAI_CHAT_KEYS,
        MapperSurface::OpenaiResponses => SUPPORTED_OPENAI_RESPONSES_KEYS,
    };

    let Some(obj) = body.as_object() else {
        return Ok(());
    };

    let unknown = obj
        .keys()
        .filter(|k| !supported.contains(&k.as_str()))
        .cloned()
        .collect::<Vec<_>>();

    if unknown.is_empty() {
        return Ok(());
    }

    match source {
        MapperSurface::AnthropicMessages => Err(format!(
            "Unsupported Claude fields in strict mode: {}",
            unknown.join(", ")
        )),
        MapperSurface::OpenaiChatCompletions | MapperSurface::OpenaiResponses => Err(format!(
            "Unsupported OpenAI fields in strict mode: {}",
            unknown.join(", ")
        )),
    }
}
