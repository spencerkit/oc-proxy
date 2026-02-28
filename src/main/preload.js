const { contextBridge, ipcRenderer } = require("electron");

contextBridge.exposeInMainWorld("proxyApp", {
  getStatus: () => ipcRenderer.invoke("app:get-status"),
  startServer: () => ipcRenderer.invoke("app:start-server"),
  stopServer: () => ipcRenderer.invoke("app:stop-server"),
  getConfig: () => ipcRenderer.invoke("config:get"),
  saveConfig: (config) => ipcRenderer.invoke("config:save", config),
  listLogs: (max) => ipcRenderer.invoke("logs:list", max),
  clearLogs: () => ipcRenderer.invoke("logs:clear")
});
