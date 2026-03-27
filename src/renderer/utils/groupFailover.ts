import type { GroupFailoverConfig } from "@/types"

const DEFAULT_GROUP_FAILOVER_CONFIG: GroupFailoverConfig = {
  enabled: false,
  failureThreshold: 3,
  cooldownSeconds: 300,
}

export function normalizeGroupFailoverConfig(
  failover?: Partial<GroupFailoverConfig> | null
): GroupFailoverConfig {
  return {
    ...DEFAULT_GROUP_FAILOVER_CONFIG,
    ...(failover ?? {}),
  }
}
