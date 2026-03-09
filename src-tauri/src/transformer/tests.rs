#[cfg(test)]
mod tests {
    use crate::transformer::convert::{
        claude_openai, claude_openai_responses, claude_openai_responses_stream,
        openai_chat_responses, openai_claude,
    };
    use crate::transformer::types::StreamContext;
    use crate::transformer::{cx, Transformer};
    use serde_json::json;
    use std::collections::HashSet;

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
            "gpt-4",
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
            serde_json::to_vec(&openai_resp).unwrap().as_slice(),
        );

        assert!(result.is_ok());
        let claude_resp: serde_json::Value = serde_json::from_slice(&result.unwrap()).unwrap();
        assert_eq!(claude_resp["type"], "message");
        assert_eq!(claude_resp["role"], "assistant");
        assert_eq!(claude_resp["content"][0]["type"], "text");
        assert_eq!(
            claude_resp["content"][0]["text"],
            "Hello! How can I help you?"
        );
        assert_eq!(claude_resp["stop_reason"], "end_turn");
    }

    #[test]
    fn test_openai_req_to_claude_maps_system_tool_calls_and_tool_results() {
        let openai_req = json!({
            "model": "gpt-4.1",
            "stream": true,
            "messages": [
                {"role": "system", "content": "You are helpful"},
                {"role": "user", "content": "What is the weather?"},
                {
                    "role": "assistant",
                    "content": "Checking now",
                    "tool_calls": [{
                        "id": "call_123",
                        "type": "function",
                        "function": {
                            "name": "get_weather",
                            "arguments": "{\"city\":\"LA\"}"
                        }
                    }]
                },
                {
                    "role": "tool",
                    "tool_call_id": "call_123",
                    "content": "Sunny"
                }
            ],
            "tools": [{
                "type": "function",
                "function": {
                    "name": "get_weather",
                    "description": "Fetch weather",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "city": {"type": "string"}
                        }
                    }
                }
            }]
        });

        let result = claude_openai::openai_req_to_claude(
            serde_json::to_vec(&openai_req).unwrap().as_slice(),
            "claude-sonnet-4-6",
        );

        assert!(result.is_ok());
        let claude_req: serde_json::Value = serde_json::from_slice(&result.unwrap()).unwrap();
        let messages = claude_req["messages"].as_array().unwrap();

        assert_eq!(claude_req["model"], "claude-sonnet-4-6");
        assert_eq!(claude_req["system"], "You are helpful");
        assert_eq!(messages[0]["role"], "user");
        assert_eq!(messages[0]["content"], "What is the weather?");
        assert_eq!(messages[1]["role"], "assistant");
        assert_eq!(messages[1]["content"][0]["type"], "text");
        assert_eq!(messages[1]["content"][0]["text"], "Checking now");
        assert_eq!(messages[1]["content"][1]["type"], "tool_use");
        assert_eq!(messages[1]["content"][1]["id"], "call_123");
        assert_eq!(messages[1]["content"][1]["name"], "get_weather");
        assert_eq!(messages[2]["role"], "user");
        assert_eq!(messages[2]["content"][0]["type"], "tool_result");
        assert_eq!(messages[2]["content"][0]["tool_use_id"], "call_123");
        assert_eq!(messages[2]["content"][0]["content"], "Sunny");
        assert_eq!(claude_req["tools"][0]["name"], "get_weather");
        assert_eq!(claude_req["stream"], true);
    }

    #[test]
    fn test_claude_resp_to_openai_maps_tool_use_and_finish_reason() {
        let claude_resp = json!({
            "id": "msg_123",
            "type": "message",
            "role": "assistant",
            "content": [
                {"type": "text", "text": "Checking weather"},
                {
                    "type": "tool_use",
                    "id": "call_456",
                    "name": "get_weather",
                    "input": {"city": "LA"}
                }
            ],
            "model": "claude-sonnet-4-6",
            "stop_reason": "tool_use",
            "usage": {
                "input_tokens": 12,
                "output_tokens": 8
            }
        });

        let result = claude_openai::claude_resp_to_openai(
            serde_json::to_vec(&claude_resp).unwrap().as_slice(),
            "gpt-4.1",
        );

        assert!(result.is_ok());
        let openai_resp: serde_json::Value = serde_json::from_slice(&result.unwrap()).unwrap();
        let message = &openai_resp["choices"][0]["message"];

        assert_eq!(openai_resp["model"], "gpt-4.1");
        assert_eq!(message["role"], "assistant");
        assert_eq!(message["content"], "Checking weather");
        assert_eq!(openai_resp["choices"][0]["finish_reason"], "tool_calls");
        assert_eq!(message["tool_calls"][0]["id"], "call_456");
        assert_eq!(message["tool_calls"][0]["function"]["name"], "get_weather");
        assert_eq!(
            message["tool_calls"][0]["function"]["arguments"],
            "{\"city\":\"LA\"}"
        );
        assert_eq!(openai_resp["usage"]["total_tokens"], 20);
    }

    #[test]
    fn test_openai_stream_to_claude_emits_text_and_tool_use_flow() {
        let mut ctx = StreamContext::new();
        ctx.model_name = "gpt-4.1".to_string();

        let created = b"data: {\"id\":\"chatcmpl_1\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"gpt-4.1\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Checking \"},\"finish_reason\":null}]}\n\n";
        let tool_start = b"data: {\"id\":\"chatcmpl_1\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"gpt-4.1\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"type\":\"function\",\"function\":{\"name\":\"get_weather\",\"arguments\":\"\"}}]},\"finish_reason\":null}]}\n\n";
        let tool_args = b"data: {\"id\":\"chatcmpl_1\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"gpt-4.1\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"\",\"type\":\"function\",\"function\":{\"name\":\"\",\"arguments\":\"{\\\"city\\\":\\\"LA\\\"}\"}}]},\"finish_reason\":null}]}\n\n";
        let finished = b"data: {\"id\":\"chatcmpl_1\",\"object\":\"chat.completion.chunk\",\"created\":1,\"model\":\"gpt-4.1\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n";
        let done = b"data: [DONE]\n\n";

        let mut full = String::new();
        for chunk in [
            created.as_slice(),
            tool_start.as_slice(),
            tool_args.as_slice(),
            finished.as_slice(),
            done.as_slice(),
        ] {
            let converted = claude_openai::openai_stream_to_claude(chunk, &mut ctx).unwrap();
            full.push_str(&String::from_utf8(converted).unwrap());
        }

        assert!(full.contains("\"type\":\"message_start\""));
        assert!(full.contains("\"type\":\"text_delta\""));
        assert!(full.contains("\"text\":\"Checking \""));
        assert!(full.contains("\"type\":\"tool_use\""));
        assert!(full.contains("\"name\":\"get_weather\""));
        assert!(full.contains("\"partial_json\":\"{\\\"city\\\":\\\"LA\\\"}\""));
        assert!(full.contains("\"stop_reason\":\"tool_use\""));
        assert!(full.contains("\"type\":\"message_stop\""));
    }

    #[test]
    fn test_cx_chat_openai_passthrough_only_overrides_model() {
        let transformer = cx::chat::openai::OpenAITransformer::new("gpt-5-chat".to_string());
        let request = json!({
            "model": "original",
            "messages": [{"role": "user", "content": "hello"}],
            "stream": true
        });
        let request_bytes = serde_json::to_vec(&request).unwrap();

        let transformed = transformer.transform_request(&request_bytes).unwrap();
        let transformed_json: serde_json::Value = serde_json::from_slice(&transformed).unwrap();
        assert_eq!(transformed_json["model"], "gpt-5-chat");
        assert_eq!(transformed_json["messages"], request["messages"]);
        assert_eq!(
            transformer
                .transform_response(b"{\"ok\":true}", false)
                .unwrap(),
            b"{\"ok\":true}"
        );
    }

    #[test]
    fn test_cx_resp_openai2_passthrough_only_overrides_model() {
        let transformer = cx::responses::openai2::OpenAI2Transformer::new("gpt-5-resp".to_string());
        let request = json!({
            "model": "original",
            "input": [{"type": "message", "role": "user", "content": [{"type": "input_text", "text": "hello"}]}],
            "stream": true
        });
        let request_bytes = serde_json::to_vec(&request).unwrap();

        let transformed = transformer.transform_request(&request_bytes).unwrap();
        let transformed_json: serde_json::Value = serde_json::from_slice(&transformed).unwrap();
        assert_eq!(transformed_json["model"], "gpt-5-resp");
        assert_eq!(transformed_json["input"], request["input"]);
        assert_eq!(
            transformer
                .transform_response(b"{\"ok\":true}", false)
                .unwrap(),
            b"{\"ok\":true}"
        );
    }

    #[test]
    fn test_claude_messages_to_responses_preserves_tool_chain() {
        let claude_req = json!({
            "model": "claude-sonnet-4-6",
            "messages": [
                {"role": "user", "content": [{"type": "text", "text": "run tool"}]},
                {"role": "assistant", "content": [{"type": "tool_use", "id": "call_1", "name": "list_files", "input": {"path": "."}}]},
                {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "call_1", "content": "file_a\\nfile_b"}]}
            ],
            "stream": true
        });

        let result = claude_openai_responses::claude_req_to_openai_responses(
            serde_json::to_vec(&claude_req).unwrap().as_slice(),
            "gpt-5.2",
        );

        assert!(result.is_ok());
        let responses_req: serde_json::Value = serde_json::from_slice(&result.unwrap()).unwrap();
        let input = responses_req["input"].as_array().unwrap();

        assert_eq!(input[0]["type"], "message");
        assert_eq!(input[0]["role"], "user");
        assert_eq!(input[0]["content"][0]["type"], "input_text");

        assert_eq!(input[1]["type"], "function_call");
        assert_eq!(input[1]["call_id"], "call_1");
        assert_eq!(input[1]["name"], "list_files");
        let args = input[1]["arguments"].as_str().unwrap_or("");
        assert!(args.contains("\"path\":\".\""));

        assert_eq!(input[2]["type"], "function_call_output");
        assert_eq!(input[2]["call_id"], "call_1");
        let output = input[2]["output"].as_str().unwrap_or("");
        assert!(output.contains("file_a"));
        assert!(output.contains("file_b"));

        let serialized = serde_json::to_string(&responses_req).unwrap();
        assert!(!serialized.contains("[Tool Call:"));
        assert!(!serialized.contains("[Tool Result:"));
    }

    #[test]
    fn test_claude_messages_to_responses_maps_assistant_text_to_output_text() {
        let claude_req = json!({
            "model": "claude-sonnet-4-6",
            "messages": [
                {"role": "user", "content": [{"type": "text", "text": "hello"}]},
                {"role": "assistant", "content": [{"type": "text", "text": "I can help."}]}
            ],
            "stream": false
        });

        let result = claude_openai_responses::claude_req_to_openai_responses(
            serde_json::to_vec(&claude_req).unwrap().as_slice(),
            "gpt-5.2",
        );

        assert!(result.is_ok());
        let responses_req: serde_json::Value = serde_json::from_slice(&result.unwrap()).unwrap();
        let input = responses_req["input"].as_array().unwrap();
        assert_eq!(input[1]["role"], "assistant");
        assert_eq!(input[1]["content"][0]["type"], "output_text");
        assert_eq!(input[1]["content"][0]["text"], "I can help.");
    }

    #[test]
    fn test_claude_messages_to_responses_maps_tool_choice_any_to_required() {
        let claude_req = json!({
            "model": "claude-sonnet-4-6",
            "messages": [
                {"role": "user", "content": [{"type": "text", "text": "test"}]}
            ],
            "tools": [{
                "name": "Write",
                "description": "Write file",
                "input_schema": {
                    "type": "object"
                }
            }],
            "tool_choice": {"type": "any"},
            "stream": true
        });

        let result = claude_openai_responses::claude_req_to_openai_responses(
            serde_json::to_vec(&claude_req).unwrap().as_slice(),
            "gpt-5.2",
        );

        assert!(result.is_ok());
        let responses_req: serde_json::Value = serde_json::from_slice(&result.unwrap()).unwrap();
        assert_eq!(responses_req["tool_choice"], "required");
    }

    #[test]
    fn test_claude_messages_to_responses_defaults_tool_choice_auto_after_tool_result() {
        let claude_req = json!({
            "model": "claude-sonnet-4-6",
            "messages": [
                {"role": "user", "content": [{"type": "text", "text": "run tool"}]},
                {"role": "assistant", "content": [{"type": "tool_use", "id": "call_1", "name": "Write", "input": {"path": "."}}]},
                {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "call_1", "content": "ok"}]}
            ],
            "tools": [{
                "name": "Write",
                "description": "Write file",
                "input_schema": {
                    "type": "object"
                }
            }],
            "stream": false
        });

        let result = claude_openai_responses::claude_req_to_openai_responses(
            serde_json::to_vec(&claude_req).unwrap().as_slice(),
            "gpt-5.2",
        )
        .expect("convert");
        let responses_req: serde_json::Value = serde_json::from_slice(&result).unwrap();

        assert_eq!(responses_req["tool_choice"], "auto");
    }

    #[test]
    fn test_claude_messages_to_responses_defaults_stream_to_true_when_missing_or_null() {
        let missing_stream_req = json!({
            "model": "claude-sonnet-4-6",
            "messages": [
                {"role": "user", "content": [{"type": "text", "text": "hello"}]}
            ]
        });

        let missing_result = claude_openai_responses::claude_req_to_openai_responses(
            serde_json::to_vec(&missing_stream_req).unwrap().as_slice(),
            "gpt-5.2",
        )
        .expect("convert missing stream");
        let missing_json: serde_json::Value = serde_json::from_slice(&missing_result).unwrap();
        assert_eq!(missing_json["stream"], true);

        let null_stream_req = json!({
            "model": "claude-sonnet-4-6",
            "messages": [
                {"role": "user", "content": [{"type": "text", "text": "hello"}]}
            ],
            "stream": null
        });

        let null_result = claude_openai_responses::claude_req_to_openai_responses(
            serde_json::to_vec(&null_stream_req).unwrap().as_slice(),
            "gpt-5.2",
        )
        .expect("convert null stream");
        let null_json: serde_json::Value = serde_json::from_slice(&null_result).unwrap();
        assert_eq!(null_json["stream"], true);
    }

    #[test]
    fn test_openai_responses_req_to_claude_accepts_output_text_message_parts() {
        let openai_req = json!({
            "model": "gpt-5.2",
            "input": [
                {"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "tool says hi"}]},
                {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "continue"}]}
            ]
        });

        let result = claude_openai_responses::openai_responses_req_to_claude(
            serde_json::to_vec(&openai_req).unwrap().as_slice(),
            "claude-sonnet-4-6",
        );

        assert!(result.is_ok());
        let claude_req: serde_json::Value = serde_json::from_slice(&result.unwrap()).unwrap();
        let messages = claude_req["messages"].as_array().unwrap();
        assert_eq!(messages[0]["role"], "assistant");
        assert_eq!(messages[0]["content"], "tool says hi");
        assert_eq!(messages[1]["role"], "user");
        assert_eq!(messages[1]["content"], "continue");
    }

    #[test]
    fn test_openai_responses_req_to_claude_supports_custom_tools_and_limits() {
        let openai_req = json!({
            "model": "gpt-5.2",
            "stream": true,
            "max_output_tokens": 256,
            "temperature": 0.2,
            "tools": [{
                "type": "custom",
                "name": "Bash",
                "description": "Run shell command"
            }]
        });

        let result = claude_openai_responses::openai_responses_req_to_claude(
            serde_json::to_vec(&openai_req).unwrap().as_slice(),
            "claude-sonnet-4-6",
        )
        .expect("convert");
        let claude_req: serde_json::Value = serde_json::from_slice(&result).unwrap();

        assert_eq!(claude_req["max_tokens"], 256);
        assert_eq!(claude_req["temperature"], 0.2);
        assert_eq!(claude_req["tools"][0]["name"], "Bash");
        assert_eq!(
            claude_req["tools"][0]["input_schema"]["properties"]["input"]["type"],
            "string"
        );
    }

    #[test]
    fn test_openai_responses_req_to_claude_preserves_tools_without_description() {
        let openai_req = json!({
            "model": "gpt-5.2",
            "tools": [
                {
                    "type": "function",
                    "name": "Read",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "path": {"type": "string"}
                        }
                    }
                },
                {
                    "type": "custom",
                    "name": "Bash"
                }
            ]
        });

        let result = claude_openai_responses::openai_responses_req_to_claude(
            serde_json::to_vec(&openai_req).unwrap().as_slice(),
            "claude-sonnet-4-6",
        )
        .expect("convert");
        let claude_req: serde_json::Value = serde_json::from_slice(&result).unwrap();
        let tools = claude_req["tools"].as_array().expect("tools array");

        assert_eq!(tools.len(), 2);
        assert_eq!(tools[0]["name"], "Read");
        assert_eq!(tools[0]["description"], "");
        assert_eq!(
            tools[0]["input_schema"]["properties"]["path"]["type"],
            "string"
        );
        assert_eq!(tools[1]["name"], "Bash");
        assert_eq!(tools[1]["description"], "");
        assert_eq!(
            tools[1]["input_schema"]["properties"]["input"]["type"],
            "string"
        );
    }

    #[test]
    fn test_claude_resp_to_openai_responses_maps_text_and_tool_use() {
        let claude_resp = json!({
            "id": "msg_1",
            "type": "message",
            "role": "assistant",
            "content": [
                {"type": "text", "text": "Working on it"},
                {"type": "tool_use", "id": "call_1", "name": "list_files", "input": {"path": "."}}
            ],
            "model": "claude-sonnet-4-6",
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 11, "output_tokens": 7}
        });

        let result = claude_openai_responses::claude_resp_to_openai_responses(
            serde_json::to_vec(&claude_resp).unwrap().as_slice(),
        );

        assert!(result.is_ok());
        let responses_resp: serde_json::Value = serde_json::from_slice(&result.unwrap()).unwrap();
        assert_eq!(responses_resp["object"], "response");
        assert_eq!(responses_resp["status"], "completed");
        assert_eq!(responses_resp["output"][0]["type"], "message");
        assert_eq!(
            responses_resp["output"][0]["content"][0]["type"],
            "output_text"
        );
        assert_eq!(
            responses_resp["output"][0]["content"][0]["text"],
            "Working on it"
        );
        assert_eq!(responses_resp["output"][1]["type"], "function_call");
        assert_eq!(responses_resp["output"][1]["call_id"], "call_1");
        assert_eq!(responses_resp["output"][1]["name"], "list_files");
        let args = responses_resp["output"][1]["arguments"]
            .as_str()
            .unwrap_or("");
        assert!(args.contains("\"path\":\".\""));
        assert_eq!(responses_resp["usage"]["input_tokens"], 11);
        assert_eq!(responses_resp["usage"]["output_tokens"], 7);
        assert_eq!(responses_resp["usage"]["total_tokens"], 18);
    }

    #[test]
    fn test_claude_stream_to_openai_responses_propagates_upstream_error_event() {
        let mut ctx = StreamContext::new();
        let event = b"event: message_start\ndata: {\"type\":\"error\",\"error\":{\"message\":\"upstream boom\"}}\n\n";

        let result =
            claude_openai_responses_stream::claude_stream_to_openai_responses(event, &mut ctx);
        assert!(result.is_err());
        let err = result.err().unwrap_or_default();
        assert!(err.contains("upstream error"));
        assert!(err.contains("upstream boom"));
    }

    #[test]
    fn test_openai_responses_stream_to_claude_emits_text_flow() {
        let mut ctx = StreamContext::new();
        ctx.model_name = "claude-sonnet-4-6".to_string();

        let created = b"event: response.created\ndata: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\",\"status\":\"in_progress\",\"instructions\":\"keep raw created\"}}\n\n";
        let added = b"event: response.output_item.added\ndata: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\",\"role\":\"assistant\"}}\n\n";
        let delta = b"event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"delta\":\"hello\"}\n\n";
        let completed = b"event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"usage\":{\"output_tokens\":5}}}\n\n";
        let done = b"data: [DONE]\n\n";

        let mut out = Vec::new();
        out.extend(
            claude_openai_responses_stream::openai_responses_stream_to_claude(created, &mut ctx)
                .expect("created"),
        );
        out.extend(
            claude_openai_responses_stream::openai_responses_stream_to_claude(added, &mut ctx)
                .expect("added"),
        );
        out.extend(
            claude_openai_responses_stream::openai_responses_stream_to_claude(delta, &mut ctx)
                .expect("delta"),
        );
        out.extend(
            claude_openai_responses_stream::openai_responses_stream_to_claude(completed, &mut ctx)
                .expect("completed"),
        );

        let s = String::from_utf8(out.clone()).expect("utf8");
        assert!(s.contains("event: message_start"));
        assert!(s.contains("\"type\":\"message_start\""));
        assert!(s.contains("\"id\":\"resp_1\""));
        assert!(s.contains("event: content_block_start"));
        assert!(s.contains("\"type\":\"content_block_start\""));
        assert!(s.contains("\"type\":\"text\""));
        assert!(s.contains("\"type\":\"content_block_delta\""));
        assert!(s.contains("\"type\":\"text_delta\""));
        assert!(s.contains("\"text\":\"hello\""));
        assert!(s.contains("event: content_block_stop"));
        assert!(s.contains("event: message_delta"));
        assert!(s.contains("\"stop_reason\":\"end_turn\""));
        assert!(s.contains("event: message_stop"));

        let done_out =
            claude_openai_responses_stream::openai_responses_stream_to_claude(done, &mut ctx)
                .expect("done");
        assert!(done_out.is_empty());

        let after_done = String::from_utf8(out).expect("utf8");
        assert_eq!(after_done.matches("event: message_stop").count(), 1);
    }

    #[test]
    fn test_openai_responses_stream_to_claude_splits_think_tags() {
        let mut ctx = StreamContext::new();
        ctx.model_name = "claude-sonnet-4-6".to_string();

        let created = b"event: response.created\ndata: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\"}}\n\n";
        let delta = b"event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"<think>Reason</think>Hello\"}\n\n";
        let completed = b"event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"status\":\"completed\"}}\n\n";
        let done = b"data: [DONE]\n\n";

        let mut out = Vec::new();
        out.extend(
            claude_openai_responses_stream::openai_responses_stream_to_claude(created, &mut ctx)
                .expect("created"),
        );
        out.extend(
            claude_openai_responses_stream::openai_responses_stream_to_claude(delta, &mut ctx)
                .expect("delta"),
        );
        out.extend(
            claude_openai_responses_stream::openai_responses_stream_to_claude(completed, &mut ctx)
                .expect("completed"),
        );
        out.extend(
            claude_openai_responses_stream::openai_responses_stream_to_claude(done, &mut ctx)
                .expect("done"),
        );

        let s = String::from_utf8(out).expect("utf8");
        assert!(s.contains("\"type\":\"thinking\""));
        assert!(s.contains("\"thinking\":\"Reason\""));
        assert!(s.contains("\"type\":\"thinking_delta\""));
        assert!(s.contains("\"text\":\"Hello\""));
        assert!(s.contains("\"type\":\"text_delta\""));
        assert!(s.contains("event: content_block_stop"));
    }

    #[test]
    fn test_openai_responses_stream_to_claude_handles_multibyte_text() {
        let mut ctx = StreamContext::new();
        ctx.model_name = "claude-sonnet-4-6".to_string();

        let created = b"event: response.created\ndata: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\"}}\n\n";
        let delta = "event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"我\"}\n\n";
        let completed = b"event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"status\":\"completed\"}}\n\n";

        let mut out = Vec::new();
        out.extend(
            claude_openai_responses_stream::openai_responses_stream_to_claude(created, &mut ctx)
                .expect("created"),
        );
        out.extend(
            claude_openai_responses_stream::openai_responses_stream_to_claude(
                delta.as_bytes(),
                &mut ctx,
            )
            .expect("delta"),
        );
        out.extend(
            claude_openai_responses_stream::openai_responses_stream_to_claude(completed, &mut ctx)
                .expect("completed"),
        );

        let s = String::from_utf8(out).expect("utf8");
        assert!(s.contains("\"text\":\"我\""));
        assert!(s.contains("\"stop_reason\":\"end_turn\""));
    }

    #[test]
    fn test_openai_responses_stream_to_claude_emits_tool_use_flow() {
        let mut ctx = StreamContext::new();
        ctx.model_name = "claude-sonnet-4-6".to_string();

        let created = b"event: response.created\ndata: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\"}}\n\n";
        let added = b"event: response.output_item.added\ndata: {\"type\":\"response.output_item.added\",\"output_index\":3,\"item\":{\"type\":\"function_call\",\"id\":\"fc_1\",\"call_id\":\"call_1\",\"name\":\"Bash\",\"arguments\":\"\",\"status\":\"in_progress\"}}\n\n";
        let delta = b"event: response.function_call_arguments.delta\ndata: {\"type\":\"response.function_call_arguments.delta\",\"output_index\":3,\"delta\":\"{\\\"command\\\":\\\"pwd\\\"}\"}\n\n";
        let item_done = b"event: response.output_item.done\ndata: {\"type\":\"response.output_item.done\",\"output_index\":3,\"item\":{\"type\":\"function_call\",\"id\":\"fc_1\",\"call_id\":\"call_1\",\"name\":\"Bash\",\"arguments\":\"{\\\"command\\\":\\\"pwd\\\"}\",\"status\":\"completed\"}}\n\n";
        let completed = b"event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"status\":\"completed\",\"usage\":{\"output_tokens\":5}}}\n\n";

        let mut out = Vec::new();
        out.extend(
            claude_openai_responses_stream::openai_responses_stream_to_claude(created, &mut ctx)
                .expect("created"),
        );
        out.extend(
            claude_openai_responses_stream::openai_responses_stream_to_claude(added, &mut ctx)
                .expect("added"),
        );
        out.extend(
            claude_openai_responses_stream::openai_responses_stream_to_claude(delta, &mut ctx)
                .expect("delta"),
        );
        out.extend(
            claude_openai_responses_stream::openai_responses_stream_to_claude(item_done, &mut ctx)
                .expect("item_done"),
        );
        out.extend(
            claude_openai_responses_stream::openai_responses_stream_to_claude(completed, &mut ctx)
                .expect("completed"),
        );

        let s = String::from_utf8(out).expect("utf8");
        assert!(s.contains("event: message_start"));
        assert!(s.contains("\"type\":\"tool_use\""));
        assert!(s.contains("\"id\":\"call_1\""));
        assert!(s.contains("\"name\":\"Bash\""));
        assert!(s.contains("\"index\":0"));
        assert!(s.contains("\"type\":\"input_json_delta\""));
        assert!(s.contains("\\\"command\\\":\\\"pwd\\\""));
        assert!(s.contains("event: content_block_stop"));
        assert!(s.contains("\"stop_reason\":\"tool_use\""));
        assert!(s.contains("event: message_delta"));
        assert!(s.contains("event: message_stop"));
    }

    #[test]
    fn test_openai_responses_stream_to_claude_finalizes_on_eof_without_completed() {
        let mut ctx = StreamContext::new();
        ctx.model_name = "claude-sonnet-4-6".to_string();

        let created = b"event: response.created\ndata: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\"}}\n\n";
        let added = b"event: response.output_item.added\ndata: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\",\"role\":\"assistant\"}}\n\n";
        let delta = b"event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"delta\":\"hello\"}\n\n";
        let done = b"event: response.output_item.done\ndata: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\",\"role\":\"assistant\",\"status\":\"completed\"}}\n\n";

        let mut out = Vec::new();
        out.extend(
            claude_openai_responses_stream::openai_responses_stream_to_claude(created, &mut ctx)
                .expect("created"),
        );
        out.extend(
            claude_openai_responses_stream::openai_responses_stream_to_claude(added, &mut ctx)
                .expect("added"),
        );
        out.extend(
            claude_openai_responses_stream::openai_responses_stream_to_claude(delta, &mut ctx)
                .expect("delta"),
        );
        out.extend(
            claude_openai_responses_stream::openai_responses_stream_to_claude(done, &mut ctx)
                .expect("done"),
        );
        out.extend(
            claude_openai_responses_stream::finalize_openai_responses_stream_to_claude(&mut ctx),
        );

        let s = String::from_utf8(out).expect("utf8");
        assert!(s.contains("event: message_start"));
        assert!(s.contains("\"type\":\"text_delta\""));
        assert!(s.contains("\"text\":\"hello\""));
        assert!(!s.contains("event: message_delta"));
        assert!(s.contains("event: message_stop"));
    }

    #[test]
    fn test_openai_responses_resp_to_claude_text_tool_call_fallback_enabled() {
        let openai_resp = json!({
            "id": "resp_1",
            "output": [{
                "type": "message",
                "role": "assistant",
                "content": [{
                    "type": "output_text",
                    "text": "[Tool Call: Bash({\"command\":\"pwd\"})]"
                }]
            }],
            "usage": {"input_tokens": 1, "output_tokens": 1}
        });

        let mut allowed = HashSet::new();
        allowed.insert("Bash".to_string());
        let options = claude_openai_responses::ResponsesToClaudeOptions {
            text_tool_call_fallback_enabled: true,
            allowed_tool_names: allowed,
        };

        let result = claude_openai_responses::openai_responses_resp_to_claude_with_options(
            serde_json::to_vec(&openai_resp).unwrap().as_slice(),
            &options,
        )
        .expect("convert");
        let claude_resp: serde_json::Value = serde_json::from_slice(&result).expect("json");

        assert_eq!(claude_resp["content"][0]["type"], "tool_use");
        assert_eq!(claude_resp["content"][0]["name"], "Bash");
        assert_eq!(claude_resp["content"][0]["input"]["command"], "pwd");
        assert_eq!(claude_resp["stop_reason"], "tool_use");
    }

    #[test]
    fn test_openai_responses_resp_to_claude_splits_think_tags() {
        let openai_resp = json!({
            "id": "resp_1",
            "output": [{
                "type": "message",
                "role": "assistant",
                "content": [{
                    "type": "output_text",
                    "text": "<think>Reason</think>Answer"
                }]
            }],
            "usage": {"input_tokens": 3, "output_tokens": 5}
        });

        let result = claude_openai_responses::openai_responses_resp_to_claude(
            serde_json::to_vec(&openai_resp).unwrap().as_slice(),
        )
        .expect("convert");
        let claude_resp: serde_json::Value = serde_json::from_slice(&result).expect("json");

        assert_eq!(claude_resp["content"][0]["type"], "thinking");
        assert_eq!(claude_resp["content"][0]["thinking"], "Reason");
        assert_eq!(claude_resp["content"][1]["type"], "text");
        assert_eq!(claude_resp["content"][1]["text"], "Answer");
    }

    #[test]
    fn test_openai_responses_resp_to_claude_falls_back_to_item_id() {
        let openai_resp = json!({
            "id": "resp_1",
            "output": [{
                "type": "function_call",
                "id": "fc_123",
                "name": "Write",
                "arguments": "{\"file_path\":\"/tmp/a.txt\"}"
            }],
            "usage": {"input_tokens": 1, "output_tokens": 2}
        });

        let result = claude_openai_responses::openai_responses_resp_to_claude(
            serde_json::to_vec(&openai_resp).unwrap().as_slice(),
        )
        .expect("convert");
        let claude_resp: serde_json::Value = serde_json::from_slice(&result).expect("json");

        assert_eq!(claude_resp["content"][0]["type"], "tool_use");
        assert_eq!(claude_resp["content"][0]["id"], "fc_123");
        assert_eq!(claude_resp["content"][0]["name"], "Write");
    }

    #[test]
    fn test_openai_responses_resp_to_claude_text_tool_call_fallback_with_prefix_text() {
        let openai_resp = json!({
            "id": "resp_1",
            "output": [{
                "type": "message",
                "role": "assistant",
                "content": [{
                    "type": "output_text",
                    "text": "先说明一下：\n[Tool Call: Bash({\"command\":\"pwd\"})]"
                }]
            }],
            "usage": {"input_tokens": 1, "output_tokens": 1}
        });

        let mut allowed = HashSet::new();
        allowed.insert("Bash".to_string());
        let options = claude_openai_responses::ResponsesToClaudeOptions {
            text_tool_call_fallback_enabled: true,
            allowed_tool_names: allowed,
        };

        let result = claude_openai_responses::openai_responses_resp_to_claude_with_options(
            serde_json::to_vec(&openai_resp).unwrap().as_slice(),
            &options,
        )
        .expect("convert");
        let claude_resp: serde_json::Value = serde_json::from_slice(&result).expect("json");

        assert_eq!(claude_resp["content"][0]["type"], "tool_use");
        assert_eq!(claude_resp["content"][0]["name"], "Bash");
        assert_eq!(claude_resp["content"][0]["input"]["command"], "pwd");
        assert_eq!(claude_resp["stop_reason"], "tool_use");
    }

    #[test]
    fn test_openai_responses_resp_to_claude_text_tool_call_fallback_allows_whitespace_after_marker()
    {
        let openai_resp = json!({
            "id": "resp_1",
            "output": [{
                "type": "message",
                "role": "assistant",
                "content": [{
                    "type": "output_text",
                    "text": "[Tool Call:\n  Bash({\"command\":\"pwd\"})]"
                }]
            }],
            "usage": {"input_tokens": 1, "output_tokens": 1}
        });

        let mut allowed = HashSet::new();
        allowed.insert("Bash".to_string());
        let options = claude_openai_responses::ResponsesToClaudeOptions {
            text_tool_call_fallback_enabled: true,
            allowed_tool_names: allowed,
        };

        let result = claude_openai_responses::openai_responses_resp_to_claude_with_options(
            serde_json::to_vec(&openai_resp).unwrap().as_slice(),
            &options,
        )
        .expect("convert");
        let claude_resp: serde_json::Value = serde_json::from_slice(&result).expect("json");

        assert_eq!(claude_resp["content"][0]["type"], "tool_use");
        assert_eq!(claude_resp["content"][0]["name"], "Bash");
        assert_eq!(claude_resp["content"][0]["input"]["command"], "pwd");
        assert_eq!(claude_resp["stop_reason"], "tool_use");
    }

    #[test]
    fn test_openai_responses_resp_to_claude_text_tool_call_fallback_command_array_json() {
        let openai_resp = json!({
            "id": "resp_1",
            "output": [{
                "type": "message",
                "role": "assistant",
                "content": [{
                    "type": "output_text",
                    "text": r#"{"command":["bash","-lc","pwd"],"timeout_ms":120000}"#
                }]
            }],
            "usage": {"input_tokens": 1, "output_tokens": 1}
        });

        let mut allowed = HashSet::new();
        allowed.insert("Bash".to_string());
        let options = claude_openai_responses::ResponsesToClaudeOptions {
            text_tool_call_fallback_enabled: true,
            allowed_tool_names: allowed,
        };

        let result = claude_openai_responses::openai_responses_resp_to_claude_with_options(
            serde_json::to_vec(&openai_resp).unwrap().as_slice(),
            &options,
        )
        .expect("convert");
        let claude_resp: serde_json::Value = serde_json::from_slice(&result).expect("json");

        assert_eq!(claude_resp["content"][0]["type"], "tool_use");
        assert_eq!(claude_resp["content"][0]["name"], "Bash");
        assert_eq!(claude_resp["content"][0]["input"]["command"], "pwd");
        assert_eq!(claude_resp["content"][0]["input"]["timeout"], 120000);
        assert_eq!(claude_resp["stop_reason"], "tool_use");
    }

    #[test]
    fn test_openai_responses_resp_to_claude_text_tool_call_fallback_respects_whitelist() {
        let openai_resp = json!({
            "id": "resp_1",
            "output": [{
                "type": "message",
                "role": "assistant",
                "content": [{
                    "type": "output_text",
                    "text": "[Tool Call: Bash({\"command\":\"pwd\"})]"
                }]
            }],
            "usage": {"input_tokens": 1, "output_tokens": 1}
        });

        let mut allowed = HashSet::new();
        allowed.insert("Read".to_string());
        let options = claude_openai_responses::ResponsesToClaudeOptions {
            text_tool_call_fallback_enabled: true,
            allowed_tool_names: allowed,
        };

        let result = claude_openai_responses::openai_responses_resp_to_claude_with_options(
            serde_json::to_vec(&openai_resp).unwrap().as_slice(),
            &options,
        )
        .expect("convert");
        let claude_resp: serde_json::Value = serde_json::from_slice(&result).expect("json");

        assert_eq!(claude_resp["content"][0]["type"], "text");
        assert_eq!(
            claude_resp["content"][0]["text"],
            "[Tool Call: Bash({\"command\":\"pwd\"})]"
        );
        assert_eq!(claude_resp["stop_reason"], "end_turn");
    }

    #[test]
    fn test_openai_responses_resp_to_claude_text_tool_call_fallback_requires_valid_json() {
        let openai_resp = json!({
            "id": "resp_1",
            "output": [{
                "type": "message",
                "role": "assistant",
                "content": [{
                    "type": "output_text",
                    "text": "[Tool Call: Bash({command:pwd})]"
                }]
            }],
            "usage": {"input_tokens": 1, "output_tokens": 1}
        });

        let mut allowed = HashSet::new();
        allowed.insert("Bash".to_string());
        let options = claude_openai_responses::ResponsesToClaudeOptions {
            text_tool_call_fallback_enabled: true,
            allowed_tool_names: allowed,
        };

        let result = claude_openai_responses::openai_responses_resp_to_claude_with_options(
            serde_json::to_vec(&openai_resp).unwrap().as_slice(),
            &options,
        )
        .expect("convert");
        let claude_resp: serde_json::Value = serde_json::from_slice(&result).expect("json");

        assert_eq!(claude_resp["content"][0]["type"], "text");
        assert_eq!(
            claude_resp["content"][0]["text"],
            "[Tool Call: Bash({command:pwd})]"
        );
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
            "gpt-4",
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
            "gpt-4",
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
            "gpt-4",
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
            serde_json::to_vec(&resp).unwrap().as_slice(),
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
        assert_eq!(
            tool_calls[0]["function"]["arguments"],
            "{\"location\":\"NYC\"}"
        );

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
            "gpt-4",
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
            serde_json::to_vec(&chat_resp).unwrap().as_slice(),
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
    fn test_responses_req_to_chat_preserves_developer_role() {
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
            "gpt-4",
        );

        assert!(result.is_ok());
        let chat_req: serde_json::Value = serde_json::from_slice(&result.unwrap()).unwrap();

        let messages = chat_req["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 3);

        assert_eq!(messages[0]["role"], "developer");
        assert_eq!(messages[0]["content"], "System instruction");

        assert_eq!(messages[1]["role"], "user");
        assert_eq!(messages[1]["content"], "Hello");

        assert_eq!(messages[2]["role"], "developer");
        assert_eq!(messages[2]["content"], "Another instruction");
    }

    #[test]
    fn test_responses_req_to_chat_with_string_input() {
        let resp_req = json!({
            "model": "gpt-4",
            "input": "hello from responses",
            "stream": true
        });

        let result = openai_chat_responses::openai_responses_req_to_chat(
            serde_json::to_vec(&resp_req).unwrap().as_slice(),
            "gpt-4",
        );

        assert!(result.is_ok());
        let chat_req: serde_json::Value = serde_json::from_slice(&result.unwrap()).unwrap();
        let messages = chat_req["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["role"], "user");
        assert_eq!(messages[0]["content"], "hello from responses");
        assert_eq!(chat_req["stream"], true);
    }

    #[test]
    fn test_responses_req_to_chat_groups_pending_tool_calls() {
        let resp_req = json!({
            "model": "gpt-4",
            "input": [
                {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "run tools"}]},
                {"type": "function_call", "call_id": "call_1", "name": "tool_a", "arguments": "{\"x\":1}"},
                {"type": "function_call", "call_id": "call_2", "name": "tool_b", "arguments": "{\"y\":2}"},
                {"type": "function_call_output", "call_id": "call_1", "output": "a_done"},
                {"type": "function_call_output", "call_id": "call_2", "output": "b_done"}
            ]
        });

        let result = openai_chat_responses::openai_responses_req_to_chat(
            serde_json::to_vec(&resp_req).unwrap().as_slice(),
            "gpt-4",
        );

        assert!(result.is_ok());
        let chat_req: serde_json::Value = serde_json::from_slice(&result.unwrap()).unwrap();
        let messages = chat_req["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 4);

        assert_eq!(messages[1]["role"], "assistant");
        let tool_calls = messages[1]["tool_calls"].as_array().unwrap();
        assert_eq!(tool_calls.len(), 2);
        assert_eq!(tool_calls[0]["id"], "call_1");
        assert_eq!(tool_calls[1]["id"], "call_2");

        assert_eq!(messages[2]["role"], "tool");
        assert_eq!(messages[2]["tool_call_id"], "call_1");
        assert_eq!(messages[3]["role"], "tool");
        assert_eq!(messages[3]["tool_call_id"], "call_2");
    }

    #[test]
    fn test_responses_req_to_chat_converts_custom_tool() {
        let resp_req = json!({
            "model": "gpt-4",
            "input": [{"type": "message", "role": "user", "content": [{"type": "input_text", "text": "hi"}]}],
            "tools": [{
                "type": "custom",
                "name": "apply_patch",
                "description": "Apply patch using lark grammar"
            }],
            "max_output_tokens": 256
        });

        let result = openai_chat_responses::openai_responses_req_to_chat(
            serde_json::to_vec(&resp_req).unwrap().as_slice(),
            "gpt-4",
        );

        assert!(result.is_ok());
        let chat_req: serde_json::Value = serde_json::from_slice(&result.unwrap()).unwrap();
        let tools = chat_req["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["type"], "function");
        assert_eq!(tools[0]["function"]["name"], "apply_patch");
        assert_eq!(tools[0]["function"]["parameters"]["type"], "object");
        assert_eq!(chat_req["max_completion_tokens"], 256);
    }
}
