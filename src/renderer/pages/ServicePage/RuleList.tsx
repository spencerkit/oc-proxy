import { Check, ChevronRight, Folder, Loader2, Play, Plus, RefreshCw, Trash2 } from "lucide-react"
import type React from "react"
import { useNavigate } from "react-router-dom"
import { Button } from "@/components"
import { useTranslation } from "@/hooks"
import type { Group, RuleQuotaSnapshot } from "@/types"
import styles from "./ServicePage.module.css"

export interface ServicePageProps {
  groups: Group[]
  activeGroupId: string | null
  onSelectGroup: (groupId: string) => void
  onAddGroup: () => void
  onDeleteGroup: (groupId: string) => void
}

/**
 * GroupList Component
 * Displays a list of groups in the sidebar
 */
export const GroupList: React.FC<{
  groups: Group[]
  activeGroupId: string | null
  onSelect: (groupId: string) => void
  onAdd: () => void
}> = ({ groups, activeGroupId, onSelect, onAdd }) => {
  const { t } = useTranslation()

  return (
    <div className={styles.groupList}>
      <div className={styles.groupListHeader}>
        <h3>{t("servicePage.groupInfo")}</h3>
        <Button
          variant="ghost"
          size="small"
          icon={Plus}
          onClick={onAdd}
          title={t("header.addGroup")}
        />
      </div>
      <div className={styles.groupListContent}>
        {groups.length === 0 ? (
          <p className={styles.emptyHint}>{t("servicePage.noGroupsHint")}</p>
        ) : (
          <ul className={styles.groupItems}>
            {groups.map(group => (
              <li key={group.id}>
                <button
                  type="button"
                  className={`${styles.groupItem} ${group.id === activeGroupId ? styles.active : ""}`}
                  onClick={() => onSelect(group.id)}
                >
                  <Folder size={16} className={styles.groupIcon} />
                  <span className={styles.groupName}>{group.name}</span>
                  <span className={styles.groupPath}>/{group.id}</span>
                  {group.id === activeGroupId && <Check size={14} className={styles.activeIcon} />}
                  <ChevronRight size={14} className={styles.chevron} />
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  )
}

/**
 * RuleList Component
 * Displays rules within a group
 */
export const RuleList: React.FC<{
  rules: Group["rules"]
  activeRuleId: string | null
  onSelect: (ruleId: string) => void
  onActivate: (ruleId: string) => void | Promise<void>
  activatingRuleId?: string | null
  quotaByRuleId?: Record<string, RuleQuotaSnapshot | undefined>
  quotaLoadingByRuleId?: Record<string, boolean | undefined>
  onRefreshQuota?: (ruleId: string) => void | Promise<void>
  onDelete: (ruleId: string) => void
  groupName: string
  groupId: string
}> = ({
  rules,
  activeRuleId,
  onSelect,
  onActivate,
  activatingRuleId,
  quotaByRuleId,
  quotaLoadingByRuleId,
  onRefreshQuota,
  onDelete,
  groupName,
  groupId,
}) => {
  const { t } = useTranslation()
  const navigate = useNavigate()

  const handleRuleClick = (ruleId: string) => {
    navigate(`/groups/${groupId}/rules/${ruleId}/edit`)
  }

  const handleAddRuleClick = () => {
    navigate(`/groups/${groupId}/rules/new`)
  }

  const formatQuotaValue = (value?: number | null) => {
    if (value === null || value === undefined || Number.isNaN(value)) {
      return "-"
    }
    const abs = Math.abs(value)
    if (abs >= 1) {
      return value.toFixed(2).replace(/\\.00$/, "")
    }
    return value.toFixed(4).replace(/0+$/, "").replace(/\\.$/, "")
  }

  const formatTokenValue = (value?: number | null) => {
    if (value === null || value === undefined || Number.isNaN(value)) {
      return "-"
    }
    const abs = Math.abs(value)
    if (abs >= 1_000_000) {
      return `${(value / 1_000_000).toFixed(2).replace(/\\.00$/, "")}M`
    }
    return Number.isInteger(value)
      ? String(value)
      : value
          .toFixed(2)
          .replace(/\\.00$/, "")
          .replace(/\\.$/, "")
  }

  const formatResetAt = (raw?: string | null) => {
    if (!raw) return null
    const text = raw.trim()
    if (!text) return null

    let timestampMs: number | null = null
    if (/^-?\d+(\.\d+)?$/.test(text)) {
      const parsed = Number(text)
      if (Number.isFinite(parsed)) {
        const absText = text.startsWith("-") ? text.slice(1) : text
        timestampMs = absText.length <= 10 ? parsed * 1000 : parsed
      }
    } else {
      const parsed = Date.parse(text)
      if (Number.isFinite(parsed)) {
        timestampMs = parsed
      }
    }

    if (timestampMs === null) return null
    const date = new Date(timestampMs)
    if (Number.isNaN(date.getTime())) return null

    const yy = String(date.getFullYear()).slice(-2)
    const MM = String(date.getMonth() + 1).padStart(2, "0")
    const dd = String(date.getDate()).padStart(2, "0")
    const HH = String(date.getHours()).padStart(2, "0")
    const mm = String(date.getMinutes()).padStart(2, "0")
    return `${yy}-${MM}-${dd} ${HH}:${mm}`
  }

  const resolveQuotaBadge = (rule: Group["rules"][number]) => {
    if (!rule.quota?.enabled) {
      return {
        className: styles.quotaBadgeUnsupported,
        text: t("ruleQuota.unsupported"),
        resetAt: null,
      }
    }

    const snapshot = quotaByRuleId?.[rule.id]
    if (!snapshot) {
      return {
        className: styles.quotaBadgeUnknown,
        text: t("ruleQuota.pending"),
        resetAt: null,
      }
    }

    if (snapshot.status === "empty") {
      return {
        className: styles.quotaBadgeEmpty,
        text: t("ruleQuota.empty"),
        resetAt: formatResetAt(snapshot.resetAt),
      }
    }

    if (snapshot.status === "error") {
      return {
        className: styles.quotaBadgeError,
        text: t("ruleQuota.error"),
        resetAt: formatResetAt(snapshot.resetAt),
      }
    }

    if (snapshot.status === "unknown") {
      return {
        className: styles.quotaBadgeUnknown,
        text: t("ruleQuota.unknown"),
        resetAt: formatResetAt(snapshot.resetAt),
      }
    }

    if (snapshot.status === "unsupported") {
      return {
        className: styles.quotaBadgeUnsupported,
        text: t("ruleQuota.unsupported"),
        resetAt: formatResetAt(snapshot.resetAt),
      }
    }

    const rawUnitType = rule.quota?.unitType
    const unitType =
      rawUnitType === "amount" || rawUnitType === "tokens" || rawUnitType === "percentage"
        ? rawUnitType
        : "percentage"
    const resolveDisplayValue = () => {
      if (unitType === "percentage") {
        const basis =
          snapshot.percent !== null && snapshot.percent !== undefined
            ? snapshot.percent
            : snapshot.remaining !== null && snapshot.remaining !== undefined
              ? snapshot.remaining
              : null
        if (basis === null || Number.isNaN(basis)) return "-"
        const value = formatQuotaValue(basis)
        return value.endsWith("%") ? value : `${value}%`
      }

      if (unitType === "amount") {
        return formatQuotaValue(snapshot.remaining)
      }

      if (snapshot.unit?.trim()) {
        return `${formatQuotaValue(snapshot.remaining)} ${snapshot.unit.trim()}`
      }
      return formatTokenValue(snapshot.remaining)
    }
    const renderedValue = resolveDisplayValue()
    const isTokenWithoutUnit = unitType === "tokens" && !snapshot.unit?.trim()

    if (snapshot.status === "low") {
      return {
        className: styles.quotaBadgeLow,
        text: isTokenWithoutUnit
          ? t("ruleQuota.lowToken", { value: renderedValue })
          : t("ruleQuota.low", { value: renderedValue }),
        resetAt: formatResetAt(snapshot.resetAt),
      }
    }

    return {
      className: styles.quotaBadgeOk,
      text: isTokenWithoutUnit
        ? t("ruleQuota.remainingToken", { value: renderedValue })
        : t("ruleQuota.remaining", { value: renderedValue }),
      resetAt: formatResetAt(snapshot.resetAt),
    }
  }

  return (
    <div className={styles.ruleList}>
      <div className={styles.ruleListHeader}>
        <div className={styles.ruleHeaderTitle}>
          <h3>{t("servicePage.ruleName")}</h3>
          <span className={styles.countBadge}>{rules.length}</span>
          <span className={styles.ruleGroupName} title={groupName}>
            {groupName}
          </span>
        </div>
        <Button
          variant="ghost"
          size="small"
          icon={Plus}
          onClick={handleAddRuleClick}
          title={t("servicePage.addRule")}
        />
      </div>
      <div className={styles.ruleListContent}>
        {rules.length === 0 ? (
          <p className={styles.emptyHint}>{t("servicePage.noRulesHint")}</p>
        ) : (
          <ul className={styles.ruleItems}>
            {rules.map(rule => (
              <li
                key={rule.id}
                className={`${styles.ruleItemContainer} ${rule.id === activeRuleId ? styles.ruleItemContainerActive : ""}`}
              >
                <div className={styles.ruleCardTop}>
                  <button
                    type="button"
                    className={`${styles.ruleItem} ${rule.id === activeRuleId ? styles.active : ""}`}
                    onClick={() => {
                      onSelect(rule.id)
                      handleRuleClick(rule.id)
                    }}
                  >
                    <div className={styles.ruleMainLine}>
                      <span className={styles.ruleModel}>{rule.name}</span>
                      <span className={styles.ruleDirection}>
                        {t(`ruleProtocol.${rule.protocol}`)}
                      </span>
                      {rule.id === activeRuleId && (
                        <span className={styles.currentBadge}>{t("servicePage.current")}</span>
                      )}
                    </div>
                  </button>
                  <div className={styles.ruleActionButtons}>
                    {rule.id !== activeRuleId && (
                      <button
                        type="button"
                        className={styles.activateButton}
                        onClick={() => onActivate(rule.id)}
                        title={t("servicePage.activateRule")}
                        aria-label={`${t("servicePage.activateRule")}: ${rule.name}`}
                        disabled={activatingRuleId === rule.id}
                      >
                        <Play size={13} />
                        <span>{t("servicePage.activateRule")}</span>
                      </button>
                    )}
                    <button
                      type="button"
                      className={styles.deleteButton}
                      onClick={() => onDelete(rule.id)}
                      title={t("servicePage.deleteRule")}
                      aria-label={`${t("servicePage.deleteRule")}: ${rule.name}`}
                    >
                      <Trash2 size={14} />
                    </button>
                  </div>
                </div>
                {(() => {
                  const badge = resolveQuotaBadge(rule)
                  return (
                    <div className={styles.ruleCardBottom}>
                      <div className={styles.ruleQuotaWrap}>
                        <span
                          className={`${styles.quotaBadge} ${badge.className}`}
                          title={badge.text}
                        >
                          {badge.text}
                        </span>
                        {badge.resetAt && (
                          <span className={styles.quotaResetAt}>
                            {t("ruleQuota.resetAt", { value: badge.resetAt })}
                          </span>
                        )}
                      </div>
                      {rule.quota?.enabled && (
                        <button
                          type="button"
                          className={styles.quotaRefreshButton}
                          onClick={() => onRefreshQuota?.(rule.id)}
                          title={t("ruleQuota.refresh")}
                          aria-label={`${t("ruleQuota.refresh")}: ${rule.name}`}
                          disabled={Boolean(quotaLoadingByRuleId?.[rule.id])}
                        >
                          {quotaLoadingByRuleId?.[rule.id] ? (
                            <Loader2 size={14} className={styles.spinner} />
                          ) : (
                            <RefreshCw size={14} />
                          )}
                        </button>
                      )}
                    </div>
                  )
                })()}
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  )
}

export default RuleList
