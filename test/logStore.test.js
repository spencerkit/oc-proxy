const test = require("node:test");
const assert = require("node:assert/strict");
const { LogStore } = require("../src/main/logStore.ts");

test("log store keeps latest 100 entries by default", () => {
  const store = new LogStore();
  for (let i = 0; i < 120; i += 1) {
    store.append({ idx: i });
  }

  const logs = store.list();
  assert.equal(logs.length, 100);
  assert.equal(logs[0].idx, 20);
  assert.equal(logs[99].idx, 119);
});
