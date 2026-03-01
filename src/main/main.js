const path = require("node:path");
const fs = require("node:fs");
const { app, BrowserWindow, ipcMain, Menu, dialog, clipboard } = require("electron");
const { LogStore } = require("./logStore");

let mainWindow = null;
let configStore = null;
let proxyServer = null;
let logStore = null;

// 检查是否为开发模式
// 开发模式：从 out 目录运行，但源代码在 src/main 目录
// 生产模式：从打包的应用运行
const srcDir = path.join(__dirname, "../../src");
const isDev = fs.existsSync(srcDir);

function loadProxyModules() {
  const srcProxyDir = path.join(__dirname, "../../src/proxy");
  const outProxyDir = path.join(__dirname, "../proxy");
  const preferSrc = isDev && fs.existsSync(srcProxyDir);

  const firstDir = preferSrc ? srcProxyDir : outProxyDir;
  const secondDir = preferSrc ? outProxyDir : srcProxyDir;

  try {
    const { ConfigStore } = require(path.join(firstDir, "configStore.js"));
    const { ProxyServer } = require(path.join(firstDir, "server.js"));
    const { createGroupsBackupPayload, extractGroupsFromImportPayload } = require(path.join(firstDir, "groupBackup.js"));
    return {
      ConfigStore,
      ProxyServer,
      createGroupsBackupPayload,
      extractGroupsFromImportPayload
    };
  } catch (error) {
    const { ConfigStore } = require(path.join(secondDir, "configStore.js"));
    const { ProxyServer } = require(path.join(secondDir, "server.js"));
    const { createGroupsBackupPayload, extractGroupsFromImportPayload } = require(path.join(secondDir, "groupBackup.js"));
    return {
      ConfigStore,
      ProxyServer,
      createGroupsBackupPayload,
      extractGroupsFromImportPayload
    };
  }
}

const {
  ConfigStore,
  ProxyServer,
  createGroupsBackupPayload,
  extractGroupsFromImportPayload
} = loadProxyModules();

// 获取开发服务器URL
function getDevServerUrl() {
  // 尝试从环境变量获取
  if (process.env.VITE_DEV_SERVER_URL) {
    return process.env.VITE_DEV_SERVER_URL;
  }
  // 尝试读取 Vite 启动时输出的端口
  // 简单处理：默认尝试多个常用端口
  const possiblePorts = [5173, 5174, 5175, 5176, 5177, 5178, 5179, 5180];
  // 返回第一个端口，实际运行时如果失败可以尝试其他端口
  // 这里我们简化处理，直接用 5173，实际可能需要更智能的检测
  return 'http://localhost:5173';
}

const devServerUrl = isDev ? getDevServerUrl() : null;
console.log('[Main] isDev:', isDev, 'devServerUrl:', devServerUrl);

function resolveAppIconPath() {
  const isWin = process.platform === "win32";
  const candidates = isWin
    ? ["icon.ico", "icon.png", "icon.jpg"]
    : ["icon.png", "icon.jpg"];

  for (const fileName of candidates) {
    const srcPath = path.join(__dirname, "../../assets", fileName);
    const outPath = path.join(__dirname, "../assets", fileName);
    if (fs.existsSync(srcPath)) return srcPath;
    if (fs.existsSync(outPath)) return outPath;
  }

  return undefined;
}

function createWindow() {
  const appIconPath = resolveAppIconPath();
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
      sandbox: false
    }
  });

  Menu.setApplicationMenu(null);
  mainWindow.setMenuBarVisibility(false);

  // 开发模式下从 Vite 服务器加载，生产模式从文件加载
  if (isDev && devServerUrl) {
    console.log('[Main] Loading from dev server:', devServerUrl);
    mainWindow.loadURL(devServerUrl).then(() => {
      console.log('[Main] Page loaded successfully');
    }).catch((err) => {
      console.error('[Main] Failed to load page:', err);
    });
  } else {
    mainWindow.loadFile(path.join(__dirname, "../renderer/index.html"));
  }

  // 监听页面加载错误
  mainWindow.webContents.on('did-fail-load', (event, errorCode, errorDescription) => {
    console.error('[Main] Page failed to load:', errorCode, errorDescription);
  });

  mainWindow.webContents.on('did-finish-load', () => {
    console.log('[Main] Page finished loading');
  });

  // 自动打开开发者工具
  mainWindow.webContents.openDevTools();
}

function hasServerSettingChanged(prev, next) {
  return prev.server.host !== next.server.host
    || prev.server.port !== next.server.port
    || prev.server.authEnabled !== next.server.authEnabled
    || prev.server.localBearerToken !== next.server.localBearerToken;
}

function applyLaunchOnStartupSetting(config) {
  if (typeof app.setLoginItemSettings !== "function") return;

  try {
    app.setLoginItemSettings({
      openAtLogin: !!config?.ui?.launchOnStartup
    });
  } catch (error) {
    console.error("Failed to apply launch-on-startup setting:", error);
  }
}

function getBackupDefaultFileName() {
  const now = new Date();
  const iso = now.toISOString().replace(/[-:]/g, "").replace(/\..+$/, "");
  return `oa-proxy-groups-backup-${iso}.json`;
}

function buildGroupsBackupContent() {
  const current = configStore.get();
  const backupPayload = createGroupsBackupPayload(current.groups);
  const jsonText = JSON.stringify(backupPayload, null, 2);
  return {
    current,
    backupPayload,
    jsonText
  };
}

async function importGroupsAndSave(parsedInput, meta = {}) {
  const importedGroups = extractGroupsFromImportPayload(parsedInput);
  const prevConfig = configStore.get();
  const nextConfig = {
    ...prevConfig,
    groups: importedGroups
  };
  const saved = configStore.save(nextConfig);

  let restarted = false;
  if (proxyServer.isRunning() && hasServerSettingChanged(prevConfig, saved)) {
    await proxyServer.stop();
    await proxyServer.start();
    restarted = true;
  }

  return {
    ok: true,
    canceled: false,
    importedGroupCount: importedGroups.length,
    config: saved,
    restarted,
    status: proxyServer.getStatus(),
    ...meta
  };
}

function setupIpc() {
  ipcMain.handle("app:get-status", async () => {
    return proxyServer.getStatus();
  });

  ipcMain.handle("app:start-server", async () => {
    return proxyServer.start();
  });

  ipcMain.handle("app:stop-server", async () => {
    return proxyServer.stop();
  });

  ipcMain.handle("config:get", async () => {
    return configStore.get();
  });

  ipcMain.handle("config:save", async (_event, nextConfig) => {
    const prevConfig = configStore.get();
    const saved = configStore.save(nextConfig);
    applyLaunchOnStartupSetting(saved);

    let restarted = false;
    if (proxyServer.isRunning() && hasServerSettingChanged(prevConfig, saved)) {
      await proxyServer.stop();
      await proxyServer.start();
      restarted = true;
    }

    return {
      ok: true,
      config: saved,
      restarted,
      status: proxyServer.getStatus()
    };
  });

  ipcMain.handle("config:export-groups", async () => {
    const { current, backupPayload } = buildGroupsBackupContent();
    const defaultPath = path.join(app.getPath("documents"), getBackupDefaultFileName());
    const saveResult = await dialog.showSaveDialog(mainWindow || undefined, {
      title: "Export Group Rules Backup",
      defaultPath,
      filters: [{ name: "JSON", extensions: ["json"] }]
    });

    if (saveResult.canceled || !saveResult.filePath) {
      return { ok: true, canceled: true, filePath: null, groupCount: current.groups.length };
    }

    fs.writeFileSync(saveResult.filePath, JSON.stringify(backupPayload, null, 2), "utf-8");
    return {
      ok: true,
      canceled: false,
      filePath: saveResult.filePath,
      groupCount: current.groups.length,
      source: "file"
    };
  });

  ipcMain.handle("config:export-groups-folder", async () => {
    const { current, jsonText } = buildGroupsBackupContent();
    const openResult = await dialog.showOpenDialog(mainWindow || undefined, {
      title: "Choose Backup Folder",
      properties: ["openDirectory", "createDirectory"]
    });

    if (openResult.canceled || openResult.filePaths.length === 0) {
      return { ok: true, canceled: true, filePath: null, groupCount: current.groups.length, source: "folder" };
    }

    const folderPath = openResult.filePaths[0];
    const backupPath = path.join(folderPath, getBackupDefaultFileName());
    fs.writeFileSync(backupPath, jsonText, "utf-8");

    return {
      ok: true,
      canceled: false,
      filePath: backupPath,
      groupCount: current.groups.length,
      source: "folder"
    };
  });

  ipcMain.handle("config:export-groups-clipboard", async () => {
    const { current, jsonText } = buildGroupsBackupContent();
    clipboard.writeText(jsonText);
    return {
      ok: true,
      canceled: false,
      groupCount: current.groups.length,
      charCount: jsonText.length,
      source: "clipboard"
    };
  });

  ipcMain.handle("config:import-groups", async () => {
    const openResult = await dialog.showOpenDialog(mainWindow || undefined, {
      title: "Import Group Rules Backup",
      properties: ["openFile"],
      filters: [{ name: "JSON", extensions: ["json"] }]
    });

    if (openResult.canceled || openResult.filePaths.length === 0) {
      return { ok: true, canceled: true };
    }

    const importPath = openResult.filePaths[0];
    const raw = fs.readFileSync(importPath, "utf-8");

    let parsed;
    try {
      parsed = JSON.parse(raw);
    } catch (error) {
      const err = new Error("Invalid JSON file");
      err.statusCode = 400;
      throw err;
    }

    return importGroupsAndSave(parsed, {
      filePath: importPath,
      source: "file"
    });
  });

  ipcMain.handle("config:import-groups-json", async (_event, jsonText) => {
    if (typeof jsonText !== "string" || jsonText.trim().length === 0) {
      const err = new Error("Invalid JSON text");
      err.statusCode = 400;
      throw err;
    }

    let parsed;
    try {
      parsed = JSON.parse(jsonText);
    } catch (error) {
      const err = new Error("Invalid JSON text");
      err.statusCode = 400;
      throw err;
    }

    return importGroupsAndSave(parsed, {
      source: "json"
    });
  });

  ipcMain.handle("app:read-clipboard-text", async () => {
    return {
      text: clipboard.readText() || ""
    };
  });

  ipcMain.handle("logs:list", async (_event, max) => {
    return logStore.list(max || 100);
  });

  ipcMain.handle("logs:clear", async () => {
    logStore.clear();
    return { ok: true };
  });
}

app.whenReady().then(async () => {
  const configPath = path.join(app.getPath("userData"), "config.json");
  configStore = new ConfigStore(configPath);
  logStore = new LogStore(100);

  configStore.initialize();
  applyLaunchOnStartupSetting(configStore.get());
  proxyServer = new ProxyServer(configStore, logStore);

  setupIpc();
  try {
    await proxyServer.start();
  } catch (err) {
    console.error("Failed to auto-start proxy service:", err);
  }
  createWindow();

  app.on("activate", () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      createWindow();
    }
  });
});

app.on("window-all-closed", async () => {
  if (process.platform !== "darwin") {
    if (proxyServer && proxyServer.isRunning()) {
      await proxyServer.stop();
    }
    app.quit();
  }
});
