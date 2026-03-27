import { FileCode2 } from "lucide-react"
import type React from "react"
import type { TranslateFunction } from "@/hooks"
import type { AgentSourceFile, IntegrationClientKind } from "@/types"
import styles from "./AgentEditPage.module.css"

export interface AgentSourceTabsProps {
  kind: IntegrationClientKind
  sourceFiles: AgentSourceFile[]
  activeSourceFile?: AgentSourceFile
  sourceContent: string
  sourcePlaceholder: string
  onSourceSelect: (sourceId: string) => void
  onSourceChange: (value: string) => void
  onFormatCurrentFile?: () => void
  dirtySourceIds: string[]
  t: TranslateFunction
  metaFormat: string
}

export const AgentSourceTabs: React.FC<AgentSourceTabsProps> = ({
  kind,
  sourceFiles,
  activeSourceFile,
  sourceContent,
  sourcePlaceholder,
  onSourceSelect,
  onSourceChange,
  onFormatCurrentFile,
  dirtySourceIds,
  t,
  metaFormat,
}) => {
  return (
    <div className={styles.sourceLayout}>
      <div className={styles.sectionHeading}>
        <h2>{t("agentManagement.sourceEditor")}</h2>
        <p>
          {t("agentManagement.sourceHint", {
            format: activeSourceFile?.label ?? metaFormat,
          })}
        </p>
      </div>

      {sourceFiles.length > 1 && (
        <div className={styles.sourceFileTabs}>
          {sourceFiles.map(file => (
            <button
              key={file.sourceId}
              type="button"
              className={`${styles.sourceFileTab} ${activeSourceFile?.sourceId === file.sourceId ? styles.sourceFileTabActive : ""}`}
              onClick={() => onSourceSelect(file.sourceId)}
            >
              {file.label}
              {dirtySourceIds.includes(file.sourceId) ? " *" : ""}
            </button>
          ))}
        </div>
      )}

      <div className={styles.sourceMetaRow}>
        <div className={styles.sourceMeta}>
          <FileCode2 size={16} strokeWidth={2} />
          <span>{activeSourceFile?.filePath ?? metaFormat}</span>
        </div>
        {kind === "openclaw" && (
          <button type="button" className={styles.sourceActionButton} onClick={onFormatCurrentFile}>
            {t("agentManagement.formatCurrentFile")}
          </button>
        )}
      </div>

      {kind === "codex" && activeSourceFile?.sourceId === "primary" && (
        <p className={styles.sourceHintText}>{t("agentManagement.codexConfigSourceHint")}</p>
      )}
      {kind === "codex" && activeSourceFile?.sourceId === "auth" && (
        <p className={styles.sourceHintText}>{t("agentManagement.codexAuthSourceHint")}</p>
      )}
      {kind === "openclaw" && (
        <p className={styles.sourceHintText}>{t("agentManagement.openclawSourceValidationHint")}</p>
      )}
      {kind === "openclaw" && activeSourceFile?.sourceId === "primary" && (
        <p className={styles.sourceHintText}>{t("agentManagement.openclawPrimarySourceHint")}</p>
      )}
      {kind === "openclaw" && activeSourceFile?.sourceId === "auth-profiles" && (
        <p className={styles.sourceHintText}>{t("agentManagement.openclawAuthSourceHint")}</p>
      )}
      {kind === "openclaw" && activeSourceFile?.sourceId === "models" && (
        <p className={styles.sourceHintText}>{t("agentManagement.openclawModelsSourceHint")}</p>
      )}

      <textarea
        className={styles.sourceTextarea}
        value={sourceContent}
        onChange={event => onSourceChange(event.target.value)}
        placeholder={sourcePlaceholder}
        spellCheck={false}
      />
    </div>
  )
}

export default AgentSourceTabs
