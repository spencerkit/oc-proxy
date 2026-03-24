#!/usr/bin/env node

const fs = require("node:fs")
const os = require("node:os")
const path = require("node:path")
const { spawn } = require("node:child_process")

const vendorDir = path.resolve(__dirname, "..", "vendor")
const binName = process.platform === "win32" ? "ai-open-router.exe" : "ai-open-router"
const binPath = path.join(vendorDir, binName)

const cliHomeDir = process.env.AOR_CLI_HOME?.trim() || path.join(os.homedir(), ".aor")
const appDataDir = process.env.AOR_APP_DATA_DIR?.trim() || path.join(cliHomeDir, "data")
const stateFilePath = path.join(cliHomeDir, "daemon.json")
const logFilePath = path.join(cliHomeDir, "daemon.log")
const configFilePath = path.join(appDataDir, "config.json")
const DEFAULT_PORT = 8899

function printHelp() {
  console.log(`aor commands:
  aor start [--port <port>]                     Start service in background
  aor stop                                      Stop background service
  aor restart [--port <port>]                   Restart background service
  aor status                                    Show service status
  aor admin-password set <password>             Set remote management password
  aor admin-password set --password-stdin       Read password from stdin
  aor admin-password clear                      Clear remote management password`)
}

function ensureBinary() {
  if (!fs.existsSync(binPath)) {
    throw new Error("ai-open-router binary not found. Reinstall @spencer-kit/aor.")
  }
}

function ensureDirectories() {
  fs.mkdirSync(cliHomeDir, { recursive: true })
  fs.mkdirSync(appDataDir, { recursive: true })
}

function readJsonFile(filePath) {
  if (!fs.existsSync(filePath)) return null
  try {
    return JSON.parse(fs.readFileSync(filePath, "utf8"))
  } catch {
    return null
  }
}

function writeJsonFile(filePath, value) {
  fs.mkdirSync(path.dirname(filePath), { recursive: true })
  fs.writeFileSync(filePath, `${JSON.stringify(value, null, 2)}\n`)
}

function removeFileIfExists(filePath) {
  try {
    if (fs.existsSync(filePath)) fs.unlinkSync(filePath)
  } catch {
    // ignore
  }
}

function parsePort(argv) {
  for (let i = 0; i < argv.length; i += 1) {
    const token = argv[i]
    if (token === "--port") {
      const next = argv[i + 1]
      if (!next) throw new Error("Missing value for --port")
      return Number(next)
    }
    if (token.startsWith("--port=")) {
      return Number(token.slice("--port=".length))
    }
  }
  return null
}

function validatePort(port) {
  if (!Number.isInteger(port) || port < 1 || port > 65535) {
    throw new Error("Invalid --port. Allowed range: 1-65535")
  }
}

function formatManagementUrl(port) {
  return `http://127.0.0.1:${port}/management`
}

function buildApiUrl(port, pathname) {
  return `http://127.0.0.1:${port}${pathname}`
}

function isProcessAlive(pid) {
  if (!pid || typeof pid !== "number") return false
  try {
    process.kill(pid, 0)
    return true
  } catch {
    return false
  }
}

function sleep(ms) {
  return new Promise(resolve => {
    setTimeout(resolve, ms)
  })
}

async function checkHealth(port, timeoutMs = 1200) {
  const controller = new AbortController()
  const timer = setTimeout(() => controller.abort(), timeoutMs)
  try {
    const response = await fetch(buildApiUrl(port, "/api/health"), {
      signal: controller.signal,
    })
    return response.ok
  } catch {
    return false
  } finally {
    clearTimeout(timer)
  }
}

async function waitForHealth(port, timeoutMs = 15000) {
  const startedAt = Date.now()
  while (Date.now() - startedAt < timeoutMs) {
    if (await checkHealth(port, 1000)) return true
    await sleep(250)
  }
  return false
}

async function waitForExit(pid, timeoutMs = 10000) {
  const startedAt = Date.now()
  while (Date.now() - startedAt < timeoutMs) {
    if (!isProcessAlive(pid)) return true
    await sleep(250)
  }
  return false
}

function readState() {
  return readJsonFile(stateFilePath)
}

function writeState(state) {
  writeJsonFile(stateFilePath, state)
}

function clearState() {
  removeFileIfExists(stateFilePath)
}

function readConfiguredPort() {
  const config = readJsonFile(configFilePath)
  const rawPort = config?.server?.port
  if (Number.isInteger(rawPort) && rawPort >= 1 && rawPort <= 65535) {
    return rawPort
  }
  return DEFAULT_PORT
}

function applyStartupPort(port) {
  const current = readJsonFile(configFilePath)
  const next = current && typeof current === "object" ? current : {}
  const nextServer = next.server && typeof next.server === "object" ? next.server : {}
  next.server = {
    ...nextServer,
    host: nextServer.host || "127.0.0.1",
    port,
  }
  writeJsonFile(configFilePath, next)
}

function resolveRuntimePort() {
  const state = readState()
  const statePort = state?.port
  if (Number.isInteger(statePort) && statePort >= 1 && statePort <= 65535) {
    return statePort
  }
  return readConfiguredPort()
}

function parseAdminPasswordSetArgs(argv) {
  let password = null
  let passwordFromStdin = false

  for (let i = 0; i < argv.length; i += 1) {
    const token = argv[i]
    if (token === "--password-stdin") {
      if (password !== null || passwordFromStdin) {
        throw new Error("Use either a positional password or --password-stdin, not both.")
      }
      passwordFromStdin = true
      continue
    }
    if (token === "--password") {
      const next = argv[i + 1]
      if (!next) throw new Error("Missing value for --password")
      if (password !== null || passwordFromStdin) {
        throw new Error("Use either a positional password or --password, not both.")
      }
      password = next
      i += 1
      continue
    }
    if (token.startsWith("--password=")) {
      if (password !== null || passwordFromStdin) {
        throw new Error("Use either a positional password or --password, not both.")
      }
      password = token.slice("--password=".length)
      continue
    }
    if (token.startsWith("-")) {
      throw new Error(`Unknown argument for admin-password set: ${token}`)
    }
    if (password !== null) {
      throw new Error("Too many arguments for admin-password set")
    }
    password = token
  }

  return { password, passwordFromStdin }
}

async function readPasswordFromStdin(stdin = process.stdin) {
  if (stdin.isTTY) {
    throw new Error(
      "Missing password. Pass `aor admin-password set <password>` or pipe one with --password-stdin."
    )
  }

  const chunks = []
  for await (const chunk of stdin) {
    chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(String(chunk)))
  }
  const password = Buffer.concat(chunks)
    .toString("utf8")
    .replace(/\r?\n$/, "")
  if (!password.trim()) {
    throw new Error("Remote management password must not be empty.")
  }
  return password
}

async function resolveAdminPasswordInput(argv, stdin = process.stdin) {
  const { password, passwordFromStdin } = parseAdminPasswordSetArgs(argv)
  if (password !== null) {
    if (!password.trim()) {
      throw new Error("Remote management password must not be empty.")
    }
    return password
  }
  if (passwordFromStdin || !stdin.isTTY) {
    return readPasswordFromStdin(stdin)
  }

  throw new Error(
    "Missing password. Pass `aor admin-password set <password>` or pipe one with --password-stdin."
  )
}

function extractApiErrorMessage(payload) {
  if (!payload || typeof payload !== "object") return null
  const error = payload.error
  if (!error || typeof error !== "object") return null
  if (typeof error.message === "string" && error.message.trim()) {
    return error.message.trim()
  }
  return null
}

async function ensureServiceRunning(port, healthCheck = checkHealth) {
  const healthy = await healthCheck(port, 1000)
  if (healthy) return

  const portHint = port === DEFAULT_PORT ? "" : ` --port ${port}`
  throw new Error(
    `aor is not running on port ${port}. Start it first with \`aor start${portHint}\`.`
  )
}

async function requestLocalApi(port, pathname, options = {}, fetchImpl = fetch) {
  const method = options.method || "GET"
  const headers = {
    Accept: "application/json",
    ...(options.headers || {}),
  }
  const init = {
    method,
    headers,
  }

  if (options.json !== undefined) {
    headers["Content-Type"] = "application/json"
    init.body = JSON.stringify(options.json)
  }

  let response
  try {
    response = await fetchImpl(buildApiUrl(port, pathname), init)
  } catch (error) {
    throw new Error(`request to local aor service failed: ${error.message || error}`)
  }

  const rawText = await response.text()
  let payload = null
  if (rawText.trim()) {
    try {
      payload = JSON.parse(rawText)
    } catch {
      payload = rawText
    }
  }

  if (!response.ok) {
    const apiMessage = extractApiErrorMessage(payload)
    throw new Error(apiMessage || `${method} ${pathname} failed with status ${response.status}`)
  }

  return payload
}

async function commandStart(argv) {
  ensureBinary()
  ensureDirectories()

  const requestedPort = parsePort(argv)
  if (requestedPort !== null) {
    validatePort(requestedPort)
    applyStartupPort(requestedPort)
  }
  const port = requestedPort ?? readConfiguredPort()

  const state = readState()
  if (state?.pid && isProcessAlive(state.pid)) {
    const healthy = await checkHealth(state.port || port)
    if (healthy) {
      console.log(`aor is already running (pid=${state.pid})`)
      console.log(`Management: ${formatManagementUrl(state.port || port)}`)
      return
    }
  }

  const outFd = fs.openSync(logFilePath, "a")
  const child = spawn(binPath, [], {
    detached: true,
    stdio: ["ignore", outFd, outFd],
    env: {
      ...process.env,
      AOR_APP_DATA_DIR: appDataDir,
    },
  })
  child.unref()

  writeState({
    pid: child.pid,
    port,
    appDataDir,
    logFilePath,
    startedAt: new Date().toISOString(),
  })

  const healthy = await waitForHealth(port)
  if (!healthy) {
    if (isProcessAlive(child.pid)) {
      process.kill(child.pid, "SIGTERM")
    }
    clearState()
    throw new Error(`aor failed to start on port ${port}. Check logs: ${logFilePath}`)
  }

  console.log(`aor started (pid=${child.pid})`)
  console.log(`Management: ${formatManagementUrl(port)}`)
}

async function commandStop() {
  const state = readState()
  if (!state?.pid) {
    console.log("aor is not running (no pid state).")
    return
  }

  const pid = state.pid
  if (!isProcessAlive(pid)) {
    clearState()
    console.log("aor is already stopped.")
    return
  }

  process.kill(pid, "SIGTERM")
  const exited = await waitForExit(pid, 8000)
  if (!exited && isProcessAlive(pid)) {
    process.kill(pid, "SIGKILL")
    await waitForExit(pid, 3000)
  }

  clearState()
  console.log("aor stopped.")
}

async function commandStatus() {
  const state = readState()
  const port = state?.port || readConfiguredPort()
  const pid = state?.pid ?? null
  const alive = pid ? isProcessAlive(pid) : false
  const healthy = await checkHealth(port)

  console.log(`running: ${healthy ? "yes" : "no"}`)
  console.log(`pid: ${pid ?? "-"}`)
  console.log(`port: ${port}`)
  console.log(`management: ${formatManagementUrl(port)}`)
  if (state?.logFilePath) {
    console.log(`log: ${state.logFilePath}`)
  }

  if (alive && !healthy) {
    console.log("warning: process is alive but health check failed")
  }
}

async function commandRestart(argv) {
  await commandStop()
  await commandStart(argv)
}

async function commandAdminPasswordSet(argv, deps = {}) {
  const port = deps.port ?? resolveRuntimePort()
  const healthCheck = deps.checkHealth ?? checkHealth
  const stdin = deps.stdin ?? process.stdin
  const log = deps.log ?? console.log
  const request =
    deps.requestLocalApi ??
    ((targetPort, pathname, options) => requestLocalApi(targetPort, pathname, options, deps.fetch))

  await ensureServiceRunning(port, healthCheck)
  const password = await resolveAdminPasswordInput(argv, stdin)
  await request(port, "/api/config/remote-admin-password", {
    method: "PUT",
    json: { password },
  })
  log(`Remote management password configured. Management: ${formatManagementUrl(port)}`)
}

async function commandAdminPasswordClear(deps = {}) {
  const port = deps.port ?? resolveRuntimePort()
  const healthCheck = deps.checkHealth ?? checkHealth
  const log = deps.log ?? console.log
  const request =
    deps.requestLocalApi ??
    ((targetPort, pathname, options) => requestLocalApi(targetPort, pathname, options, deps.fetch))

  await ensureServiceRunning(port, healthCheck)
  await request(port, "/api/config/remote-admin-password", {
    method: "DELETE",
  })
  log(`Remote management password cleared. Management: ${formatManagementUrl(port)}`)
}

async function commandAdminPassword(argv, deps = {}) {
  const subcommand = argv[0]
  const rest = argv.slice(1)

  switch (subcommand) {
    case "set":
      await commandAdminPasswordSet(rest, deps)
      break
    case "clear":
      await commandAdminPasswordClear(deps)
      break
    default:
      throw new Error("Unknown admin-password command. Use `set` or `clear`.")
  }
}

async function main(argvInput = process.argv.slice(2), deps = {}) {
  const argv = argvInput
  if (argv.includes("--help") || argv.includes("-h")) {
    printHelp()
    return
  }

  const explicitCommand = argv[0] && !argv[0].startsWith("-") ? argv[0] : null
  const command = explicitCommand || "start"
  const rest = explicitCommand ? argv.slice(1) : argv

  switch (command) {
    case "start":
      await commandStart(rest)
      break
    case "stop":
      await commandStop()
      break
    case "restart":
      await commandRestart(rest)
      break
    case "status":
      await commandStatus()
      break
    case "admin-password":
      await commandAdminPassword(rest, deps)
      break
    default:
      throw new Error(`Unknown command: ${command}`)
  }
}

if (require.main === module) {
  main().catch(error => {
    console.error(error.message || error)
    process.exit(1)
  })
}

module.exports = {
  commandAdminPasswordClear,
  commandAdminPasswordSet,
  extractApiErrorMessage,
  main,
  parseAdminPasswordSetArgs,
  readPasswordFromStdin,
  requestLocalApi,
  resolveAdminPasswordInput,
}
