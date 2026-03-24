#!/usr/bin/env node
/* eslint-disable no-console */

const fs = require("node:fs")
const path = require("node:path")
const net = require("node:net")
const http = require("node:http")
const os = require("node:os")
const { spawn } = require("node:child_process")
const { chromium } = require("playwright")

function resolveBinaryPath() {
  const candidates = [
    process.env.AOR_HEADLESS_BIN,
    path.join("dist", "target", "release", "ai-open-router"),
    path.join("src-tauri", "target", "release", "ai-open-router"),
  ].filter(Boolean)

  for (const candidate of candidates) {
    if (fs.existsSync(candidate)) {
      return candidate
    }
  }

  throw new Error(
    "headless binary not found. Build it with `cargo build --release --bin ai-open-router --manifest-path src-tauri/Cargo.toml`"
  )
}

function getAvailablePort() {
  return new Promise((resolve, reject) => {
    const server = net.createServer()
    server.listen(0, "127.0.0.1", () => {
      const address = server.address()
      const port = typeof address === "object" && address ? address.port : 0
      server.close(() => resolve(port))
    })
    server.on("error", reject)
  })
}

async function waitForOk(url, timeoutMs = 20000) {
  const started = Date.now()
  while (Date.now() - started < timeoutMs) {
    try {
      const response = await fetch(url)
      if (response.ok) {
        return
      }
    } catch {
      // ignore
    }
    await new Promise(resolve => setTimeout(resolve, 300))
  }
  throw new Error(`endpoint did not become healthy in time: ${url}`)
}

async function requestJson(method, url, body) {
  const response = await fetch(url, {
    method,
    headers: body ? { "content-type": "application/json" } : undefined,
    body: body ? JSON.stringify(body) : undefined,
  })
  const text = await response.text()
  if (!response.ok) {
    throw new Error(text || response.statusText || `HTTP ${response.status}`)
  }
  return text.trim() ? JSON.parse(text) : null
}

function startForwardProxy({ listenPort, upstreamBaseUrl, forwardedFor }) {
  const upstream = new URL(upstreamBaseUrl)

  return new Promise((resolve, reject) => {
    const server = http.createServer((req, res) => {
      const headers = { ...req.headers }
      delete headers.connection
      headers.host = upstream.host
      headers["x-forwarded-for"] = forwardedFor
      headers["x-forwarded-host"] = req.headers.host || upstream.host
      headers["x-forwarded-proto"] = "http"
      headers.forwarded = `for=${forwardedFor};host=${req.headers.host || upstream.host};proto=http`

      const upstreamReq = http.request(
        {
          protocol: upstream.protocol,
          hostname: upstream.hostname,
          port: upstream.port,
          method: req.method,
          path: req.url,
          headers,
        },
        upstreamRes => {
          res.writeHead(upstreamRes.statusCode || 502, upstreamRes.headers)
          upstreamRes.pipe(res)
        }
      )

      upstreamReq.on("error", error => {
        if (!res.headersSent) {
          res.writeHead(502, { "content-type": "text/plain; charset=utf-8" })
        }
        res.end(String(error?.message || error))
      })

      req.pipe(upstreamReq)
    })

    server.once("error", reject)
    server.listen(listenPort, "127.0.0.1", () => resolve(server))
  })
}

async function waitForAny(page, selectors, timeout = 30000) {
  const start = Date.now()
  while (Date.now() - start < timeout) {
    for (const selector of selectors) {
      const locator = page.locator(selector)
      if (
        await locator
          .first()
          .isVisible()
          .catch(() => false)
      ) {
        return selector
      }
    }
    await page.waitForTimeout(200)
  }
  throw new Error("timeout waiting for ready state")
}

async function safeClick(page, selector) {
  const locator = page.locator(selector).first()
  await locator.waitFor({ timeout: 15000 })
  await locator.click({ timeout: 15000 })
}

async function waitForHidden(page, selector, timeout = 10000) {
  await page.locator(selector).first().waitFor({ state: "hidden", timeout })
}

async function run() {
  const binaryPath = resolveBinaryPath()
  const appPort = await getAvailablePort()
  const mockPort = await getAvailablePort()
  const proxyPort = await getAvailablePort()
  const remoteHost = "remote-aor.test"
  const forwardedFor = "203.0.113.10"
  const remotePassword = "remote-pass-123"
  const dataDir = fs.mkdtempSync(path.join(os.tmpdir(), "aor-e2e-"))
  const homeDir = path.join(dataDir, "home")
  const claudeDir = path.join(homeDir, ".claude")
  const codexDir = path.join(homeDir, ".codex")
  const openclawDir = path.join(homeDir, ".openclaw")
  const opencodeDir = path.join(homeDir, ".config", "opencode")
  const configPath = path.join(dataDir, "config.json")
  const baseUrl = `http://127.0.0.1:${appPort}`
  const remoteBaseUrl = `http://${remoteHost}:${proxyPort}`
  const mockBaseUrl = `http://127.0.0.1:${mockPort}`
  const screenshotDir = path.join(dataDir, "screenshots")
  const providerWebsite = "docs.example.test"

  fs.mkdirSync(claudeDir, { recursive: true })
  fs.mkdirSync(codexDir, { recursive: true })
  fs.mkdirSync(openclawDir, { recursive: true })
  fs.mkdirSync(opencodeDir, { recursive: true })
  fs.mkdirSync(screenshotDir, { recursive: true })

  fs.writeFileSync(
    path.join(codexDir, "config.toml"),
    'model_provider = "aor_shared"\n\n[model_providers.aor_shared]\nbase_url = "http://example"\n'
  )
  fs.writeFileSync(
    path.join(opencodeDir, "opencode.json"),
    JSON.stringify(
      {
        provider: {
          aor_shared: {
            options: {
              baseURL: "http://example",
              apiKey: "keep-opencode-token",
            },
          },
        },
      },
      null,
      2
    )
  )
  fs.writeFileSync(
    configPath,
    JSON.stringify(
      {
        server: { host: "127.0.0.1", port: appPort },
        ui: { locale: "en-US", localeMode: "manual" },
      },
      null,
      2
    )
  )

  const mockChild = spawn(
    process.execPath,
    [path.join("scripts", "mock-upstream.js"), "--host", "127.0.0.1", "--port", String(mockPort)],
    {
      cwd: process.cwd(),
      env: process.env,
      stdio: "inherit",
    }
  )

  let appChild
  let proxyServer
  let browser
  let context
  let page
  let lastStep = "init"

  const takeShot = async label => {
    if (!page) return
    try {
      await page.screenshot({ path: path.join(screenshotDir, `${label}.png`) })
    } catch {
      // ignore
    }
  }

  try {
    await waitForOk(`${mockBaseUrl}/healthz`)

    appChild = spawn(binaryPath, [], {
      env: {
        ...process.env,
        HOME: homeDir,
        USERPROFILE: homeDir,
        AOR_APP_DATA_DIR: dataDir,
      },
      stdio: "inherit",
    })

    await waitForOk(`${baseUrl}/api/health`)
    proxyServer = await startForwardProxy({
      listenPort: proxyPort,
      upstreamBaseUrl: baseUrl,
      forwardedFor,
    })

    await requestJson("PUT", `${baseUrl}/api/config/remote-admin-password`, {
      password: remotePassword,
    })

    browser = await chromium.launch({
      headless: true,
      args: [`--host-resolver-rules=MAP ${remoteHost} 127.0.0.1,EXCLUDE localhost`],
    })
    context = await browser.newContext({
      acceptDownloads: true,
      permissions: ["clipboard-read", "clipboard-write"],
    })
    page = await context.newPage()

    page.on("console", msg => {
      if (msg.type() === "warning" || msg.type() === "error") {
        console.log(`[browser:${msg.type()}] ${msg.text()}`)
      }
    })
    page.on("pageerror", err => {
      console.log(`[browser:pageerror] ${err.message}`)
    })
    page.on("response", response => {
      if (response.status() >= 400) {
        console.log(`[browser:response] ${response.status()} ${response.url()}`)
      }
    })

    const selectors = {
      remoteGate: 'xpath=//h1[contains(., "Password required for this management address")]',
      remoteUnlockButton: 'xpath=//button[normalize-space()="Unlock Management"]',
      errorScreen: ".error-screen",
      firstRunTitle: 'xpath=//h2[contains(., "Start by creating your first group")]',
      groupInfoTitle: 'xpath=//h3[contains(., "Group Info")]',
      addGroupButton: 'xpath=//button[@aria-label="Add Group" or @title="Add Group"]',
      createFirstGroupButton: 'xpath=//button[contains(., "Create First Group")]',
      createModalButton: 'xpath=//button[normalize-space()="Create"]',
      providersNav: 'xpath=//button[.//span[normalize-space()="Providers"]]',
      serviceNav: 'xpath=//button[.//span[normalize-space()="Service"]]',
      settingsNav: 'xpath=//button[.//span[normalize-space()="Settings"]]',
      agentsNav: 'xpath=//button[.//span[normalize-space()="Agents"]]',
      addProviderButton: 'xpath=//button[normalize-space()="Add Provider"]',
      createProviderButton: 'xpath=//button[normalize-space()="Create Provider"]',
      associateProviderButton: 'xpath=//button[@title="Associate Provider"]',
      integrationWriteButton: 'xpath=//button[@aria-label="Write current group address to client"]',
      writeNowButton: 'xpath=//button[normalize-space()="Write Now"]',
      agentAddConfigButton: 'xpath=//button[normalize-space()="Add Configuration Directory"]',
      agentEditTitle: 'xpath=//h1[normalize-space()="Edit Configuration"]',
      settingsTitle: 'xpath=//h2[normalize-space()="Service Settings"]',
      stopButton: 'xpath=//button[normalize-space()="Stop"]',
      saveAgentButton: 'xpath=//button[normalize-space()="Save"]',
      providerAvailable: 'xpath=.//span[normalize-space()="Available"]',
      testAllButton: 'xpath=//button[@aria-label="Test All" or @aria-label="Testing All"]',
    }

    const groupId = "e2e"
    const groupName = "E2E Group"
    const providerName = "E2E Provider"
    const providerModel = "gpt-4o-mini"
    const remoteEntryUrl = `${remoteBaseUrl}/oc/${groupId}`
    const remoteEntryUrlV1 = `${remoteEntryUrl}/v1`

    lastStep = "remote-login-gate"
    await page.goto(`${remoteBaseUrl}/management`, { waitUntil: "domcontentloaded" })
    await page.locator(selectors.remoteGate).waitFor({ timeout: 15000 })
    await page.locator("#remote-management-password").fill(remotePassword)
    await safeClick(page, selectors.remoteUnlockButton)

    lastStep = "app-ready"
    await waitForAny(page, [
      selectors.errorScreen,
      selectors.firstRunTitle,
      selectors.groupInfoTitle,
    ])
    if (
      await page
        .locator(selectors.errorScreen)
        .isVisible()
        .catch(() => false)
    ) {
      const message = await page
        .locator(selectors.errorScreen)
        .innerText()
        .catch(() => "")
      throw new Error(`app bootstrap failed: ${message}`)
    }

    const groupPathSelector = `xpath=//span[normalize-space()="/${groupId}"]`
    const groupButtonSelector = `xpath=//button[.//span[normalize-space()="/${groupId}"]]`
    const providerNameSelector = `xpath=//span[normalize-space()="${providerName}"]`

    if (
      !(await page
        .locator(groupPathSelector)
        .isVisible()
        .catch(() => false))
    ) {
      lastStep = "create-group"
      if (
        await page
          .locator(selectors.createFirstGroupButton)
          .isVisible()
          .catch(() => false)
      ) {
        await safeClick(page, selectors.createFirstGroupButton)
      } else {
        await safeClick(page, selectors.addGroupButton)
      }
      await page.locator("#groupId").fill(groupId)
      await page.locator("#groupName").fill(groupName)
      await safeClick(page, selectors.createModalButton)
      await page.locator(groupPathSelector).waitFor({ timeout: 15000 })
    }

    lastStep = "select-group"
    await safeClick(page, groupButtonSelector)

    lastStep = "providers-nav"
    await safeClick(page, selectors.providersNav)
    await page.locator('xpath=//h2[normalize-space()="Providers"]').waitFor({ timeout: 15000 })

    if (
      !(await page
        .locator(providerNameSelector)
        .isVisible()
        .catch(() => false))
    ) {
      lastStep = "create-provider"
      await safeClick(page, selectors.addProviderButton)
      await page.locator("#name").fill(providerName)
      await page.locator("#defaultModel").fill(providerModel)
      await page.locator("#token").fill("sk-e2e")
      await page.locator("#apiAddress").fill(mockBaseUrl)
      await page.locator("#website").fill(providerWebsite)

      await safeClick(page, selectors.createProviderButton)
      await page.locator(providerNameSelector).waitFor({ timeout: 15000 })
    }

    lastStep = "service-nav"
    await safeClick(page, selectors.serviceNav)
    await page.locator(selectors.groupInfoTitle).waitFor({ timeout: 15000 })

    if (
      !(await page
        .locator(providerNameSelector)
        .isVisible()
        .catch(() => false))
    ) {
      lastStep = "associate-provider"
      await safeClick(page, selectors.associateProviderButton)
      await safeClick(page, `xpath=//label[.//span[normalize-space()="${providerName}"]]`)
      await safeClick(page, 'xpath=//button[normalize-space()="Associate Provider"]')
      await page.locator(providerNameSelector).waitFor({ timeout: 15000 })
    }

    lastStep = "verify-entry-url"
    await page
      .locator(`xpath=//code[contains(., "${remoteEntryUrl}")]`)
      .first()
      .waitFor({ timeout: 15000 })
    const serviceProviderCard = page.locator(
      `xpath=//div[contains(@class, "ruleList")]//span[normalize-space()="${providerName}"]/ancestor::li[1]`
    )
    const serviceProviderCardTextBefore = await serviceProviderCard.innerText()
    const serviceProviderWebsiteLink = serviceProviderCard.locator(`a[href*="${providerWebsite}"]`)
    if (
      !(await serviceProviderWebsiteLink
        .first()
        .isVisible()
        .catch(() => false))
    ) {
      throw new Error("service provider card missing website quick link")
    }
    if (serviceProviderCardTextBefore.includes(providerWebsite)) {
      throw new Error("service provider card should not render website text inline")
    }
    if (serviceProviderCardTextBefore.includes(mockBaseUrl)) {
      throw new Error("service provider card should not expose provider API address")
    }

    lastStep = "service-batch-test"
    await safeClick(page, selectors.testAllButton)
    await page.locator("text=Available").first().waitFor({ timeout: 15000 })
    const servicePageTextAfter = await page.locator("body").innerText()
    if (!/\b\d+(?:\.\d+)?\s(?:ms|s|min)\b/.test(servicePageTextAfter)) {
      throw new Error("service provider card missing latency after batch test")
    }

    lastStep = "integration-write"
    if (
      await page
        .locator(selectors.stopButton)
        .isVisible()
        .catch(() => false)
    ) {
      throw new Error("start/stop button should be hidden in headless mode")
    }
    await safeClick(page, selectors.integrationWriteButton)
    await page.locator(selectors.writeNowButton).waitFor({ timeout: 15000 })
    await safeClick(page, 'xpath=//label[.//span[contains(., ".claude")]]')
    await safeClick(page, 'xpath=//label[.//span[contains(., ".codex")]]')
    await safeClick(
      page,
      'xpath=//section[.//h4[normalize-space()="OpenClaw"]]//label[contains(@class, "integrationTargetLabel")]'
    )
    await safeClick(page, 'xpath=//label[.//span[contains(., ".config/opencode")]]')
    await safeClick(page, selectors.writeNowButton)
    await waitForHidden(page, selectors.writeNowButton)

    const claudeSettingsPath = path.join(claudeDir, "settings.json")
    const openclawConfigPath = path.join(openclawDir, "openclaw.json")
    const openclawModelsPath = path.join(openclawDir, "agents", "default", "agent", "models.json")
    const opencodeConfigPath = path.join(opencodeDir, "opencode.json")

    const claudeSettings = JSON.parse(fs.readFileSync(claudeSettingsPath, "utf-8"))
    const codexConfigText = fs.readFileSync(path.join(codexDir, "config.toml"), "utf-8")
    const openclawConfig = JSON.parse(fs.readFileSync(openclawConfigPath, "utf-8"))
    const openclawModels = JSON.parse(fs.readFileSync(openclawModelsPath, "utf-8"))
    const opencodeConfig = JSON.parse(fs.readFileSync(opencodeConfigPath, "utf-8"))

    if (claudeSettings?.env?.ANTHROPIC_BASE_URL !== remoteEntryUrl) {
      throw new Error("claude settings should be written with remote host entry URL")
    }
    if (!codexConfigText.includes(`base_url = "${remoteEntryUrl}"`)) {
      throw new Error("codex config should be written with remote host entry URL")
    }
    if (openclawConfig?.models?.providers?.aor_shared?.baseUrl !== remoteEntryUrlV1) {
      throw new Error("openclaw primary config missing remote /v1 entry URL")
    }
    if (openclawModels?.providers?.aor_shared?.baseUrl !== remoteEntryUrlV1) {
      throw new Error("openclaw registry config missing remote /v1 entry URL")
    }
    if (opencodeConfig?.provider?.aor_shared?.options?.baseURL !== remoteEntryUrl) {
      throw new Error("opencode config should be written with remote host entry URL")
    }
    if (opencodeConfig?.provider?.aor_shared?.options?.apiKey !== "keep-opencode-token") {
      throw new Error("opencode config should preserve existing apiKey")
    }

    lastStep = "provider-batch-test"
    await safeClick(page, selectors.providersNav)
    await page.locator('xpath=//h2[normalize-space()="Providers"]').waitFor({ timeout: 15000 })
    const catalogProviderCard = page.locator(
      `xpath=//div[contains(@class, "ruleList")]//span[normalize-space()="${providerName}"]/ancestor::li[1]`
    )
    const providerCardTextBefore = await catalogProviderCard.innerText()
    const catalogProviderWebsiteLink = catalogProviderCard.locator(`a[href*="${providerWebsite}"]`)
    if (
      !(await catalogProviderWebsiteLink
        .first()
        .isVisible()
        .catch(() => false))
    ) {
      throw new Error("provider catalog card missing website quick link")
    }
    if (providerCardTextBefore.includes(providerWebsite)) {
      throw new Error("provider catalog card should not render website text inline")
    }
    if (providerCardTextBefore.includes(mockBaseUrl)) {
      throw new Error("provider catalog card should not expose provider API address")
    }
    await safeClick(page, selectors.testAllButton)
    await page.locator("text=Available").first().waitFor({ timeout: 15000 })
    const providerPageTextAfter = await page.locator("body").innerText()
    if (!/\b\d+(?:\.\d+)?\s(?:ms|s|min)\b/.test(providerPageTextAfter)) {
      throw new Error("provider catalog card missing latency after batch test")
    }

    lastStep = "agents-edit"
    await safeClick(page, selectors.agentsNav)
    await page
      .locator('xpath=//h1[normalize-space()="Agent Management"]')
      .waitFor({ timeout: 15000 })
    const addConfigButton = page.locator(selectors.agentAddConfigButton).first()
    if (!(await addConfigButton.isVisible().catch(() => false))) {
      throw new Error("agent add-config button not found in headless mode")
    }
    if (!(await addConfigButton.isDisabled().catch(() => false))) {
      throw new Error("agent add-config button should remain disabled in headless mode")
    }

    const codexEditedUrl = `${remoteEntryUrl}/codex`
    const remoteAgentUpdate = await page.evaluate(
      async ({ targetId, url, model }) => {
        const response = await fetch("/api/integration/agent-config", {
          method: "PUT",
          credentials: "include",
          headers: {
            "content-type": "application/json",
          },
          body: JSON.stringify({
            targetId,
            config: {
              url,
              model,
            },
          }),
        })

        return {
          ok: response.ok,
          text: await response.text(),
        }
      },
      {
        targetId: "default:codex",
        url: codexEditedUrl,
        model: "gpt-5-remote",
      }
    )
    if (!remoteAgentUpdate.ok) {
      throw new Error(`remote agent config update failed: ${remoteAgentUpdate.text}`)
    }

    const updatedCodexConfigText = fs.readFileSync(path.join(codexDir, "config.toml"), "utf-8")
    if (!updatedCodexConfigText.includes(`base_url = "${codexEditedUrl}"`)) {
      throw new Error("codex config edit did not persist remote entry URL")
    }
    if (!updatedCodexConfigText.includes('model = "gpt-5-remote"')) {
      throw new Error("codex config edit did not persist model change")
    }

    lastStep = "settings-check"
    await safeClick(page, selectors.settingsNav)
    await page.locator(selectors.settingsTitle).waitFor({ timeout: 15000 })
    if ((await page.locator("#port").count()) > 0) {
      throw new Error("port setting should be hidden in headless mode")
    }

    await browser.close()
    browser = null
  } catch (error) {
    await takeShot(`error-${lastStep}`)
    throw error
  } finally {
    await takeShot(`final-${lastStep}`)
    if (proxyServer) {
      await new Promise(resolve => proxyServer.close(resolve)).catch(() => {})
    }
    if (context) {
      await context.close().catch(() => {})
    }
    if (browser) {
      await browser.close().catch(() => {})
    }
    if (appChild) {
      appChild.kill("SIGTERM")
    }
    mockChild.kill("SIGTERM")
  }
}

run().catch(error => {
  console.error(error.message || error)
  process.exit(1)
})
