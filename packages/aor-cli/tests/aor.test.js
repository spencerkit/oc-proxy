const assert = require("node:assert/strict")
const { Readable } = require("node:stream")
const { test } = require("node:test")

const {
  commandAdminPasswordClear,
  commandAdminPasswordSet,
  extractApiErrorMessage,
  parseAdminPasswordSetArgs,
} = require("../bin/aor.js")

test("parseAdminPasswordSetArgs accepts a positional password", () => {
  assert.deepEqual(parseAdminPasswordSetArgs(["hunter2-pass"]), {
    password: "hunter2-pass",
    passwordFromStdin: false,
  })
})

test("parseAdminPasswordSetArgs accepts --password-stdin", () => {
  assert.deepEqual(parseAdminPasswordSetArgs(["--password-stdin"]), {
    password: null,
    passwordFromStdin: true,
  })
})

test("extractApiErrorMessage reads standard API error payloads", () => {
  assert.equal(
    extractApiErrorMessage({
      error: {
        code: "validation_error",
        message: "remote admin password must be at least 8 characters",
      },
    }),
    "remote admin password must be at least 8 characters"
  )
})

test("commandAdminPasswordSet sends the local API request", async () => {
  const stdin = Readable.from(["correct horse battery staple\n"])
  stdin.isTTY = false

  const calls = []
  const logs = []

  await commandAdminPasswordSet(["--password-stdin"], {
    port: 17777,
    stdin,
    log: message => logs.push(message),
    checkHealth: async port => {
      assert.equal(port, 17777)
      return true
    },
    requestLocalApi: async (port, pathname, options) => {
      calls.push({ port, pathname, options })
      return { authenticated: true, passwordConfigured: true }
    },
  })

  assert.deepEqual(calls, [
    {
      port: 17777,
      pathname: "/api/config/remote-admin-password",
      options: {
        method: "PUT",
        json: { password: "correct horse battery staple" },
      },
    },
  ])
  assert.deepEqual(logs, [
    "Remote management password configured. Management: http://127.0.0.1:17777/management",
  ])
})

test("commandAdminPasswordClear sends the delete request", async () => {
  const calls = []
  const logs = []

  await commandAdminPasswordClear({
    port: 18899,
    log: message => logs.push(message),
    checkHealth: async () => true,
    requestLocalApi: async (port, pathname, options) => {
      calls.push({ port, pathname, options })
      return { authenticated: true, passwordConfigured: false }
    },
  })

  assert.deepEqual(calls, [
    {
      port: 18899,
      pathname: "/api/config/remote-admin-password",
      options: {
        method: "DELETE",
      },
    },
  ])
  assert.deepEqual(logs, [
    "Remote management password cleared. Management: http://127.0.0.1:18899/management",
  ])
})
