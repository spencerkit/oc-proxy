import type React from "react"
import { useEffect, useMemo, useState } from "react"
import { useNavigate } from "react-router-dom"
import { shallow } from "zustand/shallow"
import { Button, Input, Modal } from "@/components"
import { useLogs, useTranslation } from "@/hooks"
import { useProxyStore } from "@/store"
import type { ProxyConfig, RuleCardStatsItem, RuleQuotaSnapshot } from "@/types"
import { ipc } from "@/utils/ipc"
import { RuleList } from "../ServicePage/RuleList"
import styles from "./ProvidersPage.module.css"

const providerQuotaKey = (groupId: string, providerId: string) => `${groupId}:${providerId}`
const QUOTA_REFRESH_MINUTES_DEFAULT = 5
const QUOTA_REFRESH_MINUTES_MIN = 1
const QUOTA_REFRESH_MINUTES_MAX = 1440
const COPY_SUFFIX = "copy"

function normalizeProviderName(value: string): string {
  return value.trim().toLowerCase()
}

function generateCopiedProviderName(
  originalName: string,
  existingNameKeys: Set<string>
): string {
  const baseName = originalName.trim() || "Provider"
  let candidate = `${baseName} ${COPY_SUFFIX}`
  let index = 2
  while (existingNameKeys.has(normalizeProviderName(candidate))) {
    candidate = `${baseName} ${COPY_SUFFIX} ${index}`
    index += 1
  }
  return candidate
}

function generateCopiedProviderId(originalId: string, existingIds: Set<string>): string {
  const baseId = (originalId.trim() || crypto.randomUUID()).replace(/\s+/g, "-")
  let candidate = `${baseId}-${COPY_SUFFIX}`
  let index = 2
  while (existingIds.has(candidate)) {
    candidate = `${baseId}-${COPY_SUFFIX}-${index}`
    index += 1
  }
  return candidate
}

function mergeProviderCardStats(
  providerId: string,
  groupIds: string[],
  providerCardStatsByProviderKey: Record<string, RuleCardStatsItem>
): RuleCardStatsItem | undefined {
  const statsItems = groupIds
    .map(groupId => providerCardStatsByProviderKey[providerQuotaKey(groupId, providerId)])
    .filter((item): item is RuleCardStatsItem => Boolean(item))

  if (statsItems.length === 0) return undefined
  if (statsItems.length === 1) return statsItems[0]

  const hourlyByTime = new Map<
    string,
    {
      hour: string
      requests: number
      inputTokens: number
      outputTokens: number
      tokens: number
    }
  >()

  let requests = 0
  let inputTokens = 0
  let outputTokens = 0
  let cacheReadTokens = 0
  let cacheWriteTokens = 0
  let tokens = 0
  let totalCost = 0

  for (const item of statsItems) {
    requests += item.requests
    inputTokens += item.inputTokens
    outputTokens += item.outputTokens
    cacheReadTokens += item.cacheReadTokens
    cacheWriteTokens += item.cacheWriteTokens
    tokens += item.tokens
    totalCost += item.totalCost

    for (const point of item.hourly) {
      const current = hourlyByTime.get(point.hour) ?? {
        hour: point.hour,
        requests: 0,
        inputTokens: 0,
        outputTokens: 0,
        tokens: 0,
      }
      current.requests += point.requests
      current.inputTokens += point.inputTokens
      current.outputTokens += point.outputTokens
      current.tokens += point.tokens
      hourlyByTime.set(point.hour, current)
    }
  }

  const hourly = [...hourlyByTime.values()].sort(
    (a, b) => new Date(a.hour).getTime() - new Date(b.hour).getTime()
  )

  return {
    groupId: statsItems[0].groupId,
    ruleId: providerId,
    requests,
    inputTokens,
    outputTokens,
    cacheReadTokens,
    cacheWriteTokens,
    tokens,
    totalCost,
    hourly,
  }
}

export const ProvidersPage: React.FC = () => {
  const navigate = useNavigate()
  const { t } = useTranslation()
  const { showToast } = useLogs()
  const {
    config,
    saveConfig,
    providerQuotas,
    quotaLoadingProviderKeys,
    providerCardStatsByProviderKey,
    fetchGroupQuotas,
    fetchGroupProviderCardStats,
    fetchProviderQuota,
  } = useProxyStore(
    state => ({
      config: state.config,
      saveConfig: state.saveConfig,
      providerQuotas: state.providerQuotas,
      quotaLoadingProviderKeys: state.quotaLoadingProviderKeys,
      providerCardStatsByProviderKey: state.providerCardStatsByProviderKey,
      fetchGroupQuotas: state.fetchGroupQuotas,
      fetchGroupProviderCardStats: state.fetchGroupProviderCardStats,
      fetchProviderQuota: state.fetchProviderQuota,
    }),
    shallow
  )

  const [searchValue, setSearchValue] = useState("")
  const [pendingDeleteProviderId, setPendingDeleteProviderId] = useState<string | null>(null)
  const [testingProviderIds, setTestingProviderIds] = useState<Record<string, boolean>>({})

  const providers = config?.providers ?? []
  const associatedGroupIdsByProviderId = useMemo<Record<string, string[]>>(() => {
    if (!config) return {}
    const result: Record<string, string[]> = {}
    for (const group of config.groups) {
      const providerIds = group.providerIds ?? group.providers.map(provider => provider.id)
      for (const providerId of providerIds) {
        if (!providerId) continue
        if (!result[providerId]) {
          result[providerId] = []
        }
        if (!result[providerId].includes(group.id)) {
          result[providerId].push(group.id)
        }
      }
    }
    return result
  }, [config])
  const associatedGroupIds = useMemo(() => {
    return [...new Set(Object.values(associatedGroupIdsByProviderId).flat())]
  }, [associatedGroupIdsByProviderId])
  const quotaRefreshIntervalMs = useMemo(() => {
    const minutesRaw = config?.ui.quotaAutoRefreshMinutes ?? QUOTA_REFRESH_MINUTES_DEFAULT
    const minutes = Math.min(
      QUOTA_REFRESH_MINUTES_MAX,
      Math.max(QUOTA_REFRESH_MINUTES_MIN, minutesRaw)
    )
    return minutes * 60 * 1000
  }, [config?.ui.quotaAutoRefreshMinutes])

  const filteredProviders = useMemo(() => {
    const normalized = searchValue.trim().toLowerCase()
    if (!normalized) return providers
    return providers.filter(provider => {
      return [provider.id, provider.name, provider.apiAddress].some(value =>
        value?.toLowerCase().includes(normalized)
      )
    })
  }, [providers, searchValue])

  const pendingDeleteProvider =
    providers.find(provider => provider.id === pendingDeleteProviderId) ?? null
  const quotaByRuleId = useMemo(() => {
    const result: Record<string, RuleQuotaSnapshot | undefined> = {}
    for (const provider of filteredProviders) {
      const groupIds = associatedGroupIdsByProviderId[provider.id] ?? []
      const snapshot = groupIds
        .map(groupId => providerQuotas[providerQuotaKey(groupId, provider.id)])
        .find(item => Boolean(item))
      if (snapshot) {
        result[provider.id] = snapshot
      }
    }
    return result
  }, [filteredProviders, associatedGroupIdsByProviderId, providerQuotas])
  const quotaLoadingByRuleId = useMemo(() => {
    const result: Record<string, boolean> = {}
    for (const provider of filteredProviders) {
      const groupIds = associatedGroupIdsByProviderId[provider.id] ?? []
      result[provider.id] = groupIds.some(
        groupId => quotaLoadingProviderKeys[providerQuotaKey(groupId, provider.id)]
      )
    }
    return result
  }, [filteredProviders, associatedGroupIdsByProviderId, quotaLoadingProviderKeys])
  const cardStatsByRuleId = useMemo(() => {
    const result: Record<string, RuleCardStatsItem> = {}
    for (const provider of filteredProviders) {
      const groupIds = associatedGroupIdsByProviderId[provider.id] ?? []
      const merged = mergeProviderCardStats(provider.id, groupIds, providerCardStatsByProviderKey)
      if (merged) {
        result[provider.id] = merged
      }
    }
    return result
  }, [filteredProviders, associatedGroupIdsByProviderId, providerCardStatsByProviderKey])

  const affectedGroups = useMemo(() => {
    if (!pendingDeleteProviderId || !config) return []
    return config.groups.filter(group => {
      const providerIds = group.providerIds ?? group.providers.map(provider => provider.id)
      return providerIds.includes(pendingDeleteProviderId)
    })
  }, [config, pendingDeleteProviderId])

  useEffect(() => {
    if (!associatedGroupIds.length) return
    void Promise.all([
      ...associatedGroupIds.map(groupId => fetchGroupQuotas(groupId).catch(() => undefined)),
      ...associatedGroupIds.map(groupId =>
        fetchGroupProviderCardStats(groupId).catch(() => undefined)
      ),
    ])
  }, [associatedGroupIds, fetchGroupProviderCardStats, fetchGroupQuotas])

  useEffect(() => {
    if (!associatedGroupIds.length) return

    const timer = window.setInterval(() => {
      void Promise.all(
        associatedGroupIds.map(groupId => fetchGroupQuotas(groupId).catch(() => undefined))
      )
    }, quotaRefreshIntervalMs)

    return () => window.clearInterval(timer)
  }, [associatedGroupIds, fetchGroupQuotas, quotaRefreshIntervalMs])

  const handleDeleteProvider = async () => {
    if (!config || !pendingDeleteProviderId) return

    const nextProviders = (config.providers ?? []).filter(
      provider => provider.id !== pendingDeleteProviderId
    )

    const nextGroups = config.groups.map(group => {
      const providerIds = (group.providerIds ?? group.providers.map(provider => provider.id)).filter(
        providerId => providerId !== pendingDeleteProviderId
      )
      const activeProviderId =
        group.activeProviderId && providerIds.includes(group.activeProviderId)
          ? group.activeProviderId
          : providerIds[0] ?? null
      return {
        ...group,
        providerIds,
        activeProviderId,
      }
    })

    const nextConfig: ProxyConfig = {
      ...config,
      providers: nextProviders,
      groups: nextGroups,
    }

    try {
      await saveConfig(nextConfig)
      setPendingDeleteProviderId(null)
      showToast(
        t("providersPage.providerDeletedWithImpact", {
          count: affectedGroups.length,
        }),
        "success"
      )
    } catch (error) {
      showToast(t("errors.saveFailed", { message: String(error) }), "error")
    }
  }

  const handleDuplicateProvider = async (providerId: string) => {
    if (!config) return

    const sourceProvider = (config.providers ?? []).find(provider => provider.id === providerId)
    if (!sourceProvider) {
      showToast(t("toast.ruleNotFound"), "error")
      return
    }

    const existingIds = new Set((config.providers ?? []).map(provider => provider.id))
    const existingNameKeys = new Set(
      (config.providers ?? []).map(provider => normalizeProviderName(provider.name))
    )
    const copiedId = generateCopiedProviderId(sourceProvider.id, existingIds)
    const copiedName = generateCopiedProviderName(sourceProvider.name, existingNameKeys)
    const copiedProvider = {
      ...sourceProvider,
      id: copiedId,
      name: copiedName,
      modelMappings: { ...(sourceProvider.modelMappings ?? {}) },
      quota: {
        ...sourceProvider.quota,
        customHeaders: { ...(sourceProvider.quota.customHeaders ?? {}) },
        response: { ...(sourceProvider.quota.response ?? {}) },
      },
      cost: sourceProvider.cost ? { ...sourceProvider.cost } : sourceProvider.cost,
    }

    try {
      await saveConfig({
        ...config,
        providers: [...(config.providers ?? []), copiedProvider],
      })
      showToast(t("providersPage.providerDuplicated", { name: copiedName }), "success")
    } catch (error) {
      showToast(t("errors.saveFailed", { message: String(error) }), "error")
    }
  }

  const handleTestProviderModel = async (providerId: string) => {
    if (!config) return
    if (testingProviderIds[providerId]) return

    const provider = providers.find(item => item.id === providerId)
    if (!provider) {
      showToast(t("toast.ruleNotFound"), "error")
      return
    }

    const targetGroup = config.groups.find(group => {
      if (group.providers.some(item => item.id === providerId)) return true
      const providerIds = group.providerIds ?? group.providers.map(item => item.id)
      return providerIds.includes(providerId)
    })

    if (!targetGroup) {
      showToast(
        t("toast.providerModelTestFailed", {
          provider: provider.name,
          message: t("providersPage.providerTestRequiresAssociation"),
        }),
        "error"
      )
      return
    }

    setTestingProviderIds(prev => ({ ...prev, [providerId]: true }))
    try {
      const result = await ipc.testProviderModel(targetGroup.id, providerId)
      if (!result.ok) {
        showToast(
          t("toast.providerModelTestFailed", {
            provider: provider.name,
            message:
              result.message?.trim() || t("errors.operationFailed", { message: provider.name }),
          }),
          "error"
        )
        return
      }

      const modelName =
        result.resolvedModel?.trim() ||
        result.rawText?.trim() ||
        provider.defaultModel.trim() ||
        provider.name

      showToast(
        t("toast.providerModelTestSuccess", {
          provider: provider.name,
          model: modelName,
        }),
        "success"
      )
    } catch (error) {
      showToast(
        t("toast.providerModelTestFailed", {
          provider: provider.name,
          message: String(error),
        }),
        "error"
      )
    } finally {
      setTestingProviderIds(prev => {
        const next = { ...prev }
        delete next[providerId]
        return next
      })
    }
  }

  const handleRefreshProviderQuota = async (providerId: string) => {
    const groupId = associatedGroupIdsByProviderId[providerId]?.[0]
    if (!groupId) {
      showToast(t("providersPage.providerTestRequiresAssociation"), "error")
      return
    }

    try {
      await fetchProviderQuota(groupId, providerId)
    } catch (error) {
      showToast(t("errors.operationFailed", { message: String(error) }), "error")
    }
  }

  return (
    <div className={styles.providersPage}>
      <div className="app-top-header">
        <div className="app-top-header-main">
          <h2 className="app-top-header-title">{t("providersPage.title")}</h2>
          <p className="app-top-header-subtitle">{t("providersPage.subtitle")}</p>
        </div>
        <div className="app-top-header-actions">
          <Button variant="primary" onClick={() => navigate("/providers/new")}>
            {t("providersPage.addProvider")}
          </Button>
        </div>
      </div>

      <div className={styles.searchBox}>
        <Input
          value={searchValue}
          onChange={event => setSearchValue(event.target.value)}
          placeholder={t("providersPage.searchPlaceholder")}
          fullWidth
        />
      </div>

      <RuleList
        providers={filteredProviders}
        activeProviderId={null}
        onActivate={() => {}}
        quotaByRuleId={quotaByRuleId}
        quotaLoadingByRuleId={quotaLoadingByRuleId}
        cardStatsByRuleId={cardStatsByRuleId}
        onRefreshQuota={handleRefreshProviderQuota}
        onTestModel={handleTestProviderModel}
        testingProviderIds={testingProviderIds}
        onDuplicate={providerId => void handleDuplicateProvider(providerId)}
        onDelete={providerId => setPendingDeleteProviderId(providerId)}
        onAdd={() => navigate("/providers/new")}
        onEdit={providerId => navigate(`/providers/${providerId}/edit`)}
        showActivate={false}
        addButtonTitle={t("providersPage.addProvider")}
        deleteActionLabel={t("providersPage.deleteProvider")}
        emptyMessage={t("providersPage.empty")}
        displayMode="catalog"
      />

      <Modal
        open={Boolean(pendingDeleteProvider)}
        onClose={() => setPendingDeleteProviderId(null)}
        title={t("providersPage.deleteModalTitle")}
      >
        {!pendingDeleteProvider ? null : (
          <div className={styles.deleteModalContent}>
            <p>
              {t("providersPage.deleteModalMessage", {
                name: pendingDeleteProvider.name,
                count: affectedGroups.length,
              })}
            </p>
            {affectedGroups.length > 0 ? (
              <ul className={styles.affectedGroupList}>
                {affectedGroups.map(group => (
                  <li key={group.id}>
                    {group.name} <code>/{group.id}</code>
                  </li>
                ))}
              </ul>
            ) : null}
            <div className={styles.modalActions}>
              <Button variant="default" onClick={() => setPendingDeleteProviderId(null)}>
                {t("common.cancel")}
              </Button>
              <Button variant="danger" onClick={() => void handleDeleteProvider()}>
                {t("providersPage.confirmDelete")}
              </Button>
            </div>
          </div>
        )}
      </Modal>
    </div>
  )
}

export default ProvidersPage
