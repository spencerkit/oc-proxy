use super::{
    map_anthropic_to_openai_request, map_anthropic_to_openai_response,
    map_openai_chat_to_responses, map_openai_to_anthropic_request,
    map_openai_to_anthropic_response, normalize_openai_request,
};
use serde_json::json;

#[test]
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
