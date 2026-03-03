#!/usr/bin/env node

const { spawn } = require("node:child_process")
const path = require("node:path")

function parseArgs(argv) {
  const out = {
    config: path.resolve(process.cwd(), "scripts/benchmark.example.json"),
    scenario: "all",
    output: null,
    mockHost: "127.0.0.1",
    mockPort: 19001,
    chunkDelayMs: 30,
    streamChunks: 24,
  }

  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i]
    if (arg === "--config" && argv[i + 1]) {
      out.config = path.resolve(process.cwd(), argv[i + 1])
      i += 1
      continue
    }
    if (arg === "--scenario" && argv[i + 1]) {
      out.scenario = argv[i + 1]
      i += 1
      continue
    }
    if (arg === "--output" && argv[i + 1]) {
      out.output = path.resolve(process.cwd(), argv[i + 1])
      i += 1
      continue
    }
    if (arg === "--mock-host" && argv[i + 1]) {
      out.mockHost = argv[i + 1]
      i += 1
      continue
    }
    if (arg === "--mock-port" && argv[i + 1]) {
      out.mockPort = Number(argv[i + 1])
      i += 1
      continue
    }
    if (arg === "--chunk-delay-ms" && argv[i + 1]) {
      out.chunkDelayMs = Number(argv[i + 1])
      i += 1
      continue
    }
    if (arg === "--stream-chunks" && argv[i + 1]) {
      out.streamChunks = Number(argv[i + 1])
      i += 1
      continue
    }
    if (arg === "--help") {
      printHelp()
      process.exit(0)
    }
  }

  return out
}

function printHelp() {
  console.log(`Usage:
  node scripts/run-benchmark-with-mock.js [--config <path>] [--scenario all|nonstream|stream|mixed] [--output <path>]
       [--mock-host 127.0.0.1] [--mock-port 19001] [--chunk-delay-ms 30] [--stream-chunks 24]

Notes:
  1. Keep your local proxy server running.
  2. In your active rule, set apiAddress to http://127.0.0.1:19001 (or your mock host/port).
`)
}

async function waitForHealth(url, timeoutMs = 10000) {
  const startedAt = Date.now()
  while (Date.now() - startedAt < timeoutMs) {
    try {
      const resp = await fetch(url)
      if (resp.ok) return true
    } catch {
      // retry
    }
    await new Promise(resolve => setTimeout(resolve, 200))
  }
  return false
}

function runChild(cmd, args, options = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(cmd, args, {
      stdio: "inherit",
      shell: false,
      ...options,
    })
    child.on("error", reject)
    child.on("close", code => {
      if (code === 0) {
        resolve()
      } else {
        reject(new Error(`${cmd} exited with code ${code}`))
      }
    })
  })
}

async function main() {
  const args = parseArgs(process.argv.slice(2))
  const mockScript = path.resolve(process.cwd(), "scripts/mock-upstream.js")
  const benchScript = path.resolve(process.cwd(), "scripts/benchmark-proxy.js")
  const mockUrl = `http://${args.mockHost}:${args.mockPort}/healthz`

  const mockProc = spawn(
    process.execPath,
    [
      mockScript,
      "--host",
      args.mockHost,
      "--port",
      String(args.mockPort),
      "--chunk-delay-ms",
      String(args.chunkDelayMs),
      "--stream-chunks",
      String(args.streamChunks),
    ],
    {
      stdio: "inherit",
    }
  )

  const shutdown = () => {
    if (!mockProc.killed) {
      mockProc.kill("SIGTERM")
    }
  }
  process.on("SIGINT", shutdown)
  process.on("SIGTERM", shutdown)

  try {
    const ready = await waitForHealth(mockUrl, 12000)
    if (!ready) {
      throw new Error(`mock upstream is not ready: ${mockUrl}`)
    }

    console.log(`[bench-with-mock] mock ready at http://${args.mockHost}:${args.mockPort}`)
    console.log("[bench-with-mock] ensure your proxy rule.apiAddress points to the mock upstream")

    const benchArgs = [benchScript, "--config", args.config, "--scenario", args.scenario]
    if (args.output) {
      benchArgs.push("--output", args.output)
    }
    await runChild(process.execPath, benchArgs)
  } finally {
    shutdown()
  }
}

main().catch(error => {
  console.error("[bench-with-mock] failed:", error?.message ? error.message : error)
  process.exit(1)
})
