#!/usr/bin/env node

const { spawn } = require("node:child_process")
const fs = require("node:fs")
const path = require("node:path")
const process = require("node:process")
const { chromium } = require("playwright")

const ROOT = process.cwd()
const OUTPUT_DIR = path.join(ROOT, "docs", "assets", "screenshots")
const HOST = "127.0.0.1"
const PORT = 4173
const BASE_URL = `http://${HOST}:${PORT}`

function parseArgs(argv) {
  const out = {
    outputDir: OUTPUT_DIR,
    baseUrl: BASE_URL,
    port: PORT,
    host: HOST,
    keepServer: false,
  }
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index]
    if (arg === "--output-dir" && argv[index + 1]) {
      out.outputDir = path.resolve(argv[index + 1])
      index += 1
      continue
    }
    if (arg === "--base-url" && argv[index + 1]) {
      out.baseUrl = argv[index + 1]
      index += 1
      continue
    }
    if (arg === "--host" && argv[index + 1]) {
      out.host = argv[index + 1]
      index += 1
      continue
    }
    if (arg === "--port" && argv[index + 1]) {
      out.port = Number(argv[index + 1]) || PORT
      index += 1
      continue
    }
    if (arg === "--keep-server") {
      out.keepServer = true
    }
  }
  return out
}

function isoHoursAgo(hours) {
  return new Date(Date.now() - hours * 60 * 60 * 1000).toISOString()
}

function createMockDataset() {
  const groups = [
    {
      id: "alpha",
      name: "Alpha Group",
      models: ["gpt-4o", "gpt-4o-mini", "claude-3-7-sonnet"],
      activeRuleId: "rule-openai-main",
      rules: [
        {
          id: "rule-openai-main",
          name: "OpenAI Main",
          protocol: "openai",
          token: "sk-live-main",
          apiAddress: "https://api.openai.com/v1",
          defaultModel: "gpt-4o",
          modelMappings: {},
          quota: {
            enabled: true,
            provider: "openai",
            endpoint: "https://quota.example.com/openai",
            method: "GET",
            useRuleToken: true,
            customToken: "",
            authHeader: "Authorization",
            authScheme: "Bearer",
            customHeaders: {},
            unitType: "tokens",
            lowThresholdPercent: 10,
            response: {
              remaining: "$.data.remaining",
            },
          },
        },
        {
          id: "rule-anthropic-fallback",
          name: "Anthropic Fallback",
          protocol: "anthropic",
          token: "ak-live-fallback",
          apiAddress: "https://api.anthropic.com",
          defaultModel: "claude-3-7-sonnet",
          modelMappings: {
            "gpt-4o*": "claude-3-7-sonnet",
          },
          quota: {
            enabled: true,
            provider: "anthropic",
            endpoint: "https://quota.example.com/anthropic",
            method: "GET",
            useRuleToken: true,
            customToken: "",
            authHeader: "Authorization",
            authScheme: "Bearer",
            customHeaders: {},
            unitType: "tokens",
            lowThresholdPercent: 15,
            response: {
              remaining: "$.data.remaining",
            },
          },
        },
      ],
    },
    {
      id: "research",
      name: "Research Group",
      models: ["gpt-4.1-mini", "claude-3-5-haiku"],
      activeRuleId: "rule-openai-r",
      rules: [
        {
          id: "rule-openai-r",
          name: "Research OpenAI",
          protocol: "openai_completion",
          token: "sk-live-research",
          apiAddress: "https://api.openai.com/v1",
          defaultModel: "gpt-4.1-mini",
          modelMappings: {},
          quota: {
            enabled: false,
            provider: "custom",
            endpoint: "",
            method: "GET",
            useRuleToken: true,
            customToken: "",
            authHeader: "Authorization",
            authScheme: "Bearer",
            customHeaders: {},
            unitType: "tokens",
            lowThresholdPercent: 20,
            response: {},
          },
        },
      ],
    },
  ]

  const hourly = Array.from({ length: 24 }, (_, index) => {
    const h = 23 - index
    const requests = h < 7 || h > 21 ? 0 : Math.round(60 + (Math.sin(index / 2) + 1.2) * 45)
    const inputTokens = requests * 160 + (index % 3) * 110
    const outputTokens = requests * 85 + (index % 4) * 70
    return {
      hour: isoHoursAgo(h),
      requests,
      errors: requests === 0 ? 0 : Math.round(requests * 0.04),
      inputTokens,
      outputTokens,
      cacheReadTokens: Math.round(inputTokens * 0.18),
      cacheWriteTokens: Math.round(outputTokens * 0.15),
    }
  })

  const totalRequests = hourly.reduce((sum, item) => sum + item.requests, 0)
  const totalErrors = hourly.reduce((sum, item) => sum + item.errors, 0)
  const totalInputTokens = hourly.reduce((sum, item) => sum + item.inputTokens, 0)
  const totalOutputTokens = hourly.reduce((sum, item) => sum + item.outputTokens, 0)
  const totalCacheRead = hourly.reduce((sum, item) => sum + item.cacheReadTokens, 0)
  const totalCacheWrite = hourly.reduce((sum, item) => sum + item.cacheWriteTokens, 0)
  const activeHours = hourly.filter(item => item.requests > 0).length
  const activeMinutes = Math.max(activeHours * 60, 1)

  const options = groups.flatMap(group =>
    group.rules.map(rule => ({
      key: `${group.id}::${rule.id}`,
      label: `${group.name}-${rule.name}`,
      groupId: group.id,
      ruleId: rule.id,
    }))
  )

  const stats = {
    dimension: "rule",
    hours: 24,
    ruleKey: null,
    ruleKeys: options.map(option => option.key),
    requests: totalRequests,
    errors: totalErrors,
    inputTokens: totalInputTokens,
    outputTokens: totalOutputTokens,
    cacheReadTokens: totalCacheRead,
    cacheWriteTokens: totalCacheWrite,
    rpm: Number((totalRequests / activeMinutes).toFixed(3)),
    inputTpm: Number((totalInputTokens / activeMinutes).toFixed(3)),
    outputTpm: Number((totalOutputTokens / activeMinutes).toFixed(3)),
    peakRpm: Number(Math.max(...hourly.map(item => item.requests / 60)).toFixed(3)),
    peakInputTpm: Number(Math.max(...hourly.map(item => item.inputTokens / 60)).toFixed(3)),
    peakOutputTpm: Number(Math.max(...hourly.map(item => item.outputTokens / 60)).toFixed(3)),
    comparison: {
      requestsDeltaPct: 12.5,
      errorsDeltaPct: -8.4,
      rpmDeltaPct: 11.2,
      inputTpmDeltaPct: 9.7,
      outputTpmDeltaPct: 13.4,
    },
    breakdowns: {
      errorsByStatus: [
        { key: "500", count: 18, ratio: 0.42 },
        { key: "429", count: 16, ratio: 0.37 },
        { key: "unknown", count: 9, ratio: 0.21 },
      ],
      requestsByProtocol: [
        { key: "openai", count: 810, ratio: 0.58 },
        { key: "anthropic", count: 580, ratio: 0.42 },
      ],
      tokensByProtocol: [
        { key: "openai", tokens: 368000, ratio: 0.56 },
        { key: "anthropic", tokens: 294000, ratio: 0.44 },
      ],
      requestsByRule: [
        {
          key: "alpha::rule-openai-main",
          label: "Alpha Group-OpenAI Main",
          count: 680,
          ratio: 0.49,
        },
        {
          key: "alpha::rule-anthropic-fallback",
          label: "Alpha Group-Anthropic Fallback",
          count: 460,
          ratio: 0.33,
        },
        {
          key: "research::rule-openai-r",
          label: "Research Group-Research OpenAI",
          count: 250,
          ratio: 0.18,
        },
      ],
      tokensByRule: [
        {
          key: "alpha::rule-openai-main",
          label: "Alpha Group-OpenAI Main",
          tokens: 302000,
          ratio: 0.46,
        },
        {
          key: "alpha::rule-anthropic-fallback",
          label: "Alpha Group-Anthropic Fallback",
          tokens: 240000,
          ratio: 0.36,
        },
        {
          key: "research::rule-openai-r",
          label: "Research Group-Research OpenAI",
          tokens: 120000,
          ratio: 0.18,
        },
      ],
    },
    hourly,
    options,
  }

  const logs = Array.from({ length: 16 }, (_, index) => {
    const statusList = ["ok", "ok", "ok", "error"]
    const status = statusList[index % statusList.length]
    return {
      timestamp: isoHoursAgo(index % 8),
      traceId: `trace-${String(index + 1).padStart(4, "0")}`,
      phase: "request_chain",
      status,
      method: "POST",
      requestPath: "/oc/alpha/responses",
      requestAddress: "http://127.0.0.1:8899/oc/alpha/responses",
      clientAddress: "127.0.0.1",
      groupPath: "alpha",
      groupName: "Alpha Group",
      ruleId: "rule-openai-main",
      direction: "oc",
      entryProtocol: "openai",
      downstreamProtocol: "openai",
      model: "gpt-4o",
      forwardedModel: "gpt-4o",
      forwardingAddress: "https://api.openai.com/v1/responses",
      requestHeaders: { "content-type": "application/json" },
      forwardRequestHeaders: { authorization: "Bearer ***" },
      upstreamResponseHeaders: { "content-type": "application/json" },
      responseHeaders: { "content-type": "application/json" },
      requestBody: { model: "gpt-4o", input: "hello" },
      forwardRequestBody: { model: "gpt-4o", input: "hello" },
      responseBody: { id: `resp-${index + 1}` },
      tokenUsage: {
        inputTokens: 420 + index * 6,
        outputTokens: 220 + index * 4,
        cacheReadTokens: 90 + index * 2,
        cacheWriteTokens: 35 + index,
      },
      httpStatus: status === "ok" ? 200 : 500,
      upstreamStatus: status === "ok" ? 200 : 500,
      durationMs: 560 + index * 23,
      error:
        status === "error"
          ? {
              message: "upstream timeout",
              code: "upstream_timeout",
            }
          : null,
    }
  })

  const config = {
    configVersion: 2,
    server: {
      host: "0.0.0.0",
      port: 8899,
      authEnabled: false,
      localBearerToken: "",
    },
    compat: {
      strictMode: false,
    },
    logging: {
      level: "info",
      captureBody: true,
      redactRules: ["authorization", "x-api-key"],
    },
    ui: {
      theme: "light",
      locale: "zh-CN",
      localeMode: "manual",
      launchOnStartup: false,
      closeToTray: true,
      quotaAutoRefreshMinutes: 5,
    },
    remoteGit: {
      enabled: false,
      repoUrl: "",
      token: "",
      branch: "main",
    },
    groups,
  }

  const status = {
    running: true,
    address: "http://127.0.0.1:8899",
    lanAddress: "http://192.168.31.77:8899",
    metrics: {
      requests: 1528,
      streamRequests: 324,
      errors: 43,
      avgLatencyMs: 612,
      inputTokens: totalInputTokens,
      outputTokens: totalOutputTokens,
      cacheReadTokens: totalCacheRead,
      cacheWriteTokens: totalCacheWrite,
      uptimeStartedAt: isoHoursAgo(18),
    },
  }

  const ruleCardStats = {
    alpha: [
      {
        groupId: "alpha",
        ruleId: "rule-openai-main",
        requests: 920,
        inputTokens: 236000,
        outputTokens: 126000,
        tokens: 362000,
        hourly: hourly.slice(-12).map(item => ({
          hour: item.hour,
          requests: Math.round(item.requests * 0.58),
          inputTokens: Math.round(item.inputTokens * 0.58),
          outputTokens: Math.round(item.outputTokens * 0.58),
          tokens: Math.round((item.inputTokens + item.outputTokens) * 0.58),
        })),
      },
      {
        groupId: "alpha",
        ruleId: "rule-anthropic-fallback",
        requests: 540,
        inputTokens: 152000,
        outputTokens: 92000,
        tokens: 244000,
        hourly: hourly.slice(-12).map(item => ({
          hour: item.hour,
          requests: Math.round(item.requests * 0.34),
          inputTokens: Math.round(item.inputTokens * 0.34),
          outputTokens: Math.round(item.outputTokens * 0.34),
          tokens: Math.round((item.inputTokens + item.outputTokens) * 0.34),
        })),
      },
    ],
    research: [
      {
        groupId: "research",
        ruleId: "rule-openai-r",
        requests: 260,
        inputTokens: 68000,
        outputTokens: 41000,
        tokens: 109000,
        hourly: hourly.slice(-12).map(item => ({
          hour: item.hour,
          requests: Math.round(item.requests * 0.18),
          inputTokens: Math.round(item.inputTokens * 0.18),
          outputTokens: Math.round(item.outputTokens * 0.18),
          tokens: Math.round((item.inputTokens + item.outputTokens) * 0.18),
        })),
      },
    ],
  }

  const quotasByGroup = {
    alpha: [
      {
        groupId: "alpha",
        ruleId: "rule-openai-main",
        provider: "openai",
        status: "ok",
        remaining: 182000,
        total: 300000,
        percent: 60.67,
        unit: "tokens",
        resetAt: isoHoursAgo(-6),
        fetchedAt: new Date().toISOString(),
        message: null,
      },
      {
        groupId: "alpha",
        ruleId: "rule-anthropic-fallback",
        provider: "anthropic",
        status: "low",
        remaining: 22000,
        total: 120000,
        percent: 18.33,
        unit: "tokens",
        resetAt: isoHoursAgo(-4),
        fetchedAt: new Date().toISOString(),
        message: null,
      },
    ],
    research: [
      {
        groupId: "research",
        ruleId: "rule-openai-r",
        provider: "custom",
        status: "unsupported",
        remaining: null,
        total: null,
        percent: null,
        unit: null,
        resetAt: null,
        fetchedAt: new Date().toISOString(),
        message: null,
      },
    ],
  }

  return {
    config,
    status,
    logs,
    stats,
    quotasByGroup,
    ruleCardStats,
  }
}

function createMockInitScript(_dataset) {
  return ({ state }) => {
    const clone = value => JSON.parse(JSON.stringify(value))
    const internalState = {
      config: clone(state.config),
      status: clone(state.status),
      logs: clone(state.logs),
      stats: clone(state.stats),
      quotasByGroup: clone(state.quotasByGroup),
      ruleCardStats: clone(state.ruleCardStats),
    }

    const handlers = {
      async app_get_info() {
        return { name: "AI Open Router", version: "0.2.4-beta" }
      },
      async app_get_status() {
        return clone(internalState.status)
      },
      async app_start_server() {
        internalState.status.running = true
        return clone(internalState.status)
      },
      async app_stop_server() {
        internalState.status.running = false
        return clone(internalState.status)
      },
      async app_read_clipboard_text() {
        return { text: "" }
      },
      async config_get() {
        return clone(internalState.config)
      },
      async config_save(args) {
        if (args?.nextConfig) {
          internalState.config = clone(args.nextConfig)
        }
        return {
          ok: true,
          config: clone(internalState.config),
          restarted: false,
          status: clone(internalState.status),
        }
      },
      async logs_list() {
        return clone(internalState.logs)
      },
      async logs_clear() {
        internalState.logs = []
        return { ok: true }
      },
      async logs_stats_summary() {
        return clone(internalState.stats)
      },
      async logs_stats_clear() {
        return { ok: true }
      },
      async logs_stats_rule_cards(args) {
        const groupId = args?.groupId || "alpha"
        return clone(internalState.ruleCardStats[groupId] || [])
      },
      async quota_get_group(args) {
        const groupId = args?.groupId || "alpha"
        return clone(internalState.quotasByGroup[groupId] || [])
      },
      async quota_get_rule(args) {
        const groupId = args?.groupId || "alpha"
        const ruleId = args?.ruleId || ""
        const groupQuotas = internalState.quotasByGroup[groupId] || []
        const found = groupQuotas.find(item => item.ruleId === ruleId)
        return clone(
          found || {
            groupId,
            ruleId,
            provider: "unknown",
            status: "unknown",
            remaining: null,
            total: null,
            percent: null,
            unit: null,
            resetAt: null,
            fetchedAt: new Date().toISOString(),
            message: null,
          }
        )
      },
      async config_export_groups() {
        return { ok: true, canceled: true, groupCount: internalState.config.groups.length }
      },
      async config_export_groups_folder() {
        return { ok: true, canceled: true, groupCount: internalState.config.groups.length }
      },
      async config_export_groups_clipboard() {
        return {
          ok: true,
          canceled: false,
          source: "clipboard",
          groupCount: internalState.config.groups.length,
          charCount: 256,
        }
      },
      async config_import_groups() {
        return { ok: true, canceled: true, source: "file" }
      },
      async config_import_groups_json() {
        return { ok: true, canceled: true, source: "json" }
      },
      async config_remote_rules_upload() {
        return {
          ok: true,
          changed: false,
          branch: "main",
          filePath: "groups-rules-backup.json",
          groupCount: internalState.config.groups.length,
          needsConfirmation: false,
        }
      },
      async config_remote_rules_pull() {
        return {
          ok: true,
          branch: "main",
          filePath: "groups-rules-backup.json",
          importedGroupCount: internalState.config.groups.length,
          config: clone(internalState.config),
          restarted: false,
          status: clone(internalState.status),
          needsConfirmation: false,
        }
      },
      async quota_test_draft() {
        return {
          ok: false,
          message: "mock mode: not implemented",
          rawResponse: null,
          snapshot: null,
        }
      },
    }

    const invoke = async (cmd, args) => {
      const handler = handlers[cmd]
      if (!handler) {
        console.warn(`[mock-tauri] unhandled invoke command: ${cmd}`, args || {})
        return {}
      }
      return handler(args || {})
    }

    window.__TAURI_INTERNALS__ = {
      invoke,
    }
  }
}

async function waitForServer(baseUrl, timeoutMs = 30000) {
  const start = Date.now()
  while (Date.now() - start < timeoutMs) {
    try {
      const response = await fetch(baseUrl, { method: "GET" })
      if (response.status >= 200 && response.status < 500) {
        return true
      }
    } catch {}
    await new Promise(resolve => setTimeout(resolve, 350))
  }
  return false
}

function startViteServer(host, port) {
  return spawn(
    "npm",
    ["run", "dev", "--", "--host", host, "--port", String(port), "--strictPort"],
    {
      cwd: ROOT,
      env: {
        ...process.env,
        FORCE_COLOR: "1",
      },
      stdio: "pipe",
    }
  )
}

async function captureScreenshots({ outputDir, baseUrl, host, port, keepServer }) {
  fs.mkdirSync(outputDir, { recursive: true })

  const startedByScript = baseUrl === BASE_URL
  const serverProcess = startedByScript ? startViteServer(host, port) : null

  if (serverProcess) {
    serverProcess.stdout.on("data", chunk => process.stdout.write(`[vite] ${chunk}`))
    serverProcess.stderr.on("data", chunk => process.stderr.write(`[vite] ${chunk}`))
  }

  const ready = await waitForServer(baseUrl)
  if (!ready) {
    if (serverProcess) serverProcess.kill("SIGTERM")
    throw new Error(`dev server did not become ready: ${baseUrl}`)
  }

  const browser = await chromium.launch({ headless: true })
  const context = await browser.newContext({
    viewport: { width: 1720, height: 1060 },
    deviceScaleFactor: 1.5,
  })

  const dataset = createMockDataset()
  const pages = [
    { route: "/", file: "service-page.png", readyText: "分组信息" },
    { route: "/settings", file: "settings-page.png", readyText: "服务设置" },
    { route: "/logs", file: "logs-page.png", readyText: "日志" },
  ]

  for (const item of pages) {
    const page = await context.newPage()
    page.on("console", msg => {
      if (msg.type() === "error") {
        console.error(`[browser:${item.file}]`, msg.text())
      }
    })
    await page.addInitScript(createMockInitScript(dataset), { state: dataset })
    await page.addInitScript(() => {
      const style = document.createElement("style")
      style.textContent = `
        *, *::before, *::after {
          animation-duration: 0s !important;
          animation-delay: 0s !important;
          transition-duration: 0s !important;
          transition-delay: 0s !important;
          caret-color: transparent !important;
        }
      `
      document.documentElement.appendChild(style)
    })
    await page.goto(`${baseUrl}${item.route}`, { waitUntil: "domcontentloaded" })
    await page.waitForLoadState("networkidle")
    await page.waitForSelector("body", { timeout: 15000 })
    await page.addStyleTag({
      content: `
        [class*="loading-screen"] { display: none !important; }
      `,
    })
    await page.getByText(item.readyText, { exact: false }).first().waitFor({ timeout: 15000 })
    await page.waitForTimeout(700)
    const target = path.join(outputDir, item.file)
    await page.screenshot({ path: target, fullPage: true })
    console.log(`[screenshots] saved ${target}`)
    await page.close()
  }

  await context.close()
  await browser.close()

  if (serverProcess && !keepServer) {
    serverProcess.kill("SIGTERM")
  }
}

async function main() {
  const args = parseArgs(process.argv.slice(2))
  await captureScreenshots(args)
}

main().catch(error => {
  console.error("[screenshots] failed:", error?.stack || error?.message || error)
  process.exitCode = 1
})
