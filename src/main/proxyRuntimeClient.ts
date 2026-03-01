// @ts-nocheck
const path = require("node:path")
const fs = require("node:fs")
const { fork } = require("node:child_process")
const { EventEmitter } = require("node:events")
let utilityProcess = null
try {
  // Electron runtime only; in plain Node tests this may fail.
  utilityProcess = require("electron")?.utilityProcess || null
} catch {
  utilityProcess = null
}

const DEFAULT_CALL_TIMEOUT_MS = 15_000
const DEFAULT_BOOT_TIMEOUT_MS = 20_000
const DEFAULT_SHUTDOWN_TIMEOUT_MS = 3_000
const RESTART_BACKOFF_MS = [500, 1_000, 2_000, 5_000, 10_000]

function clone(value) {
  if (value == null) return value
  return JSON.parse(JSON.stringify(value))
}

function buildRuntimeError(message, code = "proxy_runtime_error") {
  const err = new Error(message)
  err.code = code
  return err
}

function toStatusFallback() {
  return {
    running: false,
    address: null,
    metrics: {
      requests: 0,
      streamRequests: 0,
      errors: 0,
      avgLatencyMs: 0,
      inputTokens: 0,
      outputTokens: 0,
      cacheReadTokens: 0,
      cacheWriteTokens: 0,
      uptimeStartedAt: null,
    },
  }
}

function mapWorkerError(payload) {
  const err = new Error(payload?.message || "Worker request failed")
  err.code = payload?.code || "worker_request_failed"
  if (payload?.statusCode != null) err.statusCode = payload.statusCode
  if (payload?.details != null) err.details = payload.details
  if (payload?.stack) err.workerStack = payload.stack
  return err
}

function resolveWorkerPath(explicitPath) {
  const targetPath = explicitPath || path.join(__dirname, "proxyWorker.js")
  if (!targetPath.includes("app.asar")) {
    return targetPath
  }

  const unpackedPath = targetPath.replace(/([\\/])app\.asar([\\/])/, "$1app.asar.unpacked$2")
  if (unpackedPath !== targetPath && fs.existsSync(unpackedPath)) {
    return unpackedPath
  }

  return targetPath
}

function isChildConnected(child) {
  if (!child) return false
  if (typeof child.connected === "boolean") return child.connected
  return child.pid != null
}

function createUtilityChild(modulePath) {
  const utility = utilityProcess.fork(modulePath, [], {
    stdio: ["ignore", "pipe", "pipe"],
    env: {
      ...process.env,
    },
    execArgv: [],
    serviceName: "ProxyWorker",
  })

  const wrapped = new EventEmitter()
  wrapped.stdout = utility.stdout
  wrapped.stderr = utility.stderr
  wrapped.connected = true
  wrapped.pid = utility.pid
  wrapped.send = (message, callback) => {
    if (!wrapped.connected) {
      if (typeof callback === "function") {
        callback(new Error("Utility worker channel is closed"))
      }
      return
    }
    try {
      utility.postMessage(message)
      if (typeof callback === "function") callback(null)
    } catch (error) {
      if (typeof callback === "function") callback(error)
    }
  }
  wrapped.kill = () => {
    wrapped.connected = false
    try {
      return utility.kill()
    } catch {
      return false
    }
  }

  utility.on("message", messageEvent => {
    const message =
      messageEvent && typeof messageEvent === "object" && "data" in messageEvent
        ? messageEvent.data
        : messageEvent
    wrapped.emit("message", message)
  })
  utility.on("spawn", () => {
    wrapped.pid = utility.pid
    wrapped.emit("spawn")
  })
  utility.on("exit", code => {
    wrapped.connected = false
    wrapped.pid = undefined
    wrapped.emit("exit", code, null)
  })

  return wrapped
}

class ProxyRuntimeClient extends EventEmitter {
  constructor(options = {}) {
    super()
    this.workerPath = resolveWorkerPath(options.workerPath)
    this.callTimeoutMs = options.callTimeoutMs || DEFAULT_CALL_TIMEOUT_MS
    this.bootTimeoutMs = options.bootTimeoutMs || DEFAULT_BOOT_TIMEOUT_MS
    this.logLimit =
      Number.isInteger(options.logLimit) && options.logLimit > 0 ? options.logLimit : 100

    this.child = null
    this.currentConfig = null
    this.desiredRunning = false
    this.isShuttingDown = false
    this.pending = new Map()
    this.nextRequestId = 1
    this.lastKnownStatus = toStatusFallback()

    this.restartAttempts = 0
    this.restartTimer = null
    this.spawnPromise = null
  }

  async initialize(config) {
    this.currentConfig = clone(config)
    await this.ensureWorker()
  }

  async getStatus() {
    try {
      const status = await this.request("getStatus")
      this.lastKnownStatus = status
      return status
    } catch (error) {
      const fallback = {
        ...this.lastKnownStatus,
        running: false,
        address: null,
      }
      fallback.workerError = error?.message || String(error)
      return fallback
    }
  }

  async startServer() {
    this.desiredRunning = true
    const status = await this.request("start")
    this.lastKnownStatus = status
    return status
  }

  async stopServer() {
    this.desiredRunning = false
    const status = await this.request("stop")
    this.lastKnownStatus = status
    return status
  }

  async setConfig(config) {
    this.currentConfig = clone(config)
    await this.request("setConfig", { config: this.currentConfig })
    return { ok: true }
  }

  async listLogs(max = 100) {
    return this.request("listLogs", { max })
  }

  async clearLogs() {
    return this.request("clearLogs")
  }

  async shutdown(timeoutMs = DEFAULT_SHUTDOWN_TIMEOUT_MS) {
    this.isShuttingDown = true
    this.desiredRunning = false
    if (this.restartTimer) {
      clearTimeout(this.restartTimer)
      this.restartTimer = null
    }

    const activeChild = this.child
    if (!activeChild) return

    try {
      await this.requestWithChild(activeChild, "shutdown", {}, timeoutMs, false)
    } catch {
      // Worker may already be gone or unresponsive; hard kill as fallback.
    }

    await new Promise(resolve => {
      const done = () => {
        clearTimeout(timer)
        resolve()
      }
      const timer = setTimeout(() => {
        try {
          if (activeChild.connected) {
            activeChild.kill("SIGKILL")
          }
        } catch {
          // ignore forced kill errors
        }
        done()
      }, timeoutMs)

      activeChild.once("exit", done)
    })
  }

  async request(method, payload = {}) {
    await this.ensureWorker()
    const child = this.child
    if (!child) {
      throw buildRuntimeError("Proxy worker is not available", "worker_unavailable")
    }

    try {
      return await this.requestWithChild(child, method, payload, this.callTimeoutMs, true)
    } catch (error) {
      if (this.isRetryableError(error)) {
        await this.forceRespawn()
        const nextChild = this.child
        if (!nextChild) {
          throw error
        }
        return this.requestWithChild(nextChild, method, payload, this.callTimeoutMs, false)
      }
      throw error
    }
  }

  isRetryableError(error) {
    return (
      error?.code === "worker_disconnected" ||
      error?.code === "worker_exited" ||
      error?.code === "worker_timeout"
    )
  }

  async forceRespawn() {
    if (this.child) {
      try {
        this.child.kill("SIGKILL")
      } catch {
        // ignore kill errors
      }
    }
    this.child = null
    await this.ensureWorker()
  }

  async ensureWorker() {
    if (this.isShuttingDown) {
      throw buildRuntimeError("Proxy runtime is shutting down", "runtime_shutting_down")
    }

    if (isChildConnected(this.child)) {
      return
    }

    if (this.spawnPromise) {
      return this.spawnPromise
    }

    this.spawnPromise = this.spawnWorker().finally(() => {
      this.spawnPromise = null
    })
    return this.spawnPromise
  }

  async spawnWorker() {
    if (!fs.existsSync(this.workerPath)) {
      throw buildRuntimeError(
        `Proxy worker entry not found: ${this.workerPath}`,
        "worker_entry_missing"
      )
    }

    const shouldUseUtilityProcess =
      process.versions?.electron &&
      utilityProcess?.fork &&
      process.env.PROXY_WORKER_DISABLE_UTILITY_PROCESS !== "1"
    const child = shouldUseUtilityProcess
      ? createUtilityChild(this.workerPath)
      : fork(this.workerPath, [], {
          stdio: ["ignore", "pipe", "pipe", "ipc"],
          execPath: process.execPath,
          execArgv: [],
          windowsHide: true,
          env: {
            ...process.env,
            ELECTRON_RUN_AS_NODE: "1",
          },
        })

    this.child = child

    console.info(
      `[Main] Proxy worker spawn mode: ${shouldUseUtilityProcess ? "utilityProcess" : "fork"}`
    )

    if (child.stdout) {
      child.stdout.on("data", chunk => {
        process.stdout.write(`[ProxyWorker] ${String(chunk)}`)
      })
    }

    if (child.stderr) {
      child.stderr.on("data", chunk => {
        process.stderr.write(`[ProxyWorker] ${String(chunk)}`)
      })
    }

    child.on("message", message => {
      this.handleWorkerMessage(child, message)
    })

    child.on("exit", (code, signal) => {
      this.handleWorkerExit(child, code, signal)
    })

    child.on("error", error => {
      console.error("[Main] Proxy worker process error:", error)
    })

    try {
      await this.bootstrapWorker(child)
    } catch (error) {
      try {
        if (isChildConnected(child)) {
          child.kill("SIGKILL")
        }
      } catch {
        // ignore kill errors after bootstrap failure
      }
      if (this.child === child) {
        this.child = null
      }
      throw error
    }
  }

  async bootstrapWorker(child) {
    await this.requestWithChild(child, "ping", {}, this.bootTimeoutMs, false)

    if (this.currentConfig) {
      await this.requestWithChild(
        child,
        "init",
        { config: this.currentConfig, logLimit: this.logLimit },
        this.bootTimeoutMs,
        false
      )
    }

    if (this.desiredRunning) {
      const status = await this.requestWithChild(child, "start", {}, this.callTimeoutMs, false)
      this.lastKnownStatus = status
    } else {
      const status = await this.requestWithChild(child, "getStatus", {}, this.callTimeoutMs, false)
      this.lastKnownStatus = status
    }

    this.restartAttempts = 0
    this.emit("worker-ready")
  }

  handleWorkerMessage(child, message) {
    if (!message || message.type !== "response") {
      return
    }
    const entry = this.pending.get(message.id)
    if (!entry || entry.child !== child) {
      return
    }

    this.pending.delete(message.id)
    clearTimeout(entry.timer)

    if (message.ok) {
      entry.resolve(message.result)
      return
    }
    entry.reject(mapWorkerError(message.error))
  }

  handleWorkerExit(child, code, signal) {
    if (this.child !== child) {
      return
    }

    this.child = null

    for (const [id, pending] of this.pending.entries()) {
      if (pending.child !== child) continue
      clearTimeout(pending.timer)
      pending.reject(
        buildRuntimeError(
          `Proxy worker exited (code=${code ?? "null"}, signal=${signal || "none"})`,
          "worker_exited"
        )
      )
      this.pending.delete(id)
    }

    this.lastKnownStatus = {
      ...this.lastKnownStatus,
      running: false,
      address: null,
    }

    if (this.isShuttingDown) {
      return
    }

    this.scheduleRestart()
  }

  scheduleRestart() {
    if (this.restartTimer || this.isShuttingDown) {
      return
    }

    const delay = RESTART_BACKOFF_MS[Math.min(this.restartAttempts, RESTART_BACKOFF_MS.length - 1)]
    this.restartAttempts += 1

    console.error(`[Main] Proxy worker exited. Restarting in ${delay}ms...`)
    this.restartTimer = setTimeout(() => {
      this.restartTimer = null
      this.ensureWorker().catch(error => {
        console.error("[Main] Proxy worker restart failed:", error)
        this.scheduleRestart()
      })
    }, delay)
  }

  requestWithChild(child, method, payload, timeoutMs, allowSendRetry) {
    if (!isChildConnected(child)) {
      throw buildRuntimeError("Proxy worker is disconnected", "worker_disconnected")
    }

    return new Promise((resolve, reject) => {
      const id = this.nextRequestId++
      const timer = setTimeout(() => {
        this.pending.delete(id)
        const err = buildRuntimeError(
          `Proxy worker request timeout: ${method} (${timeoutMs}ms)`,
          "worker_timeout"
        )
        reject(err)
      }, timeoutMs)

      this.pending.set(id, { resolve, reject, timer, child, method })

      try {
        child.send(
          {
            type: "request",
            id,
            method,
            payload,
          },
          sendError => {
            if (!sendError) return
            const pending = this.pending.get(id)
            if (!pending) return
            clearTimeout(pending.timer)
            this.pending.delete(id)
            const err = buildRuntimeError(
              `Failed to send worker message (${method}): ${sendError.message || sendError}`,
              "worker_disconnected"
            )
            pending.reject(err)
          }
        )
      } catch (sendError) {
        const pending = this.pending.get(id)
        if (!pending) return
        clearTimeout(pending.timer)
        this.pending.delete(id)
        const err = buildRuntimeError(
          `Failed to send worker message (${method}): ${sendError.message || sendError}`,
          "worker_disconnected"
        )
        reject(err)
      }
    }).catch(error => {
      if (!allowSendRetry) {
        throw error
      }
      throw error
    })
  }
}

module.exports = {
  ProxyRuntimeClient,
}
