const test = require("node:test");
const assert = require("node:assert/strict");
const { validateConfig } = require("../src/proxy/schema.ts");
const { getDefaultConfig } = require("../src/proxy/defaultConfig.ts");

test("default config validates", () => {
  const cfg = getDefaultConfig();
  const result = validateConfig(cfg);
  assert.equal(result.valid, true);
  assert.equal(result.errors.length, 0);
});

test("invalid config returns errors", () => {
  const result = validateConfig({
    server: {},
    compat: {},
    logging: {},
    groups: [
      {
        id: "g1",
        name: "n1",
        models: [],
        activeRuleId: "r1",
        rules: []
      }
    ]
  });

  assert.equal(result.valid, false);
  assert.ok(result.errors.length > 0);
});

test("group active rule must exist", () => {
  const cfg = getDefaultConfig();
  cfg.groups = [
    {
      id: "g1",
      name: "demo",
      models: ["a1"],
      activeRuleId: "not_exists",
      rules: [
        {
          id: "r1",
          name: "rule-1",
          protocol: "anthropic",
          token: "t1",
          apiAddress: "https://api.example.com",
          defaultModel: "m1",
          modelMappings: {}
        }
      ]
    }
  ];
  const result = validateConfig(cfg);
  assert.equal(result.valid, false);
  assert.match(result.errors.join(" | "), /activeRuleId/);
});

test("ui settings must be valid", () => {
  const cfg = getDefaultConfig();
  cfg.ui.theme = "system";

  const result = validateConfig(cfg);
  assert.equal(result.valid, false);
  assert.match(result.errors.join(" | "), /ui.theme/);
});
