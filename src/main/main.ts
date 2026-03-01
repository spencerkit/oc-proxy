// @ts-nocheck
const path = require("node:path")
const fs = require("node:fs")
const {
  app,
  BrowserWindow,
  ipcMain,
  Menu,
  dialog,
  clipboard,
  Tray,
  nativeImage,
} = require("electron")
const { ProxyRuntimeClient } = require("./proxyRuntimeClient")

if (!require.extensions[".ts"]) {
  require.extensions[".ts"] = require.extensions[".js"]
}

let mainWindow = null
let configStore = null
let proxyRuntime = null
let tray = null
let isQuitting = false
const SHUTDOWN_TIMEOUT_MS = 2500
let packageMetadata = {}

// 检查是否为开发模式
// 开发模式：从 out 目录运行，但源代码在 src/main 目录
// 生产模式：从打包的应用运行
const srcDir = path.join(__dirname, "../../src")
const isDev = fs.existsSync(srcDir)

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
    const { ConfigStore } = loadModuleFromDir(firstDir, "configStore")
    const { createGroupsBackupPayload, extractGroupsFromImportPayload } = loadModuleFromDir(
      firstDir,
      "groupBackup"
    )
    return {
      ConfigStore,
      createGroupsBackupPayload,
      extractGroupsFromImportPayload,
    }
  } catch (_error) {
    const { ConfigStore } = loadModuleFromDir(secondDir, "configStore")
    const { createGroupsBackupPayload, extractGroupsFromImportPayload } = loadModuleFromDir(
      secondDir,
      "groupBackup"
    )
    return {
      ConfigStore,
      createGroupsBackupPayload,
      extractGroupsFromImportPayload,
    }
  }
}

const { ConfigStore, createGroupsBackupPayload, extractGroupsFromImportPayload } =
  loadProxyModules()

// 获取开发服务器URL
function getDevServerUrl() {
  // 尝试从环境变量获取
  if (process.env.VITE_DEV_SERVER_URL) {
    return process.env.VITE_DEV_SERVER_URL
  }
  // 尝试读取 Vite 启动时输出的端口
  // 简单处理：默认尝试多个常用端口
  const _possiblePorts = [5173, 5174, 5175, 5176, 5177, 5178, 5179, 5180]
  // 返回第一个端口，实际运行时如果失败可以尝试其他端口
  // 这里我们简化处理，直接用 5173，实际可能需要更智能的检测
  return "http://localhost:5173"
}

const devServerUrl = isDev ? getDevServerUrl() : null
console.log("[Main] isDev:", isDev, "devServerUrl:", devServerUrl)

try {
  const packageJsonPath = path.join(__dirname, "../../package.json")
  packageMetadata = JSON.parse(fs.readFileSync(packageJsonPath, "utf-8"))
} catch (error) {
  console.error("[Main] Failed to read package metadata:", error)
}

function getAppInfo() {
  const resolvedName =
    packageMetadata?.build?.productName ||
    packageMetadata?.productName ||
    app.getName() ||
    "AI Open Router"
  const resolvedVersion = app.getVersion() || packageMetadata?.version || "0.0.0"

  return {
    name: resolvedName,
    version: resolvedVersion,
  }
}

function resolveAppIconPath() {
  const isWin = process.platform === "win32"
  const candidates = isWin ? ["icon.ico", "icon.png", "icon.jpg"] : ["icon.png", "icon.jpg"]

  for (const fileName of candidates) {
    const srcPath = path.join(__dirname, "../../assets", fileName)
    const outPath = path.join(__dirname, "../assets", fileName)
    if (fs.existsSync(srcPath)) return srcPath
    if (fs.existsSync(outPath)) return outPath
  }

  return undefined
}

function createWindow() {
  const appIconPath = resolveAppIconPath()
  mainWindow = new BrowserWindow({
    width: 1240,
    height: 860,
    icon: appIconPath,
    maximizable: false,
    fullscreenable: false,
    webPreferences: {
      preload: path.join(__dirname, "../preload/preload.js"),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: false,
    },
  })

  Menu.setApplicationMenu(null)
  mainWindow.setMenuBarVisibility(false)

  // 开发模式下从 Vite 服务器加载，生产模式从文件加载
  if (isDev && devServerUrl) {
    console.log("[Main] Loading from dev server:", devServerUrl)
    mainWindow
      .loadURL(devServerUrl)
      .then(() => {
        console.log("[Main] Page loaded successfully")
      })
      .catch(err => {
        console.error("[Main] Failed to load page:", err)
      })
  } else {
    mainWindow.loadFile(path.join(__dirname, "../renderer/index.html"))
  }

  // 监听页面加载错误
  mainWindow.webContents.on("did-fail-load", (_event, errorCode, errorDescription) => {
    console.error("[Main] Page failed to load:", errorCode, errorDescription)
  })

  mainWindow.webContents.on("did-finish-load", () => {
    console.log("[Main] Page finished loading")
  })

  // 仅在开发环境打开开发者工具
  if (!app.isPackaged) {
    mainWindow.webContents.openDevTools()
  }

  mainWindow.on("close", event => {
    const closeToTrayEnabled = !!configStore?.get?.()?.ui?.closeToTray
    if (!isQuitting && closeToTrayEnabled && tray) {
      event.preventDefault()
      mainWindow.hide()
      if (process.platform === "darwin" && app.dock && typeof app.dock.hide === "function") {
        app.dock.hide()
      }
      refreshTrayMenu()
    }
  })

  mainWindow.on("show", () => {
    if (process.platform === "darwin" && app.dock && typeof app.dock.show === "function") {
      app.dock.show()
    }
    refreshTrayMenu()
  })

  mainWindow.on("hide", () => {
    refreshTrayMenu()
  })

  mainWindow.on("closed", () => {
    mainWindow = null
    refreshTrayMenu()
  })
}

function hasServerSettingChanged(prev, next) {
  return (
    prev.server.host !== next.server.host ||
    prev.server.port !== next.server.port ||
    prev.server.authEnabled !== next.server.authEnabled ||
    prev.server.localBearerToken !== next.server.localBearerToken
  )
}

function applyLaunchOnStartupSetting(config) {
  if (typeof app.setLoginItemSettings !== "function") return

  try {
    app.setLoginItemSettings({
      openAtLogin: !!config?.ui?.launchOnStartup,
    })
  } catch (error) {
    console.error("Failed to apply launch-on-startup setting:", error)
  }
}

function getBackupDefaultFileName() {
  const now = new Date()
  const iso = now.toISOString().replace(/[-:]/g, "").replace(/\..+$/, "")
  return `ai-open-router-groups-backup-${iso}.json`
}

function buildGroupsBackupContent() {
  const current = configStore.get()
  const backupPayload = createGroupsBackupPayload(current.groups)
  const jsonText = JSON.stringify(backupPayload, null, 2)
  return {
    current,
    backupPayload,
    jsonText,
  }
}

async function syncRuntimeConfig(prevConfig, nextConfig) {
  if (!proxyRuntime) {
    return {
      restarted: false,
      status: {
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
      },
    }
  }

  const statusBefore = await proxyRuntime.getStatus()
  await proxyRuntime.setConfig(nextConfig)

  let restarted = false
  if (statusBefore.running && hasServerSettingChanged(prevConfig, nextConfig)) {
    await proxyRuntime.stopServer()
    await proxyRuntime.startServer()
    restarted = true
  }

  const status = await proxyRuntime.getStatus()
  return {
    restarted,
    status,
  }
}

async function stopProxyServerWithTimeout(timeoutMs = SHUTDOWN_TIMEOUT_MS) {
  if (!proxyRuntime) {
    return
  }

  await Promise.race([
    proxyRuntime.stopServer().catch(error => {
      console.error("Failed to stop proxy server during shutdown:", error)
    }),
    new Promise(resolve => setTimeout(resolve, timeoutMs)),
  ])
}

async function shutdownRuntimeWithTimeout(timeoutMs = SHUTDOWN_TIMEOUT_MS) {
  if (!proxyRuntime) {
    return
  }

  await Promise.race([
    proxyRuntime.shutdown(timeoutMs).catch(error => {
      console.error("Failed to shutdown proxy runtime during app quit:", error)
    }),
    new Promise(resolve => setTimeout(resolve, timeoutMs)),
  ])
}

function resolveTrayIconPath() {
  const isWin = process.platform === "win32"
  const candidates = isWin
    ? ["icon.ico", "icon.png", "icon.jpg"]
    : ["icon.png", "icon.jpg", "icon.ico"]

  for (const fileName of candidates) {
    const srcPath = path.join(__dirname, "../../assets", fileName)
    const outPath = path.join(__dirname, "../assets", fileName)
    if (fs.existsSync(srcPath)) return srcPath
    if (fs.existsSync(outPath)) return outPath
  }

  return null
}

function showMainWindow() {
  if (!mainWindow) {
    createWindow()
  }

  if (mainWindow.isMinimized()) {
    mainWindow.restore()
  }
  mainWindow.show()
  mainWindow.focus()
  refreshTrayMenu()
}

function hideMainWindow() {
  if (mainWindow && !mainWindow.isDestroyed()) {
    mainWindow.hide()
    refreshTrayMenu()
  }
}

function requestAppQuit() {
  if (isQuitting) return
  isQuitting = true
  app.quit()
}

function refreshTrayMenu() {
  if (!tray) return

  const visible = !!(mainWindow && !mainWindow.isDestroyed() && mainWindow.isVisible())
  tray.setContextMenu(
    Menu.buildFromTemplate([
      {
        label: visible ? "Hide AI Open Router" : "Show AI Open Router",
        click: () => (visible ? hideMainWindow() : showMainWindow()),
      },
      { type: "separator" },
      {
        label: "Exit",
        click: () => requestAppQuit(),
      },
    ])
  )
}

function createTray() {
  if (tray) return

  const trayIconPath = resolveTrayIconPath()
  if (!trayIconPath) return

  const image = nativeImage.createFromPath(trayIconPath)
  if (image.isEmpty()) return

  tray = new Tray(image)
  tray.setToolTip("AI Open Router")
  tray.on("click", () => {
    const visible = !!(mainWindow && !mainWindow.isDestroyed() && mainWindow.isVisible())
    if (visible) {
      hideMainWindow()
    } else {
      showMainWindow()
    }
  })
  refreshTrayMenu()
}

function destroyTray() {
  if (!tray) return
  tray.destroy()
  tray = null
}

function syncTrayByConfig(config) {
  const closeToTrayEnabled = !!config?.ui?.closeToTray
  if (closeToTrayEnabled) {
    createTray()
    refreshTrayMenu()
  } else {
    destroyTray()
  }
}

async function importGroupsAndSave(parsedInput, meta = {}) {
  const importedGroups = extractGroupsFromImportPayload(parsedInput)
  const prevConfig = configStore.get()
  const nextConfig = {
    ...prevConfig,
    groups: importedGroups,
  }
  const saved = configStore.save(nextConfig)

  const { restarted, status } = await syncRuntimeConfig(prevConfig, saved)

  return {
    ok: true,
    canceled: false,
    importedGroupCount: importedGroups.length,
    config: saved,
    restarted,
    status,
    ...meta,
  }
}

function setupIpc() {
  ipcMain.handle("app:get-info", async () => {
    return getAppInfo()
  })

  ipcMain.handle("app:get-status", async () => {
    return proxyRuntime.getStatus()
  })

  ipcMain.handle("app:start-server", async () => {
    return proxyRuntime.startServer()
  })

  ipcMain.handle("app:stop-server", async () => {
    return proxyRuntime.stopServer()
  })

  ipcMain.handle("config:get", async () => {
    return configStore.get()
  })

  ipcMain.handle("config:save", async (_event, nextConfig) => {
    const prevConfig = configStore.get()
    const saved = configStore.save(nextConfig)
    applyLaunchOnStartupSetting(saved)
    syncTrayByConfig(saved)

    const { restarted, status } = await syncRuntimeConfig(prevConfig, saved)

    return {
      ok: true,
      config: saved,
      restarted,
      status,
    }
  })

  ipcMain.handle("config:export-groups", async () => {
    const { current, backupPayload } = buildGroupsBackupContent()
    const defaultPath = path.join(app.getPath("documents"), getBackupDefaultFileName())
    const saveResult = await dialog.showSaveDialog(mainWindow || undefined, {
      title: "Export Group Rules Backup",
      defaultPath,
      filters: [{ name: "JSON", extensions: ["json"] }],
    })

    if (saveResult.canceled || !saveResult.filePath) {
      return { ok: true, canceled: true, filePath: null, groupCount: current.groups.length }
    }

    fs.writeFileSync(saveResult.filePath, JSON.stringify(backupPayload, null, 2), "utf-8")
    return {
      ok: true,
      canceled: false,
      filePath: saveResult.filePath,
      groupCount: current.groups.length,
      source: "file",
    }
  })

  ipcMain.handle("config:export-groups-folder", async () => {
    const { current, jsonText } = buildGroupsBackupContent()
    const openResult = await dialog.showOpenDialog(mainWindow || undefined, {
      title: "Choose Backup Folder",
      properties: ["openDirectory", "createDirectory"],
    })

    if (openResult.canceled || openResult.filePaths.length === 0) {
      return {
        ok: true,
        canceled: true,
        filePath: null,
        groupCount: current.groups.length,
        source: "folder",
      }
    }

    const folderPath = openResult.filePaths[0]
    const backupPath = path.join(folderPath, getBackupDefaultFileName())
    fs.writeFileSync(backupPath, jsonText, "utf-8")

    return {
      ok: true,
      canceled: false,
      filePath: backupPath,
      groupCount: current.groups.length,
      source: "folder",
    }
  })

  ipcMain.handle("config:export-groups-clipboard", async () => {
    const { current, jsonText } = buildGroupsBackupContent()
    clipboard.writeText(jsonText)
    return {
      ok: true,
      canceled: false,
      groupCount: current.groups.length,
      charCount: jsonText.length,
      source: "clipboard",
    }
  })

  ipcMain.handle("config:import-groups", async () => {
    const openResult = await dialog.showOpenDialog(mainWindow || undefined, {
      title: "Import Group Rules Backup",
      properties: ["openFile"],
      filters: [{ name: "JSON", extensions: ["json"] }],
    })

    if (openResult.canceled || openResult.filePaths.length === 0) {
      return { ok: true, canceled: true }
    }

    const importPath = openResult.filePaths[0]
    const raw = fs.readFileSync(importPath, "utf-8")

    let parsed: unknown
    try {
      parsed = JSON.parse(raw)
    } catch (_error) {
      const err = new Error("Invalid JSON file")
      err.statusCode = 400
      throw err
    }

    return importGroupsAndSave(parsed, {
      filePath: importPath,
      source: "file",
    })
  })

  ipcMain.handle("config:import-groups-json", async (_event, jsonText) => {
    if (typeof jsonText !== "string" || jsonText.trim().length === 0) {
      const err = new Error("Invalid JSON text")
      err.statusCode = 400
      throw err
    }

    let parsed: unknown
    try {
      parsed = JSON.parse(jsonText)
    } catch (_error) {
      const err = new Error("Invalid JSON text")
      err.statusCode = 400
      throw err
    }

    return importGroupsAndSave(parsed, {
      source: "json",
    })
  })

  ipcMain.handle("app:read-clipboard-text", async () => {
    return {
      text: clipboard.readText() || "",
    }
  })

  ipcMain.handle("logs:list", async (_event, max) => {
    return proxyRuntime.listLogs(max || 100)
  })

  ipcMain.handle("logs:clear", async () => {
    return proxyRuntime.clearLogs()
  })
}

app.whenReady().then(async () => {
  const configPath = path.join(app.getPath("userData"), "config.json")
  configStore = new ConfigStore(configPath)

  configStore.initialize()
  applyLaunchOnStartupSetting(configStore.get())
  proxyRuntime = new ProxyRuntimeClient({
    logLimit: 100,
  })
  try {
    await proxyRuntime.initialize(configStore.get())
  } catch (err) {
    console.error("Failed to initialize proxy runtime:", err)
  }

  setupIpc()
  try {
    await proxyRuntime.startServer()
  } catch (err) {
    console.error("Failed to auto-start proxy service:", err)
  }
  createWindow()
  syncTrayByConfig(configStore.get())
  configStore.on("updated", nextConfig => {
    syncTrayByConfig(nextConfig)
  })

  app.on("activate", () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      createWindow()
    }
  })
})

app.on("window-all-closed", async () => {
  if (process.platform !== "darwin") {
    await stopProxyServerWithTimeout()
    await shutdownRuntimeWithTimeout()
    const forceExitTimer = setTimeout(() => {
      app.exit(0)
    }, SHUTDOWN_TIMEOUT_MS)

    app.once("will-quit", () => {
      clearTimeout(forceExitTimer)
    })

    app.quit()
  }
})

app.on("before-quit", async () => {
  isQuitting = true
  destroyTray()
  await shutdownRuntimeWithTimeout()
})
