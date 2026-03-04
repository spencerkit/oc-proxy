/**
 * Proxy Store - Zustand State Management
 *
 * Central store for managing AI Open Router state including:
 * - Configuration management
 * - Server status tracking
 * - Request logging
 * - Active group selection
 */

import { create } from "zustand"
import type {
  ClipboardTextResult,
  Group,
  GroupBackupExportResult,
  GroupBackupImportResult,
  LogEntry,
  ProxyConfig,
  ProxyStatus,
  RemoteRulesPullResult,
  RemoteRulesUploadResult,
  RuleCardStatsItem,
  RuleQuotaSnapshot,
  StatsDimension,
  StatsSummaryResult,
} from "@/types"
import { ipc } from "@/utils/ipc"

/**
 * Proxy State Interface
 */
interface ProxyState {
  // State properties
  config: ProxyConfig | null
  status: ProxyStatus | null
  logs: LogEntry[]
  logsStats: StatsSummaryResult | null
  providerQuotas: Record<string, RuleQuotaSnapshot>
  providerCardStatsByProviderKey: Record<string, RuleCardStatsItem>
  quotaLoadingProviderKeys: Record<string, boolean>
  activeGroupId: string | null
  loading: boolean
  error: string | null

  // Polling interval IDs
  statusIntervalId: number | null
  logsIntervalId: number | null

  // Actions
  init: () => Promise<void>
  refreshStatus: () => Promise<void>
  refreshLogs: () => Promise<void>
  refreshLogsStats: (
    hours?: number,
    ruleKeys?: string[],
    ruleKey?: string,
    dimension?: StatsDimension,
    enableComparison?: boolean
  ) => Promise<void>
  saveConfig: (config: ProxyConfig) => Promise<void>
  exportGroupsBackup: () => Promise<GroupBackupExportResult>
  exportGroupsToFolder: () => Promise<GroupBackupExportResult>
  exportGroupsToClipboard: () => Promise<GroupBackupExportResult>
  importGroupsBackup: () => Promise<GroupBackupImportResult>
  importGroupsFromJson: (jsonText: string) => Promise<GroupBackupImportResult>
  remoteRulesUpload: (force?: boolean) => Promise<RemoteRulesUploadResult>
  remoteRulesPull: (force?: boolean) => Promise<RemoteRulesPullResult>
  readClipboardText: () => Promise<ClipboardTextResult>
  setActiveGroupId: (groupId: string | null) => void
  clearLogs: () => Promise<void>
  clearLogsStats: (beforeEpochMs?: number) => Promise<void>
  fetchGroupQuotas: (groupId: string) => Promise<void>
  fetchGroupProviderCardStats: (groupId: string, hours?: number) => Promise<void>
  fetchProviderQuota: (groupId: string, providerId: string) => Promise<void>
  startPolling: () => void
  stopPolling: () => void
  startServer: () => Promise<void>
  stopServer: () => Promise<void>
}

/**
 * Polling intervals (in milliseconds)
 */
const STATUS_POLL_INTERVAL = 3000
const LOGS_POLL_INTERVAL = 3000
const MAX_LOGS = 100
const quotaKey = (groupId: string, providerId: string) => `${groupId}:${providerId}`
const ACTIVE_GROUP_STORAGE_KEY = "ai-open-router.activeGroupId"

const readPersistedActiveGroupId = (): string | null => {
  if (typeof window === "undefined") return null
  try {
    const raw = window.localStorage.getItem(ACTIVE_GROUP_STORAGE_KEY)
    const value = raw?.trim()
    return value ? value : null
  } catch {
    return null
  }
}

const persistActiveGroupId = (groupId: string | null) => {
  if (typeof window === "undefined") return
  try {
    if (groupId?.trim()) {
      window.localStorage.setItem(ACTIVE_GROUP_STORAGE_KEY, groupId)
      return
    }
    window.localStorage.removeItem(ACTIVE_GROUP_STORAGE_KEY)
  } catch {}
}

function normalizeGroup(group: Partial<Group> & Pick<Group, "id" | "name">): Group {
  const providers = group.providers ?? group.rules ?? []
  const activeProviderId = group.activeProviderId ?? group.activeRuleId ?? null
  return {
    ...group,
    providers,
    activeProviderId,
    rules: providers,
    activeRuleId: activeProviderId,
    models: group.models ?? [],
  }
}

function normalizeConfig(config: ProxyConfig): ProxyConfig {
  return {
    ...config,
    groups: (config.groups ?? []).map(group =>
      normalizeGroup(group as Partial<Group> & Pick<Group, "id" | "name">)
    ),
  }
}

function buildSaveConfigPayload(config: ProxyConfig): ProxyConfig {
  return {
    ...config,
    groups: (config.groups ?? []).map(group => {
      const providers = group.providers ?? group.rules ?? []
      const activeProviderId = group.activeProviderId ?? group.activeRuleId ?? null
      return {
        id: group.id,
        name: group.name,
        models: group.models ?? [],
        providers,
        activeProviderId,
      } as Group
    }),
  }
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

/**
 * Create Zustand store for proxy state management
 */
export const useProxyStore = create<ProxyState>((set, get) => ({
  // Initial state
  config: null,
  status: null,
  logs: [],
  logsStats: null,
  providerQuotas: {},
  providerCardStatsByProviderKey: {},
  quotaLoadingProviderKeys: {},
  activeGroupId: readPersistedActiveGroupId(),
  loading: false,
  error: null,
  statusIntervalId: null,
  logsIntervalId: null,

  /**
   * Initialize store with initial data from IPC
   * Fetches config and status, then starts polling
   */
  init: async () => {
    try {
      console.log("[Store] Initializing...")
      set({ loading: true, error: null })

      console.log("[Store] Fetching config and status...")
      // Fetch initial config and status in parallel
      const [rawConfig, status, logsStats] = await Promise.all([
        ipc.getConfig(),
        ipc.getStatus(),
        ipc.getLogsStatsSummary(undefined, undefined, undefined, "rule", false),
      ])
      const config = normalizeConfig(rawConfig)

      console.log("[Store] Config received:", config)
      console.log("[Store] Status received:", status)

      set({
        config,
        status,
        logsStats,
        loading: false,
      })

      console.log("[Store] Initialization complete")

      // Start polling for status and logs
      get().startPolling()
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : "Failed to initialize"
      console.error("[Store] Initialization error:", errorMessage)
      set({
        error: errorMessage,
        loading: false,
      })
    }
  },

  /**
   * Refresh server status from IPC
   */
  refreshStatus: async () => {
    try {
      const status = await ipc.getStatus()
      set({ status, error: null })
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : "Failed to refresh status"
      set({ error: errorMessage })
    }
  },

  /**
   * Refresh logs from IPC
   */
  refreshLogs: async () => {
    try {
      const logs = await ipc.listLogs(MAX_LOGS)
      set({ logs, error: null })
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : "Failed to refresh logs"
      set({ error: errorMessage })
    }
  },

  /**
   * Refresh request/token stats summary from IPC
   */
  refreshLogsStats: async (
    hours?: number,
    ruleKeys?: string[],
    ruleKey?: string,
    dimension?: StatsDimension,
    enableComparison?: boolean
  ) => {
    try {
      const logsStats = await ipc.getLogsStatsSummary(
        hours,
        ruleKeys,
        ruleKey,
        dimension,
        enableComparison
      )
      set({ logsStats, error: null })
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : "Failed to refresh logs stats"
      set({ error: errorMessage })
    }
  },

  /**
   * Save configuration via IPC
   * Updates local config with the result
   */
  saveConfig: async (config: ProxyConfig) => {
    try {
      set({ loading: true, error: null })

      const result = await ipc.saveConfig(buildSaveConfigPayload(normalizeConfig(config)))

      set({
        config: normalizeConfig(result.config),
        status: result.status,
        loading: false,
      })
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : "Failed to save configuration"
      set({
        error: errorMessage,
        loading: false,
      })
      throw new Error(errorMessage)
    }
  },

  /**
   * Export all groups (including nested providers) to a JSON backup file
   */
  exportGroupsBackup: async () => {
    try {
      set({ error: null })
      return await ipc.exportGroupsBackup()
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : "Failed to export group backup"
      set({ error: errorMessage })
      throw error
    }
  },

  /**
   * Export all groups/providers to a JSON file under a selected folder
   */
  exportGroupsToFolder: async () => {
    try {
      set({ error: null })
      return await ipc.exportGroupsToFolder()
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : "Failed to export group backup"
      set({ error: errorMessage })
      throw error
    }
  },

  /**
   * Export all groups/providers JSON content directly to clipboard
   */
  exportGroupsToClipboard: async () => {
    try {
      set({ error: null })
      return await ipc.exportGroupsToClipboard()
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : "Failed to export group backup"
      set({ error: errorMessage })
      throw error
    }
  },

  /**
   * Import groups backup JSON and replace current groups
   */
  importGroupsBackup: async () => {
    try {
      set({ loading: true, error: null })
      const result = await ipc.importGroupsBackup()

      if (!result.canceled && result.config && result.status) {
        set({
          config: normalizeConfig(result.config),
          status: result.status,
          loading: false,
        })
      } else {
        set({ loading: false })
      }

      return result
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : "Failed to import group backup"
      set({
        error: errorMessage,
        loading: false,
      })
      throw error
    }
  },

  /**
   * Import groups from a JSON text payload and replace current groups
   */
  importGroupsFromJson: async (jsonText: string) => {
    try {
      set({ loading: true, error: null })
      const result = await ipc.importGroupsFromJson(jsonText)

      if (!result.canceled && result.config && result.status) {
        set({
          config: normalizeConfig(result.config),
          status: result.status,
          loading: false,
        })
      } else {
        set({ loading: false })
      }

      return result
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : "Failed to import group backup"
      set({
        error: errorMessage,
        loading: false,
      })
      throw error
    }
  },

  /**
   * Upload current groups/providers backup JSON to remote git repository
   */
  remoteRulesUpload: async (force?: boolean) => {
    try {
      set({ error: null })
      return await ipc.remoteRulesUpload(force)
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : "Failed to upload remote rules"
      set({ error: errorMessage })
      throw error
    }
  },

  /**
   * Pull groups/providers backup JSON from remote git and replace local groups
   */
  remoteRulesPull: async (force?: boolean) => {
    try {
      set({ error: null })
      const result = await ipc.remoteRulesPull(force)
      if (result.config && result.status) {
        set({
          config: normalizeConfig(result.config),
          status: result.status,
        })
      }
      return result
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : "Failed to pull remote rules"
      set({ error: errorMessage })
      throw error
    }
  },

  /**
   * Read plain text from system clipboard via main process
   */
  readClipboardText: async () => {
    try {
      set({ error: null })
      return await ipc.readClipboardText()
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : "Failed to read clipboard"
      set({ error: errorMessage })
      throw error
    }
  },

  /**
   * Set the active group ID
   */
  setActiveGroupId: (groupId: string | null) => {
    persistActiveGroupId(groupId)
    set({ activeGroupId: groupId })
  },

  /**
   * Clear all logs via IPC and in state
   */
  clearLogs: async () => {
    try {
      await ipc.clearLogs()
      set({ logs: [], error: null })
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : "Failed to clear logs"
      set({ error: errorMessage })
    }
  },

  clearLogsStats: async (beforeEpochMs?: number) => {
    try {
      await ipc.clearLogsStats(beforeEpochMs)
      set({ logsStats: null, error: null })
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : "Failed to clear logs stats"
      set({ error: errorMessage })
      throw error
    }
  },

  fetchGroupQuotas: async (groupId: string) => {
    try {
      if (!groupId.trim()) return
      set({ error: null })
      const snapshots = await ipc.getGroupQuotas(groupId)
      set(state => {
        const next = { ...state.providerQuotas }
        for (const snapshot of snapshots) {
          next[quotaKey(snapshot.groupId, snapshot.ruleId)] = snapshot
        }
        return { providerQuotas: next }
      })
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : "Failed to fetch group quotas"
      set({ error: errorMessage })
      throw error
    }
  },

  fetchGroupProviderCardStats: async (groupId: string, hours?: number) => {
    try {
      if (!groupId.trim()) return
      set({ error: null })
      const items = await ipc.getRuleCardStats(groupId, hours)
      set(state => {
        const next = { ...state.providerCardStatsByProviderKey }
        const groupPrefix = `${groupId}:`
        for (const key of Object.keys(next)) {
          if (key.startsWith(groupPrefix)) {
            delete next[key]
          }
        }
        for (const item of items) {
          next[quotaKey(item.groupId, item.ruleId)] = item
        }
        return { providerCardStatsByProviderKey: next }
      })
    } catch (error) {
      const errorMessage =
        error instanceof Error ? error.message : "Failed to fetch group provider card stats"
      set({ error: errorMessage })
      throw error
    }
  },

  fetchProviderQuota: async (groupId: string, providerId: string) => {
    const key = quotaKey(groupId, providerId)
    try {
      set(state => ({
        error: null,
        quotaLoadingProviderKeys: {
          ...state.quotaLoadingProviderKeys,
          [key]: true,
        },
      }))
      const snapshot = await ipc.getProviderQuota(groupId, providerId)
      set(state => ({
        providerQuotas: {
          ...state.providerQuotas,
          [key]: snapshot,
        },
        quotaLoadingProviderKeys: {
          ...state.quotaLoadingProviderKeys,
          [key]: false,
        },
      }))
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : "Failed to fetch provider quota"
      set(state => ({
        error: errorMessage,
        quotaLoadingProviderKeys: {
          ...state.quotaLoadingProviderKeys,
          [key]: false,
        },
      }))
      throw error
    }
  },

  /**
   * Start polling for status and logs
   * Sets up interval timers to refresh data periodically
   */
  startPolling: () => {
    const state = get()

    // Clear existing intervals if any
    if (state.statusIntervalId !== null) {
      window.clearInterval(state.statusIntervalId)
    }
    if (state.logsIntervalId !== null) {
      window.clearInterval(state.logsIntervalId)
    }

    // Set up status polling
    const statusIntervalId = window.setInterval(() => {
      get().refreshStatus()
    }, STATUS_POLL_INTERVAL)

    // Set up logs polling
    const logsIntervalId = window.setInterval(() => {
      get().refreshLogs()
    }, LOGS_POLL_INTERVAL)

    set({ statusIntervalId, logsIntervalId })
  },

  /**
   * Stop polling for status and logs
   * Clears interval timers
   */
  stopPolling: () => {
    const state = get()

    if (state.statusIntervalId !== null) {
      window.clearInterval(state.statusIntervalId)
      set({ statusIntervalId: null })
    }

    if (state.logsIntervalId !== null) {
      window.clearInterval(state.logsIntervalId)
      set({ logsIntervalId: null })
    }
  },

  /**
   * Start the proxy server via IPC
   */
  startServer: async () => {
    try {
      set({ loading: true, error: null })
      const status = await ipc.startServer()
      set({ status, loading: false })
    } catch (error) {
      const errorMessage = getErrorMessage(error, "Failed to start server")
      set({ loading: false })
      throw new Error(errorMessage)
    }
  },

  /**
   * Stop the proxy server via IPC
   */
  stopServer: async () => {
    try {
      set({ loading: true, error: null })
      const status = await ipc.stopServer()
      set({ status, loading: false })
    } catch (error) {
      const errorMessage = getErrorMessage(error, "Failed to stop server")
      set({ loading: false })
      throw new Error(errorMessage)
    }
  },
}))

/**
 * Selectors for common state queries
 */
export const proxySelectors = {
  /**
   * Check if proxy server is running

   */
  isRunning: (state: ProxyState) => state.status?.running ?? false,

  /**
   * Get active group object
   */
  activeGroup: (state: ProxyState) =>
    state.config?.groups.find((group: Group) => group.id === state.activeGroupId) ?? null,

  /**
   * Get total number of requests from status metrics
   */
  totalRequests: (state: ProxyState) => state.status?.metrics.requests ?? 0,

  /**
   * Get error count from status metrics
   */
  errorCount: (state: ProxyState) => state.status?.metrics.errors ?? 0,

  /**
   * Get uptime string from status metrics
   */
  uptime: (state: ProxyState) => {
    const startedAt = state.status?.metrics.uptimeStartedAt
    if (!startedAt) return "Not running"

    const uptime = Date.now() - new Date(startedAt).getTime()
    const seconds = Math.floor(uptime / 1000)
    const minutes = Math.floor(seconds / 60)
    const hours = Math.floor(minutes / 60)

    if (hours > 0) {
      return `${hours}h ${minutes % 60}m`
    } else if (minutes > 0) {
      return `${minutes}m ${seconds % 60}s`
    } else {
      return `${seconds}s`
    }
  },
}
