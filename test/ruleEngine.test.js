const test = require("node:test");
const assert = require("node:assert/strict");
const { resolveRoute } = require("../src/proxy/ruleEngine");

const baseConfig = {
  providers: [
    { id: "p1", protocol: "openai", baseURL: "https://a" },
    { id: "p2", protocol: "anthropic", baseURL: "https://b" }
  ],
  models: [
    { name: "m1", provider: "p1", upstreamModel: "gpt-x" }
  ],
  rules: []
};

test("resolveRoute uses model default mapping", () => {
  const result = resolveRoute(baseConfig, {
    entryProtocol: "openai",
    path: "/v1/chat/completions",
    requestedModel: "m1",
    headers: {},
    body: { model: "m1" },
    traceId: "t"
  });

  assert.equal(result.provider.id, "p1");
  assert.equal(result.targetModel, "gpt-x");
});

test("resolveRoute applies highest-priority first match", () => {
  const config = {
    ...baseConfig,
    rules: [
      {
        id: "low",
        priority: 1,
        enabled: true,
        match: { model: "m1" },
        action: { targetProvider: "p2", targetModel: "claude-a" }
      },
      {
        id: "high",
        priority: 100,
        enabled: true,
        match: { model: "m1" },
        action: { targetProvider: "p2", targetModel: "claude-b" }
      }
    ]
  };

  const result = resolveRoute(config, {
    entryProtocol: "openai",
    path: "/v1/chat/completions",
    requestedModel: "m1",
    headers: {},
    body: { model: "m1" },
    traceId: "t"
  });

  assert.equal(result.provider.id, "p2");
  assert.equal(result.targetModel, "claude-b");
  assert.equal(result.matchedRule.id, "high");
});

test("resolveRoute rewrite set/remove works", () => {
  const config = {
    ...baseConfig,
    rules: [
      {
        id: "rewrite",
        priority: 50,
        enabled: true,
        match: { model: "m1" },
        action: {
          rewrite: {
            set: {
              "temperature": 0.2,
              "metadata.trace": "${traceId}"
            },
            remove: ["foo"]
          }
        }
      }
    ]
  };

  const result = resolveRoute(config, {
    entryProtocol: "openai",
    path: "/v1/chat/completions",
    requestedModel: "m1",
    headers: {},
    body: { model: "m1", foo: "bar" },
    traceId: "trace-1"
  });

  assert.equal(result.body.temperature, 0.2);
  assert.equal(result.body.metadata.trace, "trace-1");
  assert.equal(result.body.foo, undefined);
});
