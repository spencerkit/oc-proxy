const path = require("node:path");
const { app, BrowserWindow, ipcMain, Menu } = require("electron");
const { ConfigStore } = require("../proxy/configStore");
const { ProxyServer } = require("../proxy/server");
const { LogStore } = require("./logStore");

let mainWindow = null;
let configStore = null;
let proxyServer = null;
let logStore = null;

function createWindow() {
  mainWindow = new BrowserWindow({
    width: 1240,
    height: 860,
    webPreferences: {
      preload: path.join(__dirname, "preload.js"),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true
    }
  });

  Menu.setApplicationMenu(null);
  mainWindow.setMenuBarVisibility(false);
  mainWindow.loadFile(path.join(__dirname, "../renderer/index.html"));
}

function hasServerSettingChanged(prev, next) {
  return prev.server.host !== next.server.host
    || prev.server.port !== next.server.port
    || prev.server.authEnabled !== next.server.authEnabled
    || prev.server.localBearerToken !== next.server.localBearerToken;
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
