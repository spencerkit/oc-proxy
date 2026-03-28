import assert from "node:assert/strict"
import { test } from "node:test"

import { installMockPreviewRuntime, isMockPreviewEnabled } from "../../src/renderer/dev/mockPreview"

function createMockWindow(search = "") {
  return {
    location: {
      search,
    },
  } as Window & typeof globalThis
}

test("isMockPreviewEnabled only turns on for explicit query flags", () => {
  assert.equal(isMockPreviewEnabled(""), false)
  assert.equal(isMockPreviewEnabled("?mock=0"), false)
  assert.equal(isMockPreviewEnabled("?mock=false"), false)
  assert.equal(isMockPreviewEnabled("?mock=1"), true)
  assert.equal(isMockPreviewEnabled("?foo=bar&mock=true"), true)
})

test("installMockPreviewRuntime registers a usable mock Tauri bridge", async () => {
  const mockWindow = createMockWindow("?mock=1")

  assert.equal(installMockPreviewRuntime(mockWindow), true)
  assert.ok(mockWindow.__TAURI_INTERNALS__?.invoke)

  const session = await mockWindow.__TAURI_INTERNALS__?.invoke<{
    authenticated: boolean
    remoteRequest: boolean
    passwordConfigured: boolean
  }>("auth_get_session_status")
  const config = await mockWindow.__TAURI_INTERNALS__?.invoke<{
    server: { port: number }
    groups: unknown[]
    providers: unknown[]
  }>("config_get")
  const status = await mockWindow.__TAURI_INTERNALS__?.invoke<{
    running: boolean
    metrics: { requests: number }
  }>("app_get_status")

  assert.deepEqual(session, {
    authenticated: true,
    remoteRequest: false,
    passwordConfigured: false,
  })
  assert.equal(config.server.port, 8899)
  assert.deepEqual(config.groups, [])
  assert.deepEqual(config.providers, [])
  assert.equal(status.running, false)
  assert.equal(status.metrics.requests, 0)
})

test("installMockPreviewRuntime leaves existing runtime untouched", () => {
  const invoke = async <T>() => undefined as T
  const mockWindow = createMockWindow("?mock=1")
  mockWindow.__TAURI_INTERNALS__ = { invoke }

  assert.equal(installMockPreviewRuntime(mockWindow), false)
  assert.equal(mockWindow.__TAURI_INTERNALS__?.invoke, invoke)
})
