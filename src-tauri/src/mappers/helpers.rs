//! Module Overview
//! Shared helper utilities for mapper adapters.
//! Contains content flattening and protocol-specific text/tool argument extraction helpers.

use serde_json::Value;

pub(crate) fn as_array<'a>(v: &'a Value, key: &str) -> Vec<&'a Value> {
    v.get(key)
        .and_then(|x| x.as_array())
        .map(|x| x.iter().collect())
        .unwrap_or_default()
}

pub(crate) fn str_or_empty(v: Option<&Value>) -> String {
    v.and_then(|x| x.as_str()).unwrap_or_default().to_string()
}

pub(crate) fn to_tool_result_content(content: &Value) -> String {
    if let Some(s) = content.as_str() {
        return s.to_string();
    }
    if let Some(arr) = content.as_array() {
        let joined = arr
            .iter()
            .filter_map(|item| {
                let item_type = item
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                if item_type == "text" || item_type == "input_text" || item_type == "output_text" {
                    return item
                        .get("text")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                }
                None
            })
            .collect::<Vec<_>>()
            .join("");
        if !joined.is_empty() {
            return joined;
        }
    }
    content.to_string()
}

pub(crate) fn input_item_to_text(value: &Value) -> String {
    if value.is_null() {
        return String::new();
    }
    if let Some(s) = value.as_str() {
        return s.to_string();
    }
    if let Some(arr) = value.as_array() {
        let mut chunks: Vec<String> = vec![];
        for part in arr {
            if let Some(s) = part.as_str() {
                chunks.push(s.to_string());
                continue;
            }
            if let Some(obj) = part.as_object() {
                if let Some(text) = obj.get("text").and_then(|v| v.as_str()) {
                    chunks.push(text.to_string());
                    continue;
                }
                if let Some(text) = obj.get("output_text").and_then(|v| v.as_str()) {
                    chunks.push(text.to_string());
                    continue;
                }
                if let Some(text) = obj.get("input_text").and_then(|v| v.as_str()) {
                    chunks.push(text.to_string());
                    continue;
                }
            }
            chunks.push(part.to_string());
        }
        if !chunks.is_empty() {
            return chunks.join("");
        }
    }
    value.to_string()
}

pub(crate) fn input_item_function_arguments(value: Option<&Value>) -> String {
    match value {
        Some(v) if v.is_string() => v.as_str().unwrap_or_default().to_string(),
        Some(v) => serde_json::to_string(v).unwrap_or_else(|_| "{}".to_string()),
        None => "{}".to_string(),
    }
}

pub(crate) fn flatten_anthropic_text(content: &Value) -> String {
    if let Some(s) = content.as_str() {
        return s.to_string();
    }
    if let Some(arr) = content.as_array() {
        return arr
            .iter()
            .filter_map(|block| {
                if block.get("type").and_then(|v| v.as_str()) == Some("text") {
                    return block
                        .get("text")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                }
                None
            })
            .collect::<Vec<_>>()
            .join("");
    }
    String::new()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OpenAIFinishReason<'a> {
    ToolCalls,
    Length,
    Stop,
    Other(&'a str),
}

pub(crate) fn parse_openai_finish_reason(reason: &str) -> OpenAIFinishReason<'_> {
    match reason {
        "tool_calls" => OpenAIFinishReason::ToolCalls,
        "length" => OpenAIFinishReason::Length,
        "stop" => OpenAIFinishReason::Stop,
        other => OpenAIFinishReason::Other(other),
    }
}

pub(crate) fn map_openai_finish_reason_to_anthropic_stop(reason: &str) -> &str {
    match parse_openai_finish_reason(reason) {
        OpenAIFinishReason::ToolCalls => "tool_use",
        OpenAIFinishReason::Length => "max_tokens",
        OpenAIFinishReason::Stop => "end_turn",
        OpenAIFinishReason::Other(other) => other,
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct OpenAIUsageSummary {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: Option<u64>,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
}

pub(crate) fn extract_openai_usage_summary(usage: &Value) -> Option<OpenAIUsageSummary> {
    if !usage.is_object() {
        return None;
    }

    let input_tokens = usage
        .get("input_tokens")
        .and_then(|v| v.as_u64())
        .or_else(|| usage.get("prompt_tokens").and_then(|v| v.as_u64()))
        .unwrap_or(0);
    let output_tokens = usage
        .get("output_tokens")
        .and_then(|v| v.as_u64())
        .or_else(|| usage.get("completion_tokens").and_then(|v| v.as_u64()))
        .unwrap_or(0);
    let total_tokens = usage.get("total_tokens").and_then(|v| v.as_u64());
    let cache_read_tokens = usage
        .get("cache_read_input_tokens")
        .and_then(|v| v.as_u64())
        .or_else(|| {
            usage
                .get("prompt_tokens_details")
                .and_then(|v| v.get("cached_tokens"))
                .and_then(|v| v.as_u64())
        })
        .unwrap_or(0);
    let cache_write_tokens = usage
        .get("cache_creation_input_tokens")
        .and_then(|v| v.as_u64())
        .or_else(|| {
            usage
                .get("prompt_tokens_details")
                .and_then(|v| v.get("cache_creation_tokens"))
                .and_then(|v| v.as_u64())
        })
        .unwrap_or(0);

    Some(OpenAIUsageSummary {
        input_tokens,
        output_tokens,
        total_tokens,
        cache_read_tokens,
        cache_write_tokens,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        extract_openai_usage_summary, map_openai_finish_reason_to_anthropic_stop,
        parse_openai_finish_reason, OpenAIFinishReason,
    };
    use serde_json::json;

    #[test]
    fn openai_finish_reason_mapping_is_stable() {
        assert!(matches!(
            parse_openai_finish_reason("tool_calls"),
            OpenAIFinishReason::ToolCalls
        ));
        assert!(matches!(
            parse_openai_finish_reason("length"),
            OpenAIFinishReason::Length
        ));
        assert!(matches!(
            parse_openai_finish_reason("stop"),
            OpenAIFinishReason::Stop
        ));
        assert!(matches!(
            parse_openai_finish_reason("custom_reason"),
            OpenAIFinishReason::Other("custom_reason")
        ));
        assert_eq!(
            map_openai_finish_reason_to_anthropic_stop("tool_calls"),
            "tool_use"
        );
    }

    #[test]
    fn openai_usage_summary_supports_prompt_details() {
        let usage = json!({
            "prompt_tokens": 12,
            "completion_tokens": 7,
            "total_tokens": 19,
            "prompt_tokens_details": {
                "cached_tokens": 4,
                "cache_creation_tokens": 2
            }
        });

        let summary = extract_openai_usage_summary(&usage).expect("usage summary should exist");
        assert_eq!(summary.input_tokens, 12);
        assert_eq!(summary.output_tokens, 7);
        assert_eq!(summary.total_tokens, Some(19));
        assert_eq!(summary.cache_read_tokens, 4);
        assert_eq!(summary.cache_write_tokens, 2);
    }
}
