#!/usr/bin/env node
/* eslint-disable no-console */

const fs = require("node:fs")
const os = require("node:os")
const path = require("node:path")
const net = require("node:net")
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
      const { port } = server.address()
      server.close(() => resolve(port))
    })
    server.on("error", reject)
  })
}

async function waitForHealth(baseUrl, timeoutMs = 20000) {
  const started = Date.now()
  while (Date.now() - started < timeoutMs) {
    try {
      const res = await fetch(`${baseUrl}/api/health`)
      if (res.ok) return
    } catch {
      // ignore
    }
    await new Promise(resolve => setTimeout(resolve, 300))
  }
  throw new Error("server did not become healthy in time")
}

async function waitForAny(page, selectors, timeout = 30000) {
  const start = Date.now()
  while (Date.now() - start < timeout) {
    for (const selector of selectors) {
      const el = page.locator(selector)
      if (
        await el
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
  await locator.waitFor({ timeout: 10000 })
  await locator.click({ timeout: 10000 })
}

async function waitForClipboardContains(page, expected, timeoutMs = 5000) {
  const start = Date.now()
  while (Date.now() - start < timeoutMs) {
    const text = await page.evaluate(() => navigator.clipboard.readText()).catch(() => "")
    if (text?.includes(expected)) {
      return text
    }
    await page.waitForTimeout(200)
  }
  throw new Error("clipboard did not update in time")
}

async function waitForHidden(page, selector, timeout = 10000) {
  await page.locator(selector).first().waitFor({ state: "hidden", timeout })
}

async function run() {
  const binaryPath = resolveBinaryPath()
  const port = await getAvailablePort()
  const dataDir = fs.mkdtempSync(path.join(os.tmpdir(), "aor-e2e-"))
  const homeDir = path.join(dataDir, "home")
  fs.mkdirSync(homeDir, { recursive: true })
  const claudeDir = path.join(homeDir, ".claude")
  const codexDir = path.join(homeDir, ".codex")
  const openclawDir = path.join(homeDir, ".openclaw")
  const opencodeDir = path.join(homeDir, ".config", "opencode")
  fs.mkdirSync(claudeDir, { recursive: true })
  fs.mkdirSync(codexDir, { recursive: true })
  fs.mkdirSync(openclawDir, { recursive: true })
  fs.mkdirSync(opencodeDir, { recursive: true })
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
  const configPath = path.join(dataDir, "config.json")
  fs.writeFileSync(
    configPath,
    JSON.stringify(
      {
        server: { host: "127.0.0.1", port },
        ui: { locale: "en-US", localeMode: "manual" },
      },
      null,
      2
    )
  )

  const child = spawn(binaryPath, [], {
    env: {
      ...process.env,
      HOME: homeDir,
      USERPROFILE: homeDir,
      AOR_APP_DATA_DIR: dataDir,
    },
    stdio: "inherit",
  })

  const baseUrl = `http://127.0.0.1:${port}`
  const browser = await chromium.launch({ headless: true })
  const context = await browser.newContext({
    acceptDownloads: true,
    permissions: ["clipboard-read", "clipboard-write"],
  })
  const page = await context.newPage()
  const screenshotDir = path.join(dataDir, "screenshots")
  fs.mkdirSync(screenshotDir, { recursive: true })
  let lastStep = "init"
  const takeShot = async label => {
    try {
      await page.screenshot({ path: path.join(screenshotDir, `${label}.png`) })
    } catch {
      // ignore
    }
  }
  page.on("console", msg => {
    console.log(`[browser:${msg.type()}] ${msg.text()}`)
  })
  page.on("pageerror", err => {
    console.log(`[browser:pageerror] ${err.message}`)
  })
  page.on("response", response => {
    if (response.status() >= 400) {
      console.log(`[browser:response] ${response.status()} ${response.url()}`)
    }
  })

  try {
    await waitForHealth(baseUrl)
    await page.goto(`${baseUrl}/management`, { waitUntil: "domcontentloaded" })

    const selectors = {
      errorScreen: ".error-screen",
      firstRunTitleEn: 'xpath=//h2[contains(., "Start by creating your first group")]',
      firstRunTitleZh: 'xpath=//h2[contains(., "开始创建你的第一个分组")]',
      groupInfoTitleEn: 'xpath=//h3[contains(., "Group Info")]',
      groupInfoTitleZh: 'xpath=//h3[contains(., "分组信息")]',
      groupInfoTitle: 'xpath=//h3[contains(., "Group Info")]',
      addGroupButton: 'xpath=//button[@aria-label="Add Group" or @title="Add Group"]',
      createFirstGroupButton: 'xpath=//button[contains(., "Create First Group")]',
      createModalButton: 'xpath=//button[normalize-space()="Create"]',
      providersNav: 'xpath=//button[.//span[normalize-space()="Providers"]]',
      serviceNav: 'xpath=//button[.//span[normalize-space()="Service"]]',
      logsNav: 'xpath=//button[.//span[normalize-space()="Logs"]]',
      settingsNav: 'xpath=//button[.//span[normalize-space()="Settings"]]',
      agentsNav: 'xpath=//button[.//span[normalize-space()="Agents"]]',
      addProviderButton: 'xpath=//button[normalize-space()="Add Provider"]',
      createProviderButton: 'xpath=//button[normalize-space()="Create Provider"]',
      associateProviderButton: 'xpath=//button[@title="Associate Provider"]',
      logsTitle: 'xpath=//h2[normalize-space()="Logs"]',
      startButton: 'xpath=//button[normalize-space()="Start"]',
      stopButton: 'xpath=//button[normalize-space()="Stop"]',
      settingsTitleEn: 'xpath=//h2[normalize-space()="Service Settings"]',
      settingsTitleZh: 'xpath=//h2[normalize-space()="服务设置"]',
      agentsTitle: 'xpath=//h1[normalize-space()="Agent Management"]',
      integrationWriteButton: 'xpath=//button[@aria-label="Write current group address to client"]',
      agentAddConfigButton: 'xpath=//button[normalize-space()="Add Configuration Directory"]',
      exportButton: 'xpath=//button[normalize-space()="Export JSON"]',
      exportFolderChoice: 'xpath=//button[.//span[normalize-space()="Export to Folder"]]',
      exportClipboardChoice: 'xpath=//button[.//span[normalize-space()="Copy to Clipboard"]]',
      exportConfirm: 'xpath=//button[normalize-space()="Confirm Export"]',
      importButton: 'xpath=//button[normalize-space()="Import JSON"]',
      importClipboardChoice:
        'xpath=//button[.//span[contains(normalize-space(), "Clipboard") or contains(normalize-space(), "剪贴板")]]',
      importFileChoice:
        'xpath=//button[.//span[contains(normalize-space(), "JSON File") or contains(normalize-space(), "JSON 文件")]]',
      importConfirm: 'xpath=//button[normalize-space()="Confirm Import"]',
    }

    const groupId = "e2e"
    const groupName = "E2E Group"
    const providerName = "E2E Provider"
    const providerModel = "gpt-4o-mini"

    lastStep = "ready"
    await waitForAny(page, [
      selectors.errorScreen,
      selectors.firstRunTitleEn,
      selectors.firstRunTitleZh,
      selectors.groupInfoTitleEn,
      selectors.groupInfoTitleZh,
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

    const enButton = page.locator('xpath=//button[normalize-space()="EN"]')
    if (await enButton.isVisible().catch(() => false)) {
      lastStep = "switch-language"
      await enButton.click()
    }

    const groupPathSelector = `xpath=//span[normalize-space()="/${groupId}"]`
    const groupButtonSelector = `xpath=//button[.//span[normalize-space()="/${groupId}"]]`

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

    const providerNameSelector = `xpath=//span[normalize-space()="${providerName}"]`
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
      await page.locator("#apiAddress").fill("https://api.openai.com/v1")

      const openaiButton = page.locator('xpath=//button[normalize-space()="OpenAI"]')
      if (await openaiButton.isVisible().catch(() => false)) {
        await openaiButton.click()
      }

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

    lastStep = "service-status"
    if (
      await page
        .locator(selectors.stopButton)
        .isVisible()
        .catch(() => false)
    ) {
      throw new Error("start/stop button should be hidden in headless mode")
    }

    lastStep = "integration-write-open"
    const writeButton = page.locator(selectors.integrationWriteButton).first()
    if (!(await writeButton.isVisible().catch(() => false))) {
      throw new Error("integration write button should be visible in headless mode")
    }
    await writeButton.click()
    await page.locator('xpath=//button[normalize-space()="Write Now"]').waitFor({ timeout: 15000 })
    await safeClick(page, 'xpath=//label[.//span[contains(., ".claude")]]')
    await safeClick(page, 'xpath=//label[.//span[contains(., ".codex")]]')
    await safeClick(
      page,
      'xpath=//section[.//h4[normalize-space()="OpenClaw"]]//label[contains(@class, "integrationTargetLabel")]'
    )
    await safeClick(page, 'xpath=//label[.//span[contains(., ".config/opencode")]]')
    await safeClick(page, 'xpath=//button[normalize-space()="Write Now"]')
    await waitForHidden(page, 'xpath=//button[normalize-space()="Write Now"]')

    const openclawConfigPath = path.join(openclawDir, "openclaw.json")
    const openclawModelsPath = path.join(openclawDir, "agents", "default", "agent", "models.json")
    const opencodeConfigPath = path.join(opencodeDir, "opencode.json")
    const openclawConfig = JSON.parse(fs.readFileSync(openclawConfigPath, "utf-8"))
    const openclawModels = JSON.parse(fs.readFileSync(openclawModelsPath, "utf-8"))
    const opencodeConfig = JSON.parse(fs.readFileSync(opencodeConfigPath, "utf-8"))
    const expectedOpenclawPath = `/oc/${groupId}/v1`
    const expectedOpencodePath = `/oc/${groupId}`
    const hasExpectedOpenclawBaseUrl = value => {
      try {
        const parsed = new URL(value)
        return parsed.port === String(port) && parsed.pathname === expectedOpenclawPath
      } catch {
        return false
      }
    }
    const hasExpectedOpencodeBaseUrl = value => {
      try {
        const parsed = new URL(value)
        return parsed.port === String(port) && parsed.pathname === expectedOpencodePath
      } catch {
        return false
      }
    }
    const writtenOpenclawBaseUrl = openclawConfig?.models?.providers?.aor_shared?.baseUrl
    const registryOpenclawBaseUrl = openclawModels?.providers?.aor_shared?.baseUrl
    const writtenOpencodeBaseUrl = opencodeConfig?.provider?.aor_shared?.options?.baseURL
    const writtenOpencodeApiKey = opencodeConfig?.provider?.aor_shared?.options?.apiKey
    if (
      !hasExpectedOpenclawBaseUrl(writtenOpenclawBaseUrl) ||
      !hasExpectedOpenclawBaseUrl(registryOpenclawBaseUrl) ||
      writtenOpenclawBaseUrl !== registryOpenclawBaseUrl
    ) {
      throw new Error("openclaw config write missing expected /v1 baseUrl")
    }
    if (
      !hasExpectedOpencodeBaseUrl(writtenOpencodeBaseUrl) ||
      writtenOpencodeApiKey !== "keep-opencode-token"
    ) {
      throw new Error("opencode config write missing expected baseURL or preserved apiKey")
    }

    lastStep = "agents-nav"
    await safeClick(page, selectors.agentsNav)
    await page.locator(selectors.agentsTitle).waitFor({ timeout: 15000 })
    const addConfigButton = page.locator(selectors.agentAddConfigButton).first()
    if (await addConfigButton.isVisible().catch(() => false)) {
      const disabled = await addConfigButton.isDisabled().catch(() => false)
      if (!disabled) {
        throw new Error("agent config modification should be disabled in headless mode")
      }
    } else {
      throw new Error("agent add-config button not found in headless mode")
    }

    lastStep = "logs-nav"
    await safeClick(page, selectors.logsNav)
    await page.locator(selectors.logsTitle).waitFor({ timeout: 15000 })

    lastStep = "settings-nav"
    await safeClick(page, selectors.settingsNav)
    await waitForAny(page, [selectors.settingsTitleEn, selectors.settingsTitleZh], 15000)
    if ((await page.locator("#port").count()) > 0) {
      throw new Error("port setting should be hidden in headless mode")
    }

    lastStep = "export-folder"
    await safeClick(page, selectors.exportButton)
    await safeClick(page, selectors.exportFolderChoice)
    const [download] = await Promise.all([
      page.waitForEvent("download"),
      safeClick(page, selectors.exportConfirm),
    ])
    let downloadPath = await download.path()
    if (!downloadPath) {
      downloadPath = path.join(dataDir, "backup.json")
      await download.saveAs(downloadPath)
    }
    const downloadText = fs.readFileSync(downloadPath, "utf-8")
    if (!downloadText.includes('"ai-open-router-groups-backup"')) {
      throw new Error("export download missing backup payload")
    }
    await waitForHidden(page, selectors.exportConfirm)

    lastStep = "export-clipboard"
    await safeClick(page, selectors.exportButton)
    await safeClick(page, selectors.exportClipboardChoice)
    await safeClick(page, selectors.exportConfirm)
    await waitForClipboardContains(page, "ai-open-router-groups-backup")
    await waitForHidden(page, selectors.exportConfirm)

    lastStep = "import-clipboard"
    await safeClick(page, selectors.importButton)
    await safeClick(page, selectors.importClipboardChoice)
    await page.locator("#import-json").fill(downloadText)
    await safeClick(page, selectors.importConfirm)
    await waitForHidden(page, selectors.importConfirm)

    lastStep = "import-file-cancel"
    await page.evaluate(() => {
      Object.defineProperty(window, "showOpenFilePicker", {
        configurable: true,
        writable: true,
        value: async () => {
          throw new DOMException("Aborted", "AbortError")
        },
      })
    })
    await safeClick(page, selectors.importButton)
    await safeClick(page, selectors.importFileChoice)
    await safeClick(page, selectors.importConfirm)
    await waitForHidden(page, selectors.importConfirm)

    await browser.close()
  } catch (error) {
    await takeShot(`error-${lastStep}`)
    throw error
  } finally {
    await takeShot(`final-${lastStep}`)
    child.kill("SIGTERM")
    await context.close().catch(() => {})
    await browser.close().catch(() => {})
  }
}

run().catch(error => {
  console.error(error.message || error)
  process.exit(1)
})
