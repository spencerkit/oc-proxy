import { action } from "@relax-state/core"
import type {
  AppInfo,
  ClipboardTextResult,
  Group,
  GroupBackupExportResult,
  GroupBackupImportResult,
  GroupImportMode,
  ProviderModelHealthSnapshot,
  ProviderModelTestResult,
  ProxyConfig,
  RemoteRulesPullResult,
  RemoteRulesUploadResult,
  RuleQuotaConfig,
  RuleQuotaTestResult,
  StatsDimension,
} from "@/types"
import { bridge } from "@/utils/bridge"
import { normalizeGroupFailoverConfig } from "@/utils/groupFailover"
import {
  buildProviderModelHealthSnapshot,
  createProviderTestKey,
  resolveProviderTestGroupId,
} from "@/utils/providerTesting"
import {
  activeGroupIdState,
  bootstrapErrorState,
  bootstrappingState,
  configState,
  errorState,
  lastOperationErrorState,
  loadingState,
  logsErrorState,
  logsState,
  logsStatsState,
  providerCardStatsByProviderKeyState,
  providerModelHealthByProviderKeyState,
  providerQuotasState,
  quotaErrorState,
  quotaLoadingProviderKeysState,
  savingConfigState,
  serverActionState,
  statsErrorState,
  statusErrorState,
  statusIntervalIdState,
  statusState,
} from "./proxyState"
import { persistActiveGroupId } from "./proxyStorage"

const STATUS_POLL_INTERVAL = 3000
const MAX_LOGS = 100
let statusRefreshInFlight = false

const quotaKey = (groupId: string, providerId: string) => `${groupId}:${providerId}`

function createDefaultConfig(): ProxyConfig {
  return {
    server: {
      host: "0.0.0.0",
      port: 8899,
      authEnabled: false,
      localBearerToken: "",
    },
    compat: {
      strictMode: false,
      textToolCallFallbackEnabled: true,
      headerPassthroughEnabled: true,
    },
    logging: {
      captureBody: false,
    },
    ui: {
      theme: "light",
      locale: "en-US",
      localeMode: "auto",
      launchOnStartup: false,
      autoStartServer: true,
      closeToTray: true,
      quotaAutoRefreshMinutes: 5,
      autoUpdateEnabled: true,
    },
    remoteGit: {
      enabled: false,
      repoUrl: "",
      token: "",
      branch: "main",
    },
    providers: [],
    groups: [],
  }
}

type GroupLike = Partial<Group> & Pick<Group, "id" | "name">

function normalizeProviderIds(providerIds: Array<string | null | undefined>): string[] {
  return providerIds
    .map(providerId => providerId?.trim())
    .filter((providerId): providerId is string => Boolean(providerId))
}

function getFallbackGroupProviders(group: GroupLike): Group["providers"] {
  return group.providers ?? group.rules ?? []
}

function getScopedGroupProviders(group: GroupLike): {
  providerIds: string[]
  providers: Group["providers"]
} {
  const fallbackProviders = getFallbackGroupProviders(group)
  const hasExplicitProviderIds = Array.isArray(group.providerIds)
  const providerIds = normalizeProviderIds(
    hasExplicitProviderIds
      ? (group.providerIds ?? [])
      : fallbackProviders.map(provider => provider.id)
  )

  if (!hasExplicitProviderIds) {
    return {
      providerIds,
      providers: fallbackProviders,
    }
  }

  const providerIdSet = new Set(providerIds)
  return {
    providerIds,
    providers: fallbackProviders.filter(provider => providerIdSet.has(provider.id?.trim() ?? "")),
  }
}

function normalizeGroup(
  group: GroupLike,
  globalProviderMap?: Map<string, Group["providers"][number]>
): Group {
  const { providerIds, providers: scopedProviders } = getScopedGroupProviders(group)
  const scopedProviderMap = new Map(
    scopedProviders
      .filter(provider => provider?.id?.trim())
      .map(provider => [provider.id, provider] as const)
  )
  const resolvedProviders = providerIds
    .map(providerId => globalProviderMap?.get(providerId) ?? scopedProviderMap.get(providerId))
    .filter((provider): provider is Group["providers"][number] => Boolean(provider))
  const activeProviderId = group.activeProviderId ?? group.activeRuleId ?? null
  return {
    ...group,
    providerIds,
    providers: resolvedProviders,
    activeProviderId,
    rules: resolvedProviders,
    activeRuleId: activeProviderId,
    models: group.models ?? [],
    failover: normalizeGroupFailoverConfig(group.failover),
  }
}

function normalizeConfig(config: ProxyConfig | null | undefined): ProxyConfig {
  const defaults = createDefaultConfig()
  const input = config ?? defaults
  const safeConfig: ProxyConfig = {
    ...defaults,
    ...input,
    server: { ...defaults.server, ...(input.server ?? {}) },
    compat: { ...defaults.compat, ...(input.compat ?? {}) },
    logging: { ...defaults.logging, ...(input.logging ?? {}) },
    ui: { ...defaults.ui, ...(input.ui ?? {}) },
    remoteGit: { ...defaults.remoteGit, ...(input.remoteGit ?? {}) },
    providers: Array.isArray(input.providers) ? input.providers : [],
    groups: Array.isArray(input.groups) ? input.groups : [],
  }
  const dedupedProviderMap = new Map<string, Group["providers"][number]>()
  for (const provider of safeConfig.providers ?? []) {
    if (!provider?.id?.trim()) continue
    dedupedProviderMap.set(provider.id, { ...provider })
  }
  for (const group of safeConfig.groups ?? []) {
    for (const provider of getScopedGroupProviders(group as GroupLike).providers) {
      if (!provider?.id?.trim() || dedupedProviderMap.has(provider.id)) continue
      dedupedProviderMap.set(provider.id, { ...provider })
    }
  }
  const normalizedProviders = [...dedupedProviderMap.values()]
  const globalProviderMap = new Map(
    normalizedProviders.map(provider => [provider.id, provider] as const)
  )
  return {
    ...safeConfig,
    ui: {
      ...safeConfig.ui,
      autoUpdateEnabled: safeConfig.ui.autoUpdateEnabled ?? true,
    },
    providers: normalizedProviders,
    compat: {
      ...safeConfig.compat,
      headerPassthroughEnabled: safeConfig.compat.headerPassthroughEnabled ?? true,
      textToolCallFallbackEnabled: safeConfig.compat.textToolCallFallbackEnabled ?? true,
    },
    groups: (safeConfig.groups ?? []).map(group =>
      normalizeGroup(group as Partial<Group> & Pick<Group, "id" | "name">, globalProviderMap)
    ),
  }
}

function buildSaveConfigPayload(config: ProxyConfig): ProxyConfig {
  const globalProviderMap = new Map<string, Group["providers"][number]>()
  for (const provider of config.providers ?? []) {
    if (!provider?.id?.trim()) continue
    globalProviderMap.set(provider.id, { ...provider })
  }
  for (const group of config.groups ?? []) {
    for (const provider of getScopedGroupProviders(group).providers) {
      if (!provider?.id?.trim() || globalProviderMap.has(provider.id)) continue
      globalProviderMap.set(provider.id, { ...provider })
    }
  }
  const globalProviders = [...globalProviderMap.values()]
  const providerById = new Map(globalProviders.map(provider => [provider.id, provider] as const))
  return {
    ...config,
    providers: globalProviders,
    groups: (config.groups ?? []).map(group => {
      const { providerIds, providers: scopedProviders } = getScopedGroupProviders(group)
      const scopedProviderMap = new Map(
        scopedProviders
          .filter(provider => provider?.id?.trim())
          .map(provider => [provider.id, provider] as const)
      )
      const activeProviderId = group.activeProviderId ?? group.activeRuleId ?? null
      const resolvedProviders = providerIds
        .map(providerId => providerById.get(providerId) ?? scopedProviderMap.get(providerId))
        .filter((provider): provider is Group["providers"][number] => Boolean(provider))
      return {
        id: group.id,
        name: group.name,
        models: group.models ?? [],
        providerIds,
        providers: resolvedProviders,
        activeProviderId,
        failover: normalizeGroupFailoverConfig(group.failover),
      } as Group
    }),
  }
}

function collectValidProviderKeys(config: ProxyConfig): Set<string> {
  const keys = new Set<string>()
  for (const provider of config.providers ?? []) {
    if (!provider?.id?.trim()) continue
    keys.add(createProviderTestKey(undefined, provider.id))
  }
  for (const group of config.groups ?? []) {
    const providerIds = getScopedGroupProviders(group).providerIds
    for (const providerId of providerIds) {
      keys.add(createProviderTestKey(group.id, providerId))
    }
  }
  return keys
}

function pruneProviderModelHealthSnapshots(
  current: Record<string, ProviderModelHealthSnapshot>,
  config: ProxyConfig
): Record<string, ProviderModelHealthSnapshot> {
  const validKeys = collectValidProviderKeys(config)
  return Object.fromEntries(Object.entries(current).filter(([key]) => validKeys.has(key)))
}

function getErrorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message) {
    return error.message
  }
  if (typeof error === "string" && error.trim()) {
    return error
  }
  if (error && typeof error === "object" && "message" in error) {
    const message = (error as { message?: unknown }).message
    if (typeof message === "string" && message.trim()) {
      return message
    }
  }
  return fallback
}

function requirePayload<P>(payload: P | undefined, name: string): P {
  if (payload === undefined) {
    throw new Error(`${name} requires a payload`)
  }
  return payload
}

export const initAction = action<void, Promise<void>>(async store => {
  try {
    store.set(loadingState, true)
    store.set(errorState, null)
    store.set(bootstrappingState, true)
    store.set(bootstrapErrorState, null)
    store.set(lastOperationErrorState, null)

    const [rawConfig, status] = await Promise.all([bridge.getConfig(), bridge.getStatus()])
    const config = normalizeConfig(rawConfig)
    const logsStats = await bridge
      .getLogsStatsSummary(undefined, undefined, undefined, "rule", false)
      .catch(error => {
        const errorMessage = getErrorMessage(error, "Failed to load logs stats")
        store.set(statsErrorState, errorMessage)
        store.set(lastOperationErrorState, errorMessage)
        return null
      })

    store.set(configState, config)
    store.set(statusState, status)
    store.set(logsStatsState, logsStats)
    store.set(
      providerModelHealthByProviderKeyState,
      pruneProviderModelHealthSnapshots(store.get(providerModelHealthByProviderKeyState), config)
    )
    store.set(loadingState, false)
    store.set(bootstrappingState, false)
    store.set(bootstrapErrorState, null)

    startPollingAction()
  } catch (error) {
    const errorMessage = getErrorMessage(error, "Failed to initialize")
    store.set(loadingState, false)
    store.set(errorState, errorMessage)
    store.set(bootstrappingState, false)
    store.set(bootstrapErrorState, errorMessage)
    store.set(lastOperationErrorState, errorMessage)
  }
})

export const refreshStatusAction = action<void, Promise<void>>(async store => {
  if (statusRefreshInFlight) {
    return
  }

  statusRefreshInFlight = true
  try {
    const status = await bridge.getStatus()
    store.set(statusState, status)
    store.set(statusErrorState, null)
  } catch (error) {
    const errorMessage = getErrorMessage(error, "Failed to refresh status")
    store.set(statusErrorState, errorMessage)
    store.set(lastOperationErrorState, errorMessage)
  } finally {
    statusRefreshInFlight = false
  }
})

export const refreshLogsAction = action<void, Promise<void>>(async store => {
  try {
    const logs = await bridge.listLogs(MAX_LOGS)
    store.set(logsState, logs)
    store.set(logsErrorState, null)
  } catch (error) {
    const errorMessage = getErrorMessage(error, "Failed to refresh logs")
    store.set(logsErrorState, errorMessage)
    store.set(lastOperationErrorState, errorMessage)
  }
})

export const refreshLogsStatsAction = action<
  {
    hours?: number
    ruleKeys?: string[]
    ruleKey?: string
    dimension?: StatsDimension
    enableComparison?: boolean
  },
  Promise<void>
>(async (store, payload) => {
  try {
    const request = requirePayload(payload, "refreshLogsStatsAction")
    const logsStats = await bridge.getLogsStatsSummary(
      request.hours,
      request.ruleKeys,
      request.ruleKey,
      request.dimension,
      request.enableComparison
    )
    store.set(logsStatsState, logsStats)
    store.set(statsErrorState, null)
  } catch (error) {
    const errorMessage = getErrorMessage(error, "Failed to refresh logs stats")
    store.set(statsErrorState, errorMessage)
    store.set(lastOperationErrorState, errorMessage)
  }
})

export const saveConfigAction = action<ProxyConfig, Promise<void>>(async (store, config) => {
  try {
    const nextConfig = requirePayload(config, "saveConfigAction")
    store.set(savingConfigState, true)
    store.set(lastOperationErrorState, null)

    const result = await bridge.saveConfig(buildSaveConfigPayload(normalizeConfig(nextConfig)))

    const normalizedConfig = normalizeConfig(result.config)
    store.set(configState, normalizedConfig)
    store.set(statusState, result.status)
    store.set(
      providerModelHealthByProviderKeyState,
      pruneProviderModelHealthSnapshots(
        store.get(providerModelHealthByProviderKeyState),
        normalizedConfig
      )
    )
    store.set(savingConfigState, false)
    store.set(lastOperationErrorState, null)
  } catch (error) {
    const errorMessage = getErrorMessage(error, "Failed to save configuration")
    store.set(savingConfigState, false)
    store.set(lastOperationErrorState, errorMessage)
    throw new Error(errorMessage)
  }
})

export const exportGroupsBackupAction = action<void, Promise<GroupBackupExportResult>>(
  async store => {
    try {
      store.set(lastOperationErrorState, null)
      return await bridge.exportGroupsBackup()
    } catch (error) {
      const errorMessage = getErrorMessage(error, "Failed to export group backup")
      store.set(lastOperationErrorState, errorMessage)
      throw error
    }
  }
)

export const exportGroupsToFolderAction = action<void, Promise<GroupBackupExportResult>>(
  async store => {
    try {
      store.set(lastOperationErrorState, null)
      return await bridge.exportGroupsToFolder()
    } catch (error) {
      const errorMessage = getErrorMessage(error, "Failed to export group backup")
      store.set(lastOperationErrorState, errorMessage)
      throw error
    }
  }
)

export const exportGroupsToClipboardAction = action<void, Promise<GroupBackupExportResult>>(
  async store => {
    try {
      store.set(lastOperationErrorState, null)
      return await bridge.exportGroupsToClipboard()
    } catch (error) {
      const errorMessage = getErrorMessage(error, "Failed to export group backup")
      store.set(lastOperationErrorState, errorMessage)
      throw error
    }
  }
)

export const importGroupsBackupAction = action<
  { mode?: GroupImportMode } | undefined,
  Promise<GroupBackupImportResult>
>(async (store, payload) => {
  try {
    const request = payload ?? {}
    store.set(savingConfigState, true)
    store.set(lastOperationErrorState, null)
    const result = await bridge.importGroupsBackup(request.mode)

    if (!result.canceled && result.config && result.status) {
      const normalizedConfig = normalizeConfig(result.config)
      store.set(configState, normalizedConfig)
      store.set(statusState, result.status)
      store.set(
        providerModelHealthByProviderKeyState,
        pruneProviderModelHealthSnapshots(
          store.get(providerModelHealthByProviderKeyState),
          normalizedConfig
        )
      )
      store.set(savingConfigState, false)
    } else {
      store.set(savingConfigState, false)
    }

    return result
  } catch (error) {
    const errorMessage = getErrorMessage(error, "Failed to import group backup")
    store.set(savingConfigState, false)
    store.set(lastOperationErrorState, errorMessage)
    throw error
  }
})

export const importGroupsFromJsonAction = action<
  { jsonText: string; mode?: GroupImportMode },
  Promise<GroupBackupImportResult>
>(async (store, payload) => {
  try {
    const request = requirePayload(payload, "importGroupsFromJsonAction")
    store.set(savingConfigState, true)
    store.set(lastOperationErrorState, null)
    const result = await bridge.importGroupsFromJson(request.jsonText, request.mode)

    if (!result.canceled && result.config && result.status) {
      const normalizedConfig = normalizeConfig(result.config)
      store.set(configState, normalizedConfig)
      store.set(statusState, result.status)
      store.set(
        providerModelHealthByProviderKeyState,
        pruneProviderModelHealthSnapshots(
          store.get(providerModelHealthByProviderKeyState),
          normalizedConfig
        )
      )
      store.set(savingConfigState, false)
    } else {
      store.set(savingConfigState, false)
    }

    return result
  } catch (error) {
    const errorMessage = getErrorMessage(error, "Failed to import group backup")
    store.set(savingConfigState, false)
    store.set(lastOperationErrorState, errorMessage)
    throw error
  }
})

export const remoteRulesUploadAction = action<
  { force?: boolean },
  Promise<RemoteRulesUploadResult>
>(async (store, payload) => {
  try {
    const request = payload ?? {}
    store.set(lastOperationErrorState, null)
    return await bridge.remoteRulesUpload(request.force)
  } catch (error) {
    const errorMessage = getErrorMessage(error, "Failed to upload remote rules")
    store.set(lastOperationErrorState, errorMessage)
    throw error
  }
})

export const remoteRulesPullAction = action<{ force?: boolean }, Promise<RemoteRulesPullResult>>(
  async (store, payload) => {
    try {
      const request = payload ?? {}
      store.set(lastOperationErrorState, null)
      const result = await bridge.remoteRulesPull(request.force)
      if (result.config && result.status) {
        const normalizedConfig = normalizeConfig(result.config)
        store.set(configState, normalizedConfig)
        store.set(statusState, result.status)
        store.set(
          providerModelHealthByProviderKeyState,
          pruneProviderModelHealthSnapshots(
            store.get(providerModelHealthByProviderKeyState),
            normalizedConfig
          )
        )
      }
      return result
    } catch (error) {
      const errorMessage = getErrorMessage(error, "Failed to pull remote rules")
      store.set(lastOperationErrorState, errorMessage)
      throw error
    }
  }
)

export const readClipboardTextAction = action<void, Promise<ClipboardTextResult>>(async store => {
  try {
    store.set(lastOperationErrorState, null)
    return await bridge.readClipboardText()
  } catch (error) {
    const errorMessage = getErrorMessage(error, "Failed to read clipboard")
    store.set(lastOperationErrorState, errorMessage)
    throw error
  }
})

export const setActiveGroupIdAction = action<{ groupId: string | null }, void>((store, payload) => {
  const request = requirePayload(payload, "setActiveGroupIdAction")
  persistActiveGroupId(request.groupId)
  store.set(activeGroupIdState, request.groupId)
})

export const clearLogsAction = action<void, Promise<void>>(async store => {
  try {
    await bridge.clearLogs()
    store.set(logsState, [])
    store.set(logsErrorState, null)
  } catch (error) {
    const errorMessage = getErrorMessage(error, "Failed to clear logs")
    store.set(logsErrorState, errorMessage)
    store.set(lastOperationErrorState, errorMessage)
  }
})

export const clearLogsStatsAction = action<{ beforeEpochMs?: number }, Promise<void>>(
  async (store, payload) => {
    try {
      const request = payload ?? {}
      await bridge.clearLogsStats(request.beforeEpochMs)
      store.set(logsStatsState, null)
      store.set(statsErrorState, null)
    } catch (error) {
      const errorMessage = getErrorMessage(error, "Failed to clear logs stats")
      store.set(statsErrorState, errorMessage)
      store.set(lastOperationErrorState, errorMessage)
      throw error
    }
  }
)

export const fetchGroupQuotasAction = action<{ groupId: string }, Promise<void>>(
  async (store, payload) => {
    try {
      const request = requirePayload(payload, "fetchGroupQuotasAction")
      if (!request.groupId.trim()) return
      store.set(quotaErrorState, null)
      const snapshots = await bridge.getGroupQuotas(request.groupId)
      const current = store.get(providerQuotasState)
      const next = { ...current }
      for (const snapshot of snapshots) {
        next[quotaKey(snapshot.groupId, snapshot.ruleId)] = snapshot
      }
      store.set(providerQuotasState, next)
    } catch (error) {
      const errorMessage = getErrorMessage(error, "Failed to fetch group quotas")
      store.set(quotaErrorState, errorMessage)
      store.set(lastOperationErrorState, errorMessage)
      throw error
    }
  }
)

export const fetchGroupProviderCardStatsAction = action<
  { groupId: string; hours?: number },
  Promise<void>
>(async (store, payload) => {
  try {
    const request = requirePayload(payload, "fetchGroupProviderCardStatsAction")
    if (!request.groupId.trim()) return
    store.set(statsErrorState, null)
    const items = await bridge.getRuleCardStats(request.groupId, request.hours)
    const current = store.get(providerCardStatsByProviderKeyState)
    const next = { ...current }
    const groupPrefix = `${request.groupId}:`
    for (const key of Object.keys(next)) {
      if (key.startsWith(groupPrefix)) {
        delete next[key]
      }
    }
    for (const item of items) {
      next[quotaKey(item.groupId, item.ruleId)] = item
    }
    store.set(providerCardStatsByProviderKeyState, next)
  } catch (error) {
    const errorMessage = getErrorMessage(error, "Failed to fetch group provider card stats")
    store.set(statsErrorState, errorMessage)
    store.set(lastOperationErrorState, errorMessage)
    throw error
  }
})

export const fetchProviderQuotaAction = action<
  { groupId: string; providerId: string },
  Promise<void>
>(async (store, payload) => {
  const request = requirePayload(payload, "fetchProviderQuotaAction")
  const key = quotaKey(request.groupId, request.providerId)
  try {
    store.set(quotaErrorState, null)
    store.set(quotaLoadingProviderKeysState, {
      ...store.get(quotaLoadingProviderKeysState),
      [key]: true,
    })
    const snapshot = await bridge.getProviderQuota(request.groupId, request.providerId)
    store.set(providerQuotasState, {
      ...store.get(providerQuotasState),
      [key]: snapshot,
    })
    store.set(quotaLoadingProviderKeysState, {
      ...store.get(quotaLoadingProviderKeysState),
      [key]: false,
    })
  } catch (error) {
    const errorMessage = getErrorMessage(error, "Failed to fetch provider quota")
    store.set(quotaErrorState, errorMessage)
    store.set(lastOperationErrorState, errorMessage)
    store.set(quotaLoadingProviderKeysState, {
      ...store.get(quotaLoadingProviderKeysState),
      [key]: false,
    })
    throw error
  }
})

export const startPollingAction = action<void, void>(store => {
  const currentIntervalId = store.get(statusIntervalIdState)
  if (currentIntervalId !== null) {
    window.clearInterval(currentIntervalId)
  }

  const statusIntervalId = window.setInterval(() => {
    if (document.visibilityState !== "visible") {
      return
    }
    refreshStatusAction()
  }, STATUS_POLL_INTERVAL)

  store.set(statusIntervalIdState, statusIntervalId)
})

export const stopPollingAction = action<void, void>(store => {
  const currentIntervalId = store.get(statusIntervalIdState)
  if (currentIntervalId !== null) {
    window.clearInterval(currentIntervalId)
    store.set(statusIntervalIdState, null)
  }
})

export const startServerAction = action<void, Promise<void>>(async store => {
  try {
    store.set(serverActionState, "starting")
    store.set(lastOperationErrorState, null)
    const status = await bridge.startServer()
    store.set(statusState, status)
    store.set(serverActionState, null)
    store.set(statusErrorState, null)
  } catch (error) {
    const errorMessage = getErrorMessage(error, "Failed to start server")
    store.set(serverActionState, null)
    store.set(statusErrorState, errorMessage)
    store.set(lastOperationErrorState, errorMessage)
    throw new Error(errorMessage)
  }
})

export const stopServerAction = action<void, Promise<void>>(async store => {
  try {
    store.set(serverActionState, "stopping")
    store.set(lastOperationErrorState, null)
    const status = await bridge.stopServer()
    store.set(statusState, status)
    store.set(serverActionState, null)
    store.set(statusErrorState, null)
  } catch (error) {
    const errorMessage = getErrorMessage(error, "Failed to stop server")
    store.set(serverActionState, null)
    store.set(statusErrorState, errorMessage)
    store.set(lastOperationErrorState, errorMessage)
    throw new Error(errorMessage)
  }
})

export const testProviderModelAction = action<
  { groupId?: string; providerId: string },
  Promise<ProviderModelTestResult>
>(async (store, payload) => {
  const request = requirePayload(payload, "testProviderModelAction")
  const testGroupId = request.groupId
  const healthGroupId = resolveProviderTestGroupId(testGroupId)
  try {
    const result = await bridge.testProviderModel(testGroupId, request.providerId)
    const key = createProviderTestKey(testGroupId, request.providerId)
    store.set(providerModelHealthByProviderKeyState, {
      ...store.get(providerModelHealthByProviderKeyState),
      [key]: buildProviderModelHealthSnapshot({
        groupId: healthGroupId,
        providerId: request.providerId,
        ok: result.ok,
        latencyMs: result.responseTimeMs,
        resolvedModel: result.resolvedModel,
        rawText: result.rawText,
        message: result.message,
      }),
    })
    return result
  } catch (error) {
    const key = createProviderTestKey(testGroupId, request.providerId)
    store.set(providerModelHealthByProviderKeyState, {
      ...store.get(providerModelHealthByProviderKeyState),
      [key]: buildProviderModelHealthSnapshot({
        groupId: healthGroupId,
        providerId: request.providerId,
        ok: false,
        message: getErrorMessage(error, "Provider model test failed"),
      }),
    })
    throw error
  }
})

export const testRuleQuotaDraftAction = action<
  {
    groupId: string
    name: string
    token: string
    apiAddress: string
    defaultModel: string
    quotaConfig: RuleQuotaConfig
  },
  Promise<RuleQuotaTestResult>
>(async (_store, payload) => {
  const request = requirePayload(payload, "testRuleQuotaDraftAction")
  return await bridge.testRuleQuotaDraft(
    request.groupId,
    request.name,
    request.token,
    request.apiAddress,
    request.defaultModel,
    request.quotaConfig
  )
})

export const getAppInfoAction = action<void, Promise<AppInfo>>(async () => {
  return await bridge.getAppInfo()
})
