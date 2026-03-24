export type ProviderHealthStatus = "available" | "unavailable"

export const GLOBAL_PROVIDER_TEST_GROUP_ID = "__global__"

export interface ProviderModelHealthSnapshot {
  groupId: string
  providerId: string
  status: ProviderHealthStatus
  latencyMs?: number | null
  resolvedModel?: string | null
  message?: string | null
  testedAt: string
}

export function resolveProviderTestGroupId(groupId?: string | null): string {
  const normalized = groupId?.trim()
  return normalized || GLOBAL_PROVIDER_TEST_GROUP_ID
}

export function createProviderTestKey(
  groupId: string | null | undefined,
  providerId: string
): string {
  return `${resolveProviderTestGroupId(groupId)}:${providerId}`
}

export function buildProviderModelHealthSnapshot(params: {
  groupId: string
  providerId: string
  ok: boolean
  latencyMs?: number | null
  resolvedModel?: string | null
  rawText?: string | null
  message?: string | null
  testedAt?: string
}): ProviderModelHealthSnapshot {
  const testedAt = params.testedAt || new Date().toISOString()
  const resolvedModel = params.resolvedModel?.trim() || params.rawText?.trim() || null
  const message = params.message?.trim() || null

  return {
    groupId: params.groupId,
    providerId: params.providerId,
    status: params.ok ? "available" : "unavailable",
    latencyMs: normalizeLatencyMs(params.latencyMs),
    resolvedModel,
    message,
    testedAt,
  }
}

export function pickLatestProviderModelHealthSnapshot(
  snapshots: Array<ProviderModelHealthSnapshot | null | undefined>
): ProviderModelHealthSnapshot | null {
  let latest: ProviderModelHealthSnapshot | null = null
  let latestTimestamp = -Infinity

  for (const snapshot of snapshots) {
    if (!snapshot) continue
    const timestamp = Date.parse(snapshot.testedAt)
    const safeTimestamp = Number.isFinite(timestamp) ? timestamp : -Infinity
    if (!latest || safeTimestamp >= latestTimestamp) {
      latest = snapshot
      latestTimestamp = safeTimestamp
    }
  }

  return latest
}

export function formatProviderLatency(ms?: number | null): string | null {
  const safe = normalizeLatencyMs(ms)
  if (safe === null) return null
  if (safe < 1000) return `${Math.round(safe)} ms`
  if (safe < 10_000) return `${(safe / 1000).toFixed(1).replace(/\.0$/, "")} s`
  if (safe < 60_000) return `${Math.round(safe / 1000)} s`
  return `${(safe / 60_000).toFixed(1).replace(/\.0$/, "")} min`
}

function normalizeLatencyMs(ms?: number | null): number | null {
  if (ms === null || ms === undefined || !Number.isFinite(ms) || ms < 0) {
    return null
  }
  return ms
}
