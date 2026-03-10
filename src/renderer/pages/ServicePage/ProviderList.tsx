import { Copy, Pencil, Play, Plus, Trash2 } from "lucide-react"
import type React from "react"
import { useNavigate } from "react-router-dom"
import { Button } from "@/components"
import { useTranslation } from "@/hooks"
import type { Group } from "@/types"
import styles from "./ServicePage.module.css"

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
        <Button
          variant="ghost"
          size="small"
          icon={Plus}
          onClick={handleAddProviderClick}
          title={addButtonTitle || addButtonLabel || t("servicePage.addRule")}
        />
      </div>

      <div className={styles.ruleListContent}>
        {providers.length === 0 ? (
          <p className={styles.emptyHint}>{emptyMessage || t("servicePage.noRulesHint")}</p>
        ) : (
          <ul className={styles.ruleItems}>
            {providers.map(provider => (
              <li
                key={provider.id}
                className={`${styles.ruleItemContainer} ${styles.ruleItemContainerCompact} ${provider.id === activeProviderId ? styles.ruleItemContainerActive : ""}`}
              >
                <div className={styles.ruleCardTop}>
                  <div className={styles.ruleItem}>
                    <div className={styles.ruleTitleLine}>
                      <span className={styles.ruleModel}>{provider.name}</span>
                      <span className={styles.ruleDirection}>
                        {t(`ruleProtocol.${provider.protocol}`)}
                      </span>
                      {provider.id === activeProviderId && (
                        <span className={styles.ruleCurrentBadgeInline}>
                          {t("servicePage.current")}
                        </span>
                      )}
                    </div>
                    <div className={styles.ruleAssociationMeta}>
                      <span
                        className={styles.ruleAssociationMetaItem}
                        title={provider.defaultModel?.trim() || "-"}
                      >
                        <span className={styles.ruleAssociationMetaLabel}>
                          {t("servicePage.defaultModel")}
                        </span>
                        <span className={styles.ruleAssociationMetaValue}>
                          {provider.defaultModel?.trim() || "-"}
                        </span>
                      </span>
                      <span
                        className={styles.ruleAssociationMetaItem}
                        title={provider.apiAddress?.trim() || "-"}
                      >
                        <span className={styles.ruleAssociationMetaLabel}>
                          {t("servicePage.apiAddress")}
                        </span>
                        <span className={styles.ruleAssociationMetaValue}>
                          {provider.apiAddress?.trim() || "-"}
                        </span>
                      </span>
                    </div>
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
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  )
}

export default ProviderList
