const test = require("node:test");
const assert = require("node:assert/strict");
const { ProxyServer, __test__ } = require("../src/proxy/server");

const { readRequestBody, buildUpstreamError } = __test__;

class MemoryResponse {
  constructor() {
    this.statusCode = null;
    this.headers = null;
    this.body = "";
    this.ended = false;
  }

  writeHead(statusCode, headers) {
    this.statusCode = statusCode;
    this.headers = headers;
  }

  write(chunk) {
    const next = Buffer.isBuffer(chunk) ? chunk.toString("utf-8") : String(chunk);
    this.body += next;
  }

  end(chunk) {
    if (chunk != null) {
      this.write(chunk);
    }
    this.ended = true;
  }
}

function createStreamFromString(input) {
  const bytes = new TextEncoder().encode(input);
  return new ReadableStream({
    start(controller) {
      controller.enqueue(bytes);
      controller.close();
    }
  });
}

function createProxyForStreamTest() {
  return new ProxyServer(
    {
      get() {
        return {};
      }
    },
    {
      append() {}
    }
  );
}

test("buildUpstreamError keeps upstream HTTP status", () => {
  const err = buildUpstreamError(429, "rate limited");
  assert.equal(err.statusCode, 429);
  assert.equal(err.upstreamStatus, 429);
  assert.equal(err.message, "rate limited");
});

test("readRequestBody rejects oversized request", async () => {
  const tooLarge = "a".repeat((10 * 1024 * 1024) + 1);
  const payload = JSON.stringify({ data: tooLarge });
  const req = {
    async *[Symbol.asyncIterator]() {
      yield Buffer.from(payload);
    }
  };

  await assert.rejects(
    () => readRequestBody(req),
    (err) => err && err.statusCode === 413
  );
});

test("bridgeOpenAIToAnthropic emits message_stop once and maps tool deltas", async () => {
  const proxy = createProxyForStreamTest();
  const response = new MemoryResponse();
  const upstreamResponse = {
    body: createStreamFromString(
      `data: ${JSON.stringify({
        choices: [{
          delta: {
            tool_calls: [{
              index: 0,
              id: "call_1",
              type: "function",
              function: {
                name: "weather_lookup",
                arguments: "{\"city\":\""
              }
            }]
          },
          finish_reason: null
        }]
      })}\n\n`
      + `data: ${JSON.stringify({
        choices: [{
          delta: {
            tool_calls: [{
              index: 0,
              function: {
                arguments: "sf\"}"
              }
            }]
          },
          finish_reason: "tool_calls"
        }]
      })}\n\n`
      + "data: [DONE]\n\n"
    )
  };

  await proxy.bridgeOpenAIToAnthropic(upstreamResponse, response, "trace-1", "m");

  const stopCount = (response.body.match(/event: message_stop/g) || []).length;
  assert.equal(stopCount, 1);
  assert.match(response.body, /"stop_reason":"tool_use"/);
  assert.match(response.body, /"usage":\{"input_tokens":0,"output_tokens":0\}/);
  assert.match(response.body, /event: content_block_start/);
  assert.match(response.body, /"type":"tool_use"/);
  assert.match(response.body, /"type":"input_json_delta"/);
});

test("bridgeAnthropicToOpenAI maps tool deltas and emits DONE once", async () => {
  const proxy = createProxyForStreamTest();
  const response = new MemoryResponse();
  const upstreamResponse = {
    body: createStreamFromString(
      `event: message_start\ndata: ${JSON.stringify({
        type: "message_start",
        message: { id: "msg_1" }
      })}\n\n`
      + `event: content_block_start\ndata: ${JSON.stringify({
        type: "content_block_start",
        index: 0,
        content_block: {
          type: "tool_use",
          id: "toolu_1",
          name: "weather_lookup"
        }
      })}\n\n`
      + `event: content_block_delta\ndata: ${JSON.stringify({
        type: "content_block_delta",
        index: 0,
        delta: {
          type: "input_json_delta",
          partial_json: "{\"city\":\"sf\"}"
        }
      })}\n\n`
      + `event: message_delta\ndata: ${JSON.stringify({
        type: "message_delta",
        delta: {
          stop_reason: "tool_use"
        }
      })}\n\n`
      + "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n"
      + "data: [DONE]\n\n"
    )
  };

  await proxy.bridgeAnthropicToOpenAI(upstreamResponse, response, "trace-2", "m");

  const doneCount = (response.body.match(/data: \[DONE\]/g) || []).length;
  assert.equal(doneCount, 1);
  assert.match(response.body, /"tool_calls"/);
  assert.match(response.body, /"finish_reason":"tool_calls"/);
});

test("bridgeAnthropicToOpenAIResponses emits responses-style stream events", async () => {
  const proxy = createProxyForStreamTest();
  const response = new MemoryResponse();
  const upstreamResponse = {
    body: createStreamFromString(
      `event: content_block_start\ndata: ${JSON.stringify({
        type: "content_block_start",
        index: 0,
        content_block: {
          type: "text",
          text: ""
        }
      })}\n\n`
      + `event: content_block_delta\ndata: ${JSON.stringify({
        type: "content_block_delta",
        index: 0,
        delta: {
          type: "text_delta",
          text: "hello"
        }
      })}\n\n`
      + `event: content_block_start\ndata: ${JSON.stringify({
        type: "content_block_start",
        index: 1,
        content_block: {
          type: "tool_use",
          id: "toolu_1",
          name: "weather_lookup"
        }
      })}\n\n`
      + `event: content_block_delta\ndata: ${JSON.stringify({
        type: "content_block_delta",
        index: 1,
        delta: {
          type: "input_json_delta",
          partial_json: "{\"city\":\"sf\"}"
        }
      })}\n\n`
      + `event: message_delta\ndata: ${JSON.stringify({
        type: "message_delta",
        usage: {
          input_tokens: 7,
          output_tokens: 3
        },
        delta: {
          stop_reason: "tool_use"
        }
      })}\n\n`
      + "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n"
      + "data: [DONE]\n\n"
    )
  };

  await proxy.bridgeAnthropicToOpenAIResponses(upstreamResponse, response, "trace-resp", "m");

  assert.match(response.body, /event: response\.created/);
  assert.match(response.body, /event: response\.output_text\.delta/);
  assert.match(response.body, /event: response\.function_call_arguments\.delta/);
  assert.match(response.body, /event: response\.completed/);
  assert.match(response.body, /"input_tokens":7/);
  assert.match(response.body, /"output_tokens":3/);
});

test("bridgeOpenAIToAnthropic uses upstream usage when provided", async () => {
  const proxy = createProxyForStreamTest();
  const response = new MemoryResponse();
  const upstreamResponse = {
    body: createStreamFromString(
      `data: ${JSON.stringify({
        choices: [{
          delta: {
            content: "hello"
          },
          finish_reason: "stop"
        }],
        usage: {
          prompt_tokens: 12,
          completion_tokens: 3,
          total_tokens: 15
        }
      })}\n\n`
      + "data: [DONE]\n\n"
    )
  };

  await proxy.bridgeOpenAIToAnthropic(upstreamResponse, response, "trace-3", "m");

  assert.match(response.body, /"usage":\{"input_tokens":12,"output_tokens":3\}/);
});

test("bridgeOpenAIToAnthropic emits usage in message_delta when usage arrives later", async () => {
  const proxy = createProxyForStreamTest();
  const response = new MemoryResponse();
  const upstreamResponse = {
    body: createStreamFromString(
      `data: ${JSON.stringify({
        choices: [{
          delta: {
            content: "hello"
          },
          finish_reason: null
        }]
      })}\n\n`
      + `data: ${JSON.stringify({
        choices: [{
          delta: {},
          finish_reason: "stop"
        }],
        usage: {
          prompt_tokens: 20,
          completion_tokens: 4,
          total_tokens: 24
        }
      })}\n\n`
      + "data: [DONE]\n\n"
    )
  };

  await proxy.bridgeOpenAIToAnthropic(upstreamResponse, response, "trace-4", "m");

  assert.match(response.body, /event: message_start/);
  assert.match(response.body, /"content_block":\{"type":"text","text":""\}/);
  assert.match(response.body, /"type":"text_delta","text":"hello"/);
  assert.ok(
    response.body.indexOf("\"content_block\":{\"type\":\"text\",\"text\":\"\"}") <
    response.body.indexOf("\"type\":\"text_delta\",\"text\":\"hello\"")
  );
  assert.match(response.body, /event: message_delta/);
  assert.match(response.body, /"usage":\{"input_tokens":20,"output_tokens":4\}/);
});
