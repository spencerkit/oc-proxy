import assert from "node:assert/strict"
import { test } from "node:test"

import {
  formatProviderWebsiteLabel,
  openProviderWebsite,
  resolveProviderWebsiteHref,
} from "../../src/renderer/utils/providerWebsite"

type InvokeCall = {
  cmd: string
  args?: Record<string, unknown>
}

type WindowOpenCall = [url?: string | URL, target?: string, features?: string]

const originalWindow = globalThis.window

function setWindow(value: Partial<Window>) {
  Object.defineProperty(globalThis, "window", {
    configurable: true,
    value: value as Window & typeof globalThis,
    writable: true,
  })
}

function restoreWindow() {
  if (originalWindow === undefined) {
    Object.defineProperty(globalThis, "window", {
      configurable: true,
      value: undefined,
      writable: true,
    })
    return
  }

  Object.defineProperty(globalThis, "window", {
    configurable: true,
    value: originalWindow,
    writable: true,
  })
}

test("resolveProviderWebsiteHref normalizes missing protocols", () => {
  assert.equal(resolveProviderWebsiteHref("openai.com"), "https://openai.com/")
  assert.equal(resolveProviderWebsiteHref(" https://anthropic.com/ "), "https://anthropic.com/")
  assert.equal(resolveProviderWebsiteHref("not a valid url value"), null)
})

test("formatProviderWebsiteLabel trims protocol and trailing slashes", () => {
  assert.equal(formatProviderWebsiteLabel("https://openai.com/"), "openai.com")
  assert.equal(formatProviderWebsiteLabel(" api.openai.com/v1/ "), "api.openai.com/v1")
  assert.equal(formatProviderWebsiteLabel(""), null)
})

test("openProviderWebsite uses Tauri IPC when available", async () => {
  const invokeCalls: InvokeCall[] = []
  const openCalls: WindowOpenCall[] = []

  setWindow({
    __TAURI__: {
      core: {
        invoke: async <T>(cmd: string, args?: Record<string, unknown>) => {
          invokeCalls.push({ cmd, args })
          return undefined as T
        },
      },
    },
    open: (...args: WindowOpenCall) => {
      openCalls.push(args)
      return null
    },
  })

  await assert.doesNotReject(() => openProviderWebsite("openai.com"))
  assert.deepEqual(invokeCalls, [
    {
      cmd: "app_open_external_url",
      args: {
        url: "https://openai.com/",
      },
    },
  ])
  assert.deepEqual(openCalls, [])

  restoreWindow()
})

test("openProviderWebsite falls back to window.open outside Tauri", async () => {
  const openCalls: WindowOpenCall[] = []

  setWindow({
    open: (...args: WindowOpenCall) => {
      openCalls.push(args)
      return null
    },
  })

  await assert.doesNotReject(() => openProviderWebsite("https://anthropic.com"))
  assert.deepEqual(openCalls, [["https://anthropic.com/", "_blank", "noopener,noreferrer"]])

  restoreWindow()
})

test("openProviderWebsite ignores invalid websites", async () => {
  const invokeCalls: InvokeCall[] = []
  const openCalls: WindowOpenCall[] = []

  setWindow({
    __TAURI__: {
      core: {
        invoke: async <T>(cmd: string, args?: Record<string, unknown>) => {
          invokeCalls.push({ cmd, args })
          return undefined as T
        },
      },
    },
    open: (...args: WindowOpenCall) => {
      openCalls.push(args)
      return null
    },
  })

  assert.equal(await openProviderWebsite("://bad"), false)
  assert.deepEqual(invokeCalls, [])
  assert.deepEqual(openCalls, [])

  restoreWindow()
})
