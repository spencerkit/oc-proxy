const test = require("node:test");
const assert = require("node:assert/strict");
const { migrateLegacyConfig, isStockAutoGroups, pruneStockSeedGroups } = require("../src/proxy/migrate");
const { validateConfig } = require("../src/proxy/schema");

test("legacy config can migrate to group-based config", () => {
  const legacy = {
    server: { host: "0.0.0.0", port: 9000, authEnabled: false, localBearerToken: "" },
    compat: { strictMode: true },
    logging: { captureBody: true, redactRules: ["token"] },
    providers: [
      {
        id: "anthropic-default",
        protocol: "anthropic",
        baseURL: "https://api.anthropic.com",
        apiKey: "sk-anth"
      }
    ],
    models: [
      {
        name: "claude",
        provider: "anthropic-default",
        upstreamModel: "claude-3-5-sonnet-latest"
      }
    ],
    rules: []
  };

  const migrated = migrateLegacyConfig(legacy);
  const result = validateConfig(migrated);
  assert.equal(result.valid, true);
  assert.equal(migrated.server.port, 9000);
  assert.equal(migrated.groups.length, 1);
  assert.equal(migrated.groups[0].path, "claude");
  assert.equal(migrated.groups[0].rules[0].direction, "oc");
});

test("stock legacy template migrates to empty groups", () => {
  const legacy = {
    server: { host: "0.0.0.0", port: 8899, authEnabled: false, localBearerToken: "" },
    compat: { strictMode: true },
    logging: { captureBody: true, redactRules: ["token"] },
    providers: [
      { id: "anthropic-default", protocol: "anthropic", baseURL: "https://api.anthropic.com", apiKey: "" },
      { id: "openai-default", protocol: "openai", baseURL: "https://api.openai.com", apiKey: "" }
    ],
    models: [
      { name: "claude-3-5-sonnet", provider: "anthropic-default", upstreamModel: "claude-3-5-sonnet-latest" },
      { name: "gpt-4o-mini", provider: "openai-default", upstreamModel: "gpt-4o-mini" }
    ],
    rules: []
  };

  const migrated = migrateLegacyConfig(legacy);
  const result = validateConfig(migrated);
  assert.equal(result.valid, true);
  assert.equal(migrated.groups.length, 0);
});

test("detect stock auto groups", () => {
  const cfg = {
    groups: [
      {
        id: "g1",
        name: "claude-3-5-sonnet",
        path: "claude-3-5-sonnet",
        activeRuleId: "r1",
        rules: [
          { id: "r1", model: "claude-3-5-sonnet-latest", token: "", apiAddress: "https://api.anthropic.com", direction: "oc" }
        ]
      },
      {
        id: "g2",
        name: "gpt-4o-mini",
        path: "gpt-4o-mini",
        activeRuleId: "r2",
        rules: [
          { id: "r2", model: "gpt-4o-mini", token: "", apiAddress: "https://api.openai.com", direction: "co" }
        ]
      }
    ]
  };
  assert.equal(isStockAutoGroups(cfg), true);
});

test("prune stock seed groups while keeping user groups", () => {
  const cfg = {
    groups: [
      {
        id: "group_claude-3-5-sonnet",
        name: "claude-3-5-sonnet",
        path: "claude-3-5-sonnet",
        activeRuleId: "r1",
        rules: [
          { id: "r1", model: "claude-3-5-sonnet-latest", token: "", apiAddress: "https://api.anthropic.com", direction: "oc" }
        ]
      },
      {
        id: "group_user",
        name: "claude",
        path: "claude",
        activeRuleId: "ru",
        rules: [
          { id: "ru", model: "gpt-5.3-codex", token: "sk-1", apiAddress: "https://www.bytecatcode.org/v1", direction: "co" }
        ]
      },
      {
        id: "group_gpt-4o-mini",
        name: "gpt-4o-mini",
        path: "gpt-4o-mini",
        activeRuleId: "r2",
        rules: [
          { id: "r2", model: "gpt-4o-mini", token: "", apiAddress: "https://api.openai.com", direction: "co" }
        ]
      }
    ]
  };

  const result = pruneStockSeedGroups(cfg);
  assert.equal(result.changed, true);
  assert.equal(result.config.groups.length, 1);
  assert.equal(result.config.groups[0].id, "group_user");
});

test("migrate modern config without ui keeps groups and injects ui defaults", () => {
  const modernWithoutUi = {
    server: { host: "0.0.0.0", port: 8899, authEnabled: false, localBearerToken: "" },
    compat: { strictMode: true },
    logging: { captureBody: true, redactRules: ["token"] },
    groups: [
      {
        id: "group_user",
        name: "claude",
        path: "claude",
        activeRuleId: "r1",
        rules: [
          { id: "r1", model: "claude-3-5-sonnet-latest", token: "sk-1", apiAddress: "https://api.anthropic.com", direction: "oc" }
        ]
      }
    ]
  };

  const migrated = migrateLegacyConfig(modernWithoutUi);
  const result = validateConfig(migrated);

  assert.equal(result.valid, true);
  assert.equal(migrated.groups.length, 1);
  assert.equal(migrated.groups[0].id, "group_user");
  assert.equal(migrated.ui.theme, "light");
  assert.equal(migrated.ui.locale, "en-US");
  assert.equal(migrated.ui.launchOnStartup, false);
});
