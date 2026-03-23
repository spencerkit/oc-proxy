import { Copy, Pencil, Plus, Trash2, Upload } from "lucide-react"
import React, { useEffect, useMemo, useState } from "react"
import { useNavigate } from "react-router-dom"
import { Button, Input, Modal } from "@/components"
import { useLogs, useTranslation } from "@/hooks"
import {
  activeGroupIdState,
  addIntegrationTargetAction,
  clearIntegrationTargetsAction,
  configState,
  integrationTargetsLoadingState,
  integrationTargetsState,
  loadIntegrationTargetsAction,
  pickIntegrationDirectoryAction,
  providerModelHealthByProviderKeyState,
  readAgentConfigAction,
  saveConfigAction,
  setActiveGroupIdAction,
  statusState,
  testProviderModelAction,
  updateIntegrationTargetAction,
  writeGroupEntryAction,
} from "@/store"
import type { Group, IntegrationClientKind, IntegrationTarget, ProxyConfig } from "@/types"
import { createProviderTestKey, formatProviderLatency } from "@/utils/providerTesting"
import { useActions, useRelaxValue } from "@/utils/relax"
import { isHeadlessHttpRuntime } from "@/utils/runtime"
import { resolveReachableServerBaseUrls } from "@/utils/serverAddress"
import { ProviderList } from "./ProviderList"
import styles from "./ServicePage.module.css"

const SERVICE_ACTIONS = [
  saveConfigAction,
  setActiveGroupIdAction,
  loadIntegrationTargetsAction,
  clearIntegrationTargetsAction,
  pickIntegrationDirectoryAction,
  addIntegrationTargetAction,
  updateIntegrationTargetAction,
  writeGroupEntryAction,
  readAgentConfigAction,
  testProviderModelAction,
] as const

/** Matches search text against a list of candidate strings. */
function matchesKeyword(keyword: string, values: Array<string | null | undefined>): boolean {
  const normalized = keyword.trim().toLowerCase()
  if (!normalized) return true
  return values.some(value => value?.toLowerCase().includes(normalized))
}

function normalizeComparableUrl(raw?: string | null): string {
  const value = raw?.trim()
  if (!value) return ""
  return value.replace(/\/+$/, "")
}

/**
 * ServicePage Component
 * Main page for managing proxy groups and providers
 */
export const ServicePage: React.FC = () => {
  const navigate = useNavigate()
  const { t } = useTranslation()
  const config = useRelaxValue(configState)
  const status = useRelaxValue(statusState)
  const activeGroupId = useRelaxValue(activeGroupIdState)
  const integrationTargets = useRelaxValue(integrationTargetsState)
  const integrationLoading = useRelaxValue(integrationTargetsLoadingState)
  const providerModelHealthByProviderKey = useRelaxValue(providerModelHealthByProviderKeyState)
  const [
    saveConfig,
    setActiveGroupId,
    loadIntegrationTargetsAction,
    clearIntegrationTargetsAction,
    pickIntegrationDirectory,
    addIntegrationTarget,
    updateIntegrationTarget,
    writeGroupEntry,
    readAgentConfigAction,
    testProviderModel,
  ] = useActions(SERVICE_ACTIONS)
  const { showToast } = useLogs()
  const [groupSearchValue, setGroupSearchValue] = useState("")
  const [showAddGroupModal, setShowAddGroupModal] = useState(false)
  const [showDeleteGroupModal, setShowDeleteGroupModal] = useState(false)
  const [showDeleteProviderModal, setShowDeleteProviderModal] = useState(false)
  const [showAssociateProviderModal, setShowAssociateProviderModal] = useState(false)
  const [pendingDeleteProviderId, setPendingDeleteProviderId] = useState<string | null>(null)
  const [associateProviderSearch, setAssociateProviderSearch] = useState("")
  const [associateProviderChecks, setAssociateProviderChecks] = useState<Record<string, boolean>>(
    {}
  )
  const [activatingProviderId, setActivatingProviderId] = useState<string | null>(null)
  const [testingProviderIds, setTestingProviderIds] = useState<Record<string, boolean>>({})
  const [testingAllProviders, setTestingAllProviders] = useState(false)
  const [showIntegrationWriteModal, setShowIntegrationWriteModal] = useState(false)
  const [integrationStatusRefreshing, setIntegrationStatusRefreshing] = useState(false)
  const [integrationTargetUrlById, setIntegrationTargetUrlById] = useState<Record<string, string>>(
    {}
  )
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
  const isHeadlessRuntime = isHeadlessHttpRuntime()

  const groups = config?.groups ?? []
  const globalProviders = config?.providers ?? []
  const filteredGroups = useMemo(() => {
    return groups.filter(group =>
      matchesKeyword(groupSearchValue, [group.name, group.id, `/${group.id}`])
    )
  }, [groupSearchValue, groups])
  const activeGroup = groups.find(group => group.id === activeGroupId) ?? null
  const activeGroupProviderIds = useMemo(() => {
    if (!activeGroup) return []
    return activeGroup.providerIds ?? activeGroup.providers.map(provider => provider.id)
  }, [activeGroup])
  const activeGroupProviderIdSet = useMemo(() => {
    return new Set(activeGroupProviderIds)
  }, [activeGroupProviderIds])
  const providerHealthByProviderId = useMemo(() => {
    if (!activeGroup) return {}
    return Object.fromEntries(
      activeGroup.providers.map(provider => {
        const key = createProviderTestKey(activeGroup.id, provider.id)
        return [provider.id, providerModelHealthByProviderKey[key]]
      })
    )
  }, [activeGroup, providerModelHealthByProviderKey])
  const associateCandidates = useMemo(() => {
    const normalized = associateProviderSearch.trim().toLowerCase()
    const candidates = globalProviders.filter(
      provider => !activeGroupProviderIdSet.has(provider.id)
    )
    if (!normalized) return candidates
    return candidates.filter(provider =>
      [
        provider.name,
        provider.id,
        provider.apiAddress,
        provider.defaultModel,
        provider.website,
      ].some(value => value?.toLowerCase().includes(normalized))
    )
  }, [activeGroupProviderIdSet, associateProviderSearch, globalProviders])
  const selectedAssociateProviderIds = useMemo(() => {
    return Object.entries(associateProviderChecks)
      .filter(([, checked]) => checked)
      .map(([providerId]) => providerId)
  }, [associateProviderChecks])
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
        kind: "openclaw" as const,
        title: t("integration.openclaw.title"),
        hint: t("integration.openclaw.hint"),
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
      openclaw: integrationTargets.filter(item => item.kind === "openclaw"),
      opencode: integrationTargets.filter(item => item.kind === "opencode"),
    }),
    [integrationTargets]
  )

  useEffect(() => {
    if (groups.length === 0) {
      if (activeGroupId !== null) {
        setActiveGroupId({ groupId: null })
      }
      return
    }

    const activeGroupExists = groups.some(group => group.id === activeGroupId)
    if (!activeGroupExists) {
      setActiveGroupId({ groupId: groups[0].id })
    }
  }, [groups, activeGroupId, setActiveGroupId])

  const handleSelectGroup = (groupId: string) => {
    setActiveGroupId({ groupId })
    setShowDeleteProviderModal(false)
    setShowAssociateProviderModal(false)
    setPendingDeleteProviderId(null)
    setTestingProviderIds({})
    setTestingAllProviders(false)
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
      providerIds: [],
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
      setActiveGroupId({ groupId: newGroup.id })
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
      setActiveGroupId({ groupId: newGroups.length > 0 ? newGroups[0].id : null })
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

  const openAssociateProviderModal = () => {
    setAssociateProviderSearch("")
    setAssociateProviderChecks({})
    setShowAssociateProviderModal(true)
  }

  const closeAssociateProviderModal = () => {
    setShowAssociateProviderModal(false)
    setAssociateProviderSearch("")
    setAssociateProviderChecks({})
  }

  const handleToggleAssociateProvider = (providerId: string, checked: boolean) => {
    setAssociateProviderChecks(prev => ({
      ...prev,
      [providerId]: checked,
    }))
  }

  const handleAssociateProviders = async () => {
    if (!config || !activeGroupId || selectedAssociateProviderIds.length === 0) return

    const nextGroups = config.groups.map(group => {
      if (group.id !== activeGroupId) return group
      const currentProviderIds = group.providerIds ?? group.providers.map(provider => provider.id)
      const mergedProviderIds = [...currentProviderIds]
      for (const providerId of selectedAssociateProviderIds) {
        if (!mergedProviderIds.includes(providerId)) {
          mergedProviderIds.push(providerId)
        }
      }
      return {
        ...group,
        providerIds: mergedProviderIds,
        activeProviderId: group.activeProviderId ?? mergedProviderIds[0] ?? null,
      }
    })

    try {
      await saveConfig({
        ...config,
        groups: nextGroups,
      })
      closeAssociateProviderModal()
      showToast(t("servicePage.associateRule"), "success")
    } catch (error) {
      showToast(t("errors.saveFailed", { message: String(error) }), "error")
    }
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

  const handleTestProviderModel = async (providerId: string) => {
    if (!activeGroup) return
    if (testingProviderIds[providerId]) return

    const provider = activeGroup.providers.find(item => item.id === providerId)
    if (!provider) {
      showToast(t("toast.ruleNotFound"), "error")
      return
    }

    setTestingProviderIds(prev => ({ ...prev, [providerId]: true }))
    try {
      const result = await testProviderModel({ groupId: activeGroup.id, providerId })
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
      const latencyLabel = formatProviderLatency(result.responseTimeMs)

      showToast(
        t("toast.providerModelTestSuccess", {
          provider: provider.name,
          model: modelName,
          latency: latencyLabel || "-",
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

  const handleTestAllProviders = async () => {
    if (!activeGroup || testingAllProviders || activeGroup.providers.length === 0) return

    setTestingAllProviders(true)
    setTestingProviderIds(
      Object.fromEntries(activeGroup.providers.map(provider => [provider.id, true]))
    )

    let available = 0
    let unavailable = 0

    for (const provider of activeGroup.providers) {
      try {
        const result = await testProviderModel({
          groupId: activeGroup.id,
          providerId: provider.id,
        })
        if (result.ok) {
          available += 1
        } else {
          unavailable += 1
        }
      } catch {
        unavailable += 1
      } finally {
        setTestingProviderIds(prev => {
          const next = { ...prev }
          delete next[provider.id]
          return next
        })
      }
    }

    setTestingAllProviders(false)
    showToast(
      t("toast.providerBatchTestSummary", {
        available,
        unavailable,
        skipped: 0,
      }),
      unavailable > 0 ? "error" : "success"
    )
  }

  const handleDeleteProvider = async () => {
    if (!activeGroupId || !config || !pendingDeleteProviderId) return

    const newGroups = config.groups.map(group => {
      if (group.id === activeGroupId) {
        const currentProviderIds = group.providerIds ?? group.providers.map(provider => provider.id)
        const nextProviderIds = currentProviderIds.filter(
          providerId => providerId !== pendingDeleteProviderId
        )
        const nextProviders = group.providers.filter(
          provider => provider.id !== pendingDeleteProviderId
        )
        const newActiveId =
          group.activeProviderId === pendingDeleteProviderId
            ? nextProviderIds.length > 0
              ? nextProviderIds[0]
              : null
            : group.activeProviderId
        return {
          ...group,
          providerIds: nextProviderIds,
          providers: nextProviders,
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
      showToast(t("servicePage.unlinkRule"), "success")
    } catch (error) {
      showToast(t("errors.saveFailed", { message: String(error) }), "error")
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
      case "openclaw":
        return t("integration.openclaw.title")
      default:
        return t("integration.opencode.title")
    }
  }

  const loadIntegrationTargetsSafe = React.useCallback(async () => {
    try {
      await loadIntegrationTargetsAction()
    } catch (error) {
      showToast(t("errors.operationFailed", { message: String(error) }), "error")
    }
  }, [loadIntegrationTargetsAction, showToast, t])

  const refreshIntegrationTargetStatus = React.useCallback(
    async (targets: IntegrationTarget[]) => {
      if (!targets.length) {
        setIntegrationTargetUrlById({})
        return
      }

      setIntegrationStatusRefreshing(true)
      try {
        const results = await Promise.all(
          targets.map(async target => {
            try {
              const file = await readAgentConfigAction({ targetId: target.id })
              return [target.id, file.parsedConfig?.url?.trim() || ""] as const
            } catch {
              return [target.id, ""] as const
            }
          })
        )
        setIntegrationTargetUrlById(Object.fromEntries(results))
      } finally {
        setIntegrationStatusRefreshing(false)
      }
    },
    [readAgentConfigAction]
  )

  const openIntegrationWriteModal = async () => {
    if (!activeGroup) return
    setShowIntegrationWriteModal(true)
    setSelectedIntegrationIds({})
    await loadIntegrationTargetsSafe()
  }

  const closeIntegrationWriteModal = () => {
    if (integrationWriting) return
    setShowIntegrationWriteModal(false)
    setSelectedIntegrationIds({})
  }

  useEffect(() => {
    if (!activeGroup) {
      clearIntegrationTargetsAction()
      setIntegrationTargetUrlById({})
      return
    }
    void loadIntegrationTargetsSafe()
  }, [activeGroup, clearIntegrationTargetsAction, loadIntegrationTargetsSafe])

  useEffect(() => {
    if (!activeGroup) return
    if (!integrationTargets.length) {
      setIntegrationTargetUrlById({})
      return
    }
    void refreshIntegrationTargetStatus(integrationTargets)
  }, [activeGroup, integrationTargets, refreshIntegrationTargetStatus])

  const handleAddIntegrationTarget = async (kind: IntegrationClientKind) => {
    if (isHeadlessRuntime) {
      showToast(t("integration.headlessDisabled"), "error")
      return
    }
    setIntegrationAddingKind(kind)
    try {
      const pickedDir = await pickIntegrationDirectory({ kind })
      if (!pickedDir) return
      const created = await addIntegrationTarget({ kind, configDir: pickedDir })
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
    if (isHeadlessRuntime) {
      showToast(t("integration.headlessDisabled"), "error")
      return
    }
    setIntegrationUpdatingTargetId(target.id)
    try {
      const pickedDir = await pickIntegrationDirectory({ initialDir: target.configDir })
      if (!pickedDir) return
      const updated = await updateIntegrationTarget({ targetId: target.id, configDir: pickedDir })
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
      const result = await writeGroupEntry({
        groupId: activeGroup.id,
        targetIds: selectedIntegrationTargetIds,
      })
      if (result.failed === 0) {
        showToast(
          t("integration.toastWriteSuccess", {
            count: result.succeeded,
          }),
          "success"
        )
        await loadIntegrationTargetsSafe()
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
      case "openclaw":
        return `${target.configDir}${withSlash ? "" : "/"}openclaw.json`
      default:
        return `${target.configDir}${withSlash ? "" : "/"}opencode.json(c)`
    }
  }

  const getWriteTargetField = (kind: IntegrationClientKind): string => {
    switch (kind) {
      case "claude":
        return "env.ANTHROPIC_BASE_URL"
      case "codex":
        return "model_providers.<model_provider>.base_url"
      case "openclaw":
        return "models.providers.<providerId>.baseUrl"
      default:
        return "provider.aor_shared.options.baseURL"
    }
  }

  const serverBaseUrls = React.useMemo(() => {
    return resolveReachableServerBaseUrls({
      currentOrigin: isHeadlessRuntime ? window.location.origin : null,
      statusAddress: status?.address,
      statusLanAddress: status?.lanAddress,
      configHost: config?.server.host,
      configPort: config?.server.port,
    })
  }, [
    status?.address,
    status?.lanAddress,
    config?.server.host,
    config?.server.port,
    isHeadlessRuntime,
  ])

  const entryUrls = React.useMemo(() => {
    if (!activeGroup) return []
    return serverBaseUrls.map(baseUrl => `${baseUrl}/oc/${activeGroup.id}`)
  }, [activeGroup, serverBaseUrls])
  const preferredEntryUrl = React.useMemo(() => {
    return entryUrls.find(url => !url.includes("://localhost")) ?? entryUrls[0] ?? ""
  }, [entryUrls])
  const entryUrlSet = useMemo(
    () =>
      new Set(
        entryUrls
          .flatMap(url => [normalizeComparableUrl(url), normalizeComparableUrl(`${url}/v1`)])
          .filter(Boolean)
      ),
    [entryUrls]
  )
  const integrationSnapshotSections = useMemo(() => {
    return integrationSections
      .map(section => {
        const targets = integrationTargetsByKind[section.kind]
        const items = targets
          .map(target => {
            const configuredUrl = normalizeComparableUrl(integrationTargetUrlById[target.id])
            const matched = configuredUrl ? entryUrlSet.has(configuredUrl) : false
            return {
              target,
              matched,
            }
          })
          .filter(item => item.matched)
        return {
          ...section,
          total: targets.length,
          matched: items.length,
          items,
        }
      })
      .filter(section => section.matched > 0)
  }, [entryUrlSet, integrationSections, integrationTargetUrlById, integrationTargetsByKind])
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
                      <span className={styles.groupRuleCount}>
                        {(group.providerIds ?? group.providers.map(provider => provider.id)).length}
                      </span>
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
                <div className={styles.groupTitleRow}>
                  <h2>{activeGroup.name}</h2>
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
                <div className={styles.groupInfoGrid}>
                  <div className={styles.entryUrl}>
                    <div className={styles.infoPanelTitle}>{t("servicePage.entryUrl")}</div>
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

                  <div className={styles.integrationSnapshot}>
                    <div className={styles.integrationSnapshotHeader}>
                      <span className={styles.integrationSnapshotTitle}>
                        {t("integration.snapshotTitle")}
                      </span>
                      <Button
                        size="small"
                        variant="ghost"
                        onClick={() => {
                          void refreshIntegrationTargetStatus(integrationTargets)
                        }}
                        loading={integrationStatusRefreshing}
                        disabled={integrationLoading || integrationTargets.length === 0}
                      >
                        {t("integration.snapshotRefresh")}
                      </Button>
                    </div>

                    {integrationLoading ? (
                      <div className={styles.integrationSnapshotEmpty}>{t("common.loading")}</div>
                    ) : integrationSnapshotSections.length === 0 ? (
                      <div className={styles.integrationSnapshotEmpty}>
                        {t("integration.snapshotEmpty")}
                      </div>
                    ) : (
                      <div className={styles.integrationSnapshotSections}>
                        {integrationSnapshotSections.map(section => (
                          <div key={section.kind} className={styles.integrationSnapshotSection}>
                            <ul className={styles.integrationSnapshotList}>
                              {section.items.map(item => (
                                <li key={item.target.id} className={styles.integrationSnapshotItem}>
                                  <span className={styles.integrationSnapshotAgent}>
                                    {getIntegrationClientLabel(item.target.kind)}
                                  </span>
                                  <code
                                    className={styles.integrationSnapshotPath}
                                    title={item.target.configDir}
                                  >
                                    {item.target.configDir}
                                  </code>
                                </li>
                              ))}
                            </ul>
                          </div>
                        ))}
                      </div>
                    )}
                  </div>
                </div>
              </div>
            </div>

            <ProviderList
              providers={activeGroup.providers}
              activeProviderId={activeGroup.activeProviderId}
              onActivate={handleActivateProvider}
              activatingProviderId={activatingProviderId}
              onTestModel={handleTestProviderModel}
              onTestAll={() => void handleTestAllProviders()}
              testingProviderIds={testingProviderIds}
              providerHealthByProviderId={providerHealthByProviderId}
              testingAll={testingAllProviders}
              onDelete={handleRequestDeleteProvider}
              onEdit={providerId => navigate(`/providers/${providerId}/edit`)}
              onAdd={openAssociateProviderModal}
              groupId={activeGroup.id}
              addButtonTitle={t("servicePage.associateRule")}
              deleteActionLabel={t("servicePage.unlinkRule")}
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
                          isHeadlessRuntime ||
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
                              disabled={
                                isHeadlessRuntime ||
                                integrationWriting ||
                                integrationUpdatingTargetId !== null
                              }
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
        open={showAssociateProviderModal}
        onClose={closeAssociateProviderModal}
        title={t("servicePage.associateRule")}
      >
        <div className={styles.modalContent}>
          {globalProviders.length === 0 ? (
            <div className={styles.emptyHint}>
              <p>{t("providersPage.empty")}</p>
              <Button variant="primary" onClick={() => navigate("/providers")}>
                {t("header.providers")}
              </Button>
            </div>
          ) : (
            <>
              <div className={styles.formGroup}>
                <label htmlFor="associate-provider-search">
                  {t("providersPage.searchPlaceholder")}
                </label>
                <Input
                  id="associate-provider-search"
                  value={associateProviderSearch}
                  onChange={event => setAssociateProviderSearch(event.target.value)}
                  placeholder={t("providersPage.searchPlaceholder")}
                />
              </div>

              {associateCandidates.length === 0 ? (
                <p>{t("servicePage.noRulesHint")}</p>
              ) : (
                <ul className={styles.integrationTargetList}>
                  {associateCandidates.map(provider => (
                    <li key={provider.id} className={styles.integrationTargetItem}>
                      <label className={styles.integrationTargetLabel}>
                        <input
                          type="checkbox"
                          checked={Boolean(associateProviderChecks[provider.id])}
                          onChange={event =>
                            handleToggleAssociateProvider(provider.id, event.target.checked)
                          }
                        />
                        <span className={styles.integrationTargetPathWrap}>
                          <span className={styles.integrationTargetPath}>{provider.name}</span>
                          <span className={styles.integrationTargetWriteDetail}>
                            {provider.protocol} · {provider.defaultModel || "-"}
                          </span>
                        </span>
                      </label>
                    </li>
                  ))}
                </ul>
              )}

              <div className={styles.modalActions}>
                <Button variant="default" onClick={closeAssociateProviderModal}>
                  {t("common.cancel")}
                </Button>
                <Button
                  variant="primary"
                  onClick={() => {
                    void handleAssociateProviders()
                  }}
                  disabled={selectedAssociateProviderIds.length === 0}
                >
                  {t("servicePage.associateRule")}
                </Button>
              </div>
            </>
          )}
        </div>
      </Modal>

      <Modal
        open={showDeleteProviderModal}
        onClose={() => {
          setShowDeleteProviderModal(false)
          setPendingDeleteProviderId(null)
        }}
        title={t("servicePage.unlinkRule")}
      >
        <div className={styles.modalContent}>
          <p>{`${t("servicePage.unlinkRule")} ${pendingDeleteProvider?.name ?? ""} ?`}</p>
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
              {t("servicePage.unlinkRule")}
            </Button>
          </div>
        </div>
      </Modal>
    </div>
  )
}

export default ServicePage
