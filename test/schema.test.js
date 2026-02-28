const test = require("node:test");
const assert = require("node:assert/strict");
const { validateConfig } = require("../src/proxy/schema");
const { getDefaultConfig } = require("../src/proxy/defaultConfig");

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
        path: "bad path",
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
      path: "demo",
      activeRuleId: "not_exists",
      rules: [
        {
          id: "r1",
          model: "m1",
          token: "t1",
          apiAddress: "https://api.example.com",
          direction: "oc"
        }
      ]
    }
  ];
  const result = validateConfig(cfg);
  assert.equal(result.valid, false);
  assert.match(result.errors.join(" | "), /activeRuleId/);
});
