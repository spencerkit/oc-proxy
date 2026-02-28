const test = require("node:test");
const assert = require("node:assert/strict");
const { redactPayload } = require("../src/proxy/redact");

test("redactPayload masks sensitive keys recursively", () => {
  const input = {
    authorization: "Bearer abcdefghijkl",
    nested: {
      api_key: "sk-1234567890"
    },
    safe: "ok"
  };

  const out = redactPayload(input, ["authorization", "api_key"]);
  assert.match(out.authorization, /REDACTED/);
  assert.match(out.nested.api_key, /REDACTED/);
  assert.equal(out.safe, "ok");
});
