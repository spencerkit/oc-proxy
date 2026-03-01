const test = require("node:test");
const assert = require("node:assert/strict");
const {
  mapOpenAIToAnthropicRequest,
  mapAnthropicToOpenAIResponse,
  normalizeOpenAIRequest,
  mapOpenAIChatToResponses
} = require("../src/proxy/mappers/openaiToAnthropic");
const {
  mapAnthropicToOpenAIRequest,
  mapOpenAIToAnthropicResponse
} = require("../src/proxy/mappers/anthropicToOpenai");

test("openai request maps to anthropic request", () => {
  const input = {
    model: "m1",
    messages: [
      { role: "system", content: "be concise" },
      { role: "user", content: "hello" }
    ],
    stream: true,
    max_tokens: 100
  };

  const out = mapOpenAIToAnthropicRequest(input, { strictMode: true, targetModel: "claude-target" });
  assert.equal(out.model, "claude-target");
  assert.equal(out.stream, true);
  assert.equal(out.system, "be concise");
  assert.equal(out.messages[0].role, "user");
});

test("anthropic request maps to openai request", () => {
  const input = {
    model: "claude-x",
    system: "helpful",
    messages: [{ role: "user", content: [{ type: "text", text: "hello" }] }],
    stream: false
  };

  const out = mapAnthropicToOpenAIRequest(input, { strictMode: true, targetModel: "gpt-target" });
  assert.equal(out.model, "gpt-target");
  assert.equal(out.messages[0].role, "system");
  assert.equal(out.messages[1].content, "hello");
});

test("anthropic response maps to openai response", () => {
  const input = {
    id: "msg_1",
    model: "claude-z",
    content: [{ type: "text", text: "hi" }],
    usage: { input_tokens: 3, output_tokens: 4 }
  };

  const out = mapAnthropicToOpenAIResponse(input, { requestModel: "m1" });
  assert.equal(out.choices[0].message.content, "hi");
  assert.equal(out.model, "m1");
});

test("openai response maps to anthropic response", () => {
  const input = {
    id: "chat_1",
    model: "gpt-x",
    choices: [{ message: { content: "ok" }, finish_reason: "stop" }],
    usage: { prompt_tokens: 5, completion_tokens: 2 }
  };

  const out = mapOpenAIToAnthropicResponse(input, { requestModel: "claude-m" });
  assert.equal(out.model, "claude-m");
  assert.equal(out.content[0].text, "ok");
  assert.equal(out.stop_reason, "end_turn");
});

test("strict mode rejects unknown openai fields", () => {
  assert.throws(
    () => mapOpenAIToAnthropicRequest({ model: "m", messages: [], unknown_a: true }, { strictMode: true }),
    /Unsupported OpenAI fields/
  );
});

test("responses input is normalized", () => {
  const normalized = normalizeOpenAIRequest("/v1/responses", {
    model: "m",
    input: "hello",
    stream: false,
    system: "sys",
    thinking: { type: "enabled" },
    context_management: { clear_function_results: false }
  });

  assert.equal(normalized.messages[0].role, "user");
  assert.equal(normalized.messages[0].content, "hello");
  assert.equal(normalized.system, "sys");
  assert.equal(normalized.thinking.type, "enabled");
  assert.equal(normalized.context_management.clear_function_results, false);
});

test("responses function call I/O is normalized to chat-style tool messages", () => {
  const normalized = normalizeOpenAIRequest("/v1/responses", {
    model: "m",
    max_output_tokens: 2048,
    instructions: "system prompt",
    input: [
      {
        type: "function_call",
        call_id: "call_1",
        name: "weather_lookup",
        arguments: { city: "sf" }
      },
      {
        type: "function_call_output",
        call_id: "call_1",
        output: [{ type: "output_text", text: "sunny" }]
      }
    ]
  });

  assert.equal(normalized.max_tokens, 2048);
  assert.equal(normalized.system, "system prompt");
  assert.equal(normalized.messages[0].role, "assistant");
  assert.equal(normalized.messages[0].tool_calls[0].id, "call_1");
  assert.equal(normalized.messages[1].role, "tool");
  assert.equal(normalized.messages[1].tool_call_id, "call_1");
});

test("strict mode allows anthropic thinking/context_management fields", () => {
  const input = {
    model: "claude-x",
    messages: [{ role: "user", content: [{ type: "text", text: "hello" }] }],
    thinking: { type: "enabled" },
    context_management: { clear_function_results: false }
  };

  const out = mapAnthropicToOpenAIRequest(input, { strictMode: true, targetModel: "gpt-target" });
  assert.equal(out.model, "gpt-target");
  assert.equal(out.messages[0].role, "user");
});

test("strict mode allows openai system/thinking/context_management fields", () => {
  const input = {
    model: "m1",
    messages: [{ role: "user", content: "hello" }],
    system: "be concise",
    thinking: { type: "enabled" },
    context_management: { clear_function_results: false }
  };

  const out = mapOpenAIToAnthropicRequest(input, { strictMode: true, targetModel: "claude-target" });
  assert.equal(out.system, "be concise");
  assert.equal(out.thinking.type, "enabled");
  assert.equal(out.context_management.clear_function_results, false);
});

test("chat response -> responses keeps tool calls", () => {
  const mapped = mapOpenAIChatToResponses({
    id: "chatcmpl_1",
    created: 123456,
    model: "gpt-4.1",
    choices: [{
      message: {
        role: "assistant",
        content: "I will call a tool",
        tool_calls: [{
          id: "call_1",
          type: "function",
          function: {
            name: "weather_lookup",
            arguments: "{\"city\":\"sf\"}"
          }
        }]
      }
    }]
  });

  assert.equal(mapped.object, "response");
  assert.equal(mapped.output[0].type, "message");
  assert.equal(mapped.output[1].type, "function_call");
  assert.equal(mapped.output[1].name, "weather_lookup");
  assert.equal(mapped.status, "completed");
  assert.equal(mapped.usage.input_tokens, 0);
  assert.equal(mapped.usage.output_tokens, 0);
});

test("openai tool message maps to anthropic tool_result", () => {
  const out = mapOpenAIToAnthropicRequest({
    model: "m",
    messages: [
      {
        role: "assistant",
        content: "",
        tool_calls: [{
          id: "call_1",
          type: "function",
          function: {
            name: "weather_lookup",
            arguments: "{\"city\":\"sf\"}"
          }
        }]
      },
      {
        role: "tool",
        tool_call_id: "call_1",
        content: "sunny"
      }
    ]
  }, { strictMode: true, targetModel: "claude-target" });

  assert.equal(out.messages[0].role, "assistant");
  assert.equal(out.messages[0].content[0].type, "tool_use");
  assert.equal(out.messages[1].role, "user");
  assert.equal(out.messages[1].content[0].type, "tool_result");
  assert.equal(out.messages[1].content[0].tool_use_id, "call_1");
  assert.equal(out.messages[1].content[0].content, "sunny");
});

test("anthropic tool_result maps to openai tool message", () => {
  const out = mapAnthropicToOpenAIRequest({
    model: "claude-x",
    messages: [
      {
        role: "assistant",
        content: [{
          type: "tool_use",
          id: "toolu_1",
          name: "weather_lookup",
          input: { city: "sf" }
        }]
      },
      {
        role: "user",
        content: [{
          type: "tool_result",
          tool_use_id: "toolu_1",
          content: [{ type: "text", text: "sunny" }]
        }]
      }
    ]
  }, { strictMode: true, targetModel: "gpt-target" });

  assert.equal(out.messages[0].role, "assistant");
  assert.equal(out.messages[0].tool_calls[0].id, "toolu_1");
  assert.equal(out.messages[1].role, "tool");
  assert.equal(out.messages[1].tool_call_id, "toolu_1");
  assert.equal(out.messages[1].content, "sunny");
});

test("openai finish_reason tool_calls maps to anthropic tool_use", () => {
  const out = mapOpenAIToAnthropicResponse({
    id: "chat_2",
    model: "gpt-x",
    choices: [{ message: { content: "" }, finish_reason: "tool_calls" }],
    usage: { prompt_tokens: 1, completion_tokens: 1 }
  }, { requestModel: "claude-m" });

  assert.equal(out.stop_reason, "tool_use");
});
