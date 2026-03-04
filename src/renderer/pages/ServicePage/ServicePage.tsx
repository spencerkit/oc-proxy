import { Copy, Pencil, Plus, Trash2 } from "lucide-react"
import React, { useState } from "react"
import { useNavigate } from "react-router-dom"
import { Button, Input, Modal } from "@/components"
import { useLogs, useTranslation } from "@/hooks"
import { useProxyStore } from "@/store"
import type { Group, ProxyConfig } from "@/types"
import { resolveReachableServerBaseUrls } from "@/utils/serverAddress"
import { RuleList } from "./RuleList"
import styles from "./ServicePage.module.css"

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
    fetchGroupQuotas,
    fetchGroupProviderCardStats,
    fetchProviderQuota,
  } = useProxyStore()
  const { showToast } = useLogs()
  const [selectedProviderId, setSelectedProviderId] = useState<string | null>(null)
  const [showAddGroupModal, setShowAddGroupModal] = useState(false)
  const [showDeleteGroupModal, setShowDeleteGroupModal] = useState(false)
  const [showDeleteProviderModal, setShowDeleteProviderModal] = useState(false)
  const [pendingDeleteProviderId, setPendingDeleteProviderId] = useState<string | null>(null)
  const [activatingProviderId, setActivatingProviderId] = useState<string | null>(null)
  const [newGroupName, setNewGroupName] = useState("")
  const [newGroupId, setNewGroupId] = useState("")
  const providerCardStatsHours = 24
  const providerCardStatsRefreshIntervalMs = 10_000

  const groups = config?.groups ?? []
  const activeGroup = groups.find(g => g.id === activeGroupId)
  const activeGroupModels = Array.isArray(activeGroup?.models) ? activeGroup.models : []
  const activeProvider = activeGroup?.providers.find(item => item.id === selectedProviderId) ?? null
  const pendingDeleteProvider =
    activeGroup?.providers.find(item => item.id === pendingDeleteProviderId) ?? null
  const quotaAutoRefreshIntervalMs = React.useMemo(() => {
    const minutes = config?.ui?.quotaAutoRefreshMinutes
    if (!Number.isInteger(minutes) || !minutes || minutes < 1 || minutes > 1440) {
      return 5 * 60 * 1000
    }
    return minutes * 60 * 1000
  }, [config?.ui?.quotaAutoRefreshMinutes])

  // Auto-select first valid group if selection is empty/invalid
  React.useEffect(() => {
    if (groups.length === 0) {
      if (activeGroupId !== null) {
        setActiveGroupId(null)
      }
      return
    }

    const activeGroupExists = !!groups.find(group => group.id === activeGroupId)
    if (!activeGroupExists) {
      setActiveGroupId(groups[0].id)
    }
  }, [groups, activeGroupId, setActiveGroupId])

  React.useEffect(() => {
    if (!activeGroupId) return
    void fetchGroupQuotas(activeGroupId)
  }, [activeGroupId, fetchGroupQuotas])

  React.useEffect(() => {
    if (!activeGroupId) return
    void fetchGroupProviderCardStats(activeGroupId, providerCardStatsHours)
  }, [activeGroupId, fetchGroupProviderCardStats])

  React.useEffect(() => {
    if (!activeGroupId) return
    const timer = window.setInterval(() => {
      void fetchGroupQuotas(activeGroupId)
    }, quotaAutoRefreshIntervalMs)
    return () => window.clearInterval(timer)
  }, [activeGroupId, fetchGroupQuotas, quotaAutoRefreshIntervalMs])

  React.useEffect(() => {
    if (!activeGroupId) return
    const timer = window.setInterval(() => {
      void fetchGroupProviderCardStats(activeGroupId, providerCardStatsHours)
    }, providerCardStatsRefreshIntervalMs)
    return () => window.clearInterval(timer)
  }, [activeGroupId, fetchGroupProviderCardStats])

  const handleSelectGroup = (groupId: string) => {
    setActiveGroupId(groupId)
    setSelectedProviderId(null)
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

    const newGroups = config.groups.filter(g => g.id !== activeGroupId)
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
      setSelectedProviderId(null)
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

  const handleCopyEntryUrl = async (url: string) => {
    if (!url) return

    try {
      await navigator.clipboard.writeText(url)
      showToast(t("toast.entryUrlCopied"), "success")
    } catch {
      showToast(t("toast.copyFailed"), "error")
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
      {/* Group List Sidebar */}
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
          <div className={styles.groupListContent}>
            {groups.length === 0 ? (
              <div className={styles.emptyHint}>
                <p>{t("servicePage.noGroupsHint")}</p>
                <Button variant="primary" size="small" icon={Plus} onClick={openAddGroupModal}>
                  {t("servicePage.createFirstGroup")}
                </Button>
              </div>
            ) : (
              <ul className={styles.groupItems}>
                {groups.map(group => (
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

      {/* Main Content */}
      <div className={styles.mainContent}>
        {!activeGroup ? (
          <div className={styles.noSelection}>
            <p>{t("servicePage.noGroupSelected")}</p>
          </div>
        ) : (
          <>
            {/* Group Header */}
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

            {/* Provider List */}
            <RuleList
              providers={activeGroup.providers}
              activeProviderId={activeGroup.activeProviderId}
              onSelect={setSelectedProviderId}
              onActivate={handleActivateProvider}
              activatingProviderId={activatingProviderId}
              quotaByRuleId={activeGroupQuotaByProviderId}
              quotaLoadingByRuleId={activeGroupQuotaLoadingByProviderId}
              cardStatsByRuleId={activeGroupProviderCardStatsByProviderId}
              onRefreshQuota={handleRefreshProviderQuota}
              onDelete={handleRequestDeleteProvider}
              groupName={activeGroup.name}
              groupId={activeGroup.id}
            />

            {/* Provider Detail (when provider is selected) */}
            {selectedProviderId && activeProvider && (
              <div className={styles.ruleDetail}>
                <h3>{activeProvider.name}</h3>
                <div className={styles.ruleInfo}>
                  <div className={styles.ruleInfoItem}>
                    <span className={styles.label}>{t("servicePage.ruleProtocol")}:</span>
                    <span>{t(`ruleProtocol.${activeProvider.protocol}`)}</span>
                  </div>
                  <div className={styles.ruleInfoItem}>
                    <span className={styles.label}>{t("servicePage.apiAddress")}:</span>
                    <span>{activeProvider.apiAddress}</span>
                  </div>
                  <div className={styles.ruleInfoItem}>
                    <span className={styles.label}>{t("servicePage.defaultModel")}:</span>
                    <span>{activeProvider.defaultModel}</span>
                  </div>
                </div>
                <div className={styles.ruleActions}>
                  <Button
                    variant="danger"
                    size="small"
                    onClick={() => handleRequestDeleteProvider(activeProvider.id)}
                  >
                    {t("servicePage.deleteRule")}
                  </Button>
                </div>
              </div>
            )}
          </>
        )}
      </div>

      {/* Add Group Modal */}
      <Modal open={showAddGroupModal} onClose={closeAddGroupModal} title={t("modal.addGroupTitle")}>
        <div className={styles.modalContent}>
          <div className={styles.formGroup}>
            <label htmlFor="groupName">{t("modal.groupNameLabel")}</label>
            <Input
              id="groupName"
              value={newGroupName}
              onChange={e => setNewGroupName(e.target.value)}
              placeholder={t("modal.groupNamePlaceholder")}
            />
          </div>
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

      {/* Delete Group Modal */}
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

      {/* Delete Provider Modal */}
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
