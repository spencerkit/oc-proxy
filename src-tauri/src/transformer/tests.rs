#[cfg(test)]
mod tests {
    use crate::transformer::convert::{claude_openai, openai_claude, openai_chat_responses};
    use serde_json::json;

    #[test]
    fn test_claude_to_openai_request() {
        let claude_req = json!({
            "model": "claude-3-5-sonnet-20241022",
            "max_tokens": 1024,
            "messages": [
                {"role": "user", "content": "Hello"}
            ]
        });

        let result = claude_openai::claude_req_to_openai(
            serde_json::to_vec(&claude_req).unwrap().as_slice(),
            "gpt-4"
        );

        assert!(result.is_ok());
        let openai_req: serde_json::Value = serde_json::from_slice(&result.unwrap()).unwrap();
        assert_eq!(openai_req["model"], "gpt-4");
        assert_eq!(openai_req["messages"][0]["role"], "user");
        assert_eq!(openai_req["messages"][0]["content"], "Hello");
    }

    #[test]
    fn test_openai_to_claude_response() {
        let openai_resp = json!({
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "created": 1677652288,
            "model": "gpt-4",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Hello! How can I help you?"
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 20,
                "total_tokens": 30
            }
        });

        let result = openai_claude::openai_resp_to_claude(
            serde_json::to_vec(&openai_resp).unwrap().as_slice()
        );

        assert!(result.is_ok());
        let claude_resp: serde_json::Value = serde_json::from_slice(&result.unwrap()).unwrap();
        assert_eq!(claude_resp["type"], "message");
        assert_eq!(claude_resp["role"], "assistant");
        assert_eq!(claude_resp["content"][0]["type"], "text");
        assert_eq!(claude_resp["content"][0]["text"], "Hello! How can I help you?");
        assert_eq!(claude_resp["stop_reason"], "end_turn");
    }

    #[test]
    fn test_chat_to_responses_with_system_message() {
        let chat_req = json!({
            "model": "gpt-4",
            "messages": [
                {"role": "system", "content": "You are a helpful assistant"},
                {"role": "user", "content": "Hello"}
            ]
        });

        let result = openai_chat_responses::openai_chat_to_responses(
            serde_json::to_vec(&chat_req).unwrap().as_slice(),
            "gpt-4"
        );

        assert!(result.is_ok());
        let resp_req: serde_json::Value = serde_json::from_slice(&result.unwrap()).unwrap();
        assert_eq!(resp_req["instructions"], "You are a helpful assistant");
        assert_eq!(resp_req["input"].as_array().unwrap().len(), 1);
        assert_eq!(resp_req["input"][0]["type"], "message");
        assert_eq!(resp_req["input"][0]["role"], "user");
    }

    #[test]
    fn test_chat_to_responses_with_tool_calls() {
        let chat_req = json!({
            "model": "gpt-4",
            "messages": [
                {"role": "user", "content": "What's the weather?"},
                {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_123",
                        "type": "function",
                        "function": {
                            "name": "get_weather",
                            "arguments": "{\"location\":\"NYC\"}"
                        }
                    }]
                },
                {
                    "role": "tool",
                    "tool_call_id": "call_123",
                    "content": "Sunny, 72°F"
                }
            ]
        });

        let result = openai_chat_responses::openai_chat_to_responses(
            serde_json::to_vec(&chat_req).unwrap().as_slice(),
            "gpt-4"
        );

        assert!(result.is_ok());
        let resp_req: serde_json::Value = serde_json::from_slice(&result.unwrap()).unwrap();
        let input = resp_req["input"].as_array().unwrap();

        // Should have user message, function_call, and function_call_output
        assert_eq!(input.len(), 3);
        assert_eq!(input[0]["type"], "message");
        assert_eq!(input[1]["type"], "function_call");
        assert_eq!(input[1]["call_id"], "call_123");
        assert_eq!(input[1]["name"], "get_weather");
        assert_eq!(input[2]["type"], "function_call_output");
        assert_eq!(input[2]["call_id"], "call_123");
        assert_eq!(input[2]["output"], "Sunny, 72°F");
    }

    #[test]
    fn test_chat_to_responses_with_tools() {
        let chat_req = json!({
            "model": "gpt-4",
            "messages": [
                {"role": "user", "content": "Hello"}
            ],
            "tools": [{
                "type": "function",
                "function": {
                    "name": "get_weather",
                    "description": "Get weather info",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "location": {"type": "string"}
                        }
                    }
                }
            }]
        });

        let result = openai_chat_responses::openai_chat_to_responses(
            serde_json::to_vec(&chat_req).unwrap().as_slice(),
            "gpt-4"
        );

        assert!(result.is_ok());
        let resp_req: serde_json::Value = serde_json::from_slice(&result.unwrap()).unwrap();
        assert!(resp_req["tools"].is_array());
        let tools = resp_req["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["type"], "function");
        assert_eq!(tools[0]["name"], "get_weather");
        assert_eq!(tools[0]["description"], "Get weather info");
    }

    #[test]
    fn test_responses_to_chat_with_tool_calls() {
        let resp = json!({
            "id": "resp_123",
            "model": "gpt-4",
            "output": [
                {
                    "type": "message",
                    "id": "msg_123",
                    "role": "assistant",
                    "content": [
                        {"type": "output_text", "text": "Let me check the weather."}
                    ]
                },
                {
                    "type": "function_call",
                    "id": "call_456",
                    "call_id": "call_456",
                    "name": "get_weather",
                    "arguments": "{\"location\":\"NYC\"}"
                }
            ],
            "usage": {
                "input_tokens": 10,
                "output_tokens": 20
            }
        });

        let result = openai_chat_responses::openai_responses_to_chat(
            serde_json::to_vec(&resp).unwrap().as_slice()
        );

        assert!(result.is_ok());
        let chat_resp: serde_json::Value = serde_json::from_slice(&result.unwrap()).unwrap();

        assert_eq!(chat_resp["choices"][0]["finish_reason"], "tool_calls");

        let message = &chat_resp["choices"][0]["message"];
        assert_eq!(message["role"], "assistant");
        assert_eq!(message["content"], "Let me check the weather.");

        let tool_calls = message["tool_calls"].as_array().unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0]["id"], "call_456");
        assert_eq!(tool_calls[0]["function"]["name"], "get_weather");
        assert_eq!(tool_calls[0]["function"]["arguments"], "{\"location\":\"NYC\"}");

        let usage = &chat_resp["usage"];
        assert_eq!(usage["prompt_tokens"], 10);
        assert_eq!(usage["completion_tokens"], 20);
        assert_eq!(usage["total_tokens"], 30);
    }

    #[test]
    fn test_responses_req_to_chat_with_tools() {
        let resp_req = json!({
            "model": "gpt-4",
            "instructions": "You are helpful",
            "input": [
                {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "Hello"}]},
                {
                    "type": "function_call",
                    "id": "call_123",
                    "call_id": "call_123",
                    "name": "get_weather",
                    "arguments": "{\"loc\":\"NYC\"}"
                },
                {
                    "type": "function_call_output",
                    "call_id": "call_123",
                    "output": "Sunny"
                }
            ],
            "tools": [{
                "type": "function",
                "name": "get_weather",
                "description": "Get weather",
                "parameters": {"type": "object"}
            }]
        });

        let result = openai_chat_responses::openai_responses_req_to_chat(
            serde_json::to_vec(&resp_req).unwrap().as_slice(),
            "gpt-4"
        );

        assert!(result.is_ok());
        let chat_req: serde_json::Value = serde_json::from_slice(&result.unwrap()).unwrap();

        let messages = chat_req["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 4);
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[0]["content"], "You are helpful");
        assert_eq!(messages[1]["role"], "user");
        assert_eq!(messages[2]["role"], "assistant");
        assert!(messages[2]["tool_calls"].is_array());
        assert_eq!(messages[3]["role"], "tool");

        let tools = chat_req["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["function"]["name"], "get_weather");
    }

    #[test]
    fn test_chat_resp_to_responses_with_tool_calls() {
        let chat_resp = json!({
            "id": "chatcmpl-123",
            "model": "gpt-4",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Checking weather",
                    "tool_calls": [{
                        "id": "call_789",
                        "type": "function",
                        "function": {
                            "name": "get_weather",
                            "arguments": "{\"city\":\"LA\"}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {
                "prompt_tokens": 5,
                "completion_tokens": 15
            }
        });

        let result = openai_chat_responses::openai_chat_resp_to_responses(
            serde_json::to_vec(&chat_resp).unwrap().as_slice()
        );

        assert!(result.is_ok());
        let resp: serde_json::Value = serde_json::from_slice(&result.unwrap()).unwrap();

        assert_eq!(resp["status"], "completed");

        let output = resp["output"].as_array().unwrap();
        // Should have message item and function_call item
        assert_eq!(output.len(), 2);

        assert_eq!(output[0]["type"], "message");
        assert_eq!(output[0]["content"][0]["type"], "output_text");
        assert_eq!(output[0]["content"][0]["text"], "Checking weather");

        assert_eq!(output[1]["type"], "function_call");
        assert_eq!(output[1]["id"], "call_789");
        assert_eq!(output[1]["call_id"], "call_789");
        assert_eq!(output[1]["name"], "get_weather");
        assert_eq!(output[1]["arguments"], "{\"city\":\"LA\"}");
        assert_eq!(output[1]["status"], "completed");

        assert_eq!(resp["usage"]["total_tokens"], 20);
    }

    #[test]
    fn test_responses_req_to_chat_maps_developer_role() {
        let resp_req = json!({
            "model": "gpt-4",
            "input": [
                {"type": "message", "role": "developer", "content": [{"type": "input_text", "text": "System instruction"}]},
                {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "Hello"}]},
                {"type": "message", "role": "developer", "content": [{"type": "input_text", "text": "Another instruction"}]}
            ]
        });

        let result = openai_chat_responses::openai_responses_req_to_chat(
            serde_json::to_vec(&resp_req).unwrap().as_slice(),
            "gpt-4"
        );

        assert!(result.is_ok());
        let chat_req: serde_json::Value = serde_json::from_slice(&result.unwrap()).unwrap();

        let messages = chat_req["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 3);

        // developer role should be mapped to user
        assert_eq!(messages[0]["role"], "user");
        assert_eq!(messages[0]["content"], "System instruction");

        assert_eq!(messages[1]["role"], "user");
        assert_eq!(messages[1]["content"], "Hello");

        assert_eq!(messages[2]["role"], "user");
        assert_eq!(messages[2]["content"], "Another instruction");
    }
}
