import type React from "react"
import { Button } from "@/components"
import { useTranslation } from "@/hooks"
import type {
  ProviderImportField,
  ProviderImportFormat,
  ProviderImportInputFormat,
  ProviderImportParseResult,
} from "@/utils/providerImport"
import styles from "./RuleFormPage.module.css"

const FORMAT_OPTIONS: Array<{ value: ProviderImportInputFormat; labelKey: string }> = [
  { value: "auto", labelKey: "ruleForm.importFormatAuto" },
  { value: "codex", labelKey: "ruleForm.importFormatCodex" },
  { value: "claude_code", labelKey: "ruleForm.importFormatClaudeCode" },
  { value: "aor", labelKey: "ruleForm.importFormatAor" },
]

const PREVIEW_FIELDS: ProviderImportField[] = [
  "name",
  "protocol",
  "token",
  "apiAddress",
  "website",
  "defaultModel",
]

export interface ProviderImportCardProps {
  showHeader?: boolean
  format: ProviderImportInputFormat
  rawValue: string
  parseError: string | null
  parseResult: ProviderImportParseResult | null
  onFormatChange: (value: ProviderImportInputFormat) => void
  onRawChange: (value: string) => void
  onParse: () => void
  onApply: () => void
  onClear: () => void
}

function getDetectedFormatLabelKey(format: ProviderImportFormat) {
  return `ruleForm.importDetectedFormatValue.${format}` as const
}

function getFieldLabelKey(field: ProviderImportField) {
  return `ruleForm.importField.${field}` as const
}

export const ProviderImportCard: React.FC<ProviderImportCardProps> = ({
  showHeader = true,
  format,
  rawValue,
  parseError,
  parseResult,
  onFormatChange,
  onRawChange,
  onParse,
  onApply,
  onClear,
}) => {
  const { t } = useTranslation()
  const previewEntries = parseResult
    ? PREVIEW_FIELDS.flatMap(field => {
        const value = parseResult.draft[field]
        return value === undefined ? [] : [{ field, value: String(value) }]
      })
    : []
  const missingFieldLabels =
    parseResult?.missingFields.map(field => t(getFieldLabelKey(field))) ?? []

  return (
    <section className={styles.importCard}>
      <div className={styles.importCardHeader}>
        {showHeader ? (
          <div className={styles.importHeaderCopy}>
            <h2 className={styles.sectionTitle}>{t("ruleForm.importTitle")}</h2>
            <p className={styles.fieldHint}>{t("ruleForm.importHint")}</p>
          </div>
        ) : null}
        <div className={styles.importActions}>
          <Button type="button" variant="default" size="small" onClick={onClear}>
            {t("ruleForm.importClear")}
          </Button>
          <Button type="button" variant="primary" size="small" onClick={onParse}>
            {t("ruleForm.importParse")}
          </Button>
        </div>
      </div>

      <div className={styles.formGroup}>
        <label htmlFor="provider-import-format">{t("ruleForm.importFormat")}</label>
        <select
          id="provider-import-format"
          className={styles.nativeSelect}
          value={format}
          onChange={event => onFormatChange(event.target.value as ProviderImportInputFormat)}
        >
          {FORMAT_OPTIONS.map(option => (
            <option key={option.value} value={option.value}>
              {t(option.labelKey)}
            </option>
          ))}
        </select>
      </div>

      <div className={styles.formGroup}>
        <label htmlFor="provider-import-raw">{t("ruleForm.importInputLabel")}</label>
        <textarea
          id="provider-import-raw"
          className={styles.importTextarea}
          value={rawValue}
          onChange={event => onRawChange(event.target.value)}
          placeholder={t("ruleForm.importInputPlaceholder")}
        />
      </div>

      {parseError ? <p className={styles.errorText}>{parseError}</p> : null}

      {parseResult ? (
        <div className={styles.importPreview}>
          <h3 className={styles.importPreviewTitle}>{t("ruleForm.importPreviewTitle")}</h3>
          <p className={styles.importDetectedFormat}>
            <span>{t("ruleForm.importDetectedFormat")}:</span>
            <strong>{t(getDetectedFormatLabelKey(parseResult.format))}</strong>
          </p>
          <div className={styles.importPreviewGrid}>
            {previewEntries.map(entry => (
              <div key={entry.field} className={styles.importPreviewItem}>
                <span className={styles.importPreviewLabel}>
                  {t(getFieldLabelKey(entry.field))}
                </span>
                <strong className={styles.importPreviewValue}>{entry.value}</strong>
              </div>
            ))}
          </div>
          {parseResult.warnings.length > 0 ? (
            <div className={styles.fieldHint}>
              <strong>{t("ruleForm.importWarnings")}:</strong> {parseResult.warnings.join(" ")}
            </div>
          ) : null}
          {missingFieldLabels.length > 0 ? (
            <p className={styles.fieldHint}>
              {t("ruleForm.importMissingFields", {
                fields: missingFieldLabels.join(", "),
              })}
            </p>
          ) : null}
          <div className={styles.importApplyRow}>
            <Button type="button" variant="primary" size="small" onClick={onApply}>
              {t("ruleForm.importApply")}
            </Button>
          </div>
        </div>
      ) : null}
    </section>
  )
}

export default ProviderImportCard
