const test = require("node:test");
const assert = require("node:assert/strict");
const {
  createGroupsBackupPayload,
  extractGroupsFromImportPayload
} = require("../src/proxy/groupBackup.ts");

test("createGroupsBackupPayload keeps groups and metadata", () => {
  const groups = [
    {
      id: "demo",
      name: "Demo",
      models: ["a1"],
      activeRuleId: "r1",
      rules: [
        {
          id: "r1",
          name: "rule-1",
          protocol: "anthropic",
          token: "t1",
          apiAddress: "https://api.example.com",
          defaultModel: "claude-3-7-sonnet",
          modelMappings: {}
        }
      ]
    }
  ];

  const payload = createGroupsBackupPayload(groups);
  assert.equal(payload.format, "oa-proxy-groups-backup");
  assert.equal(payload.version, 1);
  assert.ok(typeof payload.exportedAt === "string");
  assert.deepEqual(payload.groups, groups);
});

test("extractGroupsFromImportPayload supports root groups object", () => {
  const groups = [{ id: "g1", name: "Group 1", models: [], activeRuleId: null, rules: [] }];
  const out = extractGroupsFromImportPayload({ groups });
  assert.deepEqual(out, groups);
});

test("extractGroupsFromImportPayload supports groups array root", () => {
  const groups = [{ id: "g2", name: "Group 2", models: [], activeRuleId: null, rules: [] }];
  const out = extractGroupsFromImportPayload(groups);
  assert.deepEqual(out, groups);
});

test("extractGroupsFromImportPayload supports full config envelope", () => {
  const groups = [{ id: "g3", name: "Group 3", models: [], activeRuleId: null, rules: [] }];
  const out = extractGroupsFromImportPayload({ config: { groups } });
  assert.deepEqual(out, groups);
});

test("extractGroupsFromImportPayload rejects invalid payload", () => {
  assert.throws(() => extractGroupsFromImportPayload({ invalid: true }), /expected a groups array/);
});
