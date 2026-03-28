import { ArrowLeft, Bot, Braces, Code2, Save, Workflow } from "lucide-react"
import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { useNavigate, useParams } from "react-router-dom"
import { Button } from "@/components/common/Button"
import { useTranslation } from "@/hooks"
import {
  readAgentConfigAction,
  writeAgentConfigAction,
  writeAgentConfigSourceAction,
} from "@/store"
import type { AgentConfig, AgentConfigFile, AgentSourceFile, IntegrationClientKind } from "@/types"
import {
  formatAgentSourceDraft,
  getDirtySourceIds,
  mergeReloadedFormDraftState,
  mergeReloadedSourceDrafts,
} from "@/utils/agentSourceFormat"
import { useActions } from "@/utils/relax"
import { AgentEditContent } from "./AgentEditContent"
import styles from "./AgentEditPage.module.css"
import { buildAgentEditFormState } from "./agentEditPageSections"

const AGENT_EDIT_ACTIONS = [
  readAgentConfigAction,
  writeAgentConfigAction,
  writeAgentConfigSourceAction,
] as const

const _DEFAULT_TIMEOUT_MS = "300000"
const DEFAULT_OPENCLAW_AGENT_ID = "default"
const DEFAULT_OPENCLAW_PROVIDER_ID = "aor_shared"
const DEFAULT_OPENCLAW_API_FORMAT = "openai-responses"

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

function buildFormState(
  kind: IntegrationClientKind,
  parsed?: AgentConfig | null,
  openclawEditor?: AgentConfigFile["openclawEditor"] | null
): AgentConfig {
  return buildAgentEditFormState(kind, parsed, openclawEditor)
}

function normalizeFormConfig(config: AgentConfig): AgentConfig {
  return {
    agentId: config.agentId?.trim() || undefined,
    providerId: config.providerId?.trim() || undefined,
    url: config.url?.trim() || undefined,
    apiToken: config.apiToken?.trim() || undefined,
    apiFormat: config.apiFormat?.trim() || undefined,
    model: config.model?.trim() || undefined,
    fallbackModels: (() => {
      const items = config.fallbackModels?.map(item => item.trim()).filter(item => item.length > 0)
      return items?.length ? items : undefined
    })(),
    timeout: config.timeout,
    alwaysThinkingEnabled: config.alwaysThinkingEnabled ?? false,
    includeCoAuthoredBy: config.includeCoAuthoredBy ?? false,
    skipDangerousModePermissionPrompt: config.skipDangerousModePermissionPrompt ?? false,
  }
}

function parseFallbackModels(text: string): string[] | undefined {
  const items = text
    .split(",")
    .map(item => item.trim())
    .filter(Boolean)

  return items.length > 0 ? items : undefined
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

function buildSourcePlaceholder(kind: IntegrationClientKind, sourceId: string): string {
  switch (kind) {
    case "claude":
      return '{\n  "env": {\n    "ANTHROPIC_BASE_URL": "http://localhost:8080/oc/your-group"\n  }\n}\n'
    case "codex":
      if (sourceId === "auth") {
        return '{\n  "OPENAI_API_KEY": "sk-..."\n}\n'
      }
      return 'model_provider = "your_provider"\n\n[model_providers.your_provider]\nbase_url = "http://localhost:8080/oc/your-group"\n'
    case "openclaw":
      if (sourceId === "auth-profiles") {
        return '{\n  "profiles": {\n    "aor_shared": {\n      "apiKey": "sk-..."\n    }\n  }\n}\n'
      }
      if (sourceId === "models") {
        return '{\n  "providers": {\n    "aor_shared": {\n      "api": "openai-responses",\n      "baseUrl": "http://localhost:8080/oc/your-group/v1",\n      "apiKey": "sk-..."\n    }\n  }\n}\n'
      }
      return '{\n  "agents": {\n    "defaults": {\n      "model": {\n        "primary": "gpt-4.1-mini",\n        "fallbacks": ["gpt-4o-mini"]\n      }\n    }\n  },\n  "models": {\n    "providers": {\n      "aor_shared": {\n        "api": "openai-responses",\n        "baseUrl": "http://localhost:8080/oc/your-group/v1",\n        "apiKey": "sk-..."\n      }\n    }\n  }\n}\n'
    case "opencode":
      return '{\n  "provider": {\n    "aor_shared": {\n      "options": {\n        "baseURL": "http://localhost:8080/oc/your-group",\n        "apiKey": "sk-..."\n      }\n    }\n  }\n}\n'
  }
}

function buildSourceFiles(configFile?: AgentConfigFile | null): AgentSourceFile[] {
  if (!configFile) return []
  if (configFile.sourceFiles?.length) return configFile.sourceFiles
  const filePathParts = configFile.filePath.split(/[\\/]/)
  const fileName = filePathParts[filePathParts.length - 1] || "config"
  return [
    {
      sourceId: "primary",
      label: fileName,
      filePath: configFile.filePath,
      content: configFile.content,
    },
  ]
}

export const AgentEditPage: React.FC = () => {
  const { targetId } = useParams<{ targetId: string }>()
  const navigate = useNavigate()
  const { t } = useTranslation()
  const [readAgentConfig, writeAgentConfig, writeAgentConfigSource] = useActions(AGENT_EDIT_ACTIONS)

  const [loading, setLoading] = useState(true)
  const [configFile, setConfigFile] = useState<AgentConfigFile | null>(null)
  const [editMode, setEditMode] = useState<"form" | "source">("form")
  const [saveMode, setSaveMode] = useState<"form" | "source" | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [success, setSuccess] = useState<string | null>(null)
  const [formData, setFormData] = useState<AgentConfig>(buildFormState("claude"))
  const [timeoutText, setTimeoutText] = useState("")
  const [fallbackModelsText, setFallbackModelsText] = useState("")
  const [showApiToken, setShowApiToken] = useState(false)
  const [activeSourceId, setActiveSourceId] = useState("primary")
  const [sourceDrafts, setSourceDrafts] = useState<Record<string, string>>({})

  const sourceFiles = useMemo(() => buildSourceFiles(configFile), [configFile])
  const sourceFilesRef = useRef<AgentSourceFile[]>([])
  const formDraftStateRef = useRef({
    formData,
    timeoutText,
    fallbackModelsText,
  })

  useEffect(() => {
    sourceFilesRef.current = sourceFiles
  }, [sourceFiles])

  useEffect(() => {
    formDraftStateRef.current = {
      formData,
      timeoutText,
      fallbackModelsText,
    }
  }, [fallbackModelsText, formData, timeoutText])

  const loadConfig = useCallback(
    async (options?: { savedSourceId?: string; preserveFormDrafts?: boolean }) => {
      if (!targetId) return

      setLoading(true)
      setError(null)
      try {
        const result = await readAgentConfig({ targetId })
        setConfigFile(result)

        const nextFormState = buildFormState(
          result.kind,
          result.parsedConfig,
          result.openclawEditor
        )
        const mergedFormState = mergeReloadedFormDraftState(
          formDraftStateRef.current,
          {
            formData: nextFormState,
            timeoutText:
              result.parsedConfig?.timeout !== undefined ? String(result.parsedConfig.timeout) : "",
            fallbackModelsText: nextFormState.fallbackModels?.join(", ") ?? "",
          },
          options?.preserveFormDrafts ?? false
        )
        setFormData(mergedFormState.formData)
        setTimeoutText(mergedFormState.timeoutText)
        setFallbackModelsText(mergedFormState.fallbackModelsText)
        const nextSourceFiles = buildSourceFiles(result)
        setActiveSourceId(current =>
          nextSourceFiles.some(file => file.sourceId === current)
            ? current
            : (nextSourceFiles[0]?.sourceId ?? "primary")
        )
        setSourceDrafts(current =>
          mergeReloadedSourceDrafts(
            sourceFilesRef.current,
            current,
            nextSourceFiles,
            options?.savedSourceId
          )
        )
      } catch (err) {
        setError(String(err))
      } finally {
        setLoading(false)
      }
    },
    [readAgentConfig, targetId]
  )

  useEffect(() => {
    void loadConfig()
  }, [loadConfig])

  useEffect(() => {
    if (configFile && !configFile.parsedConfig && configFile.content.trim()) {
      setEditMode("source")
    }
  }, [configFile])

  const kind = configFile?.kind ?? "claude"
  const supportsTimeout = kind === "claude" || kind === "opencode"
  const meta = AGENT_META[kind]
  const KindIcon = meta.icon
  const activeSourceFile = useMemo(
    () => sourceFiles.find(file => file.sourceId === activeSourceId) ?? sourceFiles[0],
    [activeSourceId, sourceFiles]
  )
  const sourceContent = activeSourceFile ? (sourceDrafts[activeSourceFile.sourceId] ?? "") : ""
  const initialSourceContent = activeSourceFile?.content ?? ""
  const isActiveSourceDirty = sourceContent !== initialSourceContent
  const sourcePlaceholder = buildSourcePlaceholder(kind, activeSourceFile?.sourceId ?? "primary")
  const timeoutError =
    timeoutText.trim().length > 0 && !/^\d+$/.test(timeoutText.trim())
      ? t("agentManagement.timeoutInvalid")
      : ""

  const currentFormConfig = useMemo(
    () =>
      normalizeFormConfig({
        ...formData,
        fallbackModels: kind === "openclaw" ? parseFallbackModels(fallbackModelsText) : undefined,
        timeout: supportsTimeout && timeoutText.trim() ? Number(timeoutText.trim()) : undefined,
      }),
    [fallbackModelsText, formData, kind, supportsTimeout, timeoutText]
  )
  const initialFormConfig = useMemo(
    () => buildFormState(kind, configFile?.parsedConfig, configFile?.openclawEditor),
    [configFile?.openclawEditor, configFile?.parsedConfig, kind]
  )
  const isFormDirty = serializeConfig(currentFormConfig) !== serializeConfig(initialFormConfig)
  const dirtySourceIds = getDirtySourceIds(sourceFiles, sourceDrafts)
  const isSourceDirty = dirtySourceIds.length > 0
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
      await writeAgentConfig({ targetId, config: currentFormConfig })
      await loadConfig()
      setSuccess(t("agentManagement.saveSuccess"))
    } catch (err) {
      setError(String(err))
    } finally {
      setSaveMode(null)
    }
  }

  const handleSaveSource = async () => {
    if (!targetId || !activeSourceFile || !isActiveSourceDirty) return

    setSaveMode("source")
    setError(null)
    setSuccess(null)
    try {
      await writeAgentConfigSource({
        targetId,
        content: sourceContent,
        sourceId: activeSourceFile.sourceId,
      })
      await loadConfig({
        savedSourceId: activeSourceFile.sourceId,
        preserveFormDrafts: true,
      })
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
      <section className="app-sub-header">
        <div className="app-sub-header-top">
          <button type="button" className="app-sub-header-back" onClick={() => navigate("/agents")}>
            <ArrowLeft size={16} strokeWidth={2} />
            <span>{t("agentManagement.back")}</span>
          </button>
          <div className="app-sub-header-actions">
            <span className={styles.kindBadge}>
              <KindIcon size={14} strokeWidth={2} />
              <span>{t(`agentManagement.${kind}`)}</span>
            </span>
            <span className={styles.formatBadge}>{meta.format}</span>
          </div>
        </div>

        <div className="app-sub-header-main">
          <h1 className="app-sub-header-title">{t("agentManagement.editConfig")}</h1>
          <p className={`app-sub-header-subtitle ${styles.subtitle}`}>
            {t("agentManagement.editSubtitle")}
          </p>
        </div>

        <div className={styles.headerMetaGrid}>
          <div className={styles.headerMetaItem}>
            <span className={styles.headerMetaLabel}>{t("agentManagement.configDir")}</span>
            <code className={styles.headerMetaValue}>{configFile?.configDir || "-"}</code>
          </div>
          <div className={styles.headerMetaItem}>
            <span className={styles.headerMetaLabel}>{t("agentManagement.configFile")}</span>
            <code className={styles.headerMetaValue}>
              {activeSourceFile?.filePath || configFile?.filePath || "-"}
            </code>
          </div>
          <div className={styles.headerMetaItem}>
            <span className={styles.headerMetaLabel}>{t("agentManagement.updatedAt")}</span>
            <code className={styles.headerMetaValue}>
              {configFile?.updatedAt ? formatUpdatedAt(configFile.updatedAt) : "-"}
            </code>
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

          <span
            className={`${styles.statusBadge} ${editMode === "form" && isFormDirty ? styles.statusDirty : ""} ${editMode === "source" && isSourceDirty ? styles.statusDirty : ""}`}
          >
            {statusMessage}
          </span>
        </div>

        {error && <div className={styles.error}>{error}</div>}
        {success && <div className={styles.success}>{success}</div>}

        <AgentEditContent
          kind={kind}
          editMode={editMode}
          formData={formData}
          fallbackModelsText={fallbackModelsText}
          showApiToken={showApiToken}
          supportsTimeout={supportsTimeout}
          timeoutText={timeoutText}
          timeoutError={timeoutError}
          sourceFiles={sourceFiles}
          activeSourceFile={activeSourceFile}
          sourceContent={sourceContent}
          sourcePlaceholder={sourcePlaceholder}
          metaFormat={meta.format}
          dirtySourceIds={dirtySourceIds}
          t={t}
          onFormDataChange={setFormData}
          onFallbackModelsTextChange={setFallbackModelsText}
          onToggleApiTokenVisibility={() => setShowApiToken(current => !current)}
          onTimeoutTextChange={setTimeoutText}
          onSourceSelect={setActiveSourceId}
          onSourceChange={value =>
            setSourceDrafts(current => ({
              ...current,
              [activeSourceFile?.sourceId ?? "primary"]: value,
            }))
          }
          onFormatCurrentFile={() =>
            setSourceDrafts(current => ({
              ...current,
              [activeSourceFile?.sourceId ?? "primary"]: formatAgentSourceDraft(
                kind,
                current[activeSourceFile?.sourceId ?? "primary"] ?? ""
              ),
            }))
          }
          defaultOpenclawAgentId={DEFAULT_OPENCLAW_AGENT_ID}
          defaultOpenclawProviderId={DEFAULT_OPENCLAW_PROVIDER_ID}
          defaultOpenclawApiFormat={DEFAULT_OPENCLAW_API_FORMAT}
        />

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
              disabled={!isActiveSourceDirty}
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
