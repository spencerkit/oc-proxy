import type { EChartsOption } from "echarts"
import * as echarts from "echarts"
import { Check, ChevronLeft, ChevronRight, RotateCcw, Trash2, X } from "lucide-react"
import type React from "react"
import { useEffect, useMemo, useRef, useState } from "react"
import { useNavigate } from "react-router-dom"
import { Button, Modal } from "@/components"
import { useLogs, useTranslation } from "@/hooks"
import { useProxyStore } from "@/store"
import type { LogEntry } from "@/types"
import { formatTokenMillions } from "@/utils/tokenFormat"
import styles from "./LogsPage.module.css"

const HOURS_FILTERS = [1, 6, 24, 168, 720, 2160] as const
const MACARON = {
  axis: "#8a8fa6",
  split: "rgba(154, 162, 186, 0.28)",
  legend: "#7e8398",
  tooltipBg: "rgba(255, 255, 255, 0.94)",
  tooltipBorder: "rgba(192, 197, 221, 0.9)",
  tooltipText: "#556072",
  inputTop: "#8ee7d1",
  inputBottom: "#5fcdb6",
  outputTop: "#9fd1ff",
  outputBottom: "#6baeff",
  cacheInputTop: "#ffd4a6",
  cacheInputBottom: "#ffb87b",
  cacheOutputTop: "#e0c6ff",
  cacheOutputBottom: "#be9dff",
  requestLine: "#ff9fc0",
  requestAreaTop: "rgba(255, 159, 192, 0.26)",
  requestAreaBottom: "rgba(255, 159, 192, 0.02)",
  tpsOutputLine: "#5d95ff",
} as const

type LogsTab = "stats" | "logs"

/** Resolves display prefix for a currency code. */
function resolveCurrencyPrefix(currency?: string | null): string {
  const normalized = currency?.trim().toUpperCase()
  if (!normalized) return "$"
  if (normalized === "USD") return "$"
  if (normalized === "CNY" || normalized === "RMB") return "¥"
  if (normalized === "EUR") return "€"
  if (normalized === "JPY") return "¥"
  return `${normalized} `
}

/** Formats chart X-axis hour labels according to selected time window. */
function formatHourLabel(hourIso: string, hoursFilter: number): string {
  const date = new Date(hourIso)
  if (Number.isNaN(date.getTime())) return hourIso

  if (hoursFilter <= 24) {
    return date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hour12: false })
  }

  if (hoursFilter <= 168) {
    return date.toLocaleString([], {
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
      hour12: false,
    })
  }

  return date.toLocaleDateString([], { month: "2-digit", day: "2-digit" })
}

function toDateInputValue(date: Date): string {
  const year = date.getFullYear()
  const month = String(date.getMonth() + 1).padStart(2, "0")
  const day = String(date.getDate()).padStart(2, "0")
  return `${year}-${month}-${day}`
}

function getMonthStart(date: Date): Date {
  return new Date(date.getFullYear(), date.getMonth(), 1)
}

function getMonthDisplay(date: Date): string {
  return date.toLocaleDateString([], { year: "numeric", month: "long" })
}

/** Formats large token counts for chart axis labels (k/M). */
function formatTokenAxisValue(value: number | string): string {
  const numeric = typeof value === "number" ? value : Number(value)
  if (!Number.isFinite(numeric)) return String(value)
  if (Math.abs(numeric) >= 1_000_000) {
    const scaled = numeric / 1_000_000
    const text = Math.abs(scaled) >= 10 ? scaled.toFixed(0) : scaled.toFixed(1)
    return `${text.replace(/\.0$/, "")}M`
  }
  if (Math.abs(numeric) >= 1_000) {
    const scaled = numeric / 1_000
    const text = Math.abs(scaled) >= 10 ? scaled.toFixed(0) : scaled.toFixed(1)
    return `${text.replace(/\.0$/, "")}k`
  }
  return String(Math.round(numeric))
}

/** Formats tokens-per-second metric for UI display. */
function formatTpsMetric(value: number): string {
  if (!Number.isFinite(value) || value <= 0) return "0"
  const rounded = Math.round(value)
  if (rounded >= 1_000_000) return `${Math.round(rounded / 1_000_000)}M`
  if (rounded >= 1_000) return `${Math.round(rounded / 1_000)}k`
  return String(rounded)
}

/** Formats accumulated cost metric with currency prefix. */
function formatCostMetric(value: number, currency?: string | null): string {
  const safe = Number.isFinite(value) ? Math.max(0, value) : 0
  const prefix = resolveCurrencyPrefix(currency)
  if (safe === 0) return `${prefix}0.00`
  if (safe < 0.0001) return `${prefix}<0.0001`
  if (safe < 1) return `${prefix}${safe.toFixed(4)}`
  return `${prefix}${safe.toFixed(2)}`
}

/** Formats comparison delta as signed percentage text. */
function formatDelta(delta: number): string {
  if (!Number.isFinite(delta) || Math.abs(delta) < 0.01) return "0%"
  const abs = Math.abs(delta)
  const text = abs >= 100 ? abs.toFixed(0) : abs.toFixed(1)
  return `${delta > 0 ? "+" : "-"}${text.replace(/\.0$/, "")}%`
}

/**
 * LogsPage Component
 * Request log viewer page
 */
export const LogsPage: React.FC = () => {
  const navigate = useNavigate()
  const { t } = useTranslation()
  const { logs, logsStats, refreshLogsStats, clearLogs, clearLogsStats, loading, config } =
    useProxyStore()
  const { showToast } = useLogs()
  const [activeTab, setActiveTab] = useState<LogsTab>("stats")
  const [statusFilter, setStatusFilter] = useState<"all" | LogEntry["status"]>("all")
  const [selectedProviderKeys, setSelectedProviderKeys] = useState<string[]>([])
  const [providerSearchValue, setProviderSearchValue] = useState("")
  const [providerDropdownOpen, setProviderDropdownOpen] = useState(false)
  const [hoursFilter, setHoursFilter] = useState<number>(24)
  const [enableComparison, setEnableComparison] = useState(false)
  const [showClearConfirm, setShowClearConfirm] = useState(false)
  const [showResetStatsConfirm, setShowResetStatsConfirm] = useState(false)
  const [resetBeforeDate, setResetBeforeDate] = useState(() => toDateInputValue(new Date()))
  const [resetCalendarMonth, setResetCalendarMonth] = useState(() => getMonthStart(new Date()))
  const hasInitializedProviderSelectionRef = useRef(false)
  const providerComboboxRef = useRef<HTMLDivElement | null>(null)
  const usageChartDomRef = useRef<HTMLDivElement | null>(null)
  const usageChartRef = useRef<echarts.ECharts | null>(null)
  const rateChartDomRef = useRef<HTMLDivElement | null>(null)
  const rateChartRef = useRef<echarts.ECharts | null>(null)
  const errorBreakdownChartDomRef = useRef<HTMLDivElement | null>(null)
  const errorBreakdownChartRef = useRef<echarts.ECharts | null>(null)
  const contributionChartDomRef = useRef<HTMLDivElement | null>(null)
  const contributionChartRef = useRef<echarts.ECharts | null>(null)
  const statusFilters: Array<"all" | LogEntry["status"]> = [
    "all",
    "error",
    "processing",
    "rejected",
    "ok",
  ]

  const ruleOptions = useMemo(() => {
    const options: Array<{ key: string; label: string }> = []
    for (const group of config?.groups || []) {
      for (const provider of group.providers || []) {
        options.push({
          key: `${group.id}::${provider.id}`,
          label: `${group.name || group.id}-${provider.name || provider.id}`,
        })
      }
    }
    return options
  }, [config])

  const providerCostMetaByKey = useMemo(() => {
    const map = new Map<string, { enabled: boolean; currency: string }>()
    for (const group of config?.groups || []) {
      for (const provider of group.providers || []) {
        map.set(`${group.id}::${provider.id}`, {
          enabled: Boolean(provider.cost?.enabled),
          currency: provider.cost?.currency || "USD",
        })
      }
    }
    return map
  }, [config])

  const ruleOptionsByKey = useMemo(() => {
    const map = new Map<string, { key: string; label: string }>()
    for (const option of ruleOptions) {
      map.set(option.key, option)
    }
    return map
  }, [ruleOptions])

  const selectedProviderKeySet = useMemo(
    () => new Set(selectedProviderKeys),
    [selectedProviderKeys]
  )

  const selectedRuleOptions = useMemo(() => {
    return selectedProviderKeys
      .map(key => ruleOptionsByKey.get(key))
      .filter((option): option is { key: string; label: string } => Boolean(option))
  }, [ruleOptionsByKey, selectedProviderKeys])

  const hasAnyCostEnabledInSelection = useMemo(() => {
    return selectedProviderKeys.some(key => providerCostMetaByKey.get(key)?.enabled)
  }, [providerCostMetaByKey, selectedProviderKeys])

  const visibleRuleOptions = useMemo(() => {
    const keyword = providerSearchValue.trim().toLowerCase()
    if (!keyword) return ruleOptions
    return ruleOptions.filter(option => option.label.toLowerCase().includes(keyword))
  }, [ruleOptions, providerSearchValue])

  const allRuleKeys = useMemo(() => ruleOptions.map(option => option.key), [ruleOptions])
  const isAllSelected = allRuleKeys.length > 0 && selectedProviderKeys.length === allRuleKeys.length

  useEffect(() => {
    const validKeys = new Set(allRuleKeys)
    setSelectedProviderKeys(prev => {
      if (!hasInitializedProviderSelectionRef.current) {
        hasInitializedProviderSelectionRef.current = true
        return [...allRuleKeys]
      }
      return prev.filter(key => validKeys.has(key))
    })
  }, [allRuleKeys])

  useEffect(() => {
    if (!hasInitializedProviderSelectionRef.current) return
    void refreshLogsStats(hoursFilter, selectedProviderKeys, undefined, "rule", enableComparison)
  }, [hoursFilter, refreshLogsStats, selectedProviderKeys, enableComparison])

  useEffect(() => {
    if (!hasInitializedProviderSelectionRef.current) return
    const timer = window.setInterval(() => {
      void refreshLogsStats(hoursFilter, selectedProviderKeys, undefined, "rule", enableComparison)
    }, 3000)
    return () => window.clearInterval(timer)
  }, [hoursFilter, refreshLogsStats, selectedProviderKeys, enableComparison])

  useEffect(() => {
    const handleOutsideClick = (event: MouseEvent) => {
      if (!providerComboboxRef.current) return
      const target = event.target as Node
      if (!providerComboboxRef.current.contains(target)) {
        setProviderDropdownOpen(false)
      }
    }

    document.addEventListener("mousedown", handleOutsideClick)
    return () => document.removeEventListener("mousedown", handleOutsideClick)
  }, [])

  useEffect(() => {
    return () => {
      if (usageChartRef.current) {
        usageChartRef.current.dispose()
        usageChartRef.current = null
      }
      if (rateChartRef.current) {
        rateChartRef.current.dispose()
        rateChartRef.current = null
      }
      if (errorBreakdownChartRef.current) {
        errorBreakdownChartRef.current.dispose()
        errorBreakdownChartRef.current = null
      }
      if (contributionChartRef.current) {
        contributionChartRef.current.dispose()
        contributionChartRef.current = null
      }
    }
  }, [])

  useEffect(() => {
    if (activeTab !== "stats") {
      if (usageChartRef.current) {
        usageChartRef.current.dispose()
        usageChartRef.current = null
      }
      if (rateChartRef.current) {
        rateChartRef.current.dispose()
        rateChartRef.current = null
      }
      if (errorBreakdownChartRef.current) {
        errorBreakdownChartRef.current.dispose()
        errorBreakdownChartRef.current = null
      }
      if (contributionChartRef.current) {
        contributionChartRef.current.dispose()
        contributionChartRef.current = null
      }
    }
  }, [activeTab])

  const handleClear = async () => {
    try {
      await clearLogs()
      showToast(t("logs.clearSuccess"), "success")
    } catch (error) {
      showToast(t("errors.operationFailed", { message: String(error) }), "error")
    }
  }

  const handleResetStats = () => {
    const today = new Date()
    setResetBeforeDate(toDateInputValue(today))
    setResetCalendarMonth(getMonthStart(today))
    setShowResetStatsConfirm(true)
  }

  const handleConfirmResetStats = async () => {
    const beforeEpochMs = new Date(`${resetBeforeDate}T00:00:00`).getTime()
    if (!Number.isFinite(beforeEpochMs)) {
      showToast(t("logs.resetStatsInvalidDate"), "error")
      return
    }
    try {
      await clearLogsStats(beforeEpochMs)
      await refreshLogsStats(hoursFilter, selectedProviderKeys, undefined, "rule", enableComparison)
      showToast(t("logs.resetStatsSuccess"), "success")
      setShowResetStatsConfirm(false)
    } catch (error) {
      showToast(t("logs.resetStatsError"), "error")
      console.error(error)
    }
  }

  const handleToggleRule = (ruleKey: string) => {
    setSelectedProviderKeys(prev => {
      if (prev.includes(ruleKey)) {
        return prev.filter(key => key !== ruleKey)
      }
      return [...prev, ruleKey]
    })
  }

  const formatTimestamp = (timestamp: string) => {
    const date = new Date(timestamp)
    return date.toLocaleTimeString()
  }

  const getStatusClass = (status: LogEntry["status"]) => {
    switch (status) {
      case "ok":
        return styles.statusOk
      case "error":
        return styles.statusError
      case "processing":
        return styles.statusProcessing
      case "rejected":
        return styles.statusRejected
      default:
        return ""
    }
  }

  const orderedLogs = useMemo(() => {
    return [...logs].sort((a, b) => {
      return new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime()
    })
  }, [logs])

  const filteredLogs = useMemo(() => {
    if (statusFilter === "all") return orderedLogs
    return orderedLogs.filter(log => log.status === statusFilter)
  }, [orderedLogs, statusFilter])

  const totalRequests = logsStats?.requests ?? 0
  const totalErrors = logsStats?.errors ?? 0
  const successRate =
    totalRequests > 0
      ? Math.max(0, Math.round(((totalRequests - totalErrors) / totalRequests) * 100))
      : 0

  const getStatusText = (status: LogEntry["status"]) => t(`logs.state.${status}`)

  const getFilterLabel = (filter: "all" | LogEntry["status"]) => {
    if (filter === "all") return t("logs.filterAll")
    return getStatusText(filter)
  }

  const getHoursLabel = (hours: number) => t(`logs.statsHours${hours}`)

  const usageTrendSeries = useMemo(() => {
    const hourly = [...(logsStats?.hourly ?? [])].sort((a, b) => {
      return new Date(a.hour).getTime() - new Date(b.hour).getTime()
    })

    return {
      labels: hourly.map(point => formatHourLabel(point.hour, hoursFilter)),
      inputTokens: hourly.map(point => point.inputTokens),
      outputTokens: hourly.map(point => point.outputTokens),
      cacheInputTokens: hourly.map(point => point.cacheReadTokens),
      cacheOutputTokens: hourly.map(point => point.cacheWriteTokens),
      requests: hourly.map(point => point.requests),
    }
  }, [hoursFilter, logsStats?.hourly])

  const rateTrendSeries = useMemo(() => {
    const hourly = [...(logsStats?.hourly ?? [])].sort((a, b) => {
      return new Date(a.hour).getTime() - new Date(b.hour).getTime()
    })

    return {
      labels: hourly.map(point => formatHourLabel(point.hour, hoursFilter)),
      outputTps: hourly.map(point => {
        const value = Number(point.outputTps)
        if (!Number.isFinite(value)) return 0
        return Math.max(0, Math.round(value))
      }),
    }
  }, [hoursFilter, logsStats?.hourly])

  const comparison = logsStats?.comparison ?? null
  const errorBreakdownSeries = useMemo(() => {
    const source = (logsStats?.breakdowns?.errorsByStatus ?? []).slice(0, 8)
    return {
      labels: source.map(item => item.key),
      values: source.map(item => item.count),
    }
  }, [logsStats?.breakdowns?.errorsByStatus])

  const contributionSeries = useMemo(() => {
    const topRequests: Array<{ key: string; label?: string; count: number }> = (
      logsStats?.breakdowns?.requestsByRule ?? []
    )
      .slice(0, 5)
      .map(item => ({
        key: item.key,
        label: "label" in item && typeof item.label === "string" ? item.label : undefined,
        count: item.count,
      }))
    const topTokens: Array<{ key: string; label?: string; tokens: number }> = (
      logsStats?.breakdowns?.tokensByRule ?? []
    )
      .slice(0, 5)
      .map(item => ({
        key: item.key,
        label: "label" in item && typeof item.label === "string" ? item.label : undefined,
        tokens: item.tokens,
      }))
    const keys: string[] =
      topRequests.length >= topTokens.length
        ? topRequests.map(item => item.key)
        : topTokens.map(item => item.key)

    return {
      labels: keys.map(key => {
        const option = ruleOptionsByKey.get(key)
        if (option?.label) return option.label
        const requestItem = topRequests.find(entry => entry.key === key)
        const tokenItem = topTokens.find(entry => entry.key === key)
        return requestItem?.label || tokenItem?.label || key
      }),
      requests: keys.map(key => {
        const item = topRequests.find(entry => entry.key === key)
        return item?.count ?? 0
      }),
      tokens: keys.map(key => {
        const item = topTokens.find(entry => entry.key === key)
        return item?.tokens ?? 0
      }),
    }
  }, [logsStats?.breakdowns?.requestsByRule, logsStats?.breakdowns?.tokensByRule, ruleOptionsByKey])

  const totalCostText = useMemo(() => {
    const currency = logsStats?.costCurrency
    if (currency === "MIXED") {
      return `${t("logs.costMixedCurrency")} ${formatCostMetric(logsStats?.totalCost ?? 0, "USD")}`
    }
    return formatCostMetric(logsStats?.totalCost ?? 0, currency || "USD")
  }, [logsStats?.costCurrency, logsStats?.totalCost, t])

  const resetStatsWeekdayLabels = useMemo(() => {
    const formatter = new Intl.DateTimeFormat(undefined, { weekday: "short" })
    return Array.from({ length: 7 }, (_, index) => formatter.format(new Date(2024, 0, 7 + index)))
  }, [])

  const resetStatsCalendarCells = useMemo(() => {
    const monthStart = getMonthStart(resetCalendarMonth)
    const monthFirstWeekday = monthStart.getDay()
    const gridStart = new Date(
      monthStart.getFullYear(),
      monthStart.getMonth(),
      1 - monthFirstWeekday
    )
    const todayText = toDateInputValue(new Date())
    return Array.from({ length: 42 }, (_, index) => {
      const date = new Date(
        gridStart.getFullYear(),
        gridStart.getMonth(),
        gridStart.getDate() + index
      )
      const value = toDateInputValue(date)
      return {
        value,
        day: date.getDate(),
        inCurrentMonth: date.getMonth() === monthStart.getMonth(),
        disabled: value > todayText,
        isToday: value === todayText,
        selected: value === resetBeforeDate,
      }
    })
  }, [resetBeforeDate, resetCalendarMonth])

  useEffect(() => {
    if (activeTab !== "stats" || !usageChartDomRef.current) return

    const chart =
      usageChartRef.current && usageChartRef.current.getDom() === usageChartDomRef.current
        ? usageChartRef.current
        : echarts.init(usageChartDomRef.current)
    usageChartRef.current = chart

    const option: EChartsOption = {
      animationDuration: 260,
      backgroundColor: "transparent",
      grid: {
        left: 56,
        right: 56,
        top: 40,
        bottom: 32,
      },
      legend: {
        top: 8,
        textStyle: { color: MACARON.legend, fontSize: 12 },
        data: [
          t("logs.trendInput"),
          t("logs.trendOutput"),
          t("logs.trendCacheInput"),
          t("logs.trendCacheOutput"),
          t("logs.trendRequests"),
        ],
      },
      tooltip: {
        trigger: "axis",
        backgroundColor: MACARON.tooltipBg,
        borderColor: MACARON.tooltipBorder,
        borderWidth: 1,
        textStyle: { color: MACARON.tooltipText },
      },
      xAxis: {
        type: "category",
        data: usageTrendSeries.labels,
        axisLine: { lineStyle: { color: MACARON.axis } },
        axisLabel: { color: MACARON.axis, fontSize: 11 },
      },
      yAxis: [
        {
          type: "value",
          name: t("logs.trendTokensAxis"),
          axisLabel: {
            color: MACARON.axis,
            fontSize: 11,
            formatter: (value: number) => formatTokenAxisValue(value),
          },
          splitLine: { lineStyle: { color: MACARON.split, type: "dashed" } },
        },
        {
          type: "value",
          name: t("logs.trendRequestsAxis"),
          axisLabel: { color: MACARON.axis, fontSize: 11 },
          splitLine: { show: false },
        },
      ],
      series: [
        {
          name: t("logs.trendInput"),
          type: "bar",
          data: usageTrendSeries.inputTokens,
          yAxisIndex: 0,
          barMaxWidth: 14,
          itemStyle: {
            color: new echarts.graphic.LinearGradient(0, 0, 0, 1, [
              { offset: 0, color: MACARON.inputTop },
              { offset: 1, color: MACARON.inputBottom },
            ]),
            borderRadius: [4, 4, 0, 0],
          },
        },
        {
          name: t("logs.trendOutput"),
          type: "bar",
          data: usageTrendSeries.outputTokens,
          yAxisIndex: 0,
          barMaxWidth: 14,
          itemStyle: {
            color: new echarts.graphic.LinearGradient(0, 0, 0, 1, [
              { offset: 0, color: MACARON.outputTop },
              { offset: 1, color: MACARON.outputBottom },
            ]),
            borderRadius: [4, 4, 0, 0],
          },
        },
        {
          name: t("logs.trendCacheInput"),
          type: "bar",
          data: usageTrendSeries.cacheInputTokens,
          yAxisIndex: 0,
          barMaxWidth: 14,
          itemStyle: {
            color: new echarts.graphic.LinearGradient(0, 0, 0, 1, [
              { offset: 0, color: MACARON.cacheInputTop },
              { offset: 1, color: MACARON.cacheInputBottom },
            ]),
            borderRadius: [4, 4, 0, 0],
          },
        },
        {
          name: t("logs.trendCacheOutput"),
          type: "bar",
          data: usageTrendSeries.cacheOutputTokens,
          yAxisIndex: 0,
          barMaxWidth: 14,
          itemStyle: {
            color: new echarts.graphic.LinearGradient(0, 0, 0, 1, [
              { offset: 0, color: MACARON.cacheOutputTop },
              { offset: 1, color: MACARON.cacheOutputBottom },
            ]),
            borderRadius: [4, 4, 0, 0],
          },
        },
        {
          name: t("logs.trendRequests"),
          type: "line",
          data: usageTrendSeries.requests,
          yAxisIndex: 1,
          smooth: true,
          symbol: "circle",
          symbolSize: 6,
          lineStyle: { color: MACARON.requestLine, width: 2 },
          itemStyle: { color: MACARON.requestLine },
          areaStyle: {
            color: new echarts.graphic.LinearGradient(0, 0, 0, 1, [
              { offset: 0, color: MACARON.requestAreaTop },
              { offset: 1, color: MACARON.requestAreaBottom },
            ]),
          },
        },
      ],
    }

    chart.setOption(option, true)

    const frameId = window.requestAnimationFrame(() => chart.resize())
    const handleResize = () => chart.resize()
    window.addEventListener("resize", handleResize)

    return () => {
      window.cancelAnimationFrame(frameId)
      window.removeEventListener("resize", handleResize)
    }
  }, [activeTab, t, usageTrendSeries])

  useEffect(() => {
    if (activeTab !== "stats" || !rateChartDomRef.current) return

    const chart =
      rateChartRef.current && rateChartRef.current.getDom() === rateChartDomRef.current
        ? rateChartRef.current
        : echarts.init(rateChartDomRef.current)
    rateChartRef.current = chart

    const option: EChartsOption = {
      animationDuration: 260,
      backgroundColor: "transparent",
      grid: {
        left: 56,
        right: 56,
        top: 40,
        bottom: 32,
      },
      legend: {
        top: 8,
        textStyle: { color: MACARON.legend, fontSize: 12 },
        data: [t("logs.trendOutputTps")],
      },
      tooltip: {
        trigger: "axis",
        backgroundColor: MACARON.tooltipBg,
        borderColor: MACARON.tooltipBorder,
        borderWidth: 1,
        textStyle: { color: MACARON.tooltipText },
      },
      xAxis: {
        type: "category",
        data: rateTrendSeries.labels,
        axisLine: { lineStyle: { color: MACARON.axis } },
        axisLabel: { color: MACARON.axis, fontSize: 11 },
      },
      yAxis: {
        type: "value",
        name: t("logs.trendRateAxis"),
        axisLabel: {
          color: MACARON.axis,
          fontSize: 11,
          formatter: (value: number) => formatTpsMetric(value),
        },
        splitLine: { lineStyle: { color: MACARON.split, type: "dashed" } },
      },
      series: [
        {
          name: t("logs.trendOutputTps"),
          type: "line",
          data: rateTrendSeries.outputTps,
          smooth: true,
          symbol: "circle",
          symbolSize: 5,
          lineStyle: { color: MACARON.tpsOutputLine, width: 2 },
          itemStyle: { color: MACARON.tpsOutputLine },
          markPoint: {
            symbol: "pin",
            symbolSize: 28,
            label: { color: "#fff", fontSize: 10 },
            data: [{ type: "max", name: t("logs.peakOutputTps") }],
          },
        },
      ],
    }

    chart.setOption(option, true)

    const frameId = window.requestAnimationFrame(() => chart.resize())
    const handleResize = () => chart.resize()
    window.addEventListener("resize", handleResize)

    return () => {
      window.cancelAnimationFrame(frameId)
      window.removeEventListener("resize", handleResize)
    }
  }, [activeTab, rateTrendSeries, t])

  useEffect(() => {
    if (activeTab !== "stats" || !errorBreakdownChartDomRef.current) return

    const chart =
      errorBreakdownChartRef.current &&
      errorBreakdownChartRef.current.getDom() === errorBreakdownChartDomRef.current
        ? errorBreakdownChartRef.current
        : echarts.init(errorBreakdownChartDomRef.current)
    errorBreakdownChartRef.current = chart

    const option: EChartsOption = {
      animationDuration: 260,
      backgroundColor: "transparent",
      grid: { left: 48, right: 20, top: 20, bottom: 50 },
      tooltip: {
        trigger: "axis",
        axisPointer: { type: "shadow" },
        backgroundColor: MACARON.tooltipBg,
        borderColor: MACARON.tooltipBorder,
        borderWidth: 1,
        textStyle: { color: MACARON.tooltipText },
      },
      xAxis: {
        type: "category",
        data: errorBreakdownSeries.labels,
        axisLine: { lineStyle: { color: MACARON.axis } },
        axisLabel: { color: MACARON.axis, fontSize: 11, rotate: 24 },
      },
      yAxis: {
        type: "value",
        axisLine: { show: false },
        axisLabel: { color: MACARON.axis, fontSize: 11 },
        splitLine: { lineStyle: { color: MACARON.split, type: "dashed" } },
      },
      series: [
        {
          name: t("logs.errorsCount"),
          type: "bar",
          barMaxWidth: 28,
          data: errorBreakdownSeries.values,
          itemStyle: {
            color: new echarts.graphic.LinearGradient(0, 0, 0, 1, [
              { offset: 0, color: "#ffb0b0" },
              { offset: 1, color: "#ff8f8f" },
            ]),
            borderRadius: [6, 6, 0, 0],
          },
        },
      ],
    }

    chart.setOption(option, true)
    const frameId = window.requestAnimationFrame(() => chart.resize())
    const handleResize = () => chart.resize()
    window.addEventListener("resize", handleResize)
    return () => {
      window.cancelAnimationFrame(frameId)
      window.removeEventListener("resize", handleResize)
    }
  }, [activeTab, errorBreakdownSeries, t])

  useEffect(() => {
    if (activeTab !== "stats" || !contributionChartDomRef.current) return

    const chart =
      contributionChartRef.current &&
      contributionChartRef.current.getDom() === contributionChartDomRef.current
        ? contributionChartRef.current
        : echarts.init(contributionChartDomRef.current)
    contributionChartRef.current = chart

    const option: EChartsOption = {
      animationDuration: 260,
      backgroundColor: "transparent",
      grid: { left: 56, right: 20, top: 24, bottom: 48 },
      legend: {
        top: 0,
        textStyle: { color: MACARON.legend, fontSize: 12 },
        data: [t("logs.trendRequests"), t("logs.trendTokens")],
      },
      tooltip: {
        trigger: "axis",
        axisPointer: { type: "shadow" },
        backgroundColor: MACARON.tooltipBg,
        borderColor: MACARON.tooltipBorder,
        borderWidth: 1,
        textStyle: { color: MACARON.tooltipText },
      },
      xAxis: {
        type: "category",
        data: contributionSeries.labels,
        axisLine: { lineStyle: { color: MACARON.axis } },
        axisLabel: { color: MACARON.axis, fontSize: 11, interval: 0, rotate: 18 },
      },
      yAxis: [
        {
          type: "value",
          axisLabel: { color: MACARON.axis, fontSize: 11 },
          splitLine: { lineStyle: { color: MACARON.split, type: "dashed" } },
        },
        {
          type: "value",
          axisLabel: {
            color: MACARON.axis,
            fontSize: 11,
            formatter: (value: number) => formatTokenAxisValue(value),
          },
          splitLine: { show: false },
        },
      ],
      series: [
        {
          name: t("logs.trendRequests"),
          type: "line",
          data: contributionSeries.requests,
          smooth: true,
          symbol: "circle",
          symbolSize: 6,
          lineStyle: { color: "#ffb15f", width: 2 },
          itemStyle: { color: "#ffb15f" },
        },
        {
          name: t("logs.trendTokens"),
          type: "bar",
          yAxisIndex: 1,
          barMaxWidth: 26,
          data: contributionSeries.tokens,
          itemStyle: {
            color: new echarts.graphic.LinearGradient(0, 0, 0, 1, [
              { offset: 0, color: "#8ab9ff" },
              { offset: 1, color: MACARON.tpsOutputLine },
            ]),
            borderRadius: [6, 6, 0, 0],
          },
        },
      ],
    }

    chart.setOption(option, true)
    const frameId = window.requestAnimationFrame(() => chart.resize())
    const handleResize = () => chart.resize()
    window.addEventListener("resize", handleResize)
    return () => {
      window.cancelAnimationFrame(frameId)
      window.removeEventListener("resize", handleResize)
    }
  }, [activeTab, contributionSeries, t])

  const renderLogEntry = (log: LogEntry) => {
    return (
      <button
        key={`${log.traceId}-${log.timestamp}`}
        type="button"
        className={styles.logEntryButton}
        onClick={() => navigate(`/logs/${encodeURIComponent(log.traceId)}`)}
        title={t("logs.viewDetail")}
      >
        <div className={styles.logEntry}>
          <div className={styles.logHeader}>
            <span className={styles.timestamp}>{formatTimestamp(log.timestamp)}</span>
            <span className={styles.method}>{log.method}</span>
            <span className={styles.path}>{log.requestPath}</span>
            <span className={`${styles.status} ${getStatusClass(log.status)}`}>
              {t("logs.requestStatus", {
                status: log.httpStatus ?? "---",
                state: getStatusText(log.status),
              })}
            </span>
          </div>
          <div className={styles.logDetails}>
            {log.groupPath && (
              <div className={styles.logDetail}>
                <span className={styles.label}>{t("logs.group")}:</span>
                <span>{log.groupPath}</span>
              </div>
            )}
            {log.model && (
              <div className={styles.logDetail}>
                <span className={styles.label}>{t("logs.model")}:</span>
                <span>{log.model}</span>
              </div>
            )}
            {log.entryProtocol && (
              <div className={styles.logDetail}>
                <span className={styles.label}>{t("logs.entryProtocol")}:</span>
                <span>{t(`ruleProtocol.${log.entryProtocol}`)}</span>
              </div>
            )}
            {log.downstreamProtocol && (
              <div className={styles.logDetail}>
                <span className={styles.label}>{t("logs.downstreamProtocol")}:</span>
                <span>{t(`ruleProtocol.${log.downstreamProtocol}`)}</span>
              </div>
            )}
            {log.forwardedModel && (
              <div className={styles.logDetail}>
                <span className={styles.label}>{t("logs.forwardedModel")}:</span>
                <span>{log.forwardedModel}</span>
              </div>
            )}
            {log.forwardingAddress ? (
              <div className={styles.logDetail}>
                <span className={styles.label}>{t("logs.forwardingTo")}:</span>
                <span>{log.forwardingAddress}</span>
              </div>
            ) : (
              <div className={styles.logDetail}>
                <span className={styles.label}>{t("logs.notForwarding")}</span>
              </div>
            )}
            {log.error && (
              <div className={`${styles.logDetail} ${styles.error}`}>
                <span className={styles.label}>
                  {t("logs.errorReason", { reason: log.error.message })}
                </span>
              </div>
            )}
            {log.durationMs > 0 && (
              <div className={styles.logDetail}>
                <span className={styles.label}>{t("logs.duration")}:</span>
                <span>{log.durationMs}ms</span>
              </div>
            )}
            {log.tokenUsage && (
              <div className={styles.logDetail}>
                <span className={styles.label}>{t("logs.tokens")}:</span>
                <span>
                  {t("logs.tokensCompact", {
                    input: formatTokenMillions(log.tokenUsage.inputTokens),
                    output: formatTokenMillions(log.tokenUsage.outputTokens),
                    cacheRead: formatTokenMillions(log.tokenUsage.cacheReadTokens),
                  })}
                </span>
              </div>
            )}
            <div className={styles.logDetailAction}>
              <span>{t("logs.viewDetail")}</span>
            </div>
          </div>
        </div>
      </button>
    )
  }

  return (
    <div className={styles.logsPage}>
      <div className={styles.header}>
        <h2>{t("logs.title")}</h2>
        <p className={styles.subtitle}>
          {activeTab === "stats"
            ? `${t("logs.statsTabSubtitle", { range: getHoursLabel(hoursFilter) })} · ${t("logs.statsRulesSelected", { count: selectedProviderKeys.length })}`
            : statusFilter === "all"
              ? t("logs.recentLogs", { count: logs.length })
              : t("logs.filteredLogs", { shown: filteredLogs.length, total: logs.length })}
        </p>
      </div>

      <div className={styles.tabRow}>
        <button
          type="button"
          className={`${styles.tabButton} ${activeTab === "stats" ? styles.tabButtonActive : ""}`}
          onClick={() => setActiveTab("stats")}
          aria-pressed={activeTab === "stats"}
        >
          {t("logs.tabStats")}
        </button>
        <button
          type="button"
          className={`${styles.tabButton} ${activeTab === "logs" ? styles.tabButtonActive : ""}`}
          onClick={() => setActiveTab("logs")}
          aria-pressed={activeTab === "logs"}
        >
          {t("logs.tabRequests")}
        </button>
      </div>

      {activeTab === "stats" ? (
        <>
          <div className={styles.statsToolbar}>
            <div className={styles.advancedFilterGroup}>
              <div className={styles.ruleCombobox} ref={providerComboboxRef}>
                <input
                  className={styles.inlineInput}
                  type="text"
                  value={providerSearchValue}
                  onFocus={() => setProviderDropdownOpen(true)}
                  onChange={e => {
                    setProviderSearchValue(e.target.value)
                    setProviderDropdownOpen(true)
                  }}
                  onKeyDown={e => {
                    if (e.key === "Escape") {
                      setProviderDropdownOpen(false)
                    }
                    if (
                      e.key === "Backspace" &&
                      !providerSearchValue.trim() &&
                      selectedProviderKeys.length > 0
                    ) {
                      e.preventDefault()
                      setSelectedProviderKeys(prev => prev.slice(0, Math.max(0, prev.length - 1)))
                    }
                    if (e.key === "Enter" && visibleRuleOptions.length > 0) {
                      e.preventDefault()
                      handleToggleRule(visibleRuleOptions[0].key)
                    }
                  }}
                  placeholder={t("logs.statsRuleSearchPlaceholder")}
                />
                {providerDropdownOpen && (
                  <div className={styles.ruleDropdown}>
                    <div className={styles.ruleDropdownActions}>
                      <button
                        type="button"
                        className={styles.ruleDropdownAction}
                        onMouseDown={event => event.preventDefault()}
                        onClick={() => setSelectedProviderKeys([...allRuleKeys])}
                      >
                        {t("logs.statsSelectAll")}
                      </button>
                      <button
                        type="button"
                        className={styles.ruleDropdownAction}
                        onMouseDown={event => event.preventDefault()}
                        onClick={() => setSelectedProviderKeys([])}
                      >
                        {t("logs.statsClearSelection")}
                      </button>
                    </div>
                    {visibleRuleOptions.length === 0 ? (
                      <div className={styles.ruleDropdownEmpty}>{t("logs.noStatsData")}</div>
                    ) : (
                      visibleRuleOptions.map(option => {
                        const checked = selectedProviderKeySet.has(option.key)
                        return (
                          <button
                            key={option.key}
                            type="button"
                            className={`${styles.ruleOption} ${checked ? styles.ruleOptionActive : ""}`}
                            onMouseDown={event => event.preventDefault()}
                            onClick={() => handleToggleRule(option.key)}
                          >
                            <span className={styles.ruleOptionCheck}>
                              {checked && <Check size={12} />}
                            </span>
                            <span>{option.label}</span>
                          </button>
                        )
                      })
                    )}
                  </div>
                )}
              </div>
              <div className={styles.selectWrap}>
                <select
                  className={styles.inlineSelect}
                  value={hoursFilter}
                  onChange={e => setHoursFilter(Number(e.target.value))}
                >
                  {HOURS_FILTERS.map(hours => (
                    <option key={hours} value={hours}>
                      {getHoursLabel(hours)}
                    </option>
                  ))}
                </select>
              </div>
            </div>

            <div className={styles.toolbarActions}>
              <Button
                variant={enableComparison ? "primary" : "default"}
                onClick={() => setEnableComparison(prev => !prev)}
              >
                {enableComparison ? t("logs.disableComparison") : t("logs.enableComparison")}
              </Button>
              <Button
                variant="default"
                icon={RotateCcw}
                onClick={handleResetStats}
                loading={loading}
              >
                {t("logs.resetStats")}
              </Button>
            </div>
          </div>

          <div className={styles.ruleChipsRow}>
            <span className={styles.ruleSelectionText}>
              {isAllSelected
                ? t("logs.statsRuleAll")
                : t("logs.statsRulesSelected", { count: selectedProviderKeys.length })}
            </span>
            <button
              type="button"
              className={styles.ruleClearButton}
              onClick={() => setSelectedProviderKeys([])}
              disabled={selectedProviderKeys.length === 0}
            >
              {t("logs.statsClearSelection")}
            </button>
            <div className={styles.ruleChipList}>
              {!isAllSelected &&
                selectedRuleOptions.map(option => (
                  <span key={option.key} className={styles.ruleChip}>
                    {option.label}
                    <button
                      type="button"
                      className={styles.ruleChipRemove}
                      onClick={() => {
                        setSelectedProviderKeys(prev => prev.filter(key => key !== option.key))
                      }}
                      aria-label={`${t("logs.statsClearSelection")}: ${option.label}`}
                    >
                      <X size={11} />
                    </button>
                  </span>
                ))}
            </div>
          </div>

          <div className={styles.metricsSection}>
            <h3 className={styles.metricsTitle}>{t("logs.requestMetricsSection")}</h3>
            <div className={styles.summaryGrid}>
              <div className={styles.summaryCard}>
                <span className={styles.summaryLabel}>{t("logs.totalRequests")}</span>
                <strong className={styles.summaryValue}>{totalRequests}</strong>
                {enableComparison && comparison && (
                  <span
                    className={`${styles.summaryDelta} ${comparison.requestsDeltaPct >= 0 ? styles.summaryDeltaUp : styles.summaryDeltaDown}`}
                  >
                    {formatDelta(comparison.requestsDeltaPct)}
                  </span>
                )}
              </div>
              <div className={styles.summaryCard}>
                <span className={styles.summaryLabel}>{t("logs.errorsCount")}</span>
                <strong className={`${styles.summaryValue} ${styles.summaryValueDanger}`}>
                  {totalErrors}
                </strong>
                {enableComparison && comparison && (
                  <span
                    className={`${styles.summaryDelta} ${comparison.errorsDeltaPct >= 0 ? styles.summaryDeltaDown : styles.summaryDeltaUp}`}
                  >
                    {formatDelta(comparison.errorsDeltaPct)}
                  </span>
                )}
              </div>
              <div className={styles.summaryCard}>
                <span className={styles.summaryLabel}>{t("logs.successRate")}</span>
                <strong className={styles.summaryValue}>{successRate}%</strong>
              </div>
            </div>
          </div>

          <div className={styles.metricsSection}>
            <h3 className={styles.metricsTitle}>{t("logs.tokenMetricsSection")}</h3>
            <div className={styles.summaryGrid}>
              <div className={styles.summaryCard}>
                <span className={styles.summaryLabel}>{t("logs.tokenInOut")}</span>
                <strong className={styles.summaryValue}>
                  {formatTokenMillions(logsStats?.inputTokens ?? 0)} /{" "}
                  {formatTokenMillions(logsStats?.outputTokens ?? 0)}
                </strong>
              </div>
              <div className={styles.summaryCard}>
                <span className={styles.summaryLabel}>{t("logs.cacheHitWrite")}</span>
                <strong className={styles.summaryValue}>
                  {formatTokenMillions(logsStats?.cacheReadTokens ?? 0)} /{" "}
                  {formatTokenMillions(logsStats?.cacheWriteTokens ?? 0)}
                </strong>
              </div>
              <div className={styles.summaryCard}>
                <span className={styles.summaryLabel}>{t("logs.outputTps")}</span>
                <strong className={styles.summaryValue}>
                  {formatTpsMetric(logsStats?.outputTps ?? 0)}
                </strong>
              </div>
              <div className={styles.summaryCard}>
                <span className={styles.summaryLabel}>{t("logs.peakOutputTps")}</span>
                <strong className={styles.summaryValue}>
                  {formatTpsMetric(logsStats?.peakOutputTps ?? 0)}
                </strong>
              </div>
              {hasAnyCostEnabledInSelection ? (
                <div className={styles.summaryCard}>
                  <span className={styles.summaryLabel}>{t("logs.totalCost")}</span>
                  <strong className={styles.summaryValue}>{totalCostText}</strong>
                  {enableComparison && comparison && (
                    <span
                      className={`${styles.summaryDelta} ${comparison.totalCostDeltaPct >= 0 ? styles.summaryDeltaUp : styles.summaryDeltaDown}`}
                    >
                      {formatDelta(comparison.totalCostDeltaPct)}
                    </span>
                  )}
                </div>
              ) : (
                <div className={`${styles.summaryCard} ${styles.summaryCardNotice}`}>
                  <span className={styles.summaryLabel}>{t("logs.totalCost")}</span>
                  <strong className={styles.summaryValue}>
                    {t("logs.costSummaryUnavailable")}
                  </strong>
                </div>
              )}
            </div>
          </div>

          <div className={styles.metricsSection}>
            <h3 className={styles.metricsTitle}>{t("logs.trendChartTitle")}</h3>
            <div className={styles.chartCard}>
              <div ref={usageChartDomRef} className={styles.trendChart} />
              {usageTrendSeries.labels.length === 0 && (
                <div className={styles.chartEmpty}>{t("logs.noStatsData")}</div>
              )}
            </div>
          </div>

          <div className={styles.metricsSection}>
            <h3 className={styles.metricsTitle}>{t("logs.rateTrendChartTitle")}</h3>
            <div className={styles.chartCard}>
              <div ref={rateChartDomRef} className={styles.trendChart} />
              {rateTrendSeries.labels.length === 0 && (
                <div className={styles.chartEmpty}>{t("logs.noStatsData")}</div>
              )}
            </div>
          </div>

          <div className={styles.metricsSection}>
            <h3 className={styles.metricsTitle}>{t("logs.contributionRankingTitle")}</h3>
            <div className={styles.chartCard}>
              <div ref={contributionChartDomRef} className={styles.trendChart} />
              {contributionSeries.labels.length === 0 && (
                <div className={styles.chartEmpty}>{t("logs.noStatsData")}</div>
              )}
            </div>
          </div>

          <div className={styles.metricsSection}>
            <h3 className={styles.metricsTitle}>{t("logs.errorBreakdownTitle")}</h3>
            <div className={styles.chartCard}>
              <div ref={errorBreakdownChartDomRef} className={styles.trendChart} />
              {errorBreakdownSeries.labels.length === 0 && (
                <div className={styles.chartEmpty}>{t("logs.noStatsData")}</div>
              )}
            </div>
          </div>
        </>
      ) : (
        <>
          <div className={styles.filterRow}>
            <div className={styles.filterGroup}>
              {statusFilters.map(filter => (
                <button
                  key={filter}
                  type="button"
                  className={`${styles.filterButton} ${statusFilter === filter ? styles.filterButtonActive : ""}`}
                  onClick={() => setStatusFilter(filter)}
                  aria-pressed={statusFilter === filter}
                >
                  {getFilterLabel(filter)}
                </button>
              ))}
            </div>
            <div className={styles.toolbarActions}>
              <Button
                variant="danger"
                icon={Trash2}
                onClick={() => setShowClearConfirm(true)}
                disabled={logs.length === 0}
              >
                {t("logs.clear")}
              </Button>
            </div>
          </div>

          <div className={styles.logsContainer}>
            {logs.length === 0 ? (
              <div className={styles.emptyState}>
                <p>{t("logs.noLogs")}</p>
              </div>
            ) : filteredLogs.length === 0 ? (
              <div className={styles.emptyState}>
                <p>{t("logs.noFilteredLogs")}</p>
                <div className={styles.emptyActions}>
                  <Button variant="default" size="small" onClick={() => setStatusFilter("all")}>
                    {t("logs.resetFilter")}
                  </Button>
                </div>
              </div>
            ) : (
              filteredLogs.map(log => renderLogEntry(log))
            )}
          </div>
        </>
      )}

      <Modal
        open={showResetStatsConfirm}
        onClose={() => setShowResetStatsConfirm(false)}
        title={t("logs.resetStatsModalTitle")}
      >
        <div className={styles.modalContent}>
          <p>{t("logs.resetStatsModalConfirmText", { date: resetBeforeDate })}</p>
          <div className={styles.resetStatsCalendar}>
            <div className={styles.resetStatsCalendarHeader}>
              <button
                type="button"
                className={styles.resetStatsCalendarNav}
                onClick={() =>
                  setResetCalendarMonth(
                    prev => new Date(prev.getFullYear(), prev.getMonth() - 1, 1)
                  )
                }
                aria-label={t("logs.resetStatsCalendarPrev")}
              >
                <ChevronLeft size={14} />
              </button>
              <strong>{getMonthDisplay(resetCalendarMonth)}</strong>
              <button
                type="button"
                className={styles.resetStatsCalendarNav}
                onClick={() =>
                  setResetCalendarMonth(
                    prev => new Date(prev.getFullYear(), prev.getMonth() + 1, 1)
                  )
                }
                aria-label={t("logs.resetStatsCalendarNext")}
                disabled={
                  getMonthStart(resetCalendarMonth).getTime() >= getMonthStart(new Date()).getTime()
                }
              >
                <ChevronRight size={14} />
              </button>
            </div>
            <div className={styles.resetStatsCalendarWeekdays}>
              {resetStatsWeekdayLabels.map(label => (
                <span key={label}>{label}</span>
              ))}
            </div>
            <div className={styles.resetStatsCalendarGrid}>
              {resetStatsCalendarCells.map(cell => (
                <button
                  key={cell.value}
                  type="button"
                  className={`${styles.resetStatsCalendarDay} ${cell.inCurrentMonth ? "" : styles.resetStatsCalendarDayMuted} ${cell.selected ? styles.resetStatsCalendarDaySelected : ""} ${cell.isToday ? styles.resetStatsCalendarDayToday : ""}`}
                  onClick={() => {
                    if (cell.disabled) return
                    setResetBeforeDate(cell.value)
                  }}
                  disabled={cell.disabled}
                >
                  {cell.day}
                </button>
              ))}
            </div>
          </div>
          <div className={styles.resetStatsPickedDate}>
            {t("logs.resetStatsModalDateLabel")}: <strong>{resetBeforeDate}</strong>
          </div>
          <div className={styles.modalActions}>
            <Button variant="default" onClick={() => setShowResetStatsConfirm(false)}>
              {t("common.cancel")}
            </Button>
            <Button variant="danger" onClick={handleConfirmResetStats} disabled={!resetBeforeDate}>
              {t("logs.resetStatsModalConfirmButton")}
            </Button>
          </div>
        </div>
      </Modal>

      <Modal
        open={showClearConfirm}
        onClose={() => setShowClearConfirm(false)}
        title={t("clearLogsModal.title")}
      >
        <div className={styles.modalContent}>
          <p>{t("clearLogsModal.confirmText", { count: logs.length })}</p>
          <div className={styles.modalActions}>
            <Button variant="default" onClick={() => setShowClearConfirm(false)}>
              {t("common.cancel")}
            </Button>
            <Button
              variant="danger"
              onClick={async () => {
                await handleClear()
                setShowClearConfirm(false)
              }}
            >
              {t("clearLogsModal.confirmClear")}
            </Button>
          </div>
        </div>
      </Modal>
    </div>
  )
}

export default LogsPage
