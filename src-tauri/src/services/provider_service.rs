//! Module Overview
//! Provider model test workflow.
//! Sends a direct upstream request and normalizes the reported model identity for the renderer.

use crate::app_state::SharedState;
use crate::models::{ProviderModelTestResult, Rule, RuleProtocol};
use crate::proxy::routing::{build_rule_headers, resolve_upstream_path, resolve_upstream_url};
use crate::services::{AppError, AppResult};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::{Client, StatusCode};
use serde_json::{json, Value};
use std::time::Duration;

const MODEL_TEST_TIMEOUT_SECONDS: u64 = 20;
const MODEL_TEST_MAX_OUTPUT_TOKENS: u64 = 64;
const MODEL_TEST_MESSAGE_MAX_CHARS: usize = 240;
const MODEL_TEST_SYSTEM_PROMPT: &str = "You are running a model identity check. Reply with only the exact model identifier you are currently serving as. If the exact identifier is unavailable, reply with the most precise deployed model name you can confirm. Do not add any explanation, markdown, punctuation, or extra words.";
const MODEL_TEST_USER_PROMPT: &str =
    "Return only your exact current model name or model identifier.";

/// Tests upstream model identity with the saved provider configuration.
pub async fn test_model(
    state: &SharedState,
    group_id: String,
    provider_id: String,
) -> AppResult<ProviderModelTestResult> {
    let config = state.config_store.get();
    let group = config
        .groups
        .iter()
        .find(|group| group.id == group_id)
        .ok_or_else(|| AppError::not_found(format!("group not found: {group_id}")))?;
    let provider = group
        .providers
        .iter()
        .find(|provider| provider.id == provider_id)
        .ok_or_else(|| AppError::not_found(format!("provider not found: {provider_id}")))?
        .clone();

    validate_provider(&provider)?;

    let upstream_url = resolve_upstream_url(
        &provider.api_address,
        resolve_upstream_path(&provider.protocol),
    )
    .map_err(AppError::validation)?;
    let headers = build_request_headers(&provider)?;
    let payload = build_request_payload(&provider);
    let client = Client::builder()
        .timeout(Duration::from_secs(MODEL_TEST_TIMEOUT_SECONDS))
        .build()
        .map_err(|error| AppError::internal(format!("create model test client failed: {error}")))?;

    let response = match client
        .post(upstream_url)
        .headers(headers)
        .json(&payload)
        .send()
        .await
    {
        Ok(response) => response,
        Err(error) => {
            return Ok(failure_result(format!(
                "request to upstream failed: {}",
                clip_text(&error.to_string(), MODEL_TEST_MESSAGE_MAX_CHARS)
            )));
        }
    };

    let status = response.status();
    let raw_body = response
        .text()
        .await
        .map_err(|error| AppError::external(format!("read upstream response failed: {error}")))?;
    let parsed_body = serde_json::from_str::<Value>(&raw_body).ok();
    let sse_events = if parsed_body.is_none() {
        parse_sse_json_events(&raw_body)
    } else {
        Vec::new()
    };

    if !status.is_success() {
        let error_body = parsed_body.as_ref().or_else(|| {
            sse_events
                .iter()
                .find(|event| extract_error_message(event).is_some())
        });
        return Ok(failure_result(describe_error_response(
            status, error_body, &raw_body,
        )));
    }

    let raw_text = extract_response_text(
        &provider.protocol,
        parsed_body.as_ref(),
        &sse_events,
        &raw_body,
    )
    .map(|text| clip_text(&text, MODEL_TEST_MESSAGE_MAX_CHARS));
    let resolved_model = parsed_body
        .as_ref()
        .and_then(extract_response_model)
        .or_else(|| extract_sse_response_model(&sse_events))
        .or_else(|| raw_text.as_deref().and_then(normalize_model_text));

    if resolved_model.is_none() && raw_text.is_none() {
        return Ok(failure_result(
            "model test succeeded but upstream did not return a readable model name".to_string(),
        ));
    }

    Ok(ProviderModelTestResult {
        ok: true,
        resolved_model,
        raw_text,
        message: None,
    })
}

/// Validates provider fields required for direct upstream testing.
fn validate_provider(provider: &Rule) -> AppResult<()> {
    if provider.token.trim().is_empty() {
        return Err(AppError::validation("provider token is empty"));
    }
    if provider.api_address.trim().is_empty() {
        return Err(AppError::validation("provider apiAddress is empty"));
    }
    if provider.default_model.trim().is_empty() {
        return Err(AppError::validation("provider defaultModel is empty"));
    }
    Ok(())
}

/// Builds request headers used for provider model tests.
fn build_request_headers(provider: &Rule) -> AppResult<HeaderMap> {
    let mut headers = HeaderMap::new();
    for (key, value) in build_rule_headers(&provider.protocol, provider) {
        let header_name = HeaderName::from_bytes(key.as_bytes())
            .map_err(|_| AppError::validation(format!("invalid upstream header name: {key}")))?;
        let header_value = HeaderValue::from_str(&value).map_err(|_| {
            AppError::validation(format!("invalid upstream header value for header: {key}"))
        })?;
        headers.insert(header_name, header_value);
    }
    headers.insert(
        HeaderName::from_static("accept"),
        HeaderValue::from_static("application/json"),
    );
    Ok(headers)
}

/// Builds the direct upstream payload for the configured provider protocol.
fn build_request_payload(provider: &Rule) -> Value {
    match provider.protocol {
        RuleProtocol::Openai => json!({
            "model": provider.default_model.as_str(),
            "instructions": MODEL_TEST_SYSTEM_PROMPT,
            "input": [
                {
                    "type": "message",
                    "role": "user",
                    "content": [
                        {
                            "type": "input_text",
                            "text": MODEL_TEST_USER_PROMPT
                        }
                    ]
                }
            ],
            "temperature": 0,
            "max_output_tokens": MODEL_TEST_MAX_OUTPUT_TOKENS
        }),
        RuleProtocol::OpenaiCompletion => json!({
            "model": provider.default_model.as_str(),
            "messages": [
                {
                    "role": "system",
                    "content": MODEL_TEST_SYSTEM_PROMPT
                },
                {
                    "role": "user",
                    "content": MODEL_TEST_USER_PROMPT
                }
            ],
            "temperature": 0,
            "max_tokens": MODEL_TEST_MAX_OUTPUT_TOKENS
        }),
        RuleProtocol::Anthropic => json!({
            "model": provider.default_model.as_str(),
            "system": MODEL_TEST_SYSTEM_PROMPT,
            "messages": [
                {
                    "role": "user",
                    "content": MODEL_TEST_USER_PROMPT
                }
            ],
            "temperature": 0,
            "max_tokens": MODEL_TEST_MAX_OUTPUT_TOKENS
        }),
    }
}

/// Builds a normalized failed result payload.
fn failure_result(message: String) -> ProviderModelTestResult {
    ProviderModelTestResult {
        ok: false,
        resolved_model: None,
        raw_text: None,
        message: Some(message),
    }
}

/// Extracts top-level response model field when available.
fn extract_response_model(body: &Value) -> Option<String> {
    body.get("model")
        .and_then(Value::as_str)
        .and_then(normalize_model_text)
}

/// Extracts readable response text from provider payloads.
fn extract_response_text(
    protocol: &RuleProtocol,
    body: Option<&Value>,
    sse_events: &[Value],
    raw_body: &str,
) -> Option<String> {
    let extracted = body.and_then(|body| match protocol {
        RuleProtocol::Openai => extract_openai_responses_text(body),
        RuleProtocol::OpenaiCompletion => extract_openai_chat_text(body),
        RuleProtocol::Anthropic => extract_anthropic_text(body),
    });

    if extracted.is_some() {
        return extracted;
    }

    if let Some(streamed_text) = extract_sse_response_text(protocol, sse_events) {
        return Some(streamed_text);
    }

    if body.is_none() {
        return clean_text(raw_body);
    }

    None
}

/// Extracts model identity from parsed SSE JSON events.
fn extract_sse_response_model(events: &[Value]) -> Option<String> {
    events.iter().find_map(|event| {
        extract_response_model(event).or_else(|| {
            event
                .pointer("/response/model")
                .and_then(Value::as_str)
                .and_then(normalize_model_text)
        })
    })
}

/// Extracts readable response text from parsed SSE JSON events.
fn extract_sse_response_text(protocol: &RuleProtocol, events: &[Value]) -> Option<String> {
    if events.is_empty() {
        return None;
    }

    let texts = events
        .iter()
        .filter_map(|event| match protocol {
            RuleProtocol::Openai => extract_openai_responses_text(event),
            RuleProtocol::OpenaiCompletion => extract_openai_chat_text(event),
            RuleProtocol::Anthropic => extract_anthropic_text(event),
        })
        .collect::<Vec<_>>();
    join_texts(texts)
}

/// Extracts text from OpenAI responses payloads.
fn extract_openai_responses_text(body: &Value) -> Option<String> {
    if let Some(response) = body.get("response") {
        if let Some(text) = extract_openai_responses_text(response) {
            return Some(text);
        }
    }

    if let Some(delta_text) = body.get("delta").and_then(Value::as_str) {
        return clean_text(delta_text);
    }

    if let Some(output_text) = body.get("output_text").and_then(Value::as_str) {
        return clean_text(output_text);
    }

    let output = body.get("output")?.as_array()?;
    let texts = output
        .iter()
        .filter_map(|item| item.get("content").and_then(Value::as_array))
        .flat_map(|parts| parts.iter())
        .filter_map(extract_text_fragment)
        .collect::<Vec<_>>();

    join_texts(texts)
}

/// Extracts text from OpenAI chat-completions payloads.
fn extract_openai_chat_text(body: &Value) -> Option<String> {
    let choice = body.get("choices")?.as_array()?.first()?;
    if let Some(message_content) = choice
        .get("message")
        .and_then(|message| message.get("content"))
        .and_then(extract_message_content_text)
    {
        return Some(message_content);
    }

    choice
        .get("text")
        .and_then(Value::as_str)
        .and_then(clean_text)
        .or_else(|| {
            choice
                .get("delta")
                .and_then(|delta| delta.get("content"))
                .and_then(extract_message_content_text)
        })
}

/// Extracts text from Anthropic messages payloads.
fn extract_anthropic_text(body: &Value) -> Option<String> {
    body.get("content")
        .and_then(extract_message_content_text)
        .or_else(|| {
            body.pointer("/delta/text")
                .and_then(Value::as_str)
                .and_then(clean_text)
        })
}

/// Extracts string content from either scalar or structured message content.
fn extract_message_content_text(content: &Value) -> Option<String> {
    if let Some(text) = content.as_str() {
        return clean_text(text);
    }

    let parts = content.as_array()?;
    let texts = parts
        .iter()
        .filter_map(extract_text_fragment)
        .collect::<Vec<_>>();
    join_texts(texts)
}

/// Extracts one text fragment from a structured content block.
fn extract_text_fragment(fragment: &Value) -> Option<String> {
    fragment
        .get("text")
        .and_then(Value::as_str)
        .and_then(clean_text)
        .or_else(|| {
            fragment
                .get("value")
                .and_then(Value::as_str)
                .and_then(clean_text)
        })
}

/// Normalizes returned text for storage/display.
fn clean_text(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_string())
}

/// Joins extracted text parts into a single readable string.
fn join_texts(parts: Vec<String>) -> Option<String> {
    let joined = parts
        .into_iter()
        .filter_map(|part| clean_text(&part))
        .collect::<Vec<_>>()
        .join("\n");
    clean_text(&joined)
}

/// Normalizes free-form model text to a likely model identifier.
fn normalize_model_text(raw: &str) -> Option<String> {
    let first_line = raw
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("")
        .trim_matches(|ch| matches!(ch, '"' | '\'' | '`'));
    if first_line.is_empty() {
        return None;
    }

    let mut text = first_line.to_string();
    if let Some((left, right)) = text
        .split_once(':')
        .map(|(left, right)| (left.to_string(), right.to_string()))
    {
        if left.to_lowercase().contains("model") {
            text = right.trim().to_string();
        }
    }

    let lower = text.to_lowercase();
    for prefix in ["model is ", "i am ", "i'm ", "this is ", "assistant model "] {
        if lower.starts_with(prefix) {
            text = text[prefix.len()..].trim().to_string();
            break;
        }
    }

    let trimmed = text.trim_matches(|ch| matches!(ch, '"' | '\'' | '`' | '.' | '!' | '?'));
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_string())
}

/// Builds a concise upstream failure message.
fn describe_error_response(
    status: StatusCode,
    parsed_body: Option<&Value>,
    raw_body: &str,
) -> String {
    let status_text = match status.canonical_reason() {
        Some(reason) => format!("HTTP {} {}", status.as_u16(), reason),
        None => format!("HTTP {}", status.as_u16()),
    };

    if let Some(message) = parsed_body.and_then(extract_error_message) {
        return format!("{status_text}: {message}");
    }

    if let Some(message) = clean_text(raw_body) {
        return format!(
            "{status_text}: {}",
            clip_text(&message, MODEL_TEST_MESSAGE_MAX_CHARS)
        );
    }

    status_text
}

/// Extracts a human-readable error message from common upstream error payloads.
fn extract_error_message(body: &Value) -> Option<String> {
    body.pointer("/error/message")
        .and_then(Value::as_str)
        .and_then(clean_text)
        .or_else(|| {
            body.get("message")
                .and_then(Value::as_str)
                .and_then(clean_text)
        })
        .or_else(|| {
            body.get("error")
                .and_then(Value::as_str)
                .and_then(clean_text)
        })
        .map(|message| clip_text(&message, MODEL_TEST_MESSAGE_MAX_CHARS))
}

/// Clips long text values for concise renderer messaging.
fn clip_text(raw: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for (index, ch) in raw.chars().enumerate() {
        if index >= max_chars {
            out.push_str("...");
            break;
        }
        out.push(ch);
    }
    out
}

/// Parses JSON payloads from a raw SSE body.
fn parse_sse_json_events(raw_body: &str) -> Vec<Value> {
    let mut events = Vec::new();
    let mut data_lines = Vec::<String>::new();

    for raw_line in raw_body.lines() {
        let line = raw_line.trim_end_matches('\r');
        if let Some(data) = line.strip_prefix("data:") {
            data_lines.push(data.trim_start().trim_start_matches('\u{feff}').to_string());
            continue;
        }

        if line.trim().is_empty() {
            flush_sse_event_data(&mut data_lines, &mut events);
        }
    }

    flush_sse_event_data(&mut data_lines, &mut events);
    events
}

/// Flushes one SSE event's `data:` lines into parsed JSON when possible.
fn flush_sse_event_data(data_lines: &mut Vec<String>, events: &mut Vec<Value>) {
    if data_lines.is_empty() {
        return;
    }

    let payload = data_lines.join("\n");
    data_lines.clear();
    let trimmed = payload.trim();
    if trimmed.is_empty() || trimmed == "[DONE]" {
        return;
    }

    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        events.push(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    /// Builds OpenAI responses payload with array-form input for stricter upstream compatibility.
    fn build_request_payload_uses_array_input_for_openai_responses() {
        let provider = Rule {
            id: "provider-a".to_string(),
            name: "Provider A".to_string(),
            protocol: RuleProtocol::Openai,
            token: "token".to_string(),
            api_address: "https://example.com".to_string(),
            default_model: "gpt-5-mini".to_string(),
            model_mappings: Default::default(),
            quota: crate::domain::entities::default_rule_quota_config(),
            cost: crate::domain::entities::default_rule_cost_config(),
        };

        let payload = build_request_payload(&provider);
        let input = payload
            .get("input")
            .and_then(Value::as_array)
            .expect("openai responses test payload should use array input");

        assert_eq!(input.len(), 1);
        assert_eq!(input[0]["type"], "message");
        assert_eq!(input[0]["role"], "user");
        assert_eq!(input[0]["content"][0]["type"], "input_text");
        assert_eq!(input[0]["content"][0]["text"], MODEL_TEST_USER_PROMPT);
    }

    #[test]
    /// Extracts response model from top-level field when present.
    fn extract_response_model_prefers_top_level_field() {
        let body = json!({
            "model": "claude-sonnet-4-20250514"
        });

        assert_eq!(
            extract_response_model(&body),
            Some("claude-sonnet-4-20250514".to_string())
        );
    }

    #[test]
    /// Extracts output text from OpenAI responses content blocks.
    fn extract_openai_responses_text_reads_output_blocks() {
        let body = json!({
            "output": [
                {
                    "type": "message",
                    "content": [
                        {
                            "type": "output_text",
                            "text": "gpt-5-mini"
                        }
                    ]
                }
            ]
        });

        assert_eq!(
            extract_openai_responses_text(&body),
            Some("gpt-5-mini".to_string())
        );
    }

    #[test]
    /// Extracts output text from chat-completions message content arrays.
    fn extract_openai_chat_text_reads_message_content_array() {
        let body = json!({
            "choices": [
                {
                    "message": {
                        "content": [
                            {
                                "type": "text",
                                "text": "gpt-4.1"
                            }
                        ]
                    }
                }
            ]
        });

        assert_eq!(extract_openai_chat_text(&body), Some("gpt-4.1".to_string()));
    }

    #[test]
    /// Extracts output text from Anthropic text content arrays.
    fn extract_anthropic_text_reads_text_blocks() {
        let body = json!({
            "content": [
                {
                    "type": "text",
                    "text": "claude-3-7-sonnet-20250219"
                }
            ]
        });

        assert_eq!(
            extract_anthropic_text(&body),
            Some("claude-3-7-sonnet-20250219".to_string())
        );
    }

    #[test]
    /// Parses chat-completions SSE chunks and extracts model/text for model test display.
    fn extract_response_text_reads_openai_chat_sse_chunks() {
        let raw = concat!(
            "data: {\"id\":\"chatcmpl_1\",\"model\":\"gpt-4.1-mini\",\"choices\":[{\"delta\":{\"role\":\"assistant\"},\"index\":0}]}\n\n",
            "data: {\"id\":\"chatcmpl_1\",\"model\":\"gpt-4.1-mini\",\"choices\":[{\"delta\":{\"content\":\"gpt-4.1-mini\"},\"index\":0}]}\n\n",
            "data: [DONE]\n\n"
        );

        let events = parse_sse_json_events(raw);
        assert_eq!(
            extract_sse_response_model(&events),
            Some("gpt-4.1-mini".to_string())
        );
        assert_eq!(
            extract_response_text(&RuleProtocol::OpenaiCompletion, None, &events, raw),
            Some("gpt-4.1-mini".to_string())
        );
    }

    #[test]
    /// Parses OpenAI responses SSE delta events.
    fn extract_response_text_reads_openai_responses_sse_delta() {
        let raw = concat!(
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"gpt-5-mini\"}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\"}\n\n"
        );

        let events = parse_sse_json_events(raw);
        assert_eq!(
            extract_response_text(&RuleProtocol::Openai, None, &events, raw),
            Some("gpt-5-mini".to_string())
        );
    }

    #[test]
    /// Normalizes common prefixed model responses.
    fn normalize_model_text_trims_wrapper_words() {
        assert_eq!(
            normalize_model_text("Model: gpt-4.1"),
            Some("gpt-4.1".to_string())
        );
        assert_eq!(
            normalize_model_text("I am claude-sonnet-4-20250514."),
            Some("claude-sonnet-4-20250514".to_string())
        );
    }
}
