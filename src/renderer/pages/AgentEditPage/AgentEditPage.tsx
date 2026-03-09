import { ArrowLeft, Bot, Braces, Code2, FileCode2, Save } from "lucide-react"
import { useCallback, useEffect, useMemo, useState } from "react"
import { useNavigate, useParams } from "react-router-dom"
import { Button, Input, Switch } from "@/components"
import { useTranslation } from "@/hooks"
import type { AgentConfig, AgentConfigFile, IntegrationClientKind } from "@/types"
import { ipc } from "@/utils/ipc"
import styles from "./AgentEditPage.module.css"

const DEFAULT_TIMEOUT_MS = "300000"

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
  opencode: {
    icon: Code2,
    format: "opencode.json(c)",
  },
}

function buildFormState(parsed?: AgentConfig | null): AgentConfig {
  return {
    url: parsed?.url ?? "",
    apiToken: parsed?.apiToken ?? "",
    model: parsed?.model ?? "",
    timeout: parsed?.timeout,
    alwaysThinkingEnabled: parsed?.alwaysThinkingEnabled ?? false,
    includeCoAuthoredBy: parsed?.includeCoAuthoredBy ?? false,
    skipDangerousModePermissionPrompt: parsed?.skipDangerousModePermissionPrompt ?? false,
  }
}

function normalizeFormConfig(config: AgentConfig): AgentConfig {
  return {
    url: config.url?.trim() || undefined,
    apiToken: config.apiToken?.trim() || undefined,
    model: config.model?.trim() || undefined,
    timeout: config.timeout,
    alwaysThinkingEnabled: config.alwaysThinkingEnabled ?? false,
    includeCoAuthoredBy: config.includeCoAuthoredBy ?? false,
    skipDangerousModePermissionPrompt: config.skipDangerousModePermissionPrompt ?? false,
  }
}

function serializeConfig(config: AgentConfig): string {
  return JSON.stringify(normalizeFormConfig(config))
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

function buildSourcePlaceholder(kind: IntegrationClientKind): string {
  switch (kind) {
    case "claude":
      return '{\n  "env": {\n    "ANTHROPIC_BASE_URL": "http://localhost:8080/oc/your-group"\n  }\n}\n'
    case "codex":
      return '[model_providers.aor_shared]\nbase_url = "http://localhost:8080/oc/your-group"\n'
    case "opencode":
      return '{\n  "provider": {\n    "aor_shared": {\n      "options": {\n        "baseURL": "http://localhost:8080/oc/your-group"\n      }\n    }\n  }\n}\n'
  }
}

export const AgentEditPage: React.FC = () => {
  const { targetId } = useParams<{ targetId: string }>()
  const navigate = useNavigate()
  const { t } = useTranslation()

  const [loading, setLoading] = useState(true)
  const [configFile, setConfigFile] = useState<AgentConfigFile | null>(null)
  const [editMode, setEditMode] = useState<"form" | "source">("form")
  const [saveMode, setSaveMode] = useState<"form" | "source" | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [success, setSuccess] = useState<string | null>(null)
  const [formData, setFormData] = useState<AgentConfig>(buildFormState())
  const [timeoutText, setTimeoutText] = useState("")
  const [sourceContent, setSourceContent] = useState("")

  const loadConfig = useCallback(async () => {
    if (!targetId) return

    setLoading(true)
    setError(null)
    try {
      const result = await ipc.integrationReadAgentConfig(targetId)
      setConfigFile(result)

      const nextFormState = buildFormState(result.parsedConfig)
      setFormData(nextFormState)
      setTimeoutText(
        result.parsedConfig?.timeout !== undefined
          ? String(result.parsedConfig.timeout)
          : ""
      )
      setSourceContent(result.content)
    } catch (err) {
      setError(String(err))
    } finally {
      setLoading(false)
    }
  }, [targetId])

  useEffect(() => {
    void loadConfig()
  }, [loadConfig])

  useEffect(() => {
    if (configFile && !configFile.parsedConfig && configFile.content.trim()) {
      setEditMode("source")
    }
  }, [configFile])

  const kind = configFile?.kind ?? "claude"
  const supportsTimeout = kind !== "codex"
  const meta = AGENT_META[kind]
  const KindIcon = meta.icon
  const sourcePlaceholder = buildSourcePlaceholder(kind)
  const timeoutError =
    timeoutText.trim().length > 0 && !/^\d+$/.test(timeoutText.trim())
      ? t("agentManagement.timeoutInvalid")
      : ""

  const currentFormConfig = useMemo(
    () =>
      normalizeFormConfig({
        ...formData,
        timeout: supportsTimeout && timeoutText.trim() ? Number(timeoutText.trim()) : undefined,
      }),
    [formData, supportsTimeout, timeoutText]
  )
  const initialFormConfig = useMemo(
    () => buildFormState(configFile?.parsedConfig),
    [configFile?.parsedConfig]
  )
  const initialSourceContent = configFile?.content ?? ""
  const isFormDirty = serializeConfig(currentFormConfig) !== serializeConfig(initialFormConfig)
  const isSourceDirty = sourceContent !== initialSourceContent
  const statusMessage =
    editMode === "form"
      ? isFormDirty
        ? t("agentManagement.unsavedChanges")
        : t("agentManagement.allChangesSaved")
      : isSourceDirty
        ? t("agentManagement.unsavedChanges")
        : t("agentManagement.allChangesSaved")

  const handleSaveForm = async () => {
    if (!targetId || timeoutError || !isFormDirty) return

    setSaveMode("form")
    setError(null)
    setSuccess(null)
    try {
      await ipc.integrationWriteAgentConfig(targetId, currentFormConfig)
      await loadConfig()
      setSuccess(t("agentManagement.saveSuccess"))
    } catch (err) {
      setError(String(err))
    } finally {
      setSaveMode(null)
    }
  }

  const handleSaveSource = async () => {
    if (!targetId || !isSourceDirty) return

    setSaveMode("source")
    setError(null)
    setSuccess(null)
    try {
      await ipc.integrationWriteAgentConfigSource(targetId, sourceContent)
      await loadConfig()
      setSuccess(t("agentManagement.saveSuccess"))
    } catch (err) {
      setError(String(err))
    } finally {
      setSaveMode(null)
    }
  }

  if (loading) {
    return (
      <div className={styles.loading}>
        <p>{t("app.statusLoading")}</p>
      </div>
    )
  }

  return (
    <div className={styles.page}>
      <section className={styles.hero}>
        <button className={styles.backButton} onClick={() => navigate("/agents")}>
          <ArrowLeft size={16} strokeWidth={2} />
          <span>{t("agentManagement.back")}</span>
        </button>

        <div className={styles.heroHeading}>
          <div className={styles.kindIcon}>
            <KindIcon size={18} strokeWidth={2} />
          </div>
          <div className={styles.titleBlock}>
            <p className={styles.eyebrow}>{t(`agentManagement.${kind}`)}</p>
            <h1>{t("agentManagement.editConfig")}</h1>
            <p className={styles.subtitle}>{t("agentManagement.editSubtitle")}</p>

            <div className={styles.infoStack}>
              <div className={styles.infoRow}>
                <span className={styles.infoLabel}>{t("agentManagement.configDir")}</span>
                <code className={styles.infoValue}>{configFile?.configDir}</code>
              </div>
              <div className={styles.infoRow}>
                <span className={styles.infoLabel}>{t("agentManagement.configFile")}</span>
                <code className={styles.infoValue}>{configFile?.filePath}</code>
              </div>
              <div className={styles.infoMeta}>
                <span className={styles.formatBadge}>{meta.format}</span>
                {configFile?.updatedAt && (
                  <span className={styles.metaText}>
                    {t("agentManagement.lastUpdatedLabel", {
                      value: formatUpdatedAt(configFile.updatedAt),
                    })}
                  </span>
                )}
              </div>
            </div>
          </div>
        </div>
      </section>

      <section className={styles.editorCard}>
        <div className={styles.editorHeader}>
          <div className={styles.tabs}>
            <button
              type="button"
              className={`${styles.tab} ${editMode === "form" ? styles.tabActive : ""}`}
              onClick={() => setEditMode("form")}
            >
              {t("agentManagement.formEditor")}
            </button>
            <button
              type="button"
              className={`${styles.tab} ${editMode === "source" ? styles.tabActive : ""}`}
              onClick={() => setEditMode("source")}
            >
              {t("agentManagement.sourceEditor")}
            </button>
          </div>

          <span className={`${styles.statusBadge} ${editMode === "form" && isFormDirty ? styles.statusDirty : ""} ${editMode === "source" && isSourceDirty ? styles.statusDirty : ""}`}>
            {statusMessage}
          </span>
        </div>

        {error && <div className={styles.error}>{error}</div>}
        {success && <div className={styles.success}>{success}</div>}

        {editMode === "form" ? (
          <div className={styles.formLayout}>
            <section className={styles.formSection}>
              <div className={styles.sectionHeading}>
                <h2>{t("agentManagement.connectionSection")}</h2>
                <p>{t(`integration.${kind}.hint`)}</p>
              </div>

              <div className={styles.fieldGrid}>
                <Input
                  label={t("agentManagement.url")}
                  value={formData.url}
                  onChange={event => setFormData(current => ({ ...current, url: event.target.value }))}
                  placeholder="http://localhost:8080/oc/group"
                  fullWidth
                />
                <Input
                  label={t("agentManagement.apiToken")}
                  type="password"
                  value={formData.apiToken}
                  onChange={event =>
                    setFormData(current => ({ ...current, apiToken: event.target.value }))
                  }
                  placeholder="sk-..."
                  fullWidth
                />
              </div>
            </section>

            <section className={styles.formSection}>
              <div className={styles.sectionHeading}>
                <h2>{t("agentManagement.runtimeSection")}</h2>
                <p>{t("agentManagement.runtimeHint")}</p>
              </div>

              <div className={styles.fieldGrid}>
                <Input
                  label={t("agentManagement.model")}
                  value={formData.model}
                  onChange={event => setFormData(current => ({ ...current, model: event.target.value }))}
                  placeholder="claude-sonnet-4-5-20250929"
                  fullWidth
                />
                {supportsTimeout && (
                  <Input
                    label={t("agentManagement.timeout")}
                    type="number"
                    value={timeoutText}
                    error={timeoutError || undefined}
                    onChange={event => setTimeoutText(event.target.value)}
                    placeholder={DEFAULT_TIMEOUT_MS}
                    fullWidth
                  />
                )}
              </div>
            </section>

            {kind === "claude" && (
              <section className={styles.formSection}>
                <div className={styles.sectionHeading}>
                  <h2>{t("agentManagement.behaviorOptions")}</h2>
                  <p>{t("agentManagement.behaviorHint")}</p>
                </div>

                <div className={styles.switchGroup}>
                  <div className={styles.switchRow}>
                    <div className={styles.switchCopy}>
                      <strong>{t("agentManagement.alwaysThinkingEnabled")}</strong>
                      <span>{t("agentManagement.alwaysThinkingHint")}</span>
                    </div>
                    <Switch
                      checked={!!formData.alwaysThinkingEnabled}
                      onChange={checked =>
                        setFormData(current => ({ ...current, alwaysThinkingEnabled: checked }))
                      }
                    />
                  </div>

                  <div className={styles.switchRow}>
                    <div className={styles.switchCopy}>
                      <strong>{t("agentManagement.includeCoAuthoredBy")}</strong>
                      <span>{t("agentManagement.coAuthoredByHint")}</span>
                    </div>
                    <Switch
                      checked={!!formData.includeCoAuthoredBy}
                      onChange={checked =>
                        setFormData(current => ({ ...current, includeCoAuthoredBy: checked }))
                      }
                    />
                  </div>

                  <div className={styles.switchRow}>
                    <div className={styles.switchCopy}>
                      <strong>{t("agentManagement.skipDangerousModePermissionPrompt")}</strong>
                      <span>{t("agentManagement.skipPermissionHint")}</span>
                    </div>
                    <Switch
                      checked={!!formData.skipDangerousModePermissionPrompt}
                      onChange={checked =>
                        setFormData(current => ({
                          ...current,
                          skipDangerousModePermissionPrompt: checked,
                        }))
                      }
                    />
                  </div>
                </div>
              </section>
            )}
          </div>
        ) : (
          <div className={styles.sourceLayout}>
            <div className={styles.sectionHeading}>
              <h2>{t("agentManagement.sourceEditor")}</h2>
              <p>{t("agentManagement.sourceHint", { format: meta.format })}</p>
            </div>

            <div className={styles.sourceMeta}>
              <FileCode2 size={16} strokeWidth={2} />
              <span>{meta.format}</span>
            </div>

            <textarea
              className={styles.sourceTextarea}
              value={sourceContent}
              onChange={event => setSourceContent(event.target.value)}
              placeholder={sourcePlaceholder}
              spellCheck={false}
            />
          </div>
        )}

        <div className={styles.actions}>
          <Button variant="ghost" onClick={() => navigate("/agents")}>
            {t("agentManagement.back")}
          </Button>
          {editMode === "form" ? (
            <Button
              variant="primary"
              icon={Save}
              loading={saveMode === "form"}
              disabled={!isFormDirty || !!timeoutError}
              onClick={handleSaveForm}
            >
              {t("agentManagement.save")}
            </Button>
          ) : (
            <Button
              variant="primary"
              icon={Save}
              loading={saveMode === "source"}
              disabled={!isSourceDirty}
              onClick={handleSaveSource}
            >
              {t("agentManagement.save")}
            </Button>
          )}
        </div>
      </section>
    </div>
  )
}

export default AgentEditPage
