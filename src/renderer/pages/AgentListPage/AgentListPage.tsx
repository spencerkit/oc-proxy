import { Bot, Braces, Code2, FolderPlus, Pencil, Trash2, Workflow } from "lucide-react"
import type React from "react"
import { useCallback, useEffect, useMemo, useState } from "react"
import { useNavigate } from "react-router-dom"
import { Button, Input, Modal } from "@/components"
import { useLogs, useTranslation } from "@/hooks"
import {
  addIntegrationTargetAction,
  integrationTargetsLoadingState,
  integrationTargetsState,
  loadIntegrationTargetsAction,
  pickIntegrationDirectoryAction,
  removeIntegrationTargetAction,
} from "@/store"
import type { IntegrationClientKind, IntegrationTarget } from "@/types"
import { useActions, useRelaxValue } from "@/utils/relax"
import { isHeadlessHttpRuntime } from "@/utils/runtime"
import styles from "./AgentListPage.module.css"

const AGENT_LIST_ACTIONS = [
  loadIntegrationTargetsAction,
  pickIntegrationDirectoryAction,
  addIntegrationTargetAction,
  removeIntegrationTargetAction,
] as const

const AGENT_TYPES: IntegrationClientKind[] = ["claude", "codex", "openclaw", "opencode"]

const AGENT_META: Record<
  IntegrationClientKind,
  {
    icon: typeof Bot
    format: string
  }
> = {
  claude: {
    icon: Bot,
    format: "settings.json",
  },
  codex: {
    icon: Braces,
    format: "config.toml",
  },
  openclaw: {
    icon: Workflow,
    format: "openclaw.json + agent files",
  },
  opencode: {
    icon: Code2,
    format: "opencode.json(c)",
  },
}

function formatUpdatedAt(raw: string): string {
  const date = new Date(raw)
  if (Number.isNaN(date.getTime())) return raw

  return new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  }).format(date)
}

function getConfiguredFieldCount(target: IntegrationTarget): number {
  let count = 0

  if (target.config?.url?.trim()) count += 1
  if (target.config?.apiToken?.trim()) count += 1
  if (target.config?.apiFormat?.trim()) count += 1
  if (target.config?.model?.trim()) count += 1
  if (target.config?.providerId?.trim()) count += 1
  if (target.config?.agentId?.trim()) count += 1
  if (target.config?.fallbackModels?.length) count += 1
  if (target.config?.timeout !== undefined && target.config.timeout !== null) count += 1

  if (target.kind === "claude") {
    if (target.config?.alwaysThinkingEnabled) count += 1
    if (target.config?.includeCoAuthoredBy) count += 1
    if (target.config?.skipDangerousModePermissionPrompt) count += 1
  }

  return count
}

export const AgentListPage: React.FC = () => {
  const navigate = useNavigate()
  const { t } = useTranslation()
  const { showToast } = useLogs()
  const isHeadlessRuntime = isHeadlessHttpRuntime()

  const targets = useRelaxValue(integrationTargetsState)
  const loading = useRelaxValue(integrationTargetsLoadingState)
  const [newDir, setNewDir] = useState("")
  const [addingKind, setAddingKind] = useState<IntegrationClientKind | null>(null)
  const [addLoading, setAddLoading] = useState(false)
  const [pendingDeleteTarget, setPendingDeleteTarget] = useState<IntegrationTarget | null>(null)
  const [deleteLoading, setDeleteLoading] = useState(false)
  const [
    loadTargetsAction,
    pickIntegrationDirectory,
    addIntegrationTarget,
    removeIntegrationTarget,
  ] = useActions(AGENT_LIST_ACTIONS)

  const loadTargets = useCallback(async () => {
    try {
      await loadTargetsAction()
    } catch (err) {
      showToast(String(err), "error")
    }
  }, [loadTargetsAction, showToast])

  useEffect(() => {
    void loadTargets()
  }, [loadTargets])

  const groupedTargets = useMemo<Record<IntegrationClientKind, IntegrationTarget[]>>(
    () => ({
      claude: targets.filter(target => target.kind === "claude"),
      codex: targets.filter(target => target.kind === "codex"),
      openclaw: targets.filter(target => target.kind === "openclaw"),
      opencode: targets.filter(target => target.kind === "opencode"),
    }),
    [targets]
  )

  const handlePickDirectory = async (kind: IntegrationClientKind) => {
    if (isHeadlessRuntime) {
      showToast(t("agentManagement.headlessDisabled"), "error")
      return
    }
    try {
      const result = await pickIntegrationDirectory({ kind })
      if (result) {
        setNewDir(result)
        setAddingKind(kind)
      }
    } catch (err) {
      showToast(String(err), "error")
    }
  }

  const handleAddDirectory = async () => {
    if (!newDir.trim() || !addingKind) return
    if (isHeadlessRuntime) {
      showToast(t("agentManagement.headlessDisabled"), "error")
      return
    }

    setAddLoading(true)
    try {
      await addIntegrationTarget({ kind: addingKind, configDir: newDir.trim() })
      setNewDir("")
      setAddingKind(null)
      showToast(t("agentManagement.addSuccess"), "success")
    } catch (err) {
      showToast(String(err), "error")
    } finally {
      setAddLoading(false)
    }
  }

  const handleDelete = async () => {
    if (!pendingDeleteTarget) return
    if (isHeadlessRuntime) {
      showToast(t("agentManagement.headlessDisabled"), "error")
      return
    }

    setDeleteLoading(true)
    try {
      await removeIntegrationTarget({ targetId: pendingDeleteTarget.id })
      showToast(t("agentManagement.deleteSuccess"), "success")
      setPendingDeleteTarget(null)
    } catch (err) {
      showToast(String(err), "error")
    } finally {
      setDeleteLoading(false)
    }
  }

  const handleEdit = (targetId: string) => {
    navigate(`/agents/${targetId}/edit`)
  }

  if (loading) {
    return (
      <div className={styles.loading}>
        <p>{t("app.statusLoading")}</p>
      </div>
    )
  }

  return (
    <>
      <div className={styles.page}>
        <section className={`app-top-header ${styles.hero}`}>
          <div className="app-top-header-main">
            <h1 className="app-top-header-title">{t("agentManagement.title")}</h1>
            <p className="app-top-header-subtitle">{t("agentManagement.subtitle")}</p>
          </div>
        </section>

        <div className={styles.sectionGrid}>
          {AGENT_TYPES.map(kind => {
            const kindTargets = groupedTargets[kind]
            const meta = AGENT_META[kind]
            const Icon = meta.icon

            return (
              <section key={kind} className={styles.section}>
                <div className={styles.sectionHeader}>
                  <div className={styles.sectionTitleBlock}>
                    <div className={styles.sectionIcon}>
                      <Icon size={18} strokeWidth={2} />
                    </div>
                    <div>
                      <h2 className={styles.sectionTitle}>{t(`agentManagement.${kind}`)}</h2>
                      <p className={styles.sectionHint}>{t(`integration.${kind}.hint`)}</p>
                    </div>
                  </div>

                  <div className={styles.sectionHeaderMeta}>
                    <span className={styles.formatBadge}>{meta.format}</span>
                    <Button
                      size="small"
                      icon={FolderPlus}
                      onClick={() => handlePickDirectory(kind)}
                      disabled={isHeadlessRuntime}
                    >
                      {t("agentManagement.addConfigDir")}
                    </Button>
                  </div>
                </div>

                {addingKind === kind && (
                  <div className={styles.addPanel}>
                    <Input
                      value={newDir}
                      onChange={event => setNewDir(event.target.value)}
                      placeholder={t("agentManagement.selectDirectory")}
                      fullWidth
                    />
                    <div className={styles.addActions}>
                      <Button
                        size="small"
                        variant="ghost"
                        onClick={() => {
                          setAddingKind(null)
                          setNewDir("")
                        }}
                      >
                        {t("agentManagement.cancel")}
                      </Button>
                      <Button
                        size="small"
                        variant="primary"
                        loading={addLoading}
                        disabled={isHeadlessRuntime || !newDir.trim()}
                        onClick={handleAddDirectory}
                      >
                        {t("agentManagement.add")}
                      </Button>
                    </div>
                  </div>
                )}

                <div className={styles.targetList}>
                  {kindTargets.length === 0 ? (
                    <div className={styles.emptyState}>
                      <p className={styles.emptyTitle}>
                        {t("agentManagement.noDirectoriesConfigured")}
                      </p>
                      <p className={styles.emptyHint}>{t("agentManagement.addFirstDirectory")}</p>
                    </div>
                  ) : (
                    kindTargets.map(target => {
                      const configuredFieldCount = getConfiguredFieldCount(target)

                      return (
                        <article key={target.id} className={styles.targetCard}>
                          <div className={styles.targetMeta}>
                            <div className={styles.targetPrimary}>
                              <span className={styles.targetPath}>{target.configDir}</span>
                              <span
                                className={`${styles.statusChip} ${
                                  configuredFieldCount > 0
                                    ? styles.statusConfigured
                                    : styles.statusDraft
                                }`}
                              >
                                {configuredFieldCount > 0
                                  ? t("agentManagement.configured")
                                  : t("agentManagement.notConfigured")}
                              </span>
                            </div>

                            <div className={styles.targetDetails}>
                              <span>
                                {t("agentManagement.lastUpdatedLabel", {
                                  value: formatUpdatedAt(target.updatedAt),
                                })}
                              </span>
                              <span>
                                {target.config?.model
                                  ? t("agentManagement.modelLabel", { value: target.config.model })
                                  : t("agentManagement.modelUnset")}
                              </span>
                              <span>
                                {target.config?.url
                                  ? t("agentManagement.urlLabel", { value: target.config.url })
                                  : t("agentManagement.urlUnset")}
                              </span>
                            </div>
                          </div>

                          <div className={styles.targetActions}>
                            <Button
                              size="small"
                              variant="ghost"
                              icon={Pencil}
                              onClick={() => handleEdit(target.id)}
                            >
                              {t("agentManagement.edit")}
                            </Button>
                            <Button
                              size="small"
                              variant="danger"
                              icon={Trash2}
                              onClick={() => setPendingDeleteTarget(target)}
                              disabled={isHeadlessRuntime}
                            >
                              {t("agentManagement.delete")}
                            </Button>
                          </div>
                        </article>
                      )
                    })
                  )}
                </div>
              </section>
            )
          })}
        </div>
      </div>

      <Modal
        open={!!pendingDeleteTarget}
        onClose={() => {
          if (!deleteLoading) {
            setPendingDeleteTarget(null)
          }
        }}
        title={t("agentManagement.deleteConfig")}
        footer={
          <>
            <Button
              variant="ghost"
              onClick={() => setPendingDeleteTarget(null)}
              disabled={deleteLoading}
            >
              {t("agentManagement.cancel")}
            </Button>
            <Button
              variant="danger"
              loading={deleteLoading}
              onClick={handleDelete}
              disabled={isHeadlessRuntime}
            >
              {t("agentManagement.delete")}
            </Button>
          </>
        }
      >
        <div className={styles.deleteModalBody}>
          <p>{t("agentManagement.deleteDialogMessage")}</p>
          {pendingDeleteTarget && (
            <code className={styles.deleteModalPath}>{pendingDeleteTarget.configDir}</code>
          )}
        </div>
      </Modal>
    </>
  )
}

export default AgentListPage
