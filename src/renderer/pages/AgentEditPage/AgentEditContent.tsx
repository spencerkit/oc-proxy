import { Eye, EyeOff } from "lucide-react"
import type React from "react"
import { Input } from "@/components/common/Input"
import { Switch } from "@/components/common/Switch"
import type { TranslateFunction } from "@/hooks"
import type { AgentConfig, AgentSourceFile, IntegrationClientKind } from "@/types"
import styles from "./AgentEditPage.module.css"
import { AgentSourceTabs } from "./AgentSourceTabs"
import { OpenClawEditorForm } from "./OpenClawEditorForm"

export interface AgentEditContentProps {
  kind: IntegrationClientKind
  editMode: "form" | "source"
  formData: AgentConfig
  fallbackModelsText: string
  showApiToken: boolean
  supportsTimeout: boolean
  timeoutText: string
  timeoutError: string
  sourceFiles: AgentSourceFile[]
  activeSourceFile?: AgentSourceFile
  sourceContent: string
  sourcePlaceholder: string
  metaFormat: string
  dirtySourceIds: string[]
  t: TranslateFunction
  onFormDataChange: (updater: (current: AgentConfig) => AgentConfig) => void
  onFallbackModelsTextChange: (value: string) => void
  onToggleApiTokenVisibility: () => void
  onTimeoutTextChange: (value: string) => void
  onSourceSelect: (sourceId: string) => void
  onSourceChange: (value: string) => void
  onFormatCurrentFile?: () => void
  defaultOpenclawAgentId: string
  defaultOpenclawProviderId: string
  defaultOpenclawApiFormat: string
}

export const AgentEditContent: React.FC<AgentEditContentProps> = ({
  kind,
  editMode,
  formData,
  fallbackModelsText,
  showApiToken,
  supportsTimeout,
  timeoutText,
  timeoutError,
  sourceFiles,
  activeSourceFile,
  sourceContent,
  sourcePlaceholder,
  metaFormat,
  dirtySourceIds,
  t,
  onFormDataChange,
  onFallbackModelsTextChange,
  onToggleApiTokenVisibility,
  onTimeoutTextChange,
  onSourceSelect,
  onSourceChange,
  onFormatCurrentFile,
  defaultOpenclawAgentId,
  defaultOpenclawProviderId,
  defaultOpenclawApiFormat,
}) => {
  if (editMode === "source") {
    return (
      <AgentSourceTabs
        kind={kind}
        sourceFiles={sourceFiles}
        activeSourceFile={activeSourceFile}
        sourceContent={sourceContent}
        sourcePlaceholder={sourcePlaceholder}
        onSourceSelect={onSourceSelect}
        onSourceChange={onSourceChange}
        onFormatCurrentFile={onFormatCurrentFile}
        dirtySourceIds={dirtySourceIds}
        t={t}
        metaFormat={metaFormat}
      />
    )
  }

  return (
    <div className={styles.formLayout}>
      {kind === "openclaw" ? (
        <OpenClawEditorForm
          formData={formData}
          fallbackModelsText={fallbackModelsText}
          showApiToken={showApiToken}
          onFormDataChange={onFormDataChange}
          onFallbackModelsTextChange={onFallbackModelsTextChange}
          onToggleApiTokenVisibility={onToggleApiTokenVisibility}
          t={t}
          defaultAgentId={defaultOpenclawAgentId}
          defaultProviderId={defaultOpenclawProviderId}
          defaultApiFormat={defaultOpenclawApiFormat}
        />
      ) : (
        <>
          <section className={styles.formSection}>
            <div className={styles.sectionHeading}>
              <h2>{t("agentManagement.connectionSection")}</h2>
              <p>{t(`integration.${kind}.hint`)}</p>
            </div>

            <div className={styles.fieldGrid}>
              <Input
                label={t("agentManagement.url")}
                value={formData.url}
                onChange={event =>
                  onFormDataChange(current => ({ ...current, url: event.target.value }))
                }
                placeholder="http://localhost:8080/oc/group"
                fullWidth
              />
              <Input
                label={t("agentManagement.apiToken")}
                type={showApiToken ? "text" : "password"}
                value={formData.apiToken}
                hint={kind === "codex" ? t("agentManagement.codexTokenHint") : undefined}
                onChange={event =>
                  onFormDataChange(current => ({ ...current, apiToken: event.target.value }))
                }
                placeholder="sk-..."
                endAdornment={
                  <button
                    type="button"
                    className={styles.tokenVisibilityButton}
                    onClick={onToggleApiTokenVisibility}
                    aria-label={
                      showApiToken ? t("agentManagement.hideToken") : t("agentManagement.showToken")
                    }
                    title={
                      showApiToken ? t("agentManagement.hideToken") : t("agentManagement.showToken")
                    }
                  >
                    {showApiToken ? <EyeOff size={16} /> : <Eye size={16} />}
                  </button>
                }
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
                onChange={event =>
                  onFormDataChange(current => ({ ...current, model: event.target.value }))
                }
                placeholder="claude-sonnet-4-5-20250929"
                fullWidth
              />
              {supportsTimeout && (
                <Input
                  label={t("agentManagement.timeout")}
                  type="number"
                  value={timeoutText}
                  error={timeoutError || undefined}
                  onChange={event => onTimeoutTextChange(event.target.value)}
                  placeholder="300000"
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
                      onFormDataChange(current => ({ ...current, alwaysThinkingEnabled: checked }))
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
                      onFormDataChange(current => ({ ...current, includeCoAuthoredBy: checked }))
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
                      onFormDataChange(current => ({
                        ...current,
                        skipDangerousModePermissionPrompt: checked,
                      }))
                    }
                  />
                </div>
              </div>
            </section>
          )}
        </>
      )}
    </div>
  )
}

export default AgentEditContent
