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
  aor start [--port <port>]   Start service in background
  aor stop                    Stop background service
  aor restart [--port <port>] Restart background service
  aor status                  Show service status`)
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
    const response = await fetch(`http://127.0.0.1:${port}/api/health`, {
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
      console.log(`Management: http://127.0.0.1:${state.port || port}/management`)
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
  console.log(`Management: http://127.0.0.1:${port}/management`)
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
  console.log(`management: http://127.0.0.1:${port}/management`)
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

async function main() {
  const argv = process.argv.slice(2)
  if (argv.includes("--help") || argv.includes("-h")) {
    printHelp()
    return
  }

  const command = argv[0] && !argv[0].startsWith("-") ? argv[0] : "start"
  const rest = command === "start" ? argv.slice(command === argv[0] ? 1 : 0) : argv.slice(1)

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
    default:
      throw new Error(`Unknown command: ${command}`)
  }
}

main().catch(error => {
  console.error(error.message || error)
  process.exit(1)
})
