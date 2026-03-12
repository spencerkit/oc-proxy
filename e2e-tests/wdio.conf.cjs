const fs = require("node:fs")
const path = require("node:path")
const os = require("node:os")
const { spawn, spawnSync } = require("node:child_process")

const ROOT = path.resolve(__dirname, "..")
const APP_BINARY = path.resolve(ROOT, "dist", "target", "debug", "ai-open-router-tauri")
const TAURI_DRIVER_PATH =
  process.env.TAURI_DRIVER_PATH || path.resolve(os.homedir(), ".cargo", "bin", "tauri-driver")
const E2E_DATA_DIR = process.env.E2E_DATA_DIR || path.resolve(ROOT, ".tmp", "e2e-data")
const TAURI_DRIVER_PORT = Number(process.env.TAURI_DRIVER_PORT || 5555)
const TAURI_NATIVE_PORT = Number(process.env.TAURI_NATIVE_PORT || 5556)
const TAURI_NATIVE_DRIVER_PATH = process.env.TAURI_NATIVE_DRIVER_PATH || ""

let tauriDriverProcess = null
let shuttingDown = false

function closeTauriDriver() {
  if (shuttingDown) return
  shuttingDown = true
  if (tauriDriverProcess) {
    tauriDriverProcess.kill()
    tauriDriverProcess = null
  }
}

function onShutdown(fn) {
  process.on("exit", fn)
  process.on("SIGINT", fn)
  process.on("SIGTERM", fn)
  process.on("SIGQUIT", fn)
}

exports.config = {
  runner: "local",
  specs: ["./specs/**/*.e2e.js"],
  maxInstances: 1,
  capabilities: [
    {
      maxInstances: 1,
      "tauri:options": {
        application: APP_BINARY,
      },
    },
  ],
  logLevel: "info",
  bail: 0,
  waitforTimeout: 20000,
  connectionRetryTimeout: 120000,
  connectionRetryCount: 2,
  services: [],
  framework: "mocha",
  reporters: ["spec"],
  mochaOpts: {
    ui: "bdd",
    timeout: 120000,
  },
  host: "127.0.0.1",
  port: TAURI_DRIVER_PORT,
  onPrepare: () => {
    fs.rmSync(E2E_DATA_DIR, { recursive: true, force: true })
    fs.mkdirSync(E2E_DATA_DIR, { recursive: true })
    const result = spawnSync("cargo", ["tauri", "build", "--debug", "--no-bundle"], {
      cwd: ROOT,
      stdio: "inherit",
    })
    if (result.status !== 0) {
      process.exit(result.status ?? 1)
    }
  },
  beforeSession: () => {
    if (!fs.existsSync(TAURI_DRIVER_PATH)) {
      throw new Error(`tauri-driver not found at ${TAURI_DRIVER_PATH}`)
    }
    const args = ["--port", String(TAURI_DRIVER_PORT), "--native-port", String(TAURI_NATIVE_PORT)]
    if (TAURI_NATIVE_DRIVER_PATH) {
      args.push("--native-driver", TAURI_NATIVE_DRIVER_PATH)
    }
    tauriDriverProcess = spawn(TAURI_DRIVER_PATH, args, {
      stdio: ["ignore", process.stdout, process.stderr],
      env: {
        ...process.env,
        XDG_DATA_HOME: E2E_DATA_DIR,
      },
    })
  },
  afterSession: () => {
    closeTauriDriver()
  },
}

onShutdown(() => closeTauriDriver())
