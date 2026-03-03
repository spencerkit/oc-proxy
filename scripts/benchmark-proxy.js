#!/usr/bin/env node

const fs = require("node:fs")
const path = require("node:path")
const { performance } = require("node:perf_hooks")

function parseArgs(argv) {
  const out = {
    config: path.resolve(process.cwd(), "scripts/benchmark.example.json"),
    scenario: "all",
    output: null,
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
    if (arg === "--help" || arg === "-h") {
      printHelp()
      process.exit(0)
    }
  }
  return out
}

function printHelp() {
  console.log(`Usage:
  node scripts/benchmark-proxy.js [--config <path>] [--scenario all|nonstream|stream|mixed] [--output <path>]

Examples:
  node scripts/benchmark-proxy.js --config scripts/benchmark.example.json
  node scripts/benchmark-proxy.js --scenario nonstream
  node scripts/benchmark-proxy.js --output out/bench-report.json
`)
}

function loadConfig(configPath) {
  const raw = fs.readFileSync(configPath, "utf-8")
  return JSON.parse(raw)
}

function resolveUrl(baseUrl, sectionPath, fallbackPath) {
  const resolvedPath = sectionPath || fallbackPath
  if (!resolvedPath) {
    throw new Error("missing request path")
  }
  if (/^https?:\/\//i.test(resolvedPath)) {
    return resolvedPath
  }
  const normalized = resolvedPath.startsWith("/") ? resolvedPath : `/${resolvedPath}`
  return new URL(normalized, baseUrl).toString()
}

function nowIso() {
  return new Date().toISOString()
}

function percentile(sortedValues, p) {
  if (sortedValues.length === 0) return 0
  const rank = Math.min(
    sortedValues.length - 1,
    Math.max(0, Math.ceil((p / 100) * sortedValues.length) - 1)
  )
  return sortedValues[rank]
}

function toLatencySummary(latenciesMs) {
  if (!latenciesMs.length) {
    return {
      count: 0,
      avgMs: 0,
      minMs: 0,
      maxMs: 0,
      p50Ms: 0,
      p95Ms: 0,
      p99Ms: 0,
    }
  }

  const sorted = [...latenciesMs].sort((a, b) => a - b)
  const sum = sorted.reduce((acc, item) => acc + item, 0)

  return {
    count: sorted.length,
    avgMs: Number((sum / sorted.length).toFixed(2)),
    minMs: Number(sorted[0].toFixed(2)),
    maxMs: Number(sorted[sorted.length - 1].toFixed(2)),
    p50Ms: Number(percentile(sorted, 50).toFixed(2)),
    p95Ms: Number(percentile(sorted, 95).toFixed(2)),
    p99Ms: Number(percentile(sorted, 99).toFixed(2)),
  }
}

function createCollector() {
  return {
    started: 0,
    success: 0,
    failed: 0,
    timeouts: 0,
    statusCodeCounts: {},
    errorTypeCounts: {},
    latenciesMs: [],
    firstByteLatenciesMs: [],
  }
}

function recordStatusCode(collector, statusCode) {
  const key = String(statusCode)
  collector.statusCodeCounts[key] = (collector.statusCodeCounts[key] || 0) + 1
}

function recordErrorType(collector, type) {
  collector.errorTypeCounts[type] = (collector.errorTypeCounts[type] || 0) + 1
}

function withTimeout(timeoutMs) {
  const controller = new AbortController()
  const timer = setTimeout(() => controller.abort(), timeoutMs)
  return { controller, timer }
}

function isExpectedStatus(status, expectedStatuses) {
  if (!Array.isArray(expectedStatuses) || expectedStatuses.length === 0) {
    return status >= 200 && status < 300
  }
  return expectedStatuses.includes(status)
}

async function executeNonStreamRequest({
  url,
  method,
  headers,
  body,
  timeoutMs,
  expectedStatuses,
}) {
  const startedAt = performance.now()
  const { controller, timer } = withTimeout(timeoutMs)
  try {
    const response = await fetch(url, {
      method,
      headers,
      body: JSON.stringify(body || {}),
      signal: controller.signal,
    })
    const payload = await response.text()
    const elapsedMs = performance.now() - startedAt

    if (!isExpectedStatus(response.status, expectedStatuses)) {
      return {
        ok: false,
        elapsedMs,
        statusCode: response.status,
        errorType: `http_${response.status}`,
        detail: payload.slice(0, 200),
      }
    }

    return {
      ok: true,
      elapsedMs,
      statusCode: response.status,
    }
  } catch (error) {
    const elapsedMs = performance.now() - startedAt
    const timeout = error && error.name === "AbortError"
    return {
      ok: false,
      elapsedMs,
      statusCode: null,
      errorType: timeout ? "timeout" : "network_error",
      detail: String(error?.message || error),
    }
  } finally {
    clearTimeout(timer)
  }
}

async function executeStreamRequest({ url, method, headers, body, timeoutMs, expectedStatuses }) {
  const startedAt = performance.now()
  const { controller, timer } = withTimeout(timeoutMs)
  try {
    const response = await fetch(url, {
      method,
      headers,
      body: JSON.stringify(body || {}),
      signal: controller.signal,
    })

    if (!isExpectedStatus(response.status, expectedStatuses)) {
      const payload = await response.text()
      return {
        ok: false,
        elapsedMs: performance.now() - startedAt,
        firstByteMs: 0,
        statusCode: response.status,
        errorType: `http_${response.status}`,
        detail: payload.slice(0, 200),
      }
    }

    if (!response.body) {
      return {
        ok: false,
        elapsedMs: performance.now() - startedAt,
        firstByteMs: 0,
        statusCode: response.status,
        errorType: "stream_body_missing",
        detail: "response.body is empty",
      }
    }

    const reader = response.body.getReader()
    let firstByteMs = 0
    while (true) {
      const { done, value } = await reader.read()
      if (done) break
      if (!firstByteMs && value && value.length > 0) {
        firstByteMs = performance.now() - startedAt
      }
    }

    return {
      ok: true,
      elapsedMs: performance.now() - startedAt,
      firstByteMs,
      statusCode: response.status,
    }
  } catch (error) {
    const elapsedMs = performance.now() - startedAt
    const timeout = error && error.name === "AbortError"
    return {
      ok: false,
      elapsedMs,
      firstByteMs: 0,
      statusCode: null,
      errorType: timeout ? "timeout" : "network_error",
      detail: String(error?.message || error),
    }
  } finally {
    clearTimeout(timer)
  }
}

function applyResult(collector, result, includeFirstByte) {
  if (result.statusCode != null) {
    recordStatusCode(collector, result.statusCode)
  }
  if (result.ok) {
    collector.success += 1
    collector.latenciesMs.push(result.elapsedMs)
    if (includeFirstByte) {
      collector.firstByteLatenciesMs.push(result.firstByteMs || 0)
    }
    return
  }

  collector.failed += 1
  if (result.errorType === "timeout") {
    collector.timeouts += 1
  }
  recordErrorType(collector, result.errorType || "unknown")
}

function toScenarioSummary({ name, mode, concurrency, collector, startedAt, finishedAt, extra }) {
  const durationSec = (finishedAt - startedAt) / 1000
  const total = collector.success + collector.failed
  const throughputRps = durationSec > 0 ? total / durationSec : 0
  const successRps = durationSec > 0 ? collector.success / durationSec : 0
  return {
    name,
    mode,
    concurrency,
    startedAt: new Date(startedAt).toISOString(),
    finishedAt: new Date(finishedAt).toISOString(),
    durationSec: Number(durationSec.toFixed(2)),
    requestsStarted: collector.started,
    requestsCompleted: total,
    success: collector.success,
    failed: collector.failed,
    successRate: total > 0 ? Number(((collector.success / total) * 100).toFixed(2)) : 0,
    timeoutCount: collector.timeouts,
    throughputRps: Number(throughputRps.toFixed(2)),
    successRps: Number(successRps.toFixed(2)),
    latencyMs: toLatencySummary(collector.latenciesMs),
    firstByteLatencyMs: collector.firstByteLatenciesMs.length
      ? toLatencySummary(collector.firstByteLatenciesMs)
      : null,
    statusCodeCounts: collector.statusCodeCounts,
    errorTypeCounts: collector.errorTypeCounts,
    ...extra,
  }
}

async function runWarmupPhase({ warmupSec, concurrency, execute }) {
  if (!Number.isFinite(warmupSec) || warmupSec <= 0) {
    return
  }
  const deadline = Date.now() + warmupSec * 1000
  const workers = Array.from({ length: Math.max(1, concurrency) }, async () => {
    while (Date.now() < deadline) {
      await execute()
    }
  })
  await Promise.all(workers)
}

async function runNonStreamMatrix(config, common) {
  const section = config.nonStream || {}
  if (section.enabled === false) return []
  const list = Array.isArray(section.concurrency) ? section.concurrency : [1, 8, 32, 128]
  const mode = section.mode === "duration" ? "duration" : "fixed_requests"
  const totalRequests = Number(section.totalRequests || 400)
  const durationSec = Number(section.durationSec || 20)
  const warmupSec = Number(section.warmupSec || config.warmupSec || 0)
  const url = resolveUrl(config.baseUrl, section.path, config.path)
  const timeoutMs = Number(section.timeoutMs || common.requestTimeoutMs || 65000)
  const expectedStatuses = section.expectStatus || common.expectStatus || [200]
  const body = section.body || {}

  const results = []
  for (const concurrency of list) {
    const collector = createCollector()
    const execute = () =>
      executeNonStreamRequest({
        url,
        method: common.method,
        headers: common.headers,
        body,
        timeoutMs,
        expectedStatuses,
      })

    await runWarmupPhase({
      warmupSec,
      concurrency,
      execute,
    })

    const startedAt = Date.now()
    if (mode === "duration") {
      const deadline = startedAt + durationSec * 1000
      const workers = Array.from({ length: concurrency }, async () => {
        while (Date.now() < deadline) {
          collector.started += 1
          const result = await execute()
          applyResult(collector, result, false)
        }
      })
      await Promise.all(workers)
    } else {
      let seq = 0
      const workers = Array.from({ length: concurrency }, async () => {
        while (true) {
          const current = seq
          seq += 1
          if (current >= totalRequests) break
          collector.started += 1
          const result = await execute()
          applyResult(collector, result, false)
        }
      })
      await Promise.all(workers)
    }

    const finishedAt = Date.now()
    results.push(
      toScenarioSummary({
        name: `nonstream-c${concurrency}`,
        mode: "nonstream",
        concurrency,
        collector,
        startedAt,
        finishedAt,
        extra: {
          url,
          runMode: mode,
          totalRequests: mode === "fixed_requests" ? totalRequests : null,
          plannedDurationSec: mode === "duration" ? durationSec : null,
          warmupSec,
        },
      })
    )
  }
  return results
}

async function runStreamMatrix(config, common) {
  const section = config.stream || {}
  if (section.enabled === false) return []
  const list = Array.isArray(section.concurrency) ? section.concurrency : [10, 50, 100]
  const durationSec = Number(section.durationSec || 30)
  const url = resolveUrl(config.baseUrl, section.path, config.path)
  const timeoutMs = Number(section.timeoutMs || common.streamTimeoutMs || 600000)
  const expectedStatuses = section.expectStatus || common.expectStatus || [200]
  const body = section.body || { stream: true }

  const results = []
  for (const concurrency of list) {
    const collector = createCollector()
    const startedAt = Date.now()
    const deadline = startedAt + durationSec * 1000
    const workers = Array.from({ length: concurrency }, async () => {
      while (Date.now() < deadline) {
        collector.started += 1
        const result = await executeStreamRequest({
          url,
          method: common.method,
          headers: common.headers,
          body,
          timeoutMs,
          expectedStatuses,
        })
        applyResult(collector, result, true)
      }
    })
    await Promise.all(workers)
    const finishedAt = Date.now()
    results.push(
      toScenarioSummary({
        name: `stream-c${concurrency}`,
        mode: "stream",
        concurrency,
        collector,
        startedAt,
        finishedAt,
        extra: {
          url,
          plannedDurationSec: durationSec,
        },
      })
    )
  }
  return results
}

async function runMixedScenario(config, common) {
  const section = config.mixed || {}
  if (section.enabled === false) return null

  const durationSec = Number(section.durationSec || 30)
  const deadline = Date.now() + durationSec * 1000
  const nonStreamConcurrency = Number(section.nonStreamConcurrency || 32)
  const streamConcurrency = Number(section.streamConcurrency || 20)
  const nonStreamUrl = resolveUrl(
    config.baseUrl,
    section.nonStreamPath,
    config.nonStream?.path || config.path
  )
  const streamUrl = resolveUrl(
    config.baseUrl,
    section.streamPath,
    config.stream?.path || config.path
  )
  const nonStreamBody = section.nonStreamBody || config.nonStream?.body || {}
  const streamBody = section.streamBody || config.stream?.body || { stream: true }
  const expectedStatuses = section.expectStatus || common.expectStatus || [200]
  const nonStreamTimeoutMs = Number(section.nonStreamTimeoutMs || common.requestTimeoutMs || 65000)
  const streamTimeoutMs = Number(section.streamTimeoutMs || common.streamTimeoutMs || 600000)

  const nonStreamCollector = createCollector()
  const streamCollector = createCollector()
  const startedAt = Date.now()

  const nonStreamWorkers = Array.from({ length: nonStreamConcurrency }, async () => {
    while (Date.now() < deadline) {
      nonStreamCollector.started += 1
      const result = await executeNonStreamRequest({
        url: nonStreamUrl,
        method: common.method,
        headers: common.headers,
        body: nonStreamBody,
        timeoutMs: nonStreamTimeoutMs,
        expectedStatuses,
      })
      applyResult(nonStreamCollector, result, false)
    }
  })

  const streamWorkers = Array.from({ length: streamConcurrency }, async () => {
    while (Date.now() < deadline) {
      streamCollector.started += 1
      const result = await executeStreamRequest({
        url: streamUrl,
        method: common.method,
        headers: common.headers,
        body: streamBody,
        timeoutMs: streamTimeoutMs,
        expectedStatuses,
      })
      applyResult(streamCollector, result, true)
    }
  })

  await Promise.all([...nonStreamWorkers, ...streamWorkers])
  const finishedAt = Date.now()

  return {
    name: "mixed",
    mode: "mixed",
    startedAt: new Date(startedAt).toISOString(),
    finishedAt: new Date(finishedAt).toISOString(),
    durationSec: Number(((finishedAt - startedAt) / 1000).toFixed(2)),
    plannedDurationSec: durationSec,
    nonStreamConcurrency,
    streamConcurrency,
    nonStream: toScenarioSummary({
      name: "mixed-nonstream",
      mode: "mixed-nonstream",
      concurrency: nonStreamConcurrency,
      collector: nonStreamCollector,
      startedAt,
      finishedAt,
      extra: { url: nonStreamUrl },
    }),
    stream: toScenarioSummary({
      name: "mixed-stream",
      mode: "mixed-stream",
      concurrency: streamConcurrency,
      collector: streamCollector,
      startedAt,
      finishedAt,
      extra: { url: streamUrl },
    }),
  }
}

function printScenarioResult(result) {
  if (!result) return

  if (result.mode === "mixed") {
    console.log(`\n[${result.name}] duration=${result.durationSec}s`)
    console.table([
      {
        lane: "nonstream",
        concurrency: result.nonStreamConcurrency,
        rps: result.nonStream.throughputRps,
        successRatePct: result.nonStream.successRate,
        p95Ms: result.nonStream.latencyMs.p95Ms,
        p99Ms: result.nonStream.latencyMs.p99Ms,
        timeoutCount: result.nonStream.timeoutCount,
        failed: result.nonStream.failed,
      },
      {
        lane: "stream",
        concurrency: result.streamConcurrency,
        rps: result.stream.throughputRps,
        successRatePct: result.stream.successRate,
        p95Ms: result.stream.latencyMs.p95Ms,
        p99Ms: result.stream.latencyMs.p99Ms,
        timeoutCount: result.stream.timeoutCount,
        failed: result.stream.failed,
      },
    ])
    return
  }

  console.log(`\n[${result.name}]`)
  console.table([
    {
      mode: result.mode,
      concurrency: result.concurrency,
      durationSec: result.durationSec,
      requestsCompleted: result.requestsCompleted,
      rps: result.throughputRps,
      successRatePct: result.successRate,
      p50Ms: result.latencyMs.p50Ms,
      p95Ms: result.latencyMs.p95Ms,
      p99Ms: result.latencyMs.p99Ms,
      timeoutCount: result.timeoutCount,
      failed: result.failed,
    },
  ])
}

function printSummary(report) {
  console.log("\n=== Benchmark Summary ===")
  console.log(`startedAt: ${report.startedAt}`)
  console.log(`finishedAt: ${report.finishedAt}`)
  console.log(`scenario: ${report.scenario}`)

  for (const item of report.nonStreamResults) {
    printScenarioResult(item)
  }
  for (const item of report.streamResults) {
    printScenarioResult(item)
  }
  if (report.mixedResult) {
    printScenarioResult(report.mixedResult)
  }
}

async function main() {
  const args = parseArgs(process.argv.slice(2))
  const config = loadConfig(args.config)
  const common = {
    method: (config.method || "POST").toUpperCase(),
    headers: {
      "content-type": "application/json",
      ...(config.headers || {}),
    },
    expectStatus: config.expectStatus || [200],
    requestTimeoutMs: Number(config.requestTimeoutMs || 65000),
    streamTimeoutMs: Number(config.streamTimeoutMs || 600000),
  }

  const report = {
    startedAt: nowIso(),
    finishedAt: null,
    scenario: args.scenario,
    configPath: args.config,
    nonStreamResults: [],
    streamResults: [],
    mixedResult: null,
  }

  if (args.scenario === "all" || args.scenario === "nonstream") {
    report.nonStreamResults = await runNonStreamMatrix(config, common)
  }
  if (args.scenario === "all" || args.scenario === "stream") {
    report.streamResults = await runStreamMatrix(config, common)
  }
  if (args.scenario === "all" || args.scenario === "mixed") {
    report.mixedResult = await runMixedScenario(config, common)
  }

  report.finishedAt = nowIso()
  printSummary(report)

  if (args.output) {
    fs.mkdirSync(path.dirname(args.output), { recursive: true })
    fs.writeFileSync(args.output, JSON.stringify(report, null, 2))
    console.log(`\nreport written: ${args.output}`)
  }
}

main().catch(error => {
  console.error("benchmark failed:", error)
  process.exit(1)
})
