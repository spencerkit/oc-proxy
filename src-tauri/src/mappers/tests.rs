//! Module Overview
//! Mapper contract and behavior tests.
//! Covers protocol mapping semantics, strict-mode behavior, and snapshot-based compatibility checks.

use super::{
    map_anthropic_to_openai_request, map_anthropic_to_openai_response,
    map_anthropic_to_openai_responses_request, map_openai_chat_to_responses,
    map_openai_to_anthropic_request, map_openai_to_anthropic_response, normalize_openai_request,
};
use serde_json::{json, Value};

#[test]
/// Performs OpenAI request maps to Anthropic request.
fn openai_request_maps_to_anthropic_request() {
    let input = json!({
        "model": "m1",
        "messages": [
            { "role": "system", "content": "be concise" },
            { "role": "user", "content": "hello" }
        ],
        "stream": true,
        "max_tokens": 100
    });

    let out = map_openai_to_anthropic_request(&input, true, "claude-target")
        .expect("mapping should succeed");
    assert_eq!(out["model"], "claude-target");
    assert_eq!(out["stream"], true);
    assert_eq!(out["system"], "be concise");
    assert_eq!(out["messages"][0]["role"], "user");
}

#[test]
/// Performs Anthropic request maps to OpenAI request.
fn anthropic_request_maps_to_openai_request() {
    let input = json!({
        "model": "claude-x",
        "system": "helpful",
        "messages": [{ "role": "user", "content": [{ "type": "text", "text": "hello" }] }],
        "stream": false
    });

    let out = map_anthropic_to_openai_request(&input, true, "gpt-target")
        .expect("mapping should succeed");
    assert_eq!(out["model"], "gpt-target");
    assert_eq!(out["messages"][0]["role"], "system");
    assert_eq!(out["messages"][1]["content"], "hello");
}

#[test]
/// Performs Anthropic request without stream defaults to stream true.
fn anthropic_request_without_stream_defaults_to_stream_true() {
    let input = json!({
        "model": "claude-x",
        "messages": [{ "role": "user", "content": [{ "type": "text", "text": "hello" }] }]
    });

    let out = map_anthropic_to_openai_request(&input, true, "gpt-target")
        .expect("mapping should succeed");
    assert_eq!(out["stream"], true);
}

#[test]
/// Performs Anthropic stream request enables OpenAI include usage option.
fn anthropic_stream_request_enables_openai_include_usage_option() {
    let input = json!({
        "model": "claude-x",
        "stream": true,
        "messages": [{ "role": "user", "content": [{ "type": "text", "text": "hello" }] }]
    });

    let out = map_anthropic_to_openai_request(&input, true, "gpt-target")
        .expect("mapping should succeed");
    assert_eq!(out["stream"], true);
    assert_eq!(out["stream_options"]["include_usage"], true);
}

#[test]
/// Performs Anthropic request maps to OpenAI responses request.
fn anthropic_request_maps_to_openai_responses_request() {
    let input = json!({
        "model": "claude-x",
        "system": "helpful",
        "max_tokens": 512,
        "messages": [{ "role": "user", "content": [{ "type": "text", "text": "hello" }] }],
        "stream": false
    });

    let out = map_anthropic_to_openai_responses_request(&input, true, "gpt-target")
        .expect("mapping should succeed");
    assert_eq!(out["model"], "gpt-target");
    assert_eq!(out["instructions"], "helpful");
    assert_eq!(out["max_output_tokens"], 512);
    assert_eq!(out["input"][0]["type"], "message");
    assert_eq!(out["input"][0]["role"], "user");
    assert_eq!(out["input"][0]["content"][0]["text"], "hello");
}

#[test]
/// Performs Anthropic request maps tools and tool choice to responses shape.
fn anthropic_request_maps_tools_and_tool_choice_to_responses_shape() {
    let input = json!({
        "model": "claude-x",
        "messages": [{ "role": "user", "content": [{ "type": "text", "text": "hello" }] }],
        "tools": [{
            "name": "Read",
            "description": "Read file content",
            "input_schema": {
                "type": "object",
                "properties": {
                    "file_path": { "type": "string" }
                },
                "required": ["file_path"]
            }
        }],
        "tool_choice": {
            "type": "tool",
            "name": "Read"
        }
    });

    let out = map_anthropic_to_openai_responses_request(&input, true, "gpt-target")
        .expect("mapping should succeed");

    assert_eq!(out["tools"][0]["type"], "function");
    assert_eq!(out["tools"][0]["name"], "Read");
    assert_eq!(out["tools"][0]["description"], "Read file content");
    assert_eq!(
        out["tools"][0]["parameters"]["properties"]["file_path"]["type"],
        "string"
    );
    assert!(out["tools"][0].get("function").is_none());

    assert_eq!(out["tool_choice"]["type"], "function");
    assert_eq!(out["tool_choice"]["name"], "Read");
    assert!(out["tool_choice"].get("function").is_none());
}

#[test]
/// Performs Anthropic tool IDs are normalized for OpenAI responses requests.
fn anthropic_tool_ids_are_normalized_for_openai_responses_requests() {
    let out = map_anthropic_to_openai_responses_request(
        &json!({
            "model": "claude-x",
            "messages": [
                {
                    "role": "assistant",
                    "content": [{
                        "type": "tool_use",
                        "id": "call_function_5uh3ccwh4li3_1",
                        "name": "Glob",
                        "input": { "pattern": "**/*.md" }
                    }]
                },
                {
                    "role": "user",
                    "content": [{
                        "type": "tool_result",
                        "tool_use_id": "call_function_5uh3ccwh4li3_1",
                        "content": "README.md"
                    }]
                }
            ]
        }),
        true,
        "gpt-target",
    )
    .expect("mapping should succeed");

    let input = out["input"].as_array().expect("input should be an array");
    let call = input
        .iter()
        .find(|item| item.get("type").and_then(|v| v.as_str()) == Some("function_call"))
        .expect("function_call should exist");
    let call_output = input
        .iter()
        .find(|item| item.get("type").and_then(|v| v.as_str()) == Some("function_call_output"))
        .expect("function_call_output should exist");

    let call_id = call
        .get("call_id")
        .and_then(|v| v.as_str())
        .expect("function_call.call_id should be string");
    let output_call_id = call_output
        .get("call_id")
        .and_then(|v| v.as_str())
        .expect("function_call_output.call_id should be string");

    assert!(call_id.starts_with("fc"), "call_id={call_id}");
    assert_eq!(call["id"], call["call_id"]);
    assert_eq!(call_output["id"], call_output["call_id"]);
    assert_eq!(call_id, output_call_id);
    assert!(
        call["arguments"].is_string(),
        "function_call.arguments must be string for responses API"
    );
    assert_eq!(
        call["arguments"], "{\"pattern\":\"**/*.md\"}",
        "function_call.arguments should be serialized json string"
    );
}

#[test]
/// Performs Anthropic tool arguments follow schema key aliases for responses requests.
fn anthropic_tool_arguments_follow_schema_key_aliases_for_responses_requests() {
    let out = map_anthropic_to_openai_responses_request(
        &json!({
            "model": "claude-x",
            "tools": [{
                "name": "Read",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "file_path": { "type": "string" },
                        "offset": { "type": "integer" }
                    },
                    "required": ["file_path"]
                }
            }],
            "messages": [{
                "role": "assistant",
                "content": [{
                    "type": "tool_use",
                    "id": "call_read_1",
                    "name": "Read",
                    "input": {
                        "filePath": "/tmp/a.md",
                        "offset": 2
                    }
                }]
            }]
        }),
        true,
        "gpt-target",
    )
    .expect("mapping should succeed");

    let input = out["input"].as_array().expect("input should be an array");
    let call = input
        .iter()
        .find(|item| item.get("type").and_then(|v| v.as_str()) == Some("function_call"))
        .expect("function_call should exist");
    let args_str = call["arguments"]
        .as_str()
        .expect("function_call.arguments should be string");
    let args_json: Value =
        serde_json::from_str(args_str).expect("function_call.arguments should be valid json");

    assert_eq!(args_json["file_path"], "/tmp/a.md");
    assert_eq!(args_json["offset"], 2);
    assert!(
        args_json.get("filePath").is_none(),
        "non-schema alias key should be normalized away"
    );
}

#[test]
/// Performs Anthropic tool argument alias with ambiguous schema is not rewritten.
fn anthropic_tool_argument_alias_with_ambiguous_schema_is_not_rewritten() {
    let out = map_anthropic_to_openai_responses_request(
        &json!({
            "model": "claude-x",
            "tools": [{
                "name": "Ambiguous",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "file_path": { "type": "string" },
                        "filepath": { "type": "string" }
                    }
                }
            }],
            "messages": [{
                "role": "assistant",
                "content": [{
                    "type": "tool_use",
                    "id": "call_ambiguous_1",
                    "name": "Ambiguous",
                    "input": {
                        "file-path": "/tmp/a.md"
                    }
                }]
            }]
        }),
        true,
        "gpt-target",
    )
    .expect("mapping should succeed");

    let input = out["input"].as_array().expect("input should be an array");
    let call = input
        .iter()
        .find(|item| item.get("type").and_then(|v| v.as_str()) == Some("function_call"))
        .expect("function_call should exist");
    let args_str = call["arguments"]
        .as_str()
        .expect("function_call.arguments should be string");
    let args_json: Value =
        serde_json::from_str(args_str).expect("function_call.arguments should be valid json");

    assert_eq!(args_json["file-path"], "/tmp/a.md");
    assert!(
        args_json.get("file_path").is_none(),
        "ambiguous alias should remain unchanged"
    );
    assert!(
        args_json.get("filepath").is_none(),
        "ambiguous alias should remain unchanged"
    );
}

#[test]
/// Performs Anthropic system blocks map to string instructions.
fn anthropic_system_blocks_map_to_string_instructions() {
    let input = json!({
        "model": "claude-x",
        "system": [
            { "type": "text", "text": "first instruction" },
            { "type": "text", "text": "second instruction" }
        ],
        "messages": [{ "role": "user", "content": [{ "type": "text", "text": "hello" }] }],
        "stream": false
    });

    let out = map_anthropic_to_openai_responses_request(&input, true, "gpt-target")
        .expect("mapping should succeed");

    assert!(out["instructions"].is_string());
    assert_eq!(
        out["instructions"],
        "first instruction\n\nsecond instruction"
    );
}

#[test]
/// Performs Anthropic response maps to OpenAI response.
fn anthropic_response_maps_to_openai_response() {
    let input = json!({
        "id": "msg_1",
        "model": "claude-z",
        "content": [{ "type": "text", "text": "hi" }],
        "usage": { "input_tokens": 3, "output_tokens": 4 }
    });

    let out = map_anthropic_to_openai_response(&input, "m1");
    assert_eq!(out["choices"][0]["message"]["content"], "hi");
    assert_eq!(out["model"], "m1");
}

#[test]
/// Performs OpenAI response maps to Anthropic response.
fn openai_response_maps_to_anthropic_response() {
    let input = json!({
        "id": "chat_1",
        "model": "gpt-x",
        "choices": [{ "message": { "content": "ok" }, "finish_reason": "stop" }],
        "usage": { "prompt_tokens": 5, "completion_tokens": 2 }
    });

    let out = map_openai_to_anthropic_response(&input, "claude-m");
    assert_eq!(out["model"], "claude-m");
    assert_eq!(out["content"][0]["text"], "ok");
    assert_eq!(out["stop_reason"], "end_turn");
}

#[test]
/// Performs strict mode rejects unknown OpenAI fields.
fn strict_mode_rejects_unknown_openai_fields() {
    let input = json!({
        "model": "m",
        "messages": [],
        "unknown_a": true
    });
    let err = map_openai_to_anthropic_request(&input, true, "m").expect_err("should fail");
    assert!(err.contains("Unsupported OpenAI fields"));
}

#[test]
/// Performs responses input is normalized.
fn responses_input_is_normalized() {
    let normalized = normalize_openai_request(
        "/v1/responses",
        &json!({
            "model": "m",
            "input": "hello",
            "stream": false,
            "system": "sys",
            "thinking": { "type": "enabled" },
            "context_management": { "clear_function_results": false }
        }),
    );

    assert_eq!(normalized["messages"][0]["role"], "user");
    assert_eq!(normalized["messages"][0]["content"], "hello");
    assert_eq!(normalized["system"], "sys");
    assert_eq!(normalized["thinking"]["type"], "enabled");
    assert_eq!(
        normalized["context_management"]["clear_function_results"],
        false
    );
}

#[test]
/// Performs responses function call io is normalized to chat tool messages.
fn responses_function_call_io_is_normalized_to_chat_tool_messages() {
    let normalized = normalize_openai_request(
        "/v1/responses",
        &json!({
            "model": "m",
            "max_output_tokens": 2048,
            "instructions": "system prompt",
            "input": [
                {
                    "type": "function_call",
                    "call_id": "call_1",
                    "name": "weather_lookup",
                    "arguments": { "city": "sf" }
                },
                {
                    "type": "function_call_output",
                    "call_id": "call_1",
                    "output": [{ "type": "output_text", "text": "sunny" }]
                }
            ]
        }),
    );

    assert_eq!(normalized["max_tokens"], 2048);
    assert_eq!(normalized["system"], "system prompt");
    assert_eq!(normalized["messages"][0]["role"], "assistant");
    assert_eq!(normalized["messages"][0]["tool_calls"][0]["id"], "call_1");
    assert_eq!(normalized["messages"][1]["role"], "tool");
    assert_eq!(normalized["messages"][1]["tool_call_id"], "call_1");
}

#[test]
/// Performs strict mode allows OpenAI system thinking context fields.
fn strict_mode_allows_openai_system_thinking_context_fields() {
    let input = json!({
        "model": "m1",
        "messages": [{ "role": "user", "content": "hello" }],
        "system": "be concise",
        "thinking": { "type": "enabled" },
        "context_management": { "clear_function_results": false }
    });
    let out = map_openai_to_anthropic_request(&input, true, "claude-target")
        .expect("mapping should succeed");
    assert_eq!(out["system"], "be concise");
    assert_eq!(out["thinking"]["type"], "enabled");
    assert_eq!(out["context_management"]["clear_function_results"], false);
}

#[test]
/// Performs strict mode allows Anthropic thinking context fields.
fn strict_mode_allows_anthropic_thinking_context_fields() {
    let input = json!({
        "model": "claude-x",
        "messages": [{ "role": "user", "content": [{ "type": "text", "text": "hello" }] }],
        "thinking": { "type": "enabled" },
        "context_management": { "clear_function_results": false }
    });
    let out = map_anthropic_to_openai_request(&input, true, "gpt-target")
        .expect("mapping should succeed");
    assert_eq!(out["model"], "gpt-target");
    assert_eq!(out["messages"][0]["role"], "user");
}

#[test]
/// Performs chat response to responses keeps tool calls.
fn chat_response_to_responses_keeps_tool_calls() {
    let mapped = map_openai_chat_to_responses(&json!({
        "id": "chatcmpl_1",
        "created": 123456,
        "model": "gpt-4.1",
        "choices": [{
            "message": {
                "role": "assistant",
                "content": "I will call a tool",
                "tool_calls": [
                    {
                        "id": "call_1",
                        "type": "function",
                        "function": {
                            "name": "weather_lookup",
                            "arguments": "{\"city\":\"sf\"}"
                        }
                    }
                ]
            }
        }]
    }));

    assert_eq!(mapped["object"], "response");
    assert_eq!(mapped["output"][0]["type"], "message");
    assert_eq!(mapped["output"][1]["type"], "function_call");
    assert_eq!(mapped["output"][1]["name"], "weather_lookup");
    assert_eq!(mapped["status"], "completed");
    assert_eq!(mapped["usage"]["input_tokens"], 0);
    assert_eq!(mapped["usage"]["output_tokens"], 0);
}

#[test]
/// Performs OpenAI tool message maps to Anthropic tool result.
fn openai_tool_message_maps_to_anthropic_tool_result() {
    let out = map_openai_to_anthropic_request(
        &json!({
            "model": "m",
            "messages": [
                {
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {
                            "name": "weather_lookup",
                            "arguments": "{\"city\":\"sf\"}"
                        }
                    }]
                },
                {
                    "role": "tool",
                    "tool_call_id": "call_1",
                    "content": "sunny"
                }
            ]
        }),
        true,
        "claude-target",
    )
    .expect("mapping should succeed");

    assert_eq!(out["messages"][0]["role"], "assistant");
    assert_eq!(out["messages"][0]["content"][0]["type"], "tool_use");
    assert_eq!(out["messages"][1]["role"], "user");
    assert_eq!(out["messages"][1]["content"][0]["type"], "tool_result");
    assert_eq!(out["messages"][1]["content"][0]["tool_use_id"], "call_1");
    assert_eq!(out["messages"][1]["content"][0]["content"], "sunny");
}

#[test]
/// Performs Anthropic tool result maps to OpenAI tool message.
fn anthropic_tool_result_maps_to_openai_tool_message() {
    let out = map_anthropic_to_openai_request(
        &json!({
            "model": "claude-x",
            "messages": [
                {
                    "role": "assistant",
                    "content": [{
                        "type": "tool_use",
                        "id": "toolu_1",
                        "name": "weather_lookup",
                        "input": { "city": "sf" }
                    }]
                },
                {
                    "role": "user",
                    "content": [{
                        "type": "tool_result",
                        "tool_use_id": "toolu_1",
                        "content": [{ "type": "text", "text": "sunny" }]
                    }]
                }
            ]
        }),
        true,
        "gpt-target",
    )
    .expect("mapping should succeed");

    assert_eq!(out["messages"][0]["role"], "assistant");
    assert_eq!(out["messages"][0]["tool_calls"][0]["id"], "toolu_1");
    assert_eq!(out["messages"][1]["role"], "tool");
    assert_eq!(out["messages"][1]["tool_call_id"], "toolu_1");
    assert_eq!(out["messages"][1]["content"], "sunny");
}

#[test]
/// Performs OpenAI finish reason tool calls maps to Anthropic tool use.
fn openai_finish_reason_tool_calls_maps_to_anthropic_tool_use() {
    let out = map_openai_to_anthropic_response(
        &json!({
            "id": "chat_2",
            "model": "gpt-x",
            "choices": [{ "message": { "content": "" }, "finish_reason": "tool_calls" }],
            "usage": { "prompt_tokens": 1, "completion_tokens": 1 }
        }),
        "claude-m",
    );
    assert_eq!(out["stop_reason"], "tool_use");
}

#[test]
/// Runs a unit test for the expected behavior contract.
fn contract_openai_to_anthropic_request_snapshot() {
    let input: Value = serde_json::from_str(include_str!(
        "../contract_fixtures/mappers/openai_to_anthropic_request.input.json"
    ))
    .expect("contract input must be valid json");
    let expected: Value = serde_json::from_str(include_str!(
        "../contract_fixtures/mappers/openai_to_anthropic_request.expected.json"
    ))
    .expect("contract expected must be valid json");

    let actual = map_openai_to_anthropic_request(&input, true, "claude-3-5-sonnet")
        .expect("mapping should succeed");
    assert_eq!(actual, expected);
}
