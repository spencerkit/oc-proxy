import { Copy, ExternalLink, FlaskConical, Loader2, Pencil, Play, Plus, Trash2 } from "lucide-react"
import type React from "react"
import { useNavigate } from "react-router-dom"
import { Button } from "@/components"
import { useTranslation } from "@/hooks"
import type { Group, ProviderModelHealthSnapshot } from "@/types"
import { formatProviderLatency } from "@/utils/providerTesting"
import { resolveProviderWebsiteHref } from "@/utils/providerWebsite"
import styles from "./ServicePage.module.css"

type ProviderHealthPresentation = {
  statusLabel: string
  statusClassName: string
  latencyLabel: string | null
}

function resolveProviderHealthPresentation(
  snapshot: ProviderModelHealthSnapshot | null | undefined,
  testing: boolean,
  t: ReturnType<typeof useTranslation>["t"]
): ProviderHealthPresentation {
  if (testing) {
    return {
      statusLabel: t("servicePage.testingModel"),
      statusClassName: styles.providerHealthStatusTesting,
      latencyLabel: null,
    }
  }

  if (!snapshot) {
    return {
      statusLabel: t("servicePage.availabilityUntested"),
      statusClassName: styles.providerHealthStatusUntested,
      latencyLabel: null,
    }
  }

  if (snapshot.status === "available") {
    return {
      statusLabel: t("servicePage.availabilityAvailable"),
      statusClassName: styles.providerHealthStatusAvailable,
      latencyLabel: formatProviderLatency(snapshot.latencyMs),
    }
  }

  return {
    statusLabel: t("servicePage.availabilityUnavailable"),
    statusClassName: styles.providerHealthStatusUnavailable,
    latencyLabel: formatProviderLatency(snapshot.latencyMs),
  }
}

/**
 * ProviderList Component
 * Displays providers for a service group association view.
 */
export const ProviderList: React.FC<{
  providers: Group["providers"]
  activeProviderId: string | null
  onActivate: (providerId: string) => void | Promise<void>
  activatingProviderId?: string | null
  onDelete: (providerId: string) => void
  groupId?: string
  onEdit?: (providerId: string) => void
  onAdd?: () => void
  onDuplicate?: (providerId: string) => void | Promise<void>
  onTestModel?: (providerId: string) => void | Promise<void>
  onTestAll?: () => void | Promise<void>
  testingProviderIds?: Record<string, boolean | undefined>
  providerHealthByProviderId?: Record<string, ProviderModelHealthSnapshot | null | undefined>
  testingAll?: boolean
  showActivate?: boolean
  addButtonLabel?: string
  addButtonTitle?: string
  deleteActionLabel?: string
  emptyMessage?: string
}> = ({
  providers,
  activeProviderId,
  onActivate,
  activatingProviderId,
  onDelete,
  groupId,
  onEdit,
  onAdd,
  onDuplicate,
  onTestModel,
  onTestAll,
  testingProviderIds,
  providerHealthByProviderId,
  testingAll = false,
  showActivate = true,
  addButtonLabel,
  addButtonTitle,
  deleteActionLabel,
  emptyMessage,
}) => {
  const { t } = useTranslation()
  const navigate = useNavigate()

  const handleProviderEdit = (providerId: string) => {
    if (onEdit) {
      onEdit(providerId)
      return
    }
    if (!groupId) return
    navigate(`/groups/${groupId}/providers/${providerId}/edit`)
  }

  const handleAddProviderClick = () => {
    if (onAdd) {
      onAdd()
      return
    }
    if (!groupId) return
    navigate(`/groups/${groupId}/providers/new`)
  }

  const resolvedDeleteActionLabel = deleteActionLabel || t("servicePage.deleteRule")

  return (
    <div className={styles.ruleList}>
      <div className={styles.ruleListHeader}>
        <div className={styles.ruleHeaderTitle}>
          <h3>{t("servicePage.ruleName")}</h3>
          <span className={styles.countBadge}>{providers.length}</span>
        </div>
        <div className={styles.ruleHeaderActions}>
          {onTestAll ? (
            <button
              type="button"
              className={styles.headerIconButton}
              onClick={() => void onTestAll()}
              data-tooltip={
                testingAll
                  ? t("servicePage.testingAllProviders")
                  : t("servicePage.testAllProviders")
              }
              aria-label={
                testingAll
                  ? t("servicePage.testingAllProviders")
                  : t("servicePage.testAllProviders")
              }
              disabled={providers.length === 0 || testingAll}
            >
              {testingAll ? (
                <Loader2 size={14} className={styles.spinner} />
              ) : (
                <FlaskConical size={14} />
              )}
            </button>
          ) : null}
          <Button
            variant="ghost"
            size="small"
            icon={Plus}
            onClick={handleAddProviderClick}
            title={addButtonTitle || addButtonLabel || t("servicePage.addRule")}
          />
        </div>
      </div>

      <div className={styles.ruleListContent}>
        {providers.length === 0 ? (
          <p className={styles.emptyHint}>{emptyMessage || t("servicePage.noRulesHint")}</p>
        ) : (
          <ul className={`${styles.ruleItems} ${styles.ruleItemsTwoColumn}`}>
            {providers.map(provider => {
              const health = resolveProviderHealthPresentation(
                providerHealthByProviderId?.[provider.id],
                Boolean(testingProviderIds?.[provider.id]),
                t
              )
              const websiteHref = resolveProviderWebsiteHref(provider.website)

              return (
                <li key={provider.id}>
                  <div
                    className={`${styles.ruleItemContainer} ${styles.ruleItemContainerCompact} ${provider.id === activeProviderId ? styles.ruleItemContainerActive : ""}`}
                  >
                    <div className={`${styles.ruleCardTop} ${styles.providerCompactCardTop}`}>
                      <div className={styles.providerCompactCardHeader}>
                        <div className={styles.ruleTitleLine}>
                          <span className={styles.ruleModel}>{provider.name}</span>
                          {websiteHref ? (
                            <a
                              className={styles.ruleTitleExternalLink}
                              href={websiteHref}
                              target="_blank"
                              rel="noreferrer"
                              title={t("ruleForm.officialWebsite")}
                              aria-label={`${t("ruleForm.officialWebsite")}: ${provider.name}`}
                            >
                              <ExternalLink size={13} />
                            </a>
                          ) : null}
                          <span className={styles.ruleDirection}>
                            {t(`ruleProtocol.${provider.protocol}`)}
                          </span>
                          {provider.id === activeProviderId && (
                            <span className={styles.ruleCurrentBadgeInline}>
                              {t("servicePage.current")}
                            </span>
                          )}
                        </div>
                        <div className={styles.ruleHeaderRight}>
                          <div
                            className={`${styles.ruleActionButtons} ${styles.ruleActionButtonsCompact}`}
                          >
                            {showActivate && provider.id !== activeProviderId && (
                              <button
                                type="button"
                                className={styles.activateIconButton}
                                onClick={() => onActivate(provider.id)}
                                title={t("servicePage.activateRule")}
                                aria-label={`${t("servicePage.activateRule")}: ${provider.name}`}
                                disabled={activatingProviderId === provider.id}
                              >
                                <Play size={13} />
                              </button>
                            )}
                            <button
                              type="button"
                              className={styles.editButton}
                              onClick={() => handleProviderEdit(provider.id)}
                              data-tooltip={t("servicePage.editRule")}
                              aria-label={`${t("servicePage.editRule")}: ${provider.name}`}
                            >
                              <Pencil size={14} />
                            </button>
                            {onDuplicate && (
                              <button
                                type="button"
                                className={styles.editButton}
                                onClick={() => onDuplicate(provider.id)}
                                data-tooltip={t("providersPage.duplicateProvider")}
                                aria-label={`${t("providersPage.duplicateProvider")}: ${provider.name}`}
                              >
                                <Copy size={14} />
                              </button>
                            )}
                            {onTestModel && (
                              <button
                                type="button"
                                className={styles.testIconButton}
                                onClick={() => onTestModel(provider.id)}
                                data-tooltip={
                                  testingProviderIds?.[provider.id]
                                    ? t("servicePage.testingModel")
                                    : t("servicePage.testModel")
                                }
                                aria-label={`${t("servicePage.testModel")}: ${provider.name}`}
                                disabled={Boolean(testingProviderIds?.[provider.id])}
                              >
                                {testingProviderIds?.[provider.id] ? (
                                  <Loader2 size={14} className={styles.spinner} />
                                ) : (
                                  <FlaskConical size={14} />
                                )}
                              </button>
                            )}
                            <button
                              type="button"
                              className={styles.deleteButton}
                              onClick={() => onDelete(provider.id)}
                              data-tooltip={resolvedDeleteActionLabel}
                              aria-label={`${resolvedDeleteActionLabel}: ${provider.name}`}
                            >
                              <Trash2 size={14} />
                            </button>
                          </div>
                        </div>
                      </div>
                      <div className={styles.providerCompactSummaryRow}>
                        <div className={styles.providerCompactModelPanel}>
                          <span className={styles.providerPanelEyebrow}>
                            {t("servicePage.defaultModel")}
                          </span>
                          <span
                            className={styles.providerCompactModelValue}
                            title={provider.defaultModel?.trim() || "-"}
                          >
                            {provider.defaultModel?.trim() || "-"}
                          </span>
                        </div>
                        <div className={styles.providerCompactSignalPanel}>
                          <span
                            className={`${styles.providerHealthStatus} ${health.statusClassName}`}
                          >
                            {health.statusLabel}
                          </span>
                          {health.latencyLabel ? (
                            <span className={styles.providerHealthMetric}>
                              {health.latencyLabel}
                            </span>
                          ) : null}
                        </div>
                      </div>
                    </div>
                  </div>
                </li>
              )
            })}
          </ul>
        )}
      </div>
    </div>
  )
}

export default ProviderList
