const path = require("node:path");
const { app, BrowserWindow, ipcMain, Menu } = require("electron");
const { ConfigStore } = require("../proxy/configStore");
const { ProxyServer } = require("../proxy/server");
const { LogStore } = require("./logStore");

let mainWindow = null;
let configStore = null;
let proxyServer = null;
let logStore = null;

// 检查是否为开发模式
// 开发模式：从 out 目录运行，但源代码在 src/main 目录
// 生产模式：从打包的应用运行
const fs = require('fs');
const outDir = path.join(__dirname, '..');
const srcDir = path.join(__dirname, '../../src');
const isDev = fs.existsSync(srcDir);

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

function createWindow() {
  mainWindow = new BrowserWindow({
    width: 1240,
    height: 860,
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
