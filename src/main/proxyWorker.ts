// @ts-nocheck
const path = require("node:path")
const fs = require("node:fs")
const { LogStore } = require("./logStore")

if (!require.extensions[".ts"]) {
  require.extensions[".ts"] = require.extensions[".js"]
}

const srcDir = path.join(__dirname, "../../src")
const isDev = fs.existsSync(srcDir)

function clone(value) {
  if (value == null) return value
  return JSON.parse(JSON.stringify(value))
}

function toSerializableError(error, fallbackCode = "worker_error") {
  return {
    message: error?.message || String(error || "Unknown worker error"),
    code: error?.code || fallbackCode,
    statusCode: error?.statusCode,
    details: error?.details,
    stack: error?.stack,
  }
}

function loadProxyModules() {
  const srcProxyDir = path.join(__dirname, "../../src/proxy")
  const outProxyDir = path.join(__dirname, "../proxy")
  const preferSrc = isDev && fs.existsSync(srcProxyDir)

  const firstDir = preferSrc ? srcProxyDir : outProxyDir
  const secondDir = preferSrc ? outProxyDir : srcProxyDir

  function loadModuleFromDir(dir, baseName) {
    return require(path.join(dir, baseName))
  }

  try {
    const { ProxyServer } = loadModuleFromDir(firstDir, "server")
    return { ProxyServer }
  } catch {
    const { ProxyServer } = loadModuleFromDir(secondDir, "server")
    return { ProxyServer }
  }
}

const { ProxyServer } = loadProxyModules()

let currentConfig = null
const logStore = new LogStore(100)
const configStore = {
  get() {
    if (!currentConfig) {
      const err = new Error("Worker config is not initialized")
      err.code = "worker_config_missing"
      throw err
    }
    return clone(currentConfig)
  },
}
const proxyServer = new ProxyServer(configStore, logStore)
let shuttingDown = false

function assertConfig(config) {
  if (!config || typeof config !== "object") {
    const err = new Error("Invalid runtime config")
    err.code = "invalid_runtime_config"
    err.statusCode = 400
    throw err
  }
}

async function safeStop() {
  try {
    await proxyServer.stop()
  } catch (error) {
    console.error("[ProxyWorker] Failed to stop proxy server:", error)
  }
}

async function shutdownAndExit(code = 0) {
  if (shuttingDown) return
  shuttingDown = true
  await safeStop()
  process.exit(code)
}

async function handleCommand(method, payload) {
  switch (method) {
    case "ping":
      return { ok: true }
    case "init": {
      assertConfig(payload?.config)
      currentConfig = clone(payload.config)
      if (Number.isInteger(payload?.logLimit) && payload.logLimit > 0) {
        logStore.limit = payload.logLimit
      }
      return { ok: true }
    }
    case "setConfig": {
      assertConfig(payload?.config)
      currentConfig = clone(payload.config)
      return { ok: true }
    }
    case "getStatus":
      return proxyServer.getStatus()
    case "start":
      return proxyServer.start()
    case "stop":
      return proxyServer.stop()
    case "listLogs":
      return logStore.list(payload?.max || 100)
    case "clearLogs":
      logStore.clear()
      return { ok: true }
    case "shutdown":
      await shutdownAndExit(0)
      return { ok: true }
    default: {
      const err = new Error(`Unsupported worker method: ${method}`)
      err.code = "unsupported_worker_method"
      err.statusCode = 400
      throw err
    }
  }
}

function sendToParent(message) {
  if (typeof process.send !== "function") return
  process.send(message)
}

process.on("message", async message => {
  if (!message || message.type !== "request") return

  const id = message.id
  const method = message.method
  const payload = message.payload

  try {
    const result = await handleCommand(method, payload)
    sendToParent({
      type: "response",
      id,
      ok: true,
      result,
    })
  } catch (error) {
    sendToParent({
      type: "response",
      id,
      ok: false,
      error: toSerializableError(error),
    })
  }
})

process.on("disconnect", () => {
  shutdownAndExit(0).catch(error => {
    console.error("[ProxyWorker] Failed to shutdown on disconnect:", error)
    process.exit(1)
  })
})

process.on("SIGTERM", () => {
  shutdownAndExit(0).catch(error => {
    console.error("[ProxyWorker] Failed to shutdown on SIGTERM:", error)
    process.exit(1)
  })
})

process.on("SIGINT", () => {
  shutdownAndExit(0).catch(error => {
    console.error("[ProxyWorker] Failed to shutdown on SIGINT:", error)
    process.exit(1)
  })
})

process.on("unhandledRejection", reason => {
  console.error("[ProxyWorker] Unhandled rejection:", reason)
  shutdownAndExit(1).catch(() => process.exit(1))
})

process.on("uncaughtException", error => {
  console.error("[ProxyWorker] Uncaught exception:", error)
  shutdownAndExit(1).catch(() => process.exit(1))
})

sendToParent({ type: "ready" })
