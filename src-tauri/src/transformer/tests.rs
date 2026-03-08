#[cfg(test)]
mod tests {
    use crate::transformer::convert::{
        claude_openai, claude_openai_responses, claude_openai_responses_stream,
        openai_chat_responses, openai_claude,
    };
    use crate::transformer::types::StreamContext;
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
    fn test_claude_messages_to_responses_maps_tool_result_to_input_text() {
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

        assert_eq!(input[1]["type"], "message");
        assert_eq!(input[1]["role"], "assistant");
        assert_eq!(input[1]["content"][0]["type"], "output_text");
        let tool_call_text = input[1]["content"][0]["text"].as_str().unwrap_or("");
        assert!(tool_call_text.contains("[Tool Call: list_files("));
        assert!(tool_call_text.contains("\"path\":\".\""));

        assert_eq!(input[2]["type"], "message");
        assert_eq!(input[2]["role"], "user");
        assert_eq!(input[2]["content"][0]["type"], "input_text");
        let text = input[2]["content"][0]["text"].as_str().unwrap_or("");
        assert!(text.contains("[Tool Result:"));
        assert!(text.contains("file_a"));
        assert!(text.contains("file_b"));

        let serialized = serde_json::to_string(&responses_req).unwrap();
        assert!(!serialized.contains("\"function_call_output\""));
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
        assert_eq!(messages[0]["content"][0]["type"], "text");
        assert_eq!(messages[0]["content"][0]["text"], "tool says hi");
        assert_eq!(messages[1]["role"], "user");
        assert_eq!(messages[1]["content"][0]["text"], "continue");
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

        let created = b"event: response.created\ndata: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\"}}\n\n";
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
        assert!(s.contains("event: content_block_start"));
        assert!(s.contains("\"type\":\"content_block_start\""));
        assert!(s.contains("\"type\":\"text_delta\""));
        assert!(s.contains("\"text\":\"hello\""));
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
    fn test_openai_responses_stream_to_claude_text_tool_call_fallback_enabled() {
        let mut ctx = StreamContext::new();
        ctx.model_name = "claude-sonnet-4-6".to_string();
        ctx.text_tool_call_fallback_enabled = true;
        ctx.allowed_tool_names.insert("Bash".to_string());

        let created =
            "event: response.created\ndata: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\"}}\n\n"
                .to_string();
        let added = "event: response.output_item.added\ndata: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\",\"role\":\"assistant\"}}\n\n".to_string();
        let delta_payload = json!({
            "type": "response.output_text.delta",
            "output_index": 0,
            "delta": "[Tool Call: Bash({\"command\":\"pwd\"})]"
        })
        .to_string();
        let delta = format!("event: response.output_text.delta\ndata: {delta_payload}\n\n");
        let item_done = "event: response.output_item.done\ndata: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\",\"role\":\"assistant\",\"status\":\"completed\"}}\n\n".to_string();
        let completed = "event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"usage\":{\"output_tokens\":5}}}\n\n".to_string();

        let mut out = Vec::new();
        out.extend(
            claude_openai_responses_stream::openai_responses_stream_to_claude(
                created.as_bytes(),
                &mut ctx,
            )
            .expect("created"),
        );
        out.extend(
            claude_openai_responses_stream::openai_responses_stream_to_claude(
                added.as_bytes(),
                &mut ctx,
            )
            .expect("added"),
        );
        out.extend(
            claude_openai_responses_stream::openai_responses_stream_to_claude(
                delta.as_bytes(),
                &mut ctx,
            )
            .expect("delta"),
        );
        out.extend(
            claude_openai_responses_stream::openai_responses_stream_to_claude(
                item_done.as_bytes(),
                &mut ctx,
            )
            .expect("item_done"),
        );
        out.extend(
            claude_openai_responses_stream::openai_responses_stream_to_claude(
                completed.as_bytes(),
                &mut ctx,
            )
            .expect("completed"),
        );

        let s = String::from_utf8(out).expect("utf8");
        assert!(s.contains("\"type\":\"tool_use\""));
        assert!(s.contains("\"name\":\"Bash\""));
        assert!(s.contains("\"type\":\"input_json_delta\""));
        assert!(s.contains("\"stop_reason\":\"tool_use\""));
        assert!(!s.contains("\"type\":\"text_delta\""));
    }

    #[test]
    fn test_openai_responses_stream_to_claude_text_tool_call_fallback_command_array_json() {
        let mut ctx = StreamContext::new();
        ctx.model_name = "claude-sonnet-4-6".to_string();
        ctx.text_tool_call_fallback_enabled = true;
        ctx.allowed_tool_names.insert("Bash".to_string());

        let created =
            "event: response.created\ndata: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\"}}\n\n"
                .to_string();
        let added = "event: response.output_item.added\ndata: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\",\"role\":\"assistant\"}}\n\n".to_string();
        let delta_payload = json!({
            "type": "response.output_text.delta",
            "output_index": 0,
            "delta": r#"{"command":["bash","-lc","pwd"],"timeout_ms":120000}"#
        })
        .to_string();
        let delta = format!("event: response.output_text.delta\ndata: {delta_payload}\n\n");
        let item_done = "event: response.output_item.done\ndata: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\",\"role\":\"assistant\",\"status\":\"completed\"}}\n\n".to_string();
        let completed = "event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"usage\":{\"output_tokens\":5}}}\n\n".to_string();

        let mut out = Vec::new();
        out.extend(
            claude_openai_responses_stream::openai_responses_stream_to_claude(
                created.as_bytes(),
                &mut ctx,
            )
            .expect("created"),
        );
        out.extend(
            claude_openai_responses_stream::openai_responses_stream_to_claude(
                added.as_bytes(),
                &mut ctx,
            )
            .expect("added"),
        );
        out.extend(
            claude_openai_responses_stream::openai_responses_stream_to_claude(
                delta.as_bytes(),
                &mut ctx,
            )
            .expect("delta"),
        );
        out.extend(
            claude_openai_responses_stream::openai_responses_stream_to_claude(
                item_done.as_bytes(),
                &mut ctx,
            )
            .expect("item_done"),
        );
        out.extend(
            claude_openai_responses_stream::openai_responses_stream_to_claude(
                completed.as_bytes(),
                &mut ctx,
            )
            .expect("completed"),
        );

        let s = String::from_utf8(out).expect("utf8");
        assert!(s.contains("\"type\":\"tool_use\""));
        assert!(s.contains("\"name\":\"Bash\""));
        assert!(s.contains("\"type\":\"input_json_delta\""));
        assert!(s.contains("\\\"command\\\":\\\"pwd\\\""));
        assert!(s.contains("\"stop_reason\":\"tool_use\""));
        assert!(!s.contains("\"type\":\"text_delta\""));
    }

    #[test]
    fn test_openai_responses_stream_to_claude_text_tool_call_fallback_with_prefix_text() {
        let mut ctx = StreamContext::new();
        ctx.model_name = "claude-sonnet-4-6".to_string();
        ctx.text_tool_call_fallback_enabled = true;
        ctx.allowed_tool_names.insert("Bash".to_string());

        let created =
            "event: response.created\ndata: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\"}}\n\n"
                .to_string();
        let added = "event: response.output_item.added\ndata: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\",\"role\":\"assistant\"}}\n\n".to_string();
        let delta_payload = json!({
            "type": "response.output_text.delta",
            "output_index": 0,
            "delta": "先说明一下：\\n[Tool Call: Bash({\"command\":\"pwd\"})]"
        })
        .to_string();
        let delta = format!("event: response.output_text.delta\ndata: {delta_payload}\n\n");
        let item_done = "event: response.output_item.done\ndata: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"message\",\"id\":\"msg_1\",\"role\":\"assistant\",\"status\":\"completed\"}}\n\n".to_string();
        let completed = "event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"usage\":{\"output_tokens\":5}}}\n\n".to_string();

        let mut out = Vec::new();
        out.extend(
            claude_openai_responses_stream::openai_responses_stream_to_claude(
                created.as_bytes(),
                &mut ctx,
            )
            .expect("created"),
        );
        out.extend(
            claude_openai_responses_stream::openai_responses_stream_to_claude(
                added.as_bytes(),
                &mut ctx,
            )
            .expect("added"),
        );
        out.extend(
            claude_openai_responses_stream::openai_responses_stream_to_claude(
                delta.as_bytes(),
                &mut ctx,
            )
            .expect("delta"),
        );
        out.extend(
            claude_openai_responses_stream::openai_responses_stream_to_claude(
                item_done.as_bytes(),
                &mut ctx,
            )
            .expect("item_done"),
        );
        out.extend(
            claude_openai_responses_stream::openai_responses_stream_to_claude(
                completed.as_bytes(),
                &mut ctx,
            )
            .expect("completed"),
        );

        let s = String::from_utf8(out).expect("utf8");
        assert!(s.contains("\"type\":\"tool_use\""));
        assert!(s.contains("\"name\":\"Bash\""));
        assert!(s.contains("\"type\":\"input_json_delta\""));
        assert!(s.contains("\"stop_reason\":\"tool_use\""));
        assert!(!s.contains("\"type\":\"text_delta\""));
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
