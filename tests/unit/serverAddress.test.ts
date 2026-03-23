import assert from "node:assert/strict"
import { test } from "node:test"

import {
  formatServerAddressForDisplay,
  resolveReachableServerBaseUrl,
  resolveReachableServerBaseUrls,
} from "../../src/renderer/utils/serverAddress"

test("resolveReachableServerBaseUrls prefers the current origin for remote access", () => {
  const urls = resolveReachableServerBaseUrls({
    currentOrigin: "http://remote-aor.test:17777",
    statusAddress: "http://localhost:8899",
    statusLanAddress: "http://192.168.1.9:8899",
    configHost: "0.0.0.0",
    configPort: 8899,
  })

  assert.deepEqual(urls, [
    "http://remote-aor.test:17777",
    "http://localhost:8899",
    "http://192.168.1.9:8899",
  ])
})

test("resolveReachableServerBaseUrls normalizes loopback and wildcard candidates", () => {
  const urls = resolveReachableServerBaseUrls({
    statusAddress: "0.0.0.0:8899",
    statusLanAddress: "http://127.0.0.1:8899",
    configHost: "localhost",
    configPort: 8899,
  })

  assert.deepEqual(urls, ["http://localhost:8899"])
})

test("resolveReachableServerBaseUrl falls back to config host and port", () => {
  assert.equal(
    resolveReachableServerBaseUrl({
      configHost: "10.10.0.8",
      configPort: 9900,
    }),
    "http://10.10.0.8:9900"
  )
})

test("formatServerAddressForDisplay keeps IPv6 brackets", () => {
  assert.equal(formatServerAddressForDisplay("http://[2001:db8::1]:8899"), "[2001:db8::1]:8899")
})
