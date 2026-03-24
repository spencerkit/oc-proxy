import assert from "node:assert/strict"
import { test } from "node:test"

import {
  buildProviderModelHealthSnapshot,
  createProviderTestKey,
  formatProviderLatency,
  GLOBAL_PROVIDER_TEST_GROUP_ID,
  pickLatestProviderModelHealthSnapshot,
  resolveProviderTestGroupId,
} from "../../src/renderer/utils/providerTesting"

test("createProviderTestKey builds a stable composite key", () => {
  assert.equal(createProviderTestKey("group-a", "provider-1"), "group-a:provider-1")
})

test("createProviderTestKey falls back to a global scope when group is missing", () => {
  assert.equal(
    createProviderTestKey(undefined, "provider-1"),
    `${GLOBAL_PROVIDER_TEST_GROUP_ID}:provider-1`
  )
  assert.equal(resolveProviderTestGroupId(""), GLOBAL_PROVIDER_TEST_GROUP_ID)
})

test("buildProviderModelHealthSnapshot normalizes text and latency", () => {
  const snapshot = buildProviderModelHealthSnapshot({
    groupId: "group-a",
    providerId: "provider-1",
    ok: true,
    latencyMs: -1,
    rawText: "  gpt-4.1-mini  ",
    message: "  ok  ",
    testedAt: "2026-03-23T10:00:00.000Z",
  })

  assert.deepEqual(snapshot, {
    groupId: "group-a",
    providerId: "provider-1",
    status: "available",
    latencyMs: null,
    resolvedModel: "gpt-4.1-mini",
    message: "ok",
    testedAt: "2026-03-23T10:00:00.000Z",
  })
})

test("pickLatestProviderModelHealthSnapshot returns the latest valid snapshot", () => {
  const older = buildProviderModelHealthSnapshot({
    groupId: "group-a",
    providerId: "provider-1",
    ok: false,
    testedAt: "2026-03-23T10:00:00.000Z",
  })
  const newer = buildProviderModelHealthSnapshot({
    groupId: "group-b",
    providerId: "provider-1",
    ok: true,
    testedAt: "2026-03-23T10:05:00.000Z",
  })

  assert.equal(pickLatestProviderModelHealthSnapshot([older, null, undefined, newer]), newer)
})

test("formatProviderLatency renders milliseconds, seconds, and minutes", () => {
  assert.equal(formatProviderLatency(128), "128 ms")
  assert.equal(formatProviderLatency(1_250), "1.3 s")
  assert.equal(formatProviderLatency(12_000), "12 s")
  assert.equal(formatProviderLatency(120_000), "2 min")
  assert.equal(formatProviderLatency(Number.NaN), null)
})
