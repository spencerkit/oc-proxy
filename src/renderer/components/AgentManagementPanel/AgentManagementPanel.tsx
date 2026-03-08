import type React from "react"
import { useCallback, useEffect, useState } from "react"
import { Button, Input, Switch } from "@/components"
import { useTranslation } from "@/hooks"
import type {
  AgentConfig,
  AgentConfigFile,
  IntegrationClientKind,
  IntegrationTarget,
} from "@/types"
import { ipc } from "@/utils/ipc"
import styles from "./AgentManagementPanel.module.css"

type Step = "selectType" | "manageDirs" | "addDir" | "editConfig"

const AGENT_TYPES: IntegrationClientKind[] = ["claude", "codex", "opencode"]

interface Props {
  activeGroupId?: string
}

export const AgentManagementPanel: React.FC<Props> = ({ activeGroupId }) => {
  const { t } = useTranslation()
  const [step, setStep] = useState<Step>("selectType")
  const [targets, setTargets] = useState<IntegrationTarget[]>([])
  const [selectedKind, setSelectedKind] = useState<IntegrationClientKind | null>(null)
  const [selectedTarget, setSelectedTarget] = useState<IntegrationTarget | null>(null)
  const [configFile, setConfigFile] = useState<AgentConfigFile | null>(null)
  const [editMode, setEditMode] = useState<"form" | "source">("form")
  const [newDir, setNewDir] = useState("")
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [success, setSuccess] = useState<string | null>(null)

  // Form state
  const [formData, setFormData] = useState<AgentConfig>({
    url: "",
    apiToken: "",
    model: "",
    timeout: 300000,
    alwaysThinkingEnabled: false,
    includeCoAuthoredBy: false,
    skipDangerousModePermissionPrompt: false,
  })

  const loadTargets = useCallback(async () => {
    try {
      const result = await ipc.integrationListTargets()
      setTargets(result || [])
    } catch (err) {
      console.error("Failed to load targets:", err)
    }
  }, [])

  useEffect(() => {
    loadTargets()
  }, [loadTargets])

  const getAgentName = (kind: IntegrationClientKind) => {
    return t(`agentManagement.${kind}`)
  }

  const getAgentCount = (kind: IntegrationClientKind) => {
    return targets.filter(t => t.kind === kind).length
  }

  const handleSelectKind = (kind: IntegrationClientKind) => {
    setSelectedKind(kind)
    setStep("manageDirs")
  }

  const handlePickDirectory = async () => {
    try {
      const result = await ipc.integrationPickDirectory(undefined, selectedKind ?? undefined)
      if (result) {
        setNewDir(result)
      }
    } catch (err) {
      console.error("Failed to pick directory:", err)
    }
  }

  const handleAddDir = async () => {
    if (!newDir.trim() || !selectedKind) return
    setLoading(true)
    setError(null)
    try {
      await ipc.integrationAddTarget(selectedKind, newDir.trim())
      await loadTargets()
      setNewDir("")
      setStep("manageDirs")
    } catch (err) {
      setError(String(err))
    } finally {
      setLoading(false)
    }
  }

  const handleEdit = async (target: IntegrationTarget) => {
    setLoading(true)
    setError(null)
    try {
      const result = await ipc.integrationReadAgentConfig(target.id)
      setConfigFile(result)
      setSelectedTarget(target)

      // Populate form with parsed config
      if (result.parsedConfig) {
        setFormData({
          url: result.parsedConfig.url || "",
          apiToken: result.parsedConfig.apiToken || "",
          model: result.parsedConfig.model || "",
          timeout: result.parsedConfig.timeout || 300000,
          alwaysThinkingEnabled: result.parsedConfig.alwaysThinkingEnabled || false,
          includeCoAuthoredBy: result.parsedConfig.includeCoAuthoredBy || false,
          skipDangerousModePermissionPrompt:
            result.parsedConfig.skipDangerousModePermissionPrompt || false,
        })
      } else {
        setFormData({
          url: "",
          apiToken: "",
          model: "",
          timeout: 300000,
          alwaysThinkingEnabled: false,
          includeCoAuthoredBy: false,
          skipDangerousModePermissionPrompt: false,
        })
      }
      setEditMode("form")
      setStep("editConfig")
    } catch (err) {
      setError(String(err))
    } finally {
      setLoading(false)
    }
  }

  const handleSave = async () => {
    if (!selectedTarget) return
    setLoading(true)
    setError(null)
    setSuccess(null)
    try {
      await ipc.integrationWriteAgentConfig(selectedTarget.id, formData)
      setSuccess(t("agentManagement.saveSuccess"))
      await loadTargets()
      setTimeout(() => {
        setStep("manageDirs")
        setSuccess(null)
      }, 1500)
    } catch (err) {
      setError(String(err))
    } finally {
      setLoading(false)
    }
  }

  const handleSaveSource = async () => {
    if (!configFile || !selectedTarget) return
    setLoading(true)
    setError(null)
    try {
      // For source editing, we parse the content and then write
      // This is a simplified version - could be enhanced
      await ipc.integrationWriteAgentConfig(selectedTarget.id, formData)
      setSuccess(t("agentManagement.saveSuccess"))
      await loadTargets()
      setTimeout(() => {
        setStep("manageDirs")
        setSuccess(null)
      }, 1500)
    } catch (err) {
      setError(String(err))
    } finally {
      setLoading(false)
    }
  }

  const handleWrite = async (targetId: string) => {
    if (!activeGroupId) {
      setError("No active group selected")
      return
    }
    setLoading(true)
    setError(null)
    setSuccess(null)
    try {
      await ipc.integrationWriteGroupEntry(activeGroupId, [targetId])
      setSuccess(t("agentManagement.writeSuccess"))
    } catch (err) {
      setError(String(err))
    } finally {
      setLoading(false)
    }
  }

  const handleDelete = async (targetId: string) => {
    if (!confirm(t("agentManagement.deleteConfirm"))) return
    setLoading(true)
    setError(null)
    try {
      await ipc.integrationRemoveTarget(targetId)
      await loadTargets()
    } catch (err) {
      setError(String(err))
    } finally {
      setLoading(false)
    }
  }

  const handleBack = () => {
    if (step === "manageDirs") {
      setStep("selectType")
      setSelectedKind(null)
    } else if (step === "addDir") {
      setStep("manageDirs")
      setNewDir("")
    } else if (step === "editConfig") {
      setStep("manageDirs")
      setSelectedTarget(null)
      setConfigFile(null)
    }
    setError(null)
    setSuccess(null)
  }

  // Render functions
  const renderSelectType = () => (
    <div className={styles.selectType}>
      <h3 className={styles.title}>{t("agentManagement.selectType")}</h3>
      <div className={styles.agentCards}>
        {AGENT_TYPES.map(kind => {
          const count = getAgentCount(kind)
          return (
            <button
              key={kind}
              type="button"
              className={styles.agentCard}
              onClick={() => handleSelectKind(kind)}
            >
              <span className={styles.agentName}>{getAgentName(kind)}</span>
              <span
                className={`${styles.agentStatus} ${count > 0 ? styles.configured : styles.notConfigured}`}
              >
                {count > 0
                  ? `${count} ${t("agentManagement.configured")}`
                  : t("agentManagement.notConfigured")}
              </span>
            </button>
          )
        })}
      </div>
    </div>
  )

  const renderManageDirs = () => {
    const kindTargets = targets.filter(t => t.kind === selectedKind)
    const currentKind = selectedKind
    return (
      <div className={styles.manageDirs}>
        <div className={styles.header}>
          <button type="button" className={styles.backButton} onClick={handleBack}>
            ← {t("agentManagement.back")}
          </button>
          <h3 className={styles.title}>{currentKind ? getAgentName(currentKind) : ""}</h3>
        </div>

        {error && <div className={styles.error}>{error}</div>}
        {success && <div className={styles.success}>{success}</div>}

        <div className={styles.dirList}>
          {kindTargets.length === 0 ? (
            <div className={styles.emptyState}>
              <p>{t("agentManagement.noDirectoriesConfigured")}</p>
              <p className={styles.hint}>{t("agentManagement.addFirstDirectory")}</p>
            </div>
          ) : (
            kindTargets.map(target => (
              <div key={target.id} className={styles.dirItem}>
                <div className={styles.dirPath}>{target.configDir}</div>
                <div className={styles.dirActions}>
                  <Button size="small" onClick={() => handleEdit(target)}>
                    {t("agentManagement.edit")}
                  </Button>
                  <Button size="small" variant="danger" onClick={() => handleDelete(target.id)}>
                    {t("agentManagement.delete")}
                  </Button>
                  {activeGroupId && (
                    <Button size="small" onClick={() => handleWrite(target.id)}>
                      {t("agentManagement.write")}
                    </Button>
                  )}
                </div>
              </div>
            ))
          )}
        </div>

        <Button className={styles.addButton} onClick={() => setStep("addDir")}>
          + {t("agentManagement.addConfigDir")}
        </Button>
      </div>
    )
  }

  const renderAddDir = () => (
    <div className={styles.addDir}>
      <div className={styles.header}>
        <button type="button" className={styles.backButton} onClick={handleBack}>
          ← {t("agentManagement.back")}
        </button>
        <h3 className={styles.title}>{t("agentManagement.addConfigDir")}</h3>
      </div>

      <div className={styles.formGroup}>
        <label htmlFor="agent-config-dir">{t("agentManagement.selectDirectory")}</label>
        <div className={styles.dirInput}>
          <Input
            id="agent-config-dir"
            value={newDir}
            onChange={(e: React.ChangeEvent<HTMLInputElement>) => setNewDir(e.target.value)}
            placeholder={t("agentManagement.orEnterManually")}
          />
          <Button onClick={handlePickDirectory}>Browse</Button>
        </div>
      </div>

      <div className={styles.presetDirs}>
        <span className={styles.presetLabel}>{t("agentManagement.presetDirs")}</span>
        <div className={styles.presetList}>
          {selectedKind === "claude" && (
            <>
              <button type="button" onClick={() => setNewDir("~/.claude")}>
                ~/.claude
              </button>
              <button type="button" onClick={() => setNewDir("~/.config/claude")}>
                ~/.config/claude
              </button>
            </>
          )}
          {selectedKind === "codex" && (
            <button type="button" onClick={() => setNewDir("~/.codex")}>
              ~/.codex
            </button>
          )}
          {selectedKind === "opencode" && (
            <>
              <button type="button" onClick={() => setNewDir("~/.config/opencode")}>
                ~/.config/opencode
              </button>
              <button type="button" onClick={() => setNewDir("~/.local/share/opencode")}>
                ~/.local/share/opencode
              </button>
            </>
          )}
        </div>
      </div>

      {error && <div className={styles.error}>{error}</div>}

      <div className={styles.actions}>
        <Button onClick={handleBack}>{t("agentManagement.cancel")}</Button>
        <Button variant="primary" onClick={handleAddDir} disabled={!newDir.trim() || loading}>
          {loading ? "..." : t("agentManagement.nextConfig")}
        </Button>
      </div>
    </div>
  )

  const renderEditConfig = () => (
    <div className={styles.editConfig}>
      <div className={styles.header}>
        <button type="button" className={styles.backButton} onClick={handleBack}>
          ← {t("agentManagement.back")}
        </button>
        <h3 className={styles.title}>{t("agentManagement.editConfig")}</h3>
      </div>

      <div className={styles.configDir}>
        {t("agentManagement.configDir")}: {selectedTarget?.configDir}
      </div>

      <div className={styles.tabs}>
        <button
          type="button"
          className={`${styles.tab} ${editMode === "form" ? styles.active : ""}`}
          onClick={() => setEditMode("form")}
        >
          {t("agentManagement.formEditor")}
        </button>
        <button
          type="button"
          className={`${styles.tab} ${editMode === "source" ? styles.active : ""}`}
          onClick={() => setEditMode("source")}
        >
          {t("agentManagement.sourceEditor")}
        </button>
      </div>

      {error && <div className={styles.error}>{error}</div>}
      {success && <div className={styles.success}>{success}</div>}

      {editMode === "form" ? (
        <div className={styles.form}>
          <div className={styles.formGroup}>
            <label htmlFor="agent-url">{t("agentManagement.url")}</label>
            <Input
              id="agent-url"
              value={formData.url}
              onChange={(e: React.ChangeEvent<HTMLInputElement>) =>
                setFormData({ ...formData, url: e.target.value })
              }
              placeholder="http://localhost:8080/oc/group"
            />
          </div>

          <div className={styles.formGroup}>
            <label htmlFor="agent-api-token">{t("agentManagement.apiToken")}</label>
            <Input
              id="agent-api-token"
              type="password"
              value={formData.apiToken}
              onChange={(e: React.ChangeEvent<HTMLInputElement>) =>
                setFormData({ ...formData, apiToken: e.target.value })
              }
              placeholder="sk-..."
            />
          </div>

          <div className={styles.formGroup}>
            <label htmlFor="agent-model">{t("agentManagement.model")}</label>
            <Input
              id="agent-model"
              value={formData.model}
              onChange={(e: React.ChangeEvent<HTMLInputElement>) =>
                setFormData({ ...formData, model: e.target.value })
              }
              placeholder="claude-sonnet-4-5-20250929"
            />
          </div>

          <div className={styles.formGroup}>
            <label htmlFor="agent-timeout">{t("agentManagement.timeout")}</label>
            <Input
              id="agent-timeout"
              type="number"
              value={formData.timeout}
              onChange={(e: React.ChangeEvent<HTMLInputElement>) =>
                setFormData({ ...formData, timeout: Number(e.target.value) })
              }
            />
          </div>

          {selectedKind === "claude" && (
            <div className={styles.behaviorOptions}>
              <span className={styles.behaviorLabel}>{t("agentManagement.behaviorOptions")}</span>
              <Switch
                label={t("agentManagement.alwaysThinkingEnabled")}
                checked={formData.alwaysThinkingEnabled}
                onChange={(v: boolean) => setFormData({ ...formData, alwaysThinkingEnabled: v })}
              />
              <Switch
                label={t("agentManagement.includeCoAuthoredBy")}
                checked={formData.includeCoAuthoredBy}
                onChange={(v: boolean) => setFormData({ ...formData, includeCoAuthoredBy: v })}
              />
              <Switch
                label={t("agentManagement.skipDangerousModePermissionPrompt")}
                checked={formData.skipDangerousModePermissionPrompt}
                onChange={(v: boolean) =>
                  setFormData({ ...formData, skipDangerousModePermissionPrompt: v })
                }
              />
            </div>
          )}

          <div className={styles.actions}>
            <Button onClick={handleBack}>{t("agentManagement.cancel")}</Button>
            <Button variant="primary" onClick={handleSave} disabled={loading}>
              {loading ? "..." : t("agentManagement.save")}
            </Button>
          </div>
        </div>
      ) : (
        <div className={styles.sourceEditor}>
          <textarea
            className={styles.sourceTextarea}
            value={configFile?.content || ""}
            onChange={e =>
              setConfigFile(prev => (prev ? { ...prev, content: e.target.value } : null))
            }
          />
          <div className={styles.actions}>
            <Button onClick={handleBack}>{t("agentManagement.cancel")}</Button>
            <Button variant="primary" onClick={handleSaveSource} disabled={loading}>
              {loading ? "..." : t("agentManagement.save")}
            </Button>
          </div>
        </div>
      )}
    </div>
  )

  return (
    <div className={styles.panel}>
      {step === "selectType" && renderSelectType()}
      {step === "manageDirs" && renderManageDirs()}
      {step === "addDir" && renderAddDir()}
      {step === "editConfig" && renderEditConfig()}
    </div>
  )
}

export default AgentManagementPanel
