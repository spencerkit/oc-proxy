import { Eye, EyeOff } from "lucide-react"
import type React from "react"
import { Input } from "@/components/common/Input"
import type { TranslateFunction } from "@/hooks"
import type { AgentConfig } from "@/types"
import styles from "./AgentEditPage.module.css"

export interface OpenClawEditorFormProps {
  formData: AgentConfig
  fallbackModelsText: string
  showApiToken: boolean
  onFormDataChange: (updater: (current: AgentConfig) => AgentConfig) => void
  onFallbackModelsTextChange: (value: string) => void
  onToggleApiTokenVisibility: () => void
  t: TranslateFunction
  defaultAgentId: string
  defaultProviderId: string
  defaultApiFormat: string
}

export const OpenClawEditorForm: React.FC<OpenClawEditorFormProps> = ({
  formData,
  fallbackModelsText,
  showApiToken,
  onFormDataChange,
  onFallbackModelsTextChange,
  onToggleApiTokenVisibility,
  t,
  defaultAgentId,
  defaultProviderId,
  defaultApiFormat,
}) => {
  return (
    <>
      <section className={styles.formSection}>
        <div className={styles.sectionHeading}>
          <h2>{t("agentManagement.openclawScopeSection")}</h2>
          <p>{t("agentManagement.openclawScopeHint")}</p>
        </div>

        <div className={styles.fieldGrid}>
          <Input
            label={t("agentManagement.openclawAgentId")}
            value={formData.agentId}
            hint={t("agentManagement.openclawAgentIdHint")}
            onChange={event =>
              onFormDataChange(current => ({ ...current, agentId: event.target.value }))
            }
            placeholder={defaultAgentId}
            fullWidth
          />
          <Input
            label={t("agentManagement.openclawProviderId")}
            value={formData.providerId}
            hint={t("agentManagement.openclawProviderIdHint")}
            onChange={event =>
              onFormDataChange(current => ({ ...current, providerId: event.target.value }))
            }
            placeholder={defaultProviderId}
            fullWidth
          />
        </div>
      </section>

      <section className={styles.formSection}>
        <div className={styles.sectionHeading}>
          <h2>{t("agentManagement.connectionSection")}</h2>
          <p>{t("integration.openclaw.hint")}</p>
        </div>

        <div className={styles.fieldGrid}>
          <Input
            label={t("agentManagement.url")}
            value={formData.url}
            onChange={event =>
              onFormDataChange(current => ({ ...current, url: event.target.value }))
            }
            placeholder="http://localhost:8080/oc/group/v1"
            fullWidth
          />
          <Input
            label={t("agentManagement.apiToken")}
            type={showApiToken ? "text" : "password"}
            value={formData.apiToken}
            hint={t("agentManagement.openclawTokenHint")}
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
            placeholder="gpt-4.1"
            fullWidth
          />
          <Input
            label={t("agentManagement.openclawApiFormat")}
            value={formData.apiFormat}
            hint={t("agentManagement.openclawApiFormatHint")}
            onChange={event =>
              onFormDataChange(current => ({ ...current, apiFormat: event.target.value }))
            }
            placeholder={defaultApiFormat}
            fullWidth
          />
        </div>
      </section>

      <section className={styles.formSection}>
        <div className={styles.sectionHeading}>
          <h2>{t("agentManagement.openclawFallbackModels")}</h2>
          <p>{t("agentManagement.openclawFallbackModelsHint")}</p>
        </div>

        <Input
          label={t("agentManagement.openclawFallbackModels")}
          value={fallbackModelsText}
          onChange={event => onFallbackModelsTextChange(event.target.value)}
          placeholder="gpt-4.1-mini, gpt-4o-mini"
          fullWidth
        />
      </section>
    </>
  )
}

export default OpenClawEditorForm
