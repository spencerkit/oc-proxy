import { Copy, Pencil, Plus, Trash2, Upload } from "lucide-react"
import React, { useEffect, useMemo, useState } from "react"
import { useNavigate } from "react-router-dom"
import { shallow } from "zustand/shallow"
import { Button, Input, Modal } from "@/components"
import { useLogs, useTranslation } from "@/hooks"
import { useProxyStore } from "@/store"
import type { Group, IntegrationClientKind, IntegrationTarget, ProxyConfig } from "@/types"
import { ipc } from "@/utils/ipc"
import { resolveReachableServerBaseUrls } from "@/utils/serverAddress"
import { RuleList } from "./RuleList"
import styles from "./ServicePage.module.css"

/** Matches search text against a list of candidate strings. */
function matchesKeyword(keyword: string, values: Array<string | null | undefined>): boolean {
  const normalized = keyword.trim().toLowerCase()
  if (!normalized) return true
  return values.some(value => value?.toLowerCase().includes(normalized))
}

/**
 * ServicePage Component
 * Main page for managing proxy groups and providers
 */
export const ServicePage: React.FC = () => {
  const navigate = useNavigate()
  const { t } = useTranslation()
  const {
    config,
    saveConfig,
    status,
    activeGroupId,
    setActiveGroupId,
    providerQuotas,
    providerCardStatsByProviderKey,
    quotaLoadingProviderKeys,
    quotaError,
    statsError,
    fetchGroupQuotas,
    fetchGroupProviderCardStats,
    fetchProviderQuota,
  } = useProxyStore(
    state => ({
      config: state.config,
      saveConfig: state.saveConfig,
      status: state.status,
      activeGroupId: state.activeGroupId,
      setActiveGroupId: state.setActiveGroupId,
      providerQuotas: state.providerQuotas,
      providerCardStatsByProviderKey: state.providerCardStatsByProviderKey,
      quotaLoadingProviderKeys: state.quotaLoadingProviderKeys,
      quotaError: state.quotaError,
      statsError: state.statsError,
      fetchGroupQuotas: state.fetchGroupQuotas,
      fetchGroupProviderCardStats: state.fetchGroupProviderCardStats,
      fetchProviderQuota: state.fetchProviderQuota,
    }),
    shallow
  )
  const { showToast } = useLogs()
  const [groupSearchValue, setGroupSearchValue] = useState("")
  const [showAddGroupModal, setShowAddGroupModal] = useState(false)
  const [showDeleteGroupModal, setShowDeleteGroupModal] = useState(false)
  const [showDeleteProviderModal, setShowDeleteProviderModal] = useState(false)
  const [pendingDeleteProviderId, setPendingDeleteProviderId] = useState<string | null>(null)
  const [activatingProviderId, setActivatingProviderId] = useState<string | null>(null)
  const [testingProviderIds, setTestingProviderIds] = useState<Record<string, boolean>>({})
  const [showIntegrationWriteModal, setShowIntegrationWriteModal] = useState(false)
  const [integrationTargets, setIntegrationTargets] = useState<IntegrationTarget[]>([])
  const [integrationLoading, setIntegrationLoading] = useState(false)
  const [integrationWriting, setIntegrationWriting] = useState(false)
  const [integrationAddingKind, setIntegrationAddingKind] = useState<IntegrationClientKind | null>(
    null
  )
  const [integrationUpdatingTargetId, setIntegrationUpdatingTargetId] = useState<string | null>(
    null
  )
  const [selectedIntegrationIds, setSelectedIntegrationIds] = useState<Record<string, boolean>>({})
  const [newGroupName, setNewGroupName] = useState("")
  const [newGroupId, setNewGroupId] = useState("")
  const providerCardStatsHours = 24
  const providerCardStatsRefreshIntervalMs = 10_000

  const groups = config?.groups ?? []
  const filteredGroups = useMemo(() => {
    return groups.filter(group =>
      matchesKeyword(groupSearchValue, [group.name, group.id, `/${group.id}`])
    )
  }, [groupSearchValue, groups])
  const activeGroup = groups.find(group => group.id === activeGroupId) ?? null
  const activeGroupModels = Array.isArray(activeGroup?.models) ? activeGroup.models : []
  const pendingDeleteProvider =
    activeGroup?.providers.find(item => item.id === pendingDeleteProviderId) ?? null
  const integrationSections = useMemo(
    () => [
      {
        kind: "claude" as const,
        title: t("integration.claude.title"),
        hint: t("integration.claude.hint"),
      },
      {
        kind: "codex" as const,
        title: t("integration.codex.title"),
        hint: t("integration.codex.hint"),
      },
      {
        kind: "opencode" as const,
        title: t("integration.opencode.title"),
        hint: t("integration.opencode.hint"),
      },
    ],
    [t]
  )
  const selectedIntegrationTargetIds = useMemo(() => {
    return Object.entries(selectedIntegrationIds)
      .filter(([, checked]) => checked)
      .map(([id]) => id)
  }, [selectedIntegrationIds])
  const integrationTargetsByKind = useMemo<Record<IntegrationClientKind, IntegrationTarget[]>>(
    () => ({
      claude: integrationTargets.filter(item => item.kind === "claude"),
      codex: integrationTargets.filter(item => item.kind === "codex"),
      opencode: integrationTargets.filter(item => item.kind === "opencode"),
    }),
    [integrationTargets]
  )
  const quotaAutoRefreshIntervalMs = React.useMemo(() => {
    const minutes = config?.ui?.quotaAutoRefreshMinutes
    if (!Number.isInteger(minutes) || !minutes || minutes < 1 || minutes > 1440) {
      return 5 * 60 * 1000
    }
    return minutes * 60 * 1000
  }, [config?.ui?.quotaAutoRefreshMinutes])

  useEffect(() => {
    if (groups.length === 0) {
      if (activeGroupId !== null) {
        setActiveGroupId(null)
      }
      return
    }

    const activeGroupExists = groups.some(group => group.id === activeGroupId)
    if (!activeGroupExists) {
      setActiveGroupId(groups[0].id)
    }
  }, [groups, activeGroupId, setActiveGroupId])

  useEffect(() => {
    if (!activeGroupId) return
    void fetchGroupQuotas(activeGroupId)
    void fetchGroupProviderCardStats(activeGroupId, providerCardStatsHours)
  }, [activeGroupId, fetchGroupProviderCardStats, fetchGroupQuotas])

  useEffect(() => {
    if (!activeGroupId) return
    const timer = window.setInterval(() => {
      if (document.visibilityState !== "visible") return
      void fetchGroupQuotas(activeGroupId)
    }, quotaAutoRefreshIntervalMs)
    return () => window.clearInterval(timer)
  }, [activeGroupId, fetchGroupQuotas, quotaAutoRefreshIntervalMs])

  useEffect(() => {
    if (!activeGroupId) return
    const timer = window.setInterval(() => {
      if (document.visibilityState !== "visible") return
      void fetchGroupProviderCardStats(activeGroupId, providerCardStatsHours)
    }, providerCardStatsRefreshIntervalMs)
    return () => window.clearInterval(timer)
  }, [activeGroupId, fetchGroupProviderCardStats])

  const handleSelectGroup = (groupId: string) => {
    setActiveGroupId(groupId)
    setShowDeleteProviderModal(false)
    setPendingDeleteProviderId(null)
  }

  const openAddGroupModal = () => {
    setNewGroupName("")
    setNewGroupId("")
    setShowAddGroupModal(true)
  }

  const closeAddGroupModal = () => {
    setShowAddGroupModal(false)
    setNewGroupName("")
    setNewGroupId("")
  }

  const handleAddGroup = async () => {
    if (!newGroupName.trim() || !newGroupId.trim() || !config) return
    const normalizedId = newGroupId.trim().replace(/^\/+/, "")
    if (!/^[a-zA-Z0-9_-]+$/.test(normalizedId)) {
      showToast(t("validation.invalidFormat", { field: t("modal.groupIdLabel") }), "error")
      return
    }
    if ((config.groups || []).some(group => group.id === normalizedId)) {
      showToast(t("validation.alreadyExists", { field: t("modal.groupIdLabel") }), "error")
      return
    }

    const newGroup: Group = {
      id: normalizedId,
      name: newGroupName.trim(),
      models: [],
      activeProviderId: null,
      providers: [],
    }

    const newConfig: ProxyConfig = {
      ...config,
      groups: [...(config.groups ?? []), newGroup],
    }

    try {
      await saveConfig(newConfig)
      closeAddGroupModal()
      setActiveGroupId(newGroup.id)
      showToast(t("toast.groupCreated"), "success")
    } catch (error) {
      showToast(t("errors.saveFailed", { message: String(error) }), "error")
    }
  }

  const handleDeleteGroup = async () => {
    if (!activeGroupId || !config) return

    const newGroups = config.groups.filter(group => group.id !== activeGroupId)
    const newConfig = { ...config, groups: newGroups }

    try {
      await saveConfig(newConfig)
      setActiveGroupId(newGroups.length > 0 ? newGroups[0].id : null)
      setShowDeleteGroupModal(false)
      showToast(t("toast.groupDeleted"), "success")
    } catch (error) {
      showToast(t("errors.saveFailed", { message: String(error) }), "error")
    }
  }

  const handleRequestDeleteProvider = (providerId: string) => {
    setPendingDeleteProviderId(providerId)
    setShowDeleteProviderModal(true)
  }

  const handleActivateProvider = async (providerId: string) => {
    if (!activeGroupId || !config) return
    if (activeGroup?.activeProviderId === providerId) return

    setActivatingProviderId(providerId)
    try {
      const newGroups = config.groups.map(group => {
        if (group.id !== activeGroupId) {
          return group
        }
        return {
          ...group,
          activeProviderId: providerId,
        }
      })

      await saveConfig({
        ...config,
        groups: newGroups,
      })
      showToast(t("toast.ruleSwitched"), "success")
    } catch (error) {
      showToast(t("errors.saveFailed", { message: String(error) }), "error")
    } finally {
      setActivatingProviderId(null)
    }
  }

  const handleDeleteProvider = async () => {
    if (!activeGroupId || !config || !pendingDeleteProviderId) return

    const newGroups = config.groups.map(group => {
      if (group.id === activeGroupId) {
        const newProviders = group.providers.filter(item => item.id !== pendingDeleteProviderId)
        const newActiveId =
          group.activeProviderId === pendingDeleteProviderId
            ? newProviders.length > 0
              ? newProviders[0].id
              : null
            : group.activeProviderId
        return {
          ...group,
          providers: newProviders,
          activeProviderId: newActiveId,
        }
      }
      return group
    })

    const newConfig = { ...config, groups: newGroups }
    try {
      await saveConfig(newConfig)
      setShowDeleteProviderModal(false)
      setPendingDeleteProviderId(null)
      showToast(t("toast.ruleDeleted"), "success")
    } catch (error) {
      showToast(t("errors.saveFailed", { message: String(error) }), "error")
    }
  }

  const handleRefreshProviderQuota = async (providerId: string) => {
    if (!activeGroupId) return
    try {
      await fetchProviderQuota(activeGroupId, providerId)
    } catch (error) {
      showToast(t("errors.operationFailed", { message: String(error) }), "error")
    }
  }

  const handleTestProviderModel = async (providerId: string) => {
    if (!activeGroupId || !activeGroup) return
    if (testingProviderIds[providerId]) return

    const provider = activeGroup.providers.find(item => item.id === providerId)
    if (!provider) {
      showToast(t("toast.ruleNotFound"), "error")
      return
    }

    setTestingProviderIds(prev => ({ ...prev, [providerId]: true }))
    try {
      const result = await ipc.testProviderModel(activeGroupId, providerId)
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

  const handleCopyEntryUrl = async (url: string) => {
    if (!url) return

    try {
      await navigator.clipboard.writeText(url)
      showToast(t("toast.entryUrlCopied"), "success")
    } catch {
      showToast(t("toast.copyFailed"), "error")
    }
  }

  const getIntegrationClientLabel = (kind: IntegrationClientKind): string => {
    switch (kind) {
      case "claude":
        return t("integration.claude.title")
      case "codex":
        return t("integration.codex.title")
      default:
        return t("integration.opencode.title")
    }
  }

  const loadIntegrationTargets = React.useCallback(async () => {
    setIntegrationLoading(true)
    try {
      const targets = await ipc.integrationListTargets()
      setIntegrationTargets(targets)
    } catch (error) {
      showToast(t("errors.operationFailed", { message: String(error) }), "error")
    } finally {
      setIntegrationLoading(false)
    }
  }, [showToast, t])

  const openIntegrationWriteModal = async () => {
    if (!activeGroup) return
    setShowIntegrationWriteModal(true)
    setSelectedIntegrationIds({})
    await loadIntegrationTargets()
  }

  const closeIntegrationWriteModal = () => {
    if (integrationWriting) return
    setShowIntegrationWriteModal(false)
    setSelectedIntegrationIds({})
  }

  const handleAddIntegrationTarget = async (kind: IntegrationClientKind) => {
    setIntegrationAddingKind(kind)
    try {
      const pickedDir = await ipc.integrationPickDirectory(undefined, kind)
      if (!pickedDir) return
      const created = await ipc.integrationAddTarget(kind, pickedDir)
      setIntegrationTargets(prev => [...prev, created])
      setSelectedIntegrationIds(prev => ({ ...prev, [created.id]: true }))
      showToast(
        t("integration.toastTargetAdded", {
          client: getIntegrationClientLabel(kind),
        }),
        "success"
      )
    } catch (error) {
      showToast(t("errors.operationFailed", { message: String(error) }), "error")
    } finally {
      setIntegrationAddingKind(null)
    }
  }

  const handleUpdateIntegrationTarget = async (target: IntegrationTarget) => {
    setIntegrationUpdatingTargetId(target.id)
    try {
      const pickedDir = await ipc.integrationPickDirectory(target.configDir)
      if (!pickedDir) return
      const updated = await ipc.integrationUpdateTarget(target.id, pickedDir)
      setIntegrationTargets(prev =>
        prev.map(item => {
          if (item.id !== updated.id) return item
          return updated
        })
      )
      setSelectedIntegrationIds(prev => ({ ...prev, [updated.id]: true }))
      showToast(
        t("integration.toastTargetUpdated", {
          client: getIntegrationClientLabel(target.kind),
        }),
        "success"
      )
    } catch (error) {
      showToast(t("errors.operationFailed", { message: String(error) }), "error")
    } finally {
      setIntegrationUpdatingTargetId(null)
    }
  }

  const handleToggleIntegrationTarget = (targetId: string, checked: boolean) => {
    setSelectedIntegrationIds(prev => ({
      ...prev,
      [targetId]: checked,
    }))
  }

  const handleWriteIntegrationTargets = async () => {
    if (!activeGroup) return
    if (selectedIntegrationTargetIds.length === 0) return

    setIntegrationWriting(true)
    try {
      const result = await ipc.integrationWriteGroupEntry(
        activeGroup.id,
        selectedIntegrationTargetIds
      )
      if (result.failed === 0) {
        showToast(
          t("integration.toastWriteSuccess", {
            count: result.succeeded,
          }),
          "success"
        )
        setShowIntegrationWriteModal(false)
        setSelectedIntegrationIds({})
        return
      }
      const firstError = result.items.find(item => !item.ok)?.message ?? t("errors.unknownError")
      showToast(
        t("integration.toastWritePartial", {
          succeeded: result.succeeded,
          failed: result.failed,
          message: firstError,
        }),
        "error"
      )
    } catch (error) {
      showToast(t("errors.operationFailed", { message: String(error) }), "error")
    } finally {
      setIntegrationWriting(false)
    }
  }

  const buildWriteTargetFilePath = (target: IntegrationTarget): string => {
    const withSlash = target.configDir.endsWith("/") || target.configDir.endsWith("\\")
    switch (target.kind) {
      case "claude":
        return `${target.configDir}${withSlash ? "" : "/"}settings.json`
      case "codex":
        return `${target.configDir}${withSlash ? "" : "/"}config.toml`
      default:
        return `${target.configDir}${withSlash ? "" : "/"}opencode.json(c)`
    }
  }

  const getWriteTargetField = (kind: IntegrationClientKind): string => {
    switch (kind) {
      case "claude":
        return "env.ANTHROPIC_BASE_URL"
      case "codex":
        return "model_providers.aor_shared.base_url"
      default:
        return "provider.aor_shared.options.baseURL"
    }
  }

  const serverBaseUrls = React.useMemo(() => {
    return resolveReachableServerBaseUrls({
      statusAddress: status?.address,
      statusLanAddress: status?.lanAddress,
      configHost: config?.server.host,
      configPort: config?.server.port,
    })
  }, [status?.address, status?.lanAddress, config?.server.host, config?.server.port])

  const entryUrls = React.useMemo(() => {
    if (!activeGroup) return []
    return serverBaseUrls.map(baseUrl => `${baseUrl}/oc/${activeGroup.id}`)
  }, [activeGroup, serverBaseUrls])
  const preferredEntryUrl = React.useMemo(() => {
    return entryUrls.find(url => !url.includes("://localhost")) ?? entryUrls[0] ?? ""
  }, [entryUrls])

  const activeGroupQuotaByProviderId = React.useMemo(() => {
    const map: Record<string, (typeof providerQuotas)[string] | undefined> = {}
    if (!activeGroup) return map
    for (const provider of activeGroup.providers) {
      map[provider.id] = providerQuotas[`${activeGroup.id}:${provider.id}`]
    }
    return map
  }, [activeGroup, providerQuotas])

  const activeGroupQuotaLoadingByProviderId = React.useMemo(() => {
    const map: Record<string, boolean> = {}
    if (!activeGroup) return map
    for (const provider of activeGroup.providers) {
      map[provider.id] = !!quotaLoadingProviderKeys[`${activeGroup.id}:${provider.id}`]
    }
    return map
  }, [activeGroup, quotaLoadingProviderKeys])

  const activeGroupProviderCardStatsByProviderId = React.useMemo(() => {
    const map: Record<string, (typeof providerCardStatsByProviderKey)[string] | undefined> = {}
    if (!activeGroup) return map
    for (const provider of activeGroup.providers) {
      map[provider.id] = providerCardStatsByProviderKey[`${activeGroup.id}:${provider.id}`]
    }
    return map
  }, [activeGroup, providerCardStatsByProviderKey])

  return (
    <div className={styles.servicePage}>
      <div className={styles.sidebar}>
        <div className={styles.groupList}>
          <div className={styles.groupListHeader}>
            <div className={styles.groupHeaderTitle}>
              <h3>{t("servicePage.groupInfo")}</h3>
              <span className={styles.countBadge}>{groups.length}</span>
            </div>
            <Button
              variant="ghost"
              size="small"
              icon={Plus}
              onClick={openAddGroupModal}
              title={t("header.addGroup")}
              aria-label={t("header.addGroup")}
            />
          </div>
          <div className={styles.groupSearchBox}>
            <Input
              value={groupSearchValue}
              onChange={event => setGroupSearchValue(event.target.value)}
              placeholder={t("servicePage.searchGroups")}
              size="small"
              fullWidth
            />
          </div>
          <div className={styles.groupListContent}>
            {groups.length === 0 ? (
              <div className={styles.emptyHint}>
                <p>{t("servicePage.noGroupsHint")}</p>
                <Button variant="primary" size="small" icon={Plus} onClick={openAddGroupModal}>
                  {t("servicePage.createFirstGroup")}
                </Button>
              </div>
            ) : filteredGroups.length === 0 ? (
              <div className={styles.emptyHint}>
                <p>{t("servicePage.noGroupMatch")}</p>
              </div>
            ) : (
              <ul className={styles.groupItems}>
                {filteredGroups.map(group => (
                  <li key={group.id}>
                    <button
                      type="button"
                      className={`${styles.groupItem} ${group.id === activeGroupId ? styles.active : ""}`}
                      onClick={() => handleSelectGroup(group.id)}
                    >
                      <span className={styles.groupName}>{group.name}</span>
                      <span className={styles.groupPath}>/{group.id}</span>
                      <span className={styles.groupRuleCount}>{group.providers.length}</span>
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </div>
        </div>
      </div>

      <div className={styles.mainContent}>
        {groups.length === 0 ? (
          <div className={styles.guideCard}>
            <h2>{t("servicePage.firstRunTitle")}</h2>
            <p className={styles.guideSubtitle}>{t("servicePage.firstRunSubtitle")}</p>
            <div className={styles.guideSteps}>
              <div className={styles.guideStep}>{t("servicePage.firstRunStepGroup")}</div>
              <div className={styles.guideStep}>{t("servicePage.firstRunStepProvider")}</div>
              <div className={styles.guideStep}>{t("servicePage.firstRunStepRoute")}</div>
            </div>
            <div className={styles.guideActions}>
              <Button variant="primary" icon={Plus} onClick={openAddGroupModal}>
                {t("servicePage.createFirstGroup")}
              </Button>
            </div>
          </div>
        ) : !activeGroup ? (
          <div className={styles.noSelection}>
            <p>{t("servicePage.noGroupSelected")}</p>
          </div>
        ) : (
          <>
            <div className={styles.groupHeader}>
              <div className={styles.groupInfo}>
                <h2>{activeGroup.name}</h2>
                <div className={styles.groupMeta}>
                  <span className={styles.metaChip}>/{activeGroup.id}</span>
                  <span className={styles.metaChip}>
                    {t("servicePage.rulesCount", { count: activeGroup.providers.length })}
                  </span>
                  <span className={styles.metaChip}>
                    {t("servicePage.modelsCount", { count: activeGroupModels.length })}
                  </span>
                </div>
                <div className={styles.entryUrl}>
                  <div className={styles.entryUrlList}>
                    {entryUrls.map(url => (
                      <div key={url} className={styles.entryUrlItem}>
                        <code>{url}</code>
                        <Button
                          variant="ghost"
                          size="small"
                          icon={Copy}
                          className={styles.entryUrlCopyButton}
                          onClick={() => handleCopyEntryUrl(url)}
                          title={t("servicePage.copyEntryUrl")}
                          aria-label={`${t("servicePage.copyEntryUrl")}: ${url}`}
                        />
                      </div>
                    ))}
                  </div>
                </div>
              </div>
              <div className={styles.groupActions}>
                <Button
                  variant="default"
                  size="small"
                  icon={Upload}
                  onClick={() => {
                    void openIntegrationWriteModal()
                  }}
                  title={t("integration.openWrite")}
                  aria-label={t("integration.openWrite")}
                />
                <Button
                  variant="default"
                  size="small"
                  icon={Pencil}
                  onClick={() => navigate(`/groups/${activeGroup.id}/edit`)}
                  title={t("servicePage.editGroup")}
                  aria-label={t("servicePage.editGroup")}
                />
                <Button
                  variant="danger"
                  size="small"
                  icon={Trash2}
                  onClick={() => setShowDeleteGroupModal(true)}
                  title={t("servicePage.deleteGroup")}
                  aria-label={t("servicePage.deleteGroup")}
                />
              </div>
            </div>

            {(quotaError || statsError) && (
              <div className={styles.noticeBar}>
                <span>{quotaError || statsError}</span>
              </div>
            )}

            <RuleList
              providers={activeGroup.providers}
              activeProviderId={activeGroup.activeProviderId}
              onActivate={handleActivateProvider}
              activatingProviderId={activatingProviderId}
              quotaByRuleId={activeGroupQuotaByProviderId}
              quotaLoadingByRuleId={activeGroupQuotaLoadingByProviderId}
              cardStatsByRuleId={activeGroupProviderCardStatsByProviderId}
              onRefreshQuota={handleRefreshProviderQuota}
              onTestModel={handleTestProviderModel}
              testingProviderIds={testingProviderIds}
              onDelete={handleRequestDeleteProvider}
              groupName={activeGroup.name}
              groupId={activeGroup.id}
              emptyMessage={t("servicePage.noRulesHint")}
            />
          </>
        )}
      </div>

      <Modal
        open={showIntegrationWriteModal}
        onClose={closeIntegrationWriteModal}
        title={t("integration.modalTitle")}
        size="large"
      >
        <div className={styles.integrationModalContent}>
          <p className={styles.integrationModalHint}>
            {t("integration.modalHint", {
              path: activeGroup ? `/oc/${activeGroup.id}` : "/oc/group-id",
            })}
          </p>
          <p className={styles.integrationHiddenHint}>{t("integration.hiddenDirHint")}</p>
          {preferredEntryUrl ? (
            <div className={styles.integrationEntryUrl}>
              <code>{preferredEntryUrl}</code>
              <Button
                variant="ghost"
                size="small"
                icon={Copy}
                onClick={() => {
                  void handleCopyEntryUrl(preferredEntryUrl)
                }}
                title={t("servicePage.copyEntryUrl")}
                aria-label={t("servicePage.copyEntryUrl")}
              />
            </div>
          ) : null}

          {integrationLoading ? (
            <div className={styles.integrationLoading}>{t("common.loading")}</div>
          ) : (
            <div className={styles.integrationSectionList}>
              {integrationSections.map(section => {
                const targets = integrationTargetsByKind[section.kind]
                return (
                  <section key={section.kind} className={styles.integrationSection}>
                    <div className={styles.integrationSectionHeader}>
                      <div className={styles.integrationSectionTitleWrap}>
                        <h4>{section.title}</h4>
                        <p>{section.hint}</p>
                      </div>
                      <Button
                        variant="ghost"
                        size="small"
                        icon={Plus}
                        loading={integrationAddingKind === section.kind}
                        disabled={
                          integrationWriting ||
                          integrationUpdatingTargetId !== null ||
                          (integrationAddingKind !== null && integrationAddingKind !== section.kind)
                        }
                        onClick={() => {
                          void handleAddIntegrationTarget(section.kind)
                        }}
                      >
                        {t("integration.addConfig")}
                      </Button>
                    </div>

                    {targets.length === 0 ? (
                      <div className={styles.integrationEmpty}>{t("integration.emptyTargets")}</div>
                    ) : (
                      <ul className={styles.integrationTargetList}>
                        {targets.map(target => (
                          <li key={target.id} className={styles.integrationTargetItem}>
                            <label className={styles.integrationTargetLabel}>
                              <input
                                type="checkbox"
                                checked={!!selectedIntegrationIds[target.id]}
                                onChange={event =>
                                  handleToggleIntegrationTarget(target.id, event.target.checked)
                                }
                                disabled={
                                  integrationWriting || integrationUpdatingTargetId !== null
                                }
                              />
                              <span className={styles.integrationTargetPathWrap}>
                                <span className={styles.integrationTargetPath}>
                                  {target.configDir}
                                </span>
                                <span className={styles.integrationTargetWriteDetail}>
                                  {t("integration.writeToDetail", {
                                    filePath: buildWriteTargetFilePath(target),
                                    fieldPath: getWriteTargetField(target.kind),
                                  })}
                                </span>
                              </span>
                            </label>
                            <Button
                              variant="default"
                              size="small"
                              loading={integrationUpdatingTargetId === target.id}
                              disabled={integrationWriting || integrationUpdatingTargetId !== null}
                              onClick={() => {
                                void handleUpdateIntegrationTarget(target)
                              }}
                            >
                              {t("integration.changeConfigDirectory")}
                            </Button>
                          </li>
                        ))}
                      </ul>
                    )}
                  </section>
                )
              })}
            </div>
          )}

          <div className={styles.integrationModalActions}>
            <span className={styles.integrationSelectedCount}>
              {t("integration.selectedCount", { count: selectedIntegrationTargetIds.length })}
            </span>
            <div className={styles.integrationModalActionsRight}>
              <Button
                variant="default"
                onClick={closeIntegrationWriteModal}
                disabled={integrationWriting}
              >
                {t("common.cancel")}
              </Button>
              <Button
                variant="primary"
                onClick={() => {
                  void handleWriteIntegrationTargets()
                }}
                loading={integrationWriting}
                disabled={
                  selectedIntegrationTargetIds.length === 0 ||
                  integrationUpdatingTargetId !== null ||
                  integrationAddingKind !== null
                }
              >
                {t("integration.confirmWrite")}
              </Button>
            </div>
          </div>
        </div>
      </Modal>

      <Modal open={showAddGroupModal} onClose={closeAddGroupModal} title={t("modal.addGroupTitle")}>
        <div className={styles.modalContent}>
          <div className={styles.formGroup}>
            <label htmlFor="groupId">{t("modal.groupIdLabel")}</label>
            <Input
              id="groupId"
              value={newGroupId}
              onChange={e => setNewGroupId(e.target.value)}
              placeholder={t("modal.groupIdPlaceholder")}
            />
            <p className={styles.formHint}>
              {t("modal.groupIdHint", { id: newGroupId.trim() || "group-id" })}
            </p>
          </div>
          <div className={styles.formGroup}>
            <label htmlFor="groupName">{t("modal.groupNameLabel")}</label>
            <Input
              id="groupName"
              value={newGroupName}
              onChange={e => setNewGroupName(e.target.value)}
              placeholder={t("modal.groupNamePlaceholder")}
            />
            <p className={styles.formHint}>{t("modal.groupNameHint")}</p>
          </div>
          <div className={styles.modalActions}>
            <Button variant="default" onClick={closeAddGroupModal}>
              {t("common.cancel")}
            </Button>
            <Button
              variant="primary"
              onClick={handleAddGroup}
              disabled={!newGroupName.trim() || !newGroupId.trim()}
            >
              {t("modal.create")}
            </Button>
          </div>
        </div>
      </Modal>

      <Modal
        open={showDeleteGroupModal}
        onClose={() => setShowDeleteGroupModal(false)}
        title={t("deleteGroupModal.title")}
      >
        <div className={styles.modalContent}>
          <p>
            {t("deleteGroupModal.confirmText", {
              name: activeGroup?.name,
              path: activeGroup?.id,
            })}
          </p>
          <div className={styles.modalActions}>
            <Button variant="default" onClick={() => setShowDeleteGroupModal(false)}>
              {t("common.cancel")}
            </Button>
            <Button variant="danger" onClick={handleDeleteGroup}>
              {t("deleteGroupModal.confirmDelete")}
            </Button>
          </div>
        </div>
      </Modal>

      <Modal
        open={showDeleteProviderModal}
        onClose={() => {
          setShowDeleteProviderModal(false)
          setPendingDeleteProviderId(null)
        }}
        title={t("deleteRuleModal.title")}
      >
        <div className={styles.modalContent}>
          <p>
            {t("deleteRuleModal.confirmText", {
              model: pendingDeleteProvider?.name ?? "",
            })}
          </p>
          <div className={styles.modalActions}>
            <Button
              variant="default"
              onClick={() => {
                setShowDeleteProviderModal(false)
                setPendingDeleteProviderId(null)
              }}
            >
              {t("common.cancel")}
            </Button>
            <Button variant="danger" onClick={handleDeleteProvider}>
              {t("deleteRuleModal.confirmDelete")}
            </Button>
          </div>
        </div>
      </Modal>
    </div>
  )
}

export default ServicePage
