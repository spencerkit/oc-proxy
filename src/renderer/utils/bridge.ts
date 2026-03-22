import { httpApi, isHttpCandidate } from "./http"
import { ipc } from "./ipc"

type Transport = "http" | "ipc"

let resolvedTransport: Transport | null = null

function hasTauriInvoke(): boolean {
  return Boolean(window.__TAURI__?.core?.invoke || window.__TAURI_INTERNALS__?.invoke)
}

function resolveTransport(): Transport {
  if (hasTauriInvoke()) {
    return "ipc"
  }
  return isHttpCandidate() ? "http" : "ipc"
}

async function withBridge<T>(httpFn: () => Promise<T>, ipcFn: () => Promise<T>): Promise<T> {
  if (!resolvedTransport) {
    resolvedTransport = resolveTransport()
  }
  if (resolvedTransport === "http") {
    try {
      return await httpFn()
    } catch (error) {
      if (hasTauriInvoke()) {
        resolvedTransport = "ipc"
        return ipcFn()
      }
      throw error
    }
  }

  try {
    return await ipcFn()
  } catch (error) {
    if (isHttpCandidate()) {
      resolvedTransport = "http"
      return httpFn()
    }
    throw error
  }
}

export const bridge = {
  getAuthSession() {
    return withBridge(httpApi.getAuthSession, ipc.getAuthSession)
  },

  loginRemoteAdmin(password: string) {
    return withBridge(
      () => httpApi.loginRemoteAdmin(password),
      () => ipc.loginRemoteAdmin(password)
    )
  },

  logoutRemoteAdmin() {
    return withBridge(httpApi.logoutRemoteAdmin, ipc.logoutRemoteAdmin)
  },

  getAppInfo() {
    return withBridge(httpApi.getAppInfo, ipc.getAppInfo)
  },

  reportRendererReady() {
    return withBridge(httpApi.reportRendererReady, ipc.reportRendererReady)
  },

  reportRendererError(payload: { kind: string; message: string; stack?: string; source?: string }) {
    return withBridge(
      () => httpApi.reportRendererError(payload),
      () => ipc.reportRendererError(payload)
    )
  },

  getStatus() {
    return withBridge(httpApi.getStatus, ipc.getStatus)
  },

  readClipboardText() {
    return withBridge(httpApi.readClipboardText, ipc.readClipboardText)
  },

  startServer() {
    return withBridge(httpApi.startServer, ipc.startServer)
  },

  stopServer() {
    return withBridge(httpApi.stopServer, ipc.stopServer)
  },

  getConfig() {
    return withBridge(httpApi.getConfig, ipc.getConfig)
  },

  saveConfig(config: Parameters<typeof ipc.saveConfig>[0]) {
    return withBridge(
      () => httpApi.saveConfig(config),
      () => ipc.saveConfig(config)
    )
  },

  setRemoteAdminPassword(password: string) {
    return withBridge(
      () => httpApi.setRemoteAdminPassword(password),
      () => ipc.setRemoteAdminPassword(password)
    )
  },

  clearRemoteAdminPassword() {
    return withBridge(httpApi.clearRemoteAdminPassword, ipc.clearRemoteAdminPassword)
  },

  exportGroupsBackup() {
    return withBridge(httpApi.exportGroupsBackup, ipc.exportGroupsBackup)
  },

  exportGroupsToFolder() {
    return withBridge(httpApi.exportGroupsToFolder, ipc.exportGroupsToFolder)
  },

  exportGroupsToClipboard() {
    return withBridge(httpApi.exportGroupsToClipboard, ipc.exportGroupsToClipboard)
  },

  importGroupsBackup() {
    return withBridge(httpApi.importGroupsBackup, ipc.importGroupsBackup)
  },

  importGroupsFromJson(jsonText: string) {
    return withBridge(
      () => httpApi.importGroupsFromJson(jsonText),
      () => ipc.importGroupsFromJson(jsonText)
    )
  },

  remoteRulesUpload(force?: boolean) {
    return withBridge(
      () => httpApi.remoteRulesUpload(force),
      () => ipc.remoteRulesUpload(force)
    )
  },

  remoteRulesPull(force?: boolean) {
    return withBridge(
      () => httpApi.remoteRulesPull(force),
      () => ipc.remoteRulesPull(force)
    )
  },

  listLogs(max?: number) {
    return withBridge(
      () => httpApi.listLogs(max),
      () => ipc.listLogs(max)
    )
  },

  clearLogs() {
    return withBridge(httpApi.clearLogs, ipc.clearLogs)
  },

  getLogsStatsSummary(
    hours?: number,
    ruleKeys?: string[],
    ruleKey?: string,
    dimension?: Parameters<typeof ipc.getLogsStatsSummary>[3],
    enableComparison?: boolean
  ) {
    return withBridge(
      () => httpApi.getLogsStatsSummary(hours, ruleKeys, ruleKey, dimension, enableComparison),
      () => ipc.getLogsStatsSummary(hours, ruleKeys, ruleKey, dimension, enableComparison)
    )
  },

  getRuleCardStats(groupId: string, hours?: number) {
    return withBridge(
      () => httpApi.getRuleCardStats(groupId, hours),
      () => ipc.getRuleCardStats(groupId, hours)
    )
  },

  clearLogsStats(beforeEpochMs?: number) {
    return withBridge(
      () => httpApi.clearLogsStats(beforeEpochMs),
      () => ipc.clearLogsStats(beforeEpochMs)
    )
  },

  getProviderQuota(groupId: string, providerId: string) {
    return withBridge(
      () => httpApi.getProviderQuota(groupId, providerId),
      () => ipc.getProviderQuota(groupId, providerId)
    )
  },

  getGroupQuotas(groupId: string) {
    return withBridge(
      () => httpApi.getGroupQuotas(groupId),
      () => ipc.getGroupQuotas(groupId)
    )
  },

  testRuleQuotaDraft(
    groupId: string,
    providerName: string,
    providerToken: string,
    providerApiAddress: string,
    providerDefaultModel: string,
    quota: Parameters<typeof ipc.testRuleQuotaDraft>[5]
  ) {
    return withBridge(
      () =>
        httpApi.testRuleQuotaDraft(
          groupId,
          providerName,
          providerToken,
          providerApiAddress,
          providerDefaultModel,
          quota
        ),
      () =>
        ipc.testRuleQuotaDraft(
          groupId,
          providerName,
          providerToken,
          providerApiAddress,
          providerDefaultModel,
          quota
        )
    )
  },

  testProviderModel(groupId: string, providerId: string) {
    return withBridge(
      () => httpApi.testProviderModel(groupId, providerId),
      () => ipc.testProviderModel(groupId, providerId)
    )
  },

  integrationListTargets() {
    return withBridge(httpApi.integrationListTargets, ipc.integrationListTargets)
  },

  integrationPickDirectory(
    initialDir?: string,
    kind?: Parameters<typeof ipc.integrationPickDirectory>[1]
  ) {
    return withBridge(
      () => httpApi.integrationPickDirectory(initialDir, kind),
      () => ipc.integrationPickDirectory(initialDir, kind)
    )
  },

  integrationAddTarget(kind: Parameters<typeof ipc.integrationAddTarget>[0], configDir: string) {
    return withBridge(
      () => httpApi.integrationAddTarget(kind, configDir),
      () => ipc.integrationAddTarget(kind, configDir)
    )
  },

  integrationUpdateTarget(targetId: string, configDir: string) {
    return withBridge(
      () => httpApi.integrationUpdateTarget(targetId, configDir),
      () => ipc.integrationUpdateTarget(targetId, configDir)
    )
  },

  integrationRemoveTarget(targetId: string) {
    return withBridge(
      () => httpApi.integrationRemoveTarget(targetId),
      () => ipc.integrationRemoveTarget(targetId)
    )
  },

  integrationWriteGroupEntry(groupId: string, targetIds: string[]) {
    return withBridge(
      () => httpApi.integrationWriteGroupEntry(groupId, targetIds),
      () => ipc.integrationWriteGroupEntry(groupId, targetIds)
    )
  },

  integrationReadAgentConfig(targetId: string) {
    return withBridge(
      () => httpApi.integrationReadAgentConfig(targetId),
      () => ipc.integrationReadAgentConfig(targetId)
    )
  },

  integrationWriteAgentConfig(
    targetId: string,
    config: Parameters<typeof ipc.integrationWriteAgentConfig>[1]
  ) {
    return withBridge(
      () => httpApi.integrationWriteAgentConfig(targetId, config),
      () => ipc.integrationWriteAgentConfig(targetId, config)
    )
  },

  integrationWriteAgentConfigSource(targetId: string, content: string, sourceId?: string) {
    return withBridge(
      () => httpApi.integrationWriteAgentConfigSource(targetId, content, sourceId),
      () => ipc.integrationWriteAgentConfigSource(targetId, content, sourceId)
    )
  },
}
