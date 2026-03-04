import type { EChartsOption } from "echarts"
import * as echarts from "echarts"
import { Check, RotateCcw, Trash2, X } from "lucide-react"
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
  tpmInputLine: "#4fc8a8",
  tpmOutputLine: "#5d95ff",
} as const

type LogsTab = "stats" | "logs"

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

function formatRateMetric(value: number): string {
  if (!Number.isFinite(value) || value <= 0) return "0"
  if (value < 0.01) return "<0.01"
  if (value >= 1000) return formatTokenAxisValue(value)
  return value.toFixed(2).replace(/\.00$/, "")
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
  const [selectedRuleKeys, setSelectedRuleKeys] = useState<string[]>([])
  const [ruleSearchValue, setRuleSearchValue] = useState("")
  const [ruleDropdownOpen, setRuleDropdownOpen] = useState(false)
  const [hoursFilter, setHoursFilter] = useState<number>(24)
  const [showClearConfirm, setShowClearConfirm] = useState(false)
  const hasInitializedRuleSelectionRef = useRef(false)
  const ruleComboboxRef = useRef<HTMLDivElement | null>(null)
  const usageChartDomRef = useRef<HTMLDivElement | null>(null)
  const usageChartRef = useRef<echarts.ECharts | null>(null)
  const rateChartDomRef = useRef<HTMLDivElement | null>(null)
  const rateChartRef = useRef<echarts.ECharts | null>(null)
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
      for (const rule of group.rules || []) {
        options.push({
          key: `${group.id}::${rule.id}`,
          label: `${group.name || group.id}-${rule.name || rule.id}`,
        })
      }
    }
    return options
  }, [config])

  const ruleOptionsByKey = useMemo(() => {
    const map = new Map<string, { key: string; label: string }>()
    for (const option of ruleOptions) {
      map.set(option.key, option)
    }
    return map
  }, [ruleOptions])

  const selectedRuleKeySet = useMemo(() => new Set(selectedRuleKeys), [selectedRuleKeys])

  const selectedRuleOptions = useMemo(() => {
    return selectedRuleKeys
      .map(key => ruleOptionsByKey.get(key))
      .filter((option): option is { key: string; label: string } => Boolean(option))
  }, [ruleOptionsByKey, selectedRuleKeys])

  const visibleRuleOptions = useMemo(() => {
    const keyword = ruleSearchValue.trim().toLowerCase()
    if (!keyword) return ruleOptions
    return ruleOptions.filter(option => option.label.toLowerCase().includes(keyword))
  }, [ruleOptions, ruleSearchValue])

  const allRuleKeys = useMemo(() => ruleOptions.map(option => option.key), [ruleOptions])
  const isAllSelected = allRuleKeys.length > 0 && selectedRuleKeys.length === allRuleKeys.length

  useEffect(() => {
    const validKeys = new Set(allRuleKeys)
    setSelectedRuleKeys(prev => {
      if (!hasInitializedRuleSelectionRef.current) {
        hasInitializedRuleSelectionRef.current = true
        return [...allRuleKeys]
      }
      return prev.filter(key => validKeys.has(key))
    })
  }, [allRuleKeys])

  useEffect(() => {
    if (!hasInitializedRuleSelectionRef.current) return
    void refreshLogsStats(hoursFilter, selectedRuleKeys)
  }, [hoursFilter, refreshLogsStats, selectedRuleKeys])

  useEffect(() => {
    if (!hasInitializedRuleSelectionRef.current) return
    const timer = window.setInterval(() => {
      void refreshLogsStats(hoursFilter, selectedRuleKeys)
    }, 3000)
    return () => window.clearInterval(timer)
  }, [hoursFilter, refreshLogsStats, selectedRuleKeys])

  useEffect(() => {
    const handleOutsideClick = (event: MouseEvent) => {
      if (!ruleComboboxRef.current) return
      const target = event.target as Node
      if (!ruleComboboxRef.current.contains(target)) {
        setRuleDropdownOpen(false)
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

  const handleResetStats = async () => {
    try {
      await clearLogsStats()
      await refreshLogsStats(hoursFilter, selectedRuleKeys)
      showToast(t("logs.resetStatsSuccess"), "success")
    } catch (error) {
      showToast(t("logs.resetStatsError"), "error")
      console.error(error)
    }
  }

  const handleToggleRule = (ruleKey: string) => {
    setSelectedRuleKeys(prev => {
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
      rpm: hourly.map(point => point.requests / 60),
      inputTpm: hourly.map(point => point.inputTokens / 60),
      outputTpm: hourly.map(point => point.outputTokens / 60),
    }
  }, [hoursFilter, logsStats?.hourly])

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
        data: [t("logs.trendRpm"), t("logs.trendInputTpm"), t("logs.trendOutputTpm")],
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
      yAxis: [
        {
          type: "value",
          name: t("logs.trendRateLeftAxis"),
          axisLabel: {
            color: MACARON.axis,
            fontSize: 11,
            formatter: (value: number) => formatRateMetric(value),
          },
          splitLine: { lineStyle: { color: MACARON.split, type: "dashed" } },
        },
        {
          type: "value",
          name: t("logs.trendRateRightAxis"),
          axisLabel: {
            color: MACARON.axis,
            fontSize: 11,
            formatter: (value: number) => formatRateMetric(value),
          },
          splitLine: { show: false },
        },
      ],
      series: [
        {
          name: t("logs.trendInputTpm"),
          type: "line",
          data: rateTrendSeries.inputTpm,
          yAxisIndex: 0,
          smooth: true,
          symbol: "circle",
          symbolSize: 5,
          lineStyle: { color: MACARON.tpmInputLine, width: 2 },
          itemStyle: { color: MACARON.tpmInputLine },
        },
        {
          name: t("logs.trendOutputTpm"),
          type: "line",
          data: rateTrendSeries.outputTpm,
          yAxisIndex: 0,
          smooth: true,
          symbol: "circle",
          symbolSize: 5,
          lineStyle: { color: MACARON.tpmOutputLine, width: 2 },
          itemStyle: { color: MACARON.tpmOutputLine },
        },
        {
          name: t("logs.trendRpm"),
          type: "line",
          data: rateTrendSeries.rpm,
          yAxisIndex: 1,
          smooth: true,
          symbol: "circle",
          symbolSize: 5,
          lineStyle: { color: MACARON.requestLine, width: 2 },
          itemStyle: { color: MACARON.requestLine },
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
            ? `${t("logs.statsTabSubtitle", { range: getHoursLabel(hoursFilter) })} · ${t("logs.statsRulesSelected", { count: selectedRuleKeys.length })}`
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
              <div className={styles.ruleCombobox} ref={ruleComboboxRef}>
                <input
                  className={styles.inlineInput}
                  type="text"
                  value={ruleSearchValue}
                  onFocus={() => setRuleDropdownOpen(true)}
                  onChange={e => {
                    setRuleSearchValue(e.target.value)
                    setRuleDropdownOpen(true)
                  }}
                  onKeyDown={e => {
                    if (e.key === "Escape") {
                      setRuleDropdownOpen(false)
                    }
                    if (
                      e.key === "Backspace" &&
                      !ruleSearchValue.trim() &&
                      selectedRuleKeys.length > 0
                    ) {
                      e.preventDefault()
                      setSelectedRuleKeys(prev => prev.slice(0, Math.max(0, prev.length - 1)))
                    }
                    if (e.key === "Enter" && visibleRuleOptions.length > 0) {
                      e.preventDefault()
                      handleToggleRule(visibleRuleOptions[0].key)
                    }
                  }}
                  placeholder={t("logs.statsRuleSearchPlaceholder")}
                />
                {ruleDropdownOpen && (
                  <div className={styles.ruleDropdown}>
                    <div className={styles.ruleDropdownActions}>
                      <button
                        type="button"
                        className={styles.ruleDropdownAction}
                        onMouseDown={event => event.preventDefault()}
                        onClick={() => setSelectedRuleKeys([...allRuleKeys])}
                      >
                        {t("logs.statsSelectAll")}
                      </button>
                      <button
                        type="button"
                        className={styles.ruleDropdownAction}
                        onMouseDown={event => event.preventDefault()}
                        onClick={() => setSelectedRuleKeys([])}
                      >
                        {t("logs.statsClearSelection")}
                      </button>
                    </div>
                    {visibleRuleOptions.length === 0 ? (
                      <div className={styles.ruleDropdownEmpty}>{t("logs.noStatsData")}</div>
                    ) : (
                      visibleRuleOptions.map(option => {
                        const checked = selectedRuleKeySet.has(option.key)
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
                : t("logs.statsRulesSelected", { count: selectedRuleKeys.length })}
            </span>
            <button
              type="button"
              className={styles.ruleClearButton}
              onClick={() => setSelectedRuleKeys([])}
              disabled={selectedRuleKeys.length === 0}
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
                        setSelectedRuleKeys(prev => prev.filter(key => key !== option.key))
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
              </div>
              <div className={styles.summaryCard}>
                <span className={styles.summaryLabel}>{t("logs.errorsCount")}</span>
                <strong className={`${styles.summaryValue} ${styles.summaryValueDanger}`}>
                  {totalErrors}
                </strong>
              </div>
              <div className={styles.summaryCard}>
                <span className={styles.summaryLabel}>{t("logs.successRate")}</span>
                <strong className={styles.summaryValue}>{successRate}%</strong>
              </div>
              <div className={styles.summaryCard}>
                <span className={styles.summaryLabel}>{t("logs.rpm")}</span>
                <strong className={styles.summaryValue}>
                  {formatRateMetric(logsStats?.rpm ?? 0)}
                </strong>
              </div>
            </div>
          </div>

          <div className={styles.metricsSection}>
            <h3 className={styles.metricsTitle}>{t("logs.tokenMetricsSection")}</h3>
            <div className={styles.summaryGrid}>
              <div className={styles.summaryCard}>
                <span className={styles.summaryLabel}>{t("logs.totalInputTokens")}</span>
                <strong className={styles.summaryValue}>
                  {formatTokenMillions(logsStats?.inputTokens ?? 0)}
                </strong>
              </div>
              <div className={styles.summaryCard}>
                <span className={styles.summaryLabel}>{t("logs.totalOutputTokens")}</span>
                <strong className={styles.summaryValue}>
                  {formatTokenMillions(logsStats?.outputTokens ?? 0)}
                </strong>
              </div>
              <div className={styles.summaryCard}>
                <span className={styles.summaryLabel}>{t("logs.totalCacheReadTokens")}</span>
                <strong className={styles.summaryValue}>
                  {formatTokenMillions(logsStats?.cacheReadTokens ?? 0)}
                </strong>
              </div>
              <div className={styles.summaryCard}>
                <span className={styles.summaryLabel}>{t("logs.totalCacheWriteTokens")}</span>
                <strong className={styles.summaryValue}>
                  {formatTokenMillions(logsStats?.cacheWriteTokens ?? 0)}
                </strong>
              </div>
              <div className={styles.summaryCard}>
                <span className={styles.summaryLabel}>{t("logs.inputTpm")}</span>
                <strong className={styles.summaryValue}>
                  {formatRateMetric(logsStats?.inputTpm ?? 0)}
                </strong>
              </div>
              <div className={styles.summaryCard}>
                <span className={styles.summaryLabel}>{t("logs.outputTpm")}</span>
                <strong className={styles.summaryValue}>
                  {formatRateMetric(logsStats?.outputTpm ?? 0)}
                </strong>
              </div>
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
