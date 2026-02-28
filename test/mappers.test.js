const test = require("node:test");
const assert = require("node:assert/strict");
const {
  mapOpenAIToAnthropicRequest,
  mapAnthropicToOpenAIResponse,
  normalizeOpenAIRequest
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
