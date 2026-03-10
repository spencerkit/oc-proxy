import { Copy, FlaskConical, Loader2, Pencil, Play, Plus, RefreshCw, Trash2 } from "lucide-react"
import type React from "react"
import { useNavigate } from "react-router-dom"
import { Button } from "@/components"
import { useTranslation } from "@/hooks"
import type { Group, RuleCardStatsItem, RuleQuotaSnapshot } from "@/types"
import { formatTokenMillions } from "@/utils/tokenFormat"
import styles from "./ServicePage.module.css"

/** Resolves currency prefix. */
function resolveCurrencyPrefix(currency?: string): string {
  const normalized = currency?.trim().toUpperCase()
  if (!normalized) return "$"
  if (normalized === "USD") return "$"
  if (normalized === "CNY" || normalized === "RMB") return "¥"
  if (normalized === "EUR") return "€"
  if (normalized === "JPY") return "¥"
  return `${normalized} `
}

/**
 * RuleList Component
 * Displays rules within a group
 */
export const RuleList: React.FC<{
  providers: Group["providers"]
  activeProviderId: string | null
  onActivate: (providerId: string) => void | Promise<void>
  activatingProviderId?: string | null
  quotaByRuleId?: Record<string, RuleQuotaSnapshot | undefined>
  quotaLoadingByRuleId?: Record<string, boolean | undefined>
  cardStatsByRuleId?: Record<string, RuleCardStatsItem | undefined>
  onRefreshQuota?: (providerId: string) => void | Promise<void>
  onTestModel?: (providerId: string) => void | Promise<void>
  testingProviderIds?: Record<string, boolean | undefined>
  onDuplicate?: (providerId: string) => void | Promise<void>
  onDelete: (providerId: string) => void
  groupId?: string
  onEdit?: (providerId: string) => void
  onAdd?: () => void
  showActivate?: boolean
  addButtonLabel?: string
  addButtonTitle?: string
  deleteActionLabel?: string
  emptyMessage?: string
  displayMode?: "full" | "association" | "catalog"
}> = ({
  providers,
  activeProviderId,
  onActivate,
  activatingProviderId,
  quotaByRuleId,
  quotaLoadingByRuleId,
  cardStatsByRuleId,
  onRefreshQuota,
  onTestModel,
  testingProviderIds,
  onDuplicate,
  onDelete,
  groupId,
  onEdit,
  onAdd,
  showActivate = true,
  addButtonLabel,
  addButtonTitle,
  deleteActionLabel,
  emptyMessage,
  displayMode = "full",
}) => {
  const { t } = useTranslation()
  const navigate = useNavigate()
  const isAssociationMode = displayMode === "association"
  const isCatalogMode = displayMode === "catalog"
  const isFullMode = displayMode === "full"

  const handleProviderEdit = (providerId: string) => {
    if (onEdit) {
      onEdit(providerId)
      return
    }
    if (!groupId) return
    navigate(`/groups/${groupId}/providers/${providerId}/edit`)
  }

  const handleAddRuleClick = () => {
    if (onAdd) {
      onAdd()
      return
    }
    if (!groupId) return
    navigate(`/groups/${groupId}/providers/new`)
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

  const resolveQuotaBadge = (provider: Group["providers"][number]) => {
    if (!provider.quota?.enabled) {
      return {
        className: styles.quotaBadgeUnsupported,
        text: t("ruleQuota.unsupported"),
        resetAt: null,
      }
    }

    const snapshot = quotaByRuleId?.[provider.id]
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

    const rawUnitType = provider.quota?.unitType
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

  const formatCompactRequest = (value: number) => {
    if (!Number.isFinite(value)) return "0"
    if (Math.abs(value) >= 1_000_000) {
      return `${(value / 1_000_000).toFixed(1).replace(/\\.0$/, "")}M`
    }
    if (Math.abs(value) >= 1_000) {
      return `${(value / 1_000).toFixed(1).replace(/\\.0$/, "")}k`
    }
    return String(Math.round(value))
  }

  const formatMiniTime = (hourIso: string) => {
    const date = new Date(hourIso)
    if (Number.isNaN(date.getTime())) {
      return hourIso
    }
    const MM = String(date.getMonth() + 1).padStart(2, "0")
    const dd = String(date.getDate()).padStart(2, "0")
    const HH = String(date.getHours()).padStart(2, "0")
    return `${MM}-${dd} ${HH}:00`
  }

  const formatExactCount = (value: number) => {
    if (!Number.isFinite(value)) return "0"
    return Math.round(value).toLocaleString()
  }

  const formatTokenCount = (value: number) => {
    if (!Number.isFinite(value)) return "0"
    if (value > 1_000_000) {
      return `${(value / 1_000_000).toFixed(2).replace(/\\.00$/, "")}M`
    }
    return Math.round(value).toLocaleString()
  }

  const formatCostConsumed = (value: number, currency?: string) => {
    const safe = Number.isFinite(value) ? Math.max(0, value) : 0
    const prefix = resolveCurrencyPrefix(currency)
    if (safe === 0) return `${prefix}0.00`
    if (safe < 0.0001) return `${prefix}<0.0001`
    if (safe < 1) return `${prefix}${safe.toFixed(4)}`
    return `${prefix}${safe.toFixed(2)}`
  }

  const renderRuleMiniChart = (stats?: RuleCardStatsItem) => {
    const hourly = [...(stats?.hourly ?? [])].sort((a, b) => {
      return new Date(a.hour).getTime() - new Date(b.hour).getTime()
    })
    if (hourly.length === 0) {
      return <div className={styles.ruleMiniChartEmpty}>{t("servicePage.noStatsData")}</div>
    }

    const width = 132
    const height = 30
    const padX = 3
    const padY = 3
    const innerW = width - padX * 2
    const innerH = height - padY * 2
    const tokenMax = Math.max(1, ...hourly.map(point => point.tokens))
    const requestMax = Math.max(1, ...hourly.map(point => point.requests))
    const step = hourly.length > 1 ? innerW / (hourly.length - 1) : 0
    const barWidth = Math.max(2, Math.min(5, innerW / Math.max(hourly.length * 1.8, 1)))

    const resolveTokenBarColor = (ratioRaw: number) => {
      const ratio = Math.min(1, Math.max(0, ratioRaw))
      const hue = 160 - ratio * 145
      const saturation = 72 + ratio * 8
      const lightness = 58 - ratio * 14
      return `hsl(${hue} ${saturation}% ${lightness}%)`
    }

    const linePoints = hourly.map((point, index) => {
      const x = padX + (hourly.length === 1 ? innerW / 2 : index * step)
      const ratio = point.requests / requestMax
      const y = padY + innerH - ratio * innerH
      return { x, y }
    })

    const buildSmoothPath = (points: Array<{ x: number; y: number }>) => {
      if (points.length === 0) return ""
      if (points.length === 1) {
        return `M ${points[0].x.toFixed(2)} ${points[0].y.toFixed(2)}`
      }
      let d = `M ${points[0].x.toFixed(2)} ${points[0].y.toFixed(2)}`
      for (let i = 1; i < points.length - 1; i++) {
        const xc = (points[i].x + points[i + 1].x) / 2
        const yc = (points[i].y + points[i + 1].y) / 2
        d += ` Q ${points[i].x.toFixed(2)} ${points[i].y.toFixed(2)} ${xc.toFixed(2)} ${yc.toFixed(2)}`
      }
      const prev = points[points.length - 2]
      const last = points[points.length - 1]
      d += ` Q ${prev.x.toFixed(2)} ${prev.y.toFixed(2)} ${last.x.toFixed(2)} ${last.y.toFixed(2)}`
      return d
    }

    const smoothPath = buildSmoothPath(linePoints)
    const buildMiniTooltip = (point: RuleCardStatsItem["hourly"][number]) => {
      return [
        `${t("servicePage.miniTime")}: ${formatMiniTime(point.hour)}`,
        `${t("servicePage.miniRequests")}: ${formatExactCount(point.requests)}`,
        `${t("servicePage.miniTokens")}: ${formatTokenCount(point.tokens)}`,
      ].join("\n")
    }

    return (
      <svg
        className={styles.ruleMiniChart}
        viewBox={`0 0 ${width} ${height}`}
        preserveAspectRatio="none"
        role="img"
        aria-label={t("servicePage.ruleMiniTrend")}
      >
        {hourly.map((point, index) => {
          const centerX = padX + (hourly.length === 1 ? innerW / 2 : index * step)
          const barH = (point.tokens / tokenMax) * innerH
          const y = padY + innerH - barH
          return (
            <rect
              key={`${point.hour}-bar`}
              className={styles.ruleMiniBar}
              x={centerX - barWidth / 2}
              y={y}
              width={barWidth}
              height={Math.max(1, barH)}
              rx={1}
              fill={resolveTokenBarColor(point.tokens / tokenMax)}
            />
          )
        })}
        <path className={styles.ruleMiniLine} d={smoothPath} />
        {linePoints.map((point, index) => (
          <circle
            key={`${hourly[index].hour}-point`}
            className={styles.ruleMiniPoint}
            cx={point.x}
            cy={point.y}
            r={1.8}
          />
        ))}
        {hourly.map((point, index) => {
          const slotStart = hourly.length === 1 ? padX : padX + index * step - step / 2
          const slotEnd =
            hourly.length === 1
              ? padX + innerW
              : index === hourly.length - 1
                ? padX + innerW
                : padX + (index + 1) * step - step / 2
          const x = Math.max(padX, slotStart)
          const right = Math.min(padX + innerW, slotEnd)
          const width = Math.max(1, right - x)
          return (
            <rect
              key={`${point.hour}-hover`}
              className={styles.ruleMiniHoverSlot}
              x={x}
              y={padY}
              width={width}
              height={innerH}
            >
              <title>{buildMiniTooltip(point)}</title>
            </rect>
          )
        })}
      </svg>
    )
  }

  return (
    <div className={`${styles.ruleList} ${isCatalogMode ? styles.ruleListCatalog : ""}`}>
      <div
        className={`${styles.ruleListHeader} ${isCatalogMode ? styles.ruleListHeaderCatalog : ""}`}
      >
        <div className={styles.ruleHeaderTitle}>
          <h3>{t("servicePage.ruleName")}</h3>
          <span className={styles.countBadge}>{providers.length}</span>
        </div>
        <Button
          variant="ghost"
          size="small"
          icon={Plus}
          onClick={handleAddRuleClick}
          title={addButtonTitle || addButtonLabel || t("servicePage.addRule")}
        />
      </div>
      <div
        className={`${styles.ruleListContent} ${isCatalogMode ? styles.ruleListContentCatalog : ""}`}
      >
        {providers.length === 0 ? (
          <p className={styles.emptyHint}>{emptyMessage || t("servicePage.noRulesHint")}</p>
        ) : (
          <ul className={`${styles.ruleItems} ${isCatalogMode ? styles.ruleItemsCatalog : ""}`}>
            {providers.map(provider => {
              return (
                <li
                  key={provider.id}
                  className={`${styles.ruleItemContainer} ${isAssociationMode ? styles.ruleItemContainerCompact : ""} ${isCatalogMode ? styles.ruleItemContainerCatalog : ""} ${provider.id === activeProviderId ? styles.ruleItemContainerActive : ""}`}
                >
                  {provider.id === activeProviderId && isFullMode && (
                    <span className={styles.enabledCornerBadge}>{t("servicePage.current")}</span>
                  )}
                  <div className={styles.ruleCardTop}>
                    <div className={styles.ruleItem}>
                      <div className={styles.ruleTitleLine}>
                        <span className={styles.ruleModel}>{provider.name}</span>
                        <span className={styles.ruleDirection}>
                          {t(`ruleProtocol.${provider.protocol}`)}
                        </span>
                        {isAssociationMode && provider.id === activeProviderId && (
                          <span className={styles.ruleCurrentBadgeInline}>
                            {t("servicePage.current")}
                          </span>
                        )}
                        {isFullMode && (
                          <span
                            className={styles.ruleApiAddress}
                            title={provider.apiAddress?.trim() || "-"}
                          >
                            {provider.apiAddress?.trim() || "-"}
                          </span>
                        )}
                      </div>
                      {isAssociationMode && (
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
                      )}
                      {isCatalogMode && (
                        <>
                          <div className={styles.ruleCatalogMeta}>
                            <span
                              className={styles.ruleCatalogMetaItem}
                              title={provider.defaultModel?.trim() || "-"}
                            >
                              <span className={styles.ruleCatalogMetaLabel}>
                                {t("servicePage.defaultModel")}
                              </span>
                              <span className={styles.ruleCatalogMetaValue}>
                                {provider.defaultModel?.trim() || "-"}
                              </span>
                            </span>
                            <span
                              className={styles.ruleCatalogMetaItem}
                              title={provider.apiAddress?.trim() || "-"}
                            >
                              <span className={styles.ruleCatalogMetaLabel}>
                                {t("servicePage.apiAddress")}
                              </span>
                              <span className={styles.ruleCatalogMetaValue}>
                                {provider.apiAddress?.trim() || "-"}
                              </span>
                            </span>
                          </div>
                        </>
                      )}
                    </div>
                    <div className={styles.ruleHeaderRight}>
                      <div
                        className={`${styles.ruleActionButtons} ${isAssociationMode ? styles.ruleActionButtonsCompact : ""}`}
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
                          data-tooltip={deleteActionLabel || t("servicePage.deleteRule")}
                          aria-label={`${deleteActionLabel || t("servicePage.deleteRule")}: ${provider.name}`}
                        >
                          <Trash2 size={14} />
                        </button>
                        {onTestModel && !isAssociationMode && (
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
                      </div>
                    </div>
                  </div>
                  {(isFullMode || isCatalogMode) &&
                    (() => {
                      const badge = resolveQuotaBadge(provider)
                      const cardStats = cardStatsByRuleId?.[provider.id]
                      return (
                        <div
                          className={`${styles.ruleCardBottom} ${isCatalogMode ? styles.ruleCardBottomCatalog : ""}`}
                        >
                          <div
                            className={`${styles.ruleMetaLeft} ${isCatalogMode ? styles.ruleMetaLeftCatalog : ""}`}
                          >
                            {provider.quota?.enabled && (
                              <button
                                type="button"
                                className={styles.quotaRefreshButton}
                                onClick={() => onRefreshQuota?.(provider.id)}
                                title={t("ruleQuota.refresh")}
                                aria-label={`${t("ruleQuota.refresh")}: ${provider.name}`}
                                disabled={Boolean(quotaLoadingByRuleId?.[provider.id])}
                              >
                                {quotaLoadingByRuleId?.[provider.id] ? (
                                  <Loader2 size={14} className={styles.spinner} />
                                ) : (
                                  <RefreshCw size={14} />
                                )}
                              </button>
                            )}
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
                          </div>
                          <div
                            className={`${styles.ruleTrendWrap} ${isCatalogMode ? styles.ruleTrendWrapCatalog : ""}`}
                          >
                            <div
                              className={`${styles.ruleTrendInlineMeta} ${isCatalogMode ? styles.ruleTrendInlineMetaCatalog : ""}`}
                            >
                              {(isCatalogMode || provider.cost?.enabled) && (
                                <span>
                                  {t("servicePage.miniCostConsumed", {
                                    value: formatCostConsumed(
                                      cardStats?.totalCost ?? 0,
                                      provider.cost?.currency || "USD"
                                    ),
                                  })}
                                </span>
                              )}
                              <span>
                                {t("servicePage.miniRequests")}:{" "}
                                {formatCompactRequest(cardStats?.requests ?? 0)}
                              </span>
                              <span>
                                {t("servicePage.miniInputTokens")}:{" "}
                                {formatTokenMillions(cardStats?.inputTokens ?? 0)}
                              </span>
                              <span>
                                {t("servicePage.miniOutputTokens")}:{" "}
                                {formatTokenMillions(cardStats?.outputTokens ?? 0)}
                              </span>
                              <span>
                                {t("servicePage.miniCacheInputTokens")}:{" "}
                                {formatTokenMillions(cardStats?.cacheReadTokens ?? 0)}
                              </span>
                              <span>
                                {t("servicePage.miniCacheOutputTokens")}:{" "}
                                {formatTokenMillions(cardStats?.cacheWriteTokens ?? 0)}
                              </span>
                            </div>
                            {!isCatalogMode && renderRuleMiniChart(cardStats)}
                          </div>
                        </div>
                      )
                    })()}
                </li>
              )
            })}
          </ul>
        )}
      </div>
    </div>
  )
}

export default RuleList
