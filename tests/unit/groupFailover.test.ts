import assert from "node:assert/strict"
import { test } from "node:test"

import { normalizeGroupFailoverConfig } from "../../src/renderer/utils/groupFailover"

test("normalizeGroupFailoverConfig returns defaults when failover config is absent", () => {
  assert.deepEqual(normalizeGroupFailoverConfig(), {
    enabled: false,
    failureThreshold: 3,
    cooldownSeconds: 300,
  })
})

test("normalizeGroupFailoverConfig preserves explicit failover settings", () => {
  assert.deepEqual(
    normalizeGroupFailoverConfig({
      enabled: true,
      failureThreshold: 5,
      cooldownSeconds: 90,
    }),
    {
      enabled: true,
      failureThreshold: 5,
      cooldownSeconds: 90,
    }
  )
})
