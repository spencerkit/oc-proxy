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
