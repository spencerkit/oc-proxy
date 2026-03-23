import { computed, state } from "@relax-state/core"
import type {
  Group,
  LogEntry,
  ProviderModelHealthSnapshot,
  ProxyConfig,
  ProxyStatus,
  RuleCardStatsItem,
  RuleQuotaSnapshot,
  StatsSummaryResult,
} from "@/types"
import { readPersistedActiveGroupId } from "./proxyStorage"

export const configState = state<ProxyConfig | null>(null, "proxy.config")
export const statusState = state<ProxyStatus | null>(null, "proxy.status")
export const logsState = state<LogEntry[]>([], "proxy.logs")
export const logsStatsState = state<StatsSummaryResult | null>(null, "proxy.logsStats")
export const providerQuotasState = state<Record<string, RuleQuotaSnapshot>>(
  {},
  "proxy.providerQuotas"
)
export const providerCardStatsByProviderKeyState = state<Record<string, RuleCardStatsItem>>(
  {},
  "proxy.providerCardStats"
)
export const quotaLoadingProviderKeysState = state<Record<string, boolean>>(
  {},
  "proxy.quotaLoadingKeys"
)
export const providerModelHealthByProviderKeyState = state<
  Record<string, ProviderModelHealthSnapshot>
>({}, "proxy.providerModelHealth")
export const activeGroupIdState = state<string | null>(
  readPersistedActiveGroupId(),
  "proxy.activeGroupId"
)
export const loadingState = state<boolean>(true, "proxy.loading")
export const errorState = state<string | null>(null, "proxy.error")
export const bootstrappingState = state<boolean>(true, "proxy.bootstrapping")
export const bootstrapErrorState = state<string | null>(null, "proxy.bootstrapError")
export const savingConfigState = state<boolean>(false, "proxy.savingConfig")
export const serverActionState = state<"starting" | "stopping" | null>(null, "proxy.serverAction")
export const statusErrorState = state<string | null>(null, "proxy.statusError")
export const logsErrorState = state<string | null>(null, "proxy.logsError")
export const statsErrorState = state<string | null>(null, "proxy.statsError")
export const quotaErrorState = state<string | null>(null, "proxy.quotaError")
export const lastOperationErrorState = state<string | null>(null, "proxy.lastOperationError")
export const statusIntervalIdState = state<number | null>(null, "proxy.statusIntervalId")

export const isRunningState = computed({
  name: "proxy.isRunning",
  get: get => get(statusState)?.running ?? false,
})

export const activeGroupState = computed({
  name: "proxy.activeGroup",
  get: get => {
    const config = get(configState)
    const activeGroupId = get(activeGroupIdState)
    if (!config || !activeGroupId) return null
    return config.groups.find((group: Group) => group.id === activeGroupId) ?? null
  },
})

export const totalRequestsState = computed({
  name: "proxy.totalRequests",
  get: get => get(statusState)?.metrics.requests ?? 0,
})

export const errorCountState = computed({
  name: "proxy.errorCount",
  get: get => get(statusState)?.metrics.errors ?? 0,
})

export const uptimeState = computed({
  name: "proxy.uptime",
  get: get => {
    const startedAt = get(statusState)?.metrics.uptimeStartedAt
    if (!startedAt) return "Not running"

    const uptime = Date.now() - new Date(startedAt).getTime()
    const seconds = Math.floor(uptime / 1000)
    const minutes = Math.floor(seconds / 60)
    const hours = Math.floor(minutes / 60)

    if (hours > 0) {
      return `${hours}h ${minutes % 60}m`
    }
    if (minutes > 0) {
      return `${minutes}m ${seconds % 60}s`
    }
    return `${seconds}s`
  },
})
