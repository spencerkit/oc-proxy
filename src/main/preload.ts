// @ts-nocheck
const { contextBridge, ipcRenderer } = require("electron");

contextBridge.exposeInMainWorld("proxyApp", {
  getStatus: () => ipcRenderer.invoke("app:get-status"),
  readClipboardText: () => ipcRenderer.invoke("app:read-clipboard-text"),
  startServer: () => ipcRenderer.invoke("app:start-server"),
  stopServer: () => ipcRenderer.invoke("app:stop-server"),
  getConfig: () => ipcRenderer.invoke("config:get"),
  saveConfig: (config) => ipcRenderer.invoke("config:save", config),
  exportGroupsBackup: () => ipcRenderer.invoke("config:export-groups"),
  exportGroupsToFolder: () => ipcRenderer.invoke("config:export-groups-folder"),
  exportGroupsToClipboard: () => ipcRenderer.invoke("config:export-groups-clipboard"),
  importGroupsBackup: () => ipcRenderer.invoke("config:import-groups"),
  importGroupsFromJson: (jsonText) => ipcRenderer.invoke("config:import-groups-json", jsonText),
  listLogs: (max) => ipcRenderer.invoke("logs:list", max),
  clearLogs: () => ipcRenderer.invoke("logs:clear")
});
