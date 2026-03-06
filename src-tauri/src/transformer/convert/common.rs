//! Common utility functions for protocol conversion
//! Reference: ccNexus/internal/transformer/convert/common.go

use serde_json::Value;
use std::collections::HashSet;

/// Extract system text from Claude system prompt
pub fn extract_system_text(system: &Value) -> String {
    match system {
        Value::String(s) => s.clone(),
        Value::Array(arr) => {
            let parts: Vec<String> = arr
                .iter()
                .filter_map(|block| {
                    block.get("text").and_then(|t| t.as_str()).map(|s| s.to_string())
                })
                .collect();
            parts.join("\n")
        }
        _ => String::new(),
    }
}

/// Extract tool result content
pub fn extract_tool_result_content(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(arr) => {
            let parts: Vec<String> = arr
                .iter()
                .filter_map(|block| {
                    if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                        Some(text.to_string())
                    } else {
                        None
                    }
                })
                .collect();
            parts.join("\n")
        }
        _ => serde_json::to_string(content).unwrap_or_default(),
    }
}

/// Parse SSE event data
pub fn parse_sse(data: &[u8]) -> (String, String) {
    let text = String::from_utf8_lossy(data);
    let mut event_type = String::new();
    let mut json_data = String::new();

    for line in text.lines() {
        let line = line.trim();
        if let Some(evt) = line.strip_prefix("event: ") {
            event_type = evt.to_string();
        } else if let Some(data) = line.strip_prefix("data: ") {
            json_data = data.to_string();
        }
    }

    (event_type, json_data)
}

/// Build Claude SSE event
pub fn build_claude_event(event_type: &str, data: &Value) -> Vec<u8> {
    let payload = match data {
        Value::Object(map) => {
            let mut merged = map.clone();
            merged.insert("type".to_string(), Value::String(event_type.to_string()));
            Value::Object(merged)
        }
        _ => serde_json::json!({
            "type": event_type,
            "data": data
        }),
    };
    let json_str = serde_json::to_string(&payload).unwrap_or_default();
    format!("event: {}\ndata: {}\n\n", event_type, json_str).into_bytes()
}

/// Build OpenAI streaming chunk
pub fn build_openai_chunk(
    id: &str,
    model: &str,
    content: Option<&str>,
    finish_reason: Option<&str>,
) -> Vec<u8> {
    let mut delta = serde_json::Map::new();
    if let Some(c) = content {
        delta.insert("content".to_string(), Value::String(c.to_string()));
    }

    let chunk = serde_json::json!({
        "id": id,
        "object": "chat.completion.chunk",
        "model": model,
        "choices": [{
            "index": 0,
            "delta": delta,
            "finish_reason": finish_reason
        }]
    });

    format!("data: {}\n\n", serde_json::to_string(&chunk).unwrap_or_default()).into_bytes()
}

/// Parsed textual tool call payload extracted from placeholder format.
#[derive(Debug, Clone)]
pub struct ParsedTextToolCall {
    pub name: String,
    pub arguments: Value,
}

/// Parse textual fallback candidates into a tool call.
///
/// Order:
/// 1) strict placeholder: `[Tool Call: <tool>(<json>)]`
/// 2) codex-like JSON command object: `{"command":["bash","-lc","..."], ...}`
pub fn parse_text_tool_call_fallback(
    text: &str,
    allowed_tool_names: &HashSet<String>,
) -> Option<ParsedTextToolCall> {
    parse_strict_text_tool_call(text, allowed_tool_names)
        .or_else(|| parse_command_array_text_tool_call(text, allowed_tool_names))
}

/// Parse textual tool call placeholder in the form:
/// `[Tool Call: <tool_name>(<json_object_arguments>)]`
///
/// Safety constraints:
/// - only parses bracketed placeholders in the text;
/// - tool name must be declared in `allowed_tool_names`;
/// - arguments must parse as a JSON object.
pub fn parse_strict_text_tool_call(
    text: &str,
    allowed_tool_names: &HashSet<String>,
) -> Option<ParsedTextToolCall> {
    const MARKER: &str = "[Tool Call: ";

    let mut tool_names: Vec<&str> = allowed_tool_names
        .iter()
        .map(String::as_str)
        .collect();
    // Prefer longest names first to avoid prefix collisions.
    tool_names.sort_by_key(|name| std::cmp::Reverse(name.len()));

    let mut search_from = 0usize;
    while let Some(rel_start) = text[search_from..].find(MARKER) {
        let marker_start = search_from + rel_start;
        let inner_start = marker_start + MARKER.len();
        let inner = &text[inner_start..];

        let mut matched_tool = None;
        let mut args_start = 0usize;
        for tool_name in &tool_names {
            let prefix = format!("{tool_name}(");
            if inner.starts_with(&prefix) {
                matched_tool = Some((*tool_name).to_string());
                args_start = inner_start + prefix.len();
                break;
            }
        }

        let Some(tool_name) = matched_tool else {
            search_from = inner_start;
            continue;
        };

        // Find closing `)]` for the current marker. Try each candidate until args parse succeeds.
        let mut close_search_from = args_start;
        while let Some(rel_end) = text[close_search_from..].find(")]") {
            let close_pos = close_search_from + rel_end;
            let args_str = &text[args_start..close_pos];
            if let Ok(args) = serde_json::from_str::<Value>(args_str) {
                if args.is_object() {
                    return Some(ParsedTextToolCall {
                        name: tool_name,
                        arguments: args,
                    });
                }
            }
            close_search_from = close_pos + 2;
        }

        if inner_start >= text.len() {
            break;
        }
        search_from = inner_start;
    }

    None
}

fn parse_command_array_text_tool_call(
    text: &str,
    allowed_tool_names: &HashSet<String>,
) -> Option<ParsedTextToolCall> {
    let tool_name = resolve_allowed_tool_name(allowed_tool_names, "Bash")?;

    let mut search_from = 0usize;
    while let Some(rel_start) = text[search_from..].find('{') {
        let start = search_from + rel_start;
        let Some(end) = find_json_object_end(text, start) else {
            search_from = start.saturating_add(1);
            continue;
        };

        if let Ok(value) = serde_json::from_str::<Value>(&text[start..=end]) {
            if let Some(arguments) = normalize_bash_command_array_arguments(&value) {
                return Some(ParsedTextToolCall {
                    name: tool_name.clone(),
                    arguments,
                });
            }
        }

        search_from = start.saturating_add(1);
    }

    None
}

fn resolve_allowed_tool_name(allowed_tool_names: &HashSet<String>, preferred: &str) -> Option<String> {
    if let Some(exact) = allowed_tool_names.get(preferred) {
        return Some(exact.clone());
    }
    allowed_tool_names
        .iter()
        .find(|name| name.eq_ignore_ascii_case(preferred))
        .cloned()
}

fn find_json_object_end(text: &str, start: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    if start >= bytes.len() || bytes[start] != b'{' {
        return None;
    }

    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;

    for (idx, b) in bytes.iter().enumerate().skip(start) {
        if in_string {
            if escaped {
                escaped = false;
                continue;
            }
            match *b {
                b'\\' => escaped = true,
                b'"' => in_string = false,
                _ => {}
            }
            continue;
        }

        match *b {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(idx);
                }
            }
            _ => {}
        }
    }

    None
}

fn normalize_bash_command_array_arguments(value: &Value) -> Option<Value> {
    let obj = value.as_object()?;
    let command = obj.get("command")?.as_array()?;
    if command.is_empty() {
        return None;
    }

    let argv: Vec<&str> = command.iter().map(|v| v.as_str()).collect::<Option<Vec<_>>>()?;
    let shell = *argv.first()?;
    if !is_shell_program(shell) {
        return None;
    }

    let mut command_text = if argv.len() >= 3 && matches!(argv[1], "-lc" | "-c") {
        argv[2..].join(" ")
    } else if argv.len() >= 2 {
        argv[1..].join(" ")
    } else {
        String::new()
    };
    command_text = command_text.trim().to_string();
    if command_text.is_empty() {
        return None;
    }

    let mut args = serde_json::Map::new();
    args.insert("command".to_string(), Value::String(command_text));

    if let Some(description) = obj.get("description").and_then(|v| v.as_str()) {
        if !description.trim().is_empty() {
            args.insert(
                "description".to_string(),
                Value::String(description.to_string()),
            );
        }
    }

    let timeout = obj
        .get("timeout")
        .and_then(|v| v.as_i64())
        .or_else(|| obj.get("timeout_ms").and_then(|v| v.as_i64()));
    if let Some(timeout) = timeout {
        if timeout > 0 {
            args.insert(
                "timeout".to_string(),
                Value::Number(serde_json::Number::from(timeout)),
            );
        }
    }

    Some(Value::Object(args))
}

fn is_shell_program(program: &str) -> bool {
    matches!(
        program,
        "bash" | "/bin/bash" | "sh" | "/bin/sh" | "zsh" | "/bin/zsh"
    )
}
