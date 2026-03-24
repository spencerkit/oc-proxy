import {
  Copy,
  ExternalLink,
  FlaskConical,
  Loader2,
  Pencil,
  Plus,
  RefreshCw,
  Trash2,
} from "lucide-react"
import type React from "react"
import { memo } from "react"
import { Button } from "@/components"
import { useTranslation } from "@/hooks"
import type {
  Group,
  ProviderModelHealthSnapshot,
  RuleCardStatsItem,
  RuleQuotaSnapshot,
} from "@/types"
import { formatProviderLatency } from "@/utils/providerTesting"
import { resolveProviderWebsiteHref } from "@/utils/providerWebsite"
import { formatTokenMillions } from "@/utils/tokenFormat"
import sharedStyles from "../ServicePage/ServicePage.module.css"

type RuleQuotaBadge = {
  className: string
  text: string
  resetAt: string | null
}

type CatalogProviderCardProps = {
  provider: Group["providers"][number]
  testing: boolean
  quotaLoading: boolean
  healthSnapshot?: ProviderModelHealthSnapshot | null
  deleteActionLabel: string
  badge: RuleQuotaBadge
  cardStats?: RuleCardStatsItem
  onEdit: (providerId: string) => void
  onDuplicate?: (providerId: string) => void | Promise<void>
  onDelete: (providerId: string) => void
  onTestModel?: (providerId: string) => void | Promise<void>
  onRefreshQuota?: (providerId: string) => void | Promise<void>
  formatCostConsumed: (value: number, currency?: string) => string
  formatCompactRequest: (value: number) => string
}

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
      statusClassName: sharedStyles.providerHealthStatusTesting,
      latencyLabel: null,
    }
  }

  if (!snapshot) {
    return {
      statusLabel: t("servicePage.availabilityUntested"),
      statusClassName: sharedStyles.providerHealthStatusUntested,
      latencyLabel: null,
    }
  }

  if (snapshot.status === "available") {
    return {
      statusLabel: t("servicePage.availabilityAvailable"),
      statusClassName: sharedStyles.providerHealthStatusAvailable,
      latencyLabel: formatProviderLatency(snapshot.latencyMs),
    }
  }

  return {
    statusLabel: t("servicePage.availabilityUnavailable"),
    statusClassName: sharedStyles.providerHealthStatusUnavailable,
    latencyLabel: formatProviderLatency(snapshot.latencyMs),
  }
}

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

function areRuleCardStatsEqual(prev?: RuleCardStatsItem, next?: RuleCardStatsItem): boolean {
  if (prev === next) return true
  if (!prev || !next) return !prev && !next
  return (
    prev.requests === next.requests &&
    prev.inputTokens === next.inputTokens &&
    prev.outputTokens === next.outputTokens &&
    prev.cacheReadTokens === next.cacheReadTokens &&
    prev.cacheWriteTokens === next.cacheWriteTokens &&
    prev.tokens === next.tokens &&
    prev.totalCost === next.totalCost
  )
}

const MemoCatalogProviderCard = memo<CatalogProviderCardProps>(
  ({
    provider,
    testing,
    quotaLoading,
    healthSnapshot,
    deleteActionLabel,
    badge,
    cardStats,
    onEdit,
    onDuplicate,
    onDelete,
    onTestModel,
    onRefreshQuota,
    formatCostConsumed,
    formatCompactRequest,
  }) => {
    const { t } = useTranslation()
    const websiteHref = resolveProviderWebsiteHref(provider.website)
    const health = resolveProviderHealthPresentation(healthSnapshot, testing, t)
    const formattedCostConsumed = formatCostConsumed(
      cardStats?.totalCost ?? 0,
      provider.cost?.currency || "USD"
    )
    const aggregatedInputTokens = (cardStats?.inputTokens ?? 0) + (cardStats?.cacheReadTokens ?? 0)
    const aggregatedOutputTokens =
      (cardStats?.outputTokens ?? 0) + (cardStats?.cacheWriteTokens ?? 0)

    return (
      <li className={`${sharedStyles.ruleItemContainer} ${sharedStyles.ruleItemContainerCatalog}`}>
        <div className={`${sharedStyles.ruleCardTop} ${sharedStyles.providerCatalogCardTop}`}>
          <div className={sharedStyles.providerCatalogCardHeader}>
            <div className={sharedStyles.providerCatalogLead}>
              <div className={sharedStyles.ruleTitleLine}>
                <span className={sharedStyles.ruleModel}>{provider.name}</span>
                {websiteHref ? (
                  <a
                    className={sharedStyles.ruleTitleExternalLink}
                    href={websiteHref}
                    target="_blank"
                    rel="noreferrer"
                    title={t("ruleForm.officialWebsite")}
                    aria-label={`${t("ruleForm.officialWebsite")}: ${provider.name}`}
                  >
                    <ExternalLink size={13} />
                  </a>
                ) : null}
                <span className={sharedStyles.ruleDirection}>
                  {t(`ruleProtocol.${provider.protocol}`)}
                </span>
              </div>
              <div className={sharedStyles.providerCatalogSignalRow}>
                <span className={`${sharedStyles.providerHealthStatus} ${health.statusClassName}`}>
                  {health.statusLabel}
                </span>
                {health.latencyLabel ? (
                  <span className={sharedStyles.providerHealthMetric}>{health.latencyLabel}</span>
                ) : null}
              </div>
            </div>
            <div className={sharedStyles.ruleHeaderRight}>
              <div className={sharedStyles.ruleActionButtons}>
                <button
                  type="button"
                  className={sharedStyles.editButton}
                  onClick={() => onEdit(provider.id)}
                  data-tooltip={t("servicePage.editRule")}
                  aria-label={`${t("servicePage.editRule")}: ${provider.name}`}
                >
                  <Pencil size={14} />
                </button>
                {onDuplicate && (
                  <button
                    type="button"
                    className={sharedStyles.editButton}
                    onClick={() => onDuplicate(provider.id)}
                    data-tooltip={t("providersPage.duplicateProvider")}
                    aria-label={`${t("providersPage.duplicateProvider")}: ${provider.name}`}
                  >
                    <Copy size={14} />
                  </button>
                )}
                <button
                  type="button"
                  className={sharedStyles.deleteButton}
                  onClick={() => onDelete(provider.id)}
                  data-tooltip={deleteActionLabel}
                  aria-label={`${deleteActionLabel}: ${provider.name}`}
                >
                  <Trash2 size={14} />
                </button>
                {onTestModel && (
                  <button
                    type="button"
                    className={sharedStyles.testIconButton}
                    onClick={() => onTestModel(provider.id)}
                    data-tooltip={
                      testing ? t("servicePage.testingModel") : t("servicePage.testModel")
                    }
                    aria-label={`${t("servicePage.testModel")}: ${provider.name}`}
                    disabled={testing}
                  >
                    {testing ? (
                      <Loader2 size={14} className={sharedStyles.spinner} />
                    ) : (
                      <FlaskConical size={14} />
                    )}
                  </button>
                )}
              </div>
            </div>
          </div>
          <div className={sharedStyles.providerCatalogSummaryRow}>
            <div className={sharedStyles.providerCatalogModelPanel}>
              <span className={sharedStyles.providerPanelEyebrow}>
                {t("servicePage.defaultModel")}
              </span>
              <span
                className={sharedStyles.providerCatalogModelValue}
                title={provider.defaultModel?.trim() || "-"}
              >
                {provider.defaultModel?.trim() || "-"}
              </span>
            </div>
            <div className={sharedStyles.providerCatalogUsagePanel}>
              <div className={sharedStyles.providerCatalogUsageRow}>
                <span className={sharedStyles.providerCatalogUsageItem}>
                  <span className={sharedStyles.providerCatalogUsageLabel}>
                    {t("servicePage.providerCost")}
                  </span>
                  <span className={sharedStyles.providerCatalogUsageValue}>
                    {formattedCostConsumed}
                  </span>
                </span>
                <span className={sharedStyles.providerCatalogUsageItem}>
                  <span className={sharedStyles.providerCatalogUsageLabel}>
                    {t("servicePage.miniRequests")}
                  </span>
                  <span className={sharedStyles.providerCatalogUsageValue}>
                    {formatCompactRequest(cardStats?.requests ?? 0)}
                  </span>
                </span>
              </div>
              <div className={sharedStyles.providerCatalogUsageRow}>
                <span className={sharedStyles.providerCatalogUsageItem}>
                  <span className={sharedStyles.providerCatalogUsageLabel}>
                    {t("servicePage.miniInputTokens")}
                  </span>
                  <span className={sharedStyles.providerCatalogUsageValue}>
                    {formatTokenMillions(aggregatedInputTokens)}
                  </span>
                </span>
                <span className={sharedStyles.providerCatalogUsageItem}>
                  <span className={sharedStyles.providerCatalogUsageLabel}>
                    {t("servicePage.miniOutputTokens")}
                  </span>
                  <span className={sharedStyles.providerCatalogUsageValue}>
                    {formatTokenMillions(aggregatedOutputTokens)}
                  </span>
                </span>
              </div>
            </div>
          </div>
        </div>
        <div className={`${sharedStyles.ruleCardBottom} ${sharedStyles.ruleCardBottomCatalog}`}>
          <div className={`${sharedStyles.ruleMetaLeft} ${sharedStyles.ruleMetaLeftCatalog}`}>
            {provider.quota?.enabled && (
              <button
                type="button"
                className={sharedStyles.quotaRefreshButton}
                onClick={() => onRefreshQuota?.(provider.id)}
                title={t("ruleQuota.refresh")}
                aria-label={`${t("ruleQuota.refresh")}: ${provider.name}`}
                disabled={quotaLoading}
              >
                {quotaLoading ? (
                  <Loader2 size={14} className={sharedStyles.spinner} />
                ) : (
                  <RefreshCw size={14} />
                )}
              </button>
            )}
            <div className={sharedStyles.ruleQuotaWrap}>
              <span className={`${sharedStyles.quotaBadge} ${badge.className}`} title={badge.text}>
                {badge.text}
              </span>
              {badge.resetAt && (
                <span className={sharedStyles.quotaResetAt}>
                  {t("ruleQuota.resetAt", { value: badge.resetAt })}
                </span>
              )}
            </div>
          </div>
        </div>
      </li>
    )
  },
  (prev, next) => {
    return (
      prev.provider.id === next.provider.id &&
      prev.provider.name === next.provider.name &&
      prev.provider.protocol === next.provider.protocol &&
      prev.provider.defaultModel === next.provider.defaultModel &&
      prev.provider.website === next.provider.website &&
      Boolean(prev.provider.quota?.enabled) === Boolean(next.provider.quota?.enabled) &&
      (prev.provider.cost?.currency || "USD") === (next.provider.cost?.currency || "USD") &&
      prev.testing === next.testing &&
      prev.quotaLoading === next.quotaLoading &&
      prev.healthSnapshot?.status === next.healthSnapshot?.status &&
      prev.healthSnapshot?.latencyMs === next.healthSnapshot?.latencyMs &&
      prev.healthSnapshot?.resolvedModel === next.healthSnapshot?.resolvedModel &&
      prev.healthSnapshot?.testedAt === next.healthSnapshot?.testedAt &&
      prev.deleteActionLabel === next.deleteActionLabel &&
      prev.badge.className === next.badge.className &&
      prev.badge.text === next.badge.text &&
      prev.badge.resetAt === next.badge.resetAt &&
      areRuleCardStatsEqual(prev.cardStats, next.cardStats)
    )
  }
)

/**
 * ProviderList Component
 * Displays providers for global providers catalog management.
 */
export const ProviderList: React.FC<{
  providers: Group["providers"]
  quotaByProviderId?: Record<string, RuleQuotaSnapshot | undefined>
  quotaLoadingByProviderId?: Record<string, boolean | undefined>
  cardStatsByProviderId?: Record<string, RuleCardStatsItem | undefined>
  providerHealthByProviderId?: Record<string, ProviderModelHealthSnapshot | null | undefined>
  onRefreshQuota?: (providerId: string) => void | Promise<void>
  onTestModel?: (providerId: string) => void | Promise<void>
  onTestAll?: () => void | Promise<void>
  testingAll?: boolean
  testingProviderIds?: Record<string, boolean | undefined>
  onDuplicate?: (providerId: string) => void | Promise<void>
  onDelete: (providerId: string) => void
  onEdit: (providerId: string) => void
  onAdd: () => void
  addButtonLabel?: string
  addButtonTitle?: string
  deleteActionLabel?: string
  emptyMessage?: string
}> = ({
  providers,
  quotaByProviderId,
  quotaLoadingByProviderId,
  cardStatsByProviderId,
  providerHealthByProviderId,
  onRefreshQuota,
  onTestModel,
  onTestAll,
  testingAll = false,
  testingProviderIds,
  onDuplicate,
  onDelete,
  onEdit,
  onAdd,
  addButtonLabel,
  addButtonTitle,
  deleteActionLabel,
  emptyMessage,
}) => {
  const { t } = useTranslation()

  const formatQuotaValue = (value?: number | null) => {
    if (value === null || value === undefined || Number.isNaN(value)) {
      return "-"
    }
    const abs = Math.abs(value)
    if (abs >= 1) {
      return value.toFixed(2).replace(/\.00$/, "")
    }
    return value.toFixed(4).replace(/0+$/, "").replace(/\.$/, "")
  }

  const formatTokenValue = (value?: number | null) => {
    if (value === null || value === undefined || Number.isNaN(value)) {
      return "-"
    }
    const abs = Math.abs(value)
    if (abs >= 1_000_000) {
      return `${(value / 1_000_000).toFixed(2).replace(/\.00$/, "")}M`
    }
    return Number.isInteger(value)
      ? String(value)
      : value.toFixed(2).replace(/\.00$/, "").replace(/\.$/, "")
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

  const resolveQuotaBadge = (provider: Group["providers"][number]): RuleQuotaBadge => {
    if (!provider.quota?.enabled) {
      return {
        className: sharedStyles.quotaBadgeUnsupported,
        text: t("ruleQuota.unsupported"),
        resetAt: null,
      }
    }

    const snapshot = quotaByProviderId?.[provider.id]
    if (!snapshot) {
      return {
        className: sharedStyles.quotaBadgeUnknown,
        text: t("ruleQuota.pending"),
        resetAt: null,
      }
    }

    if (snapshot.status === "empty") {
      return {
        className: sharedStyles.quotaBadgeEmpty,
        text: t("ruleQuota.empty"),
        resetAt: formatResetAt(snapshot.resetAt),
      }
    }

    if (snapshot.status === "error") {
      return {
        className: sharedStyles.quotaBadgeError,
        text: t("ruleQuota.error"),
        resetAt: formatResetAt(snapshot.resetAt),
      }
    }

    if (snapshot.status === "unknown") {
      return {
        className: sharedStyles.quotaBadgeUnknown,
        text: t("ruleQuota.unknown"),
        resetAt: formatResetAt(snapshot.resetAt),
      }
    }

    if (snapshot.status === "unsupported") {
      return {
        className: sharedStyles.quotaBadgeUnsupported,
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
        className: sharedStyles.quotaBadgeLow,
        text: isTokenWithoutUnit
          ? t("ruleQuota.lowToken", { value: renderedValue })
          : t("ruleQuota.low", { value: renderedValue }),
        resetAt: formatResetAt(snapshot.resetAt),
      }
    }

    return {
      className: sharedStyles.quotaBadgeOk,
      text: isTokenWithoutUnit
        ? t("ruleQuota.remainingToken", { value: renderedValue })
        : t("ruleQuota.remaining", { value: renderedValue }),
      resetAt: formatResetAt(snapshot.resetAt),
    }
  }

  const formatCompactRequest = (value: number) => {
    if (!Number.isFinite(value)) return "0"
    if (Math.abs(value) >= 1_000_000) {
      return `${(value / 1_000_000).toFixed(1).replace(/\.0$/, "")}M`
    }
    if (Math.abs(value) >= 1_000) {
      return `${(value / 1_000).toFixed(1).replace(/\.0$/, "")}k`
    }
    return String(Math.round(value))
  }

  const formatCostConsumed = (value: number, currency?: string) => {
    const safe = Number.isFinite(value) ? Math.max(0, value) : 0
    const prefix = resolveCurrencyPrefix(currency)
    if (safe === 0) return `${prefix}0.00`
    if (safe < 0.0001) return `${prefix}<0.0001`
    if (safe < 1) return `${prefix}${safe.toFixed(4)}`
    return `${prefix}${safe.toFixed(2)}`
  }

  const resolvedDeleteActionLabel = deleteActionLabel || t("providersPage.deleteProvider")

  return (
    <div className={`${sharedStyles.ruleList} ${sharedStyles.ruleListCatalog}`}>
      <div className={`${sharedStyles.ruleListHeader} ${sharedStyles.ruleListHeaderCatalog}`}>
        <div className={sharedStyles.ruleHeaderTitle}>
          <h3>{t("servicePage.ruleName")}</h3>
          <span className={sharedStyles.countBadge}>{providers.length}</span>
        </div>
        <div className={sharedStyles.ruleHeaderActions}>
          {onTestAll ? (
            <button
              type="button"
              className={sharedStyles.headerIconButton}
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
                <Loader2 size={14} className={sharedStyles.spinner} />
              ) : (
                <FlaskConical size={14} />
              )}
            </button>
          ) : null}
          <Button
            variant="ghost"
            size="small"
            icon={Plus}
            onClick={onAdd}
            title={addButtonTitle || addButtonLabel || t("providersPage.addProvider")}
          />
        </div>
      </div>
      <div className={`${sharedStyles.ruleListContent} ${sharedStyles.ruleListContentCatalog}`}>
        {providers.length === 0 ? (
          <p className={sharedStyles.emptyHint}>{emptyMessage || t("providersPage.empty")}</p>
        ) : (
          <ul
            className={`${sharedStyles.ruleItems} ${sharedStyles.ruleItemsCatalog} ${sharedStyles.ruleItemsTwoColumn}`}
          >
            {providers.map(provider => (
              <MemoCatalogProviderCard
                key={provider.id}
                provider={provider}
                testing={Boolean(testingProviderIds?.[provider.id])}
                quotaLoading={Boolean(quotaLoadingByProviderId?.[provider.id])}
                healthSnapshot={providerHealthByProviderId?.[provider.id]}
                deleteActionLabel={resolvedDeleteActionLabel}
                badge={resolveQuotaBadge(provider)}
                cardStats={cardStatsByProviderId?.[provider.id]}
                onEdit={onEdit}
                onDuplicate={onDuplicate}
                onDelete={onDelete}
                onTestModel={onTestModel}
                onRefreshQuota={onRefreshQuota}
                formatCostConsumed={formatCostConsumed}
                formatCompactRequest={formatCompactRequest}
              />
            ))}
          </ul>
        )}
      </div>
    </div>
  )
}

export default ProviderList
