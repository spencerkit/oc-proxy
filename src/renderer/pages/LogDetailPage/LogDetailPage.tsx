import { ArrowLeft, RefreshCw } from "lucide-react"
import type React from "react"
import { useEffect, useMemo, useState } from "react"
import { useNavigate, useParams } from "react-router-dom"
import { Button } from "@/components"
import { useTranslation } from "@/hooks"
import { useProxyStore } from "@/store"
import type { LogEntry } from "@/types"
import { formatTokenMillions } from "@/utils/tokenFormat"
import styles from "./LogDetailPage.module.css"

function isContainer(value: unknown): value is Record<string, unknown> | unknown[] {
  return typeof value === "object" && value !== null
}

function parseStructuredValue(value: unknown): unknown | null {
  if (value === null || value === undefined) return null
  if (typeof value === "string") {
    const trimmed = value.trim()
    if (!trimmed) return null
    try {
      const parsed = JSON.parse(trimmed)
      return isContainer(parsed) ? parsed : null
    } catch {
      return null
    }
  }
  return isContainer(value) ? value : null
}

function toText(value: unknown, emptyText: string): string {
  if (value === null || value === undefined) {
    return emptyText
  }

  if (typeof value === "string") {
    return value || emptyText
  }

  if (typeof value === "number" || typeof value === "boolean") {
    return String(value)
  }

  try {
    return JSON.stringify(value, null, 2)
  } catch {
    return String(value)
  }
}

function formatTimestamp(timestamp: string): string {
  const date = new Date(timestamp)
  return date.toLocaleString()
}

function renderPrimitive(value: unknown) {
  if (value === null) {
    return <span className={styles.valueNull}>null</span>
  }
  if (typeof value === "string") {
    return <span className={styles.valueString}>"{value}"</span>
  }
  if (typeof value === "number") {
    return <span className={styles.valueNumber}>{value}</span>
  }
  if (typeof value === "boolean") {
    return <span className={styles.valueBoolean}>{String(value)}</span>
  }
  return <span className={styles.valueFallback}>{String(value)}</span>
}

interface JsonNodeProps {
  value: unknown
  label: string | null
  path: string
  depth: number
  inArray: boolean
  expandedPaths: Set<string>
  onToggle: (path: string) => void
}

function JsonNode({ value, label, path, depth, inArray, expandedPaths, onToggle }: JsonNodeProps) {
  const container = isContainer(value)
  const expanded = container ? expandedPaths.has(path) : false
  const entries = container
    ? Array.isArray(value)
      ? value.map((item, index) => [String(index), item] as const)
      : Object.entries(value)
    : []
  const summary = container
    ? Array.isArray(value)
      ? `[${entries.length} items]`
      : `{${entries.length} keys}`
    : ""

  return (
    <div>
      <div className={styles.jsonRow} style={{ paddingLeft: `${depth * 14}px` }}>
        {container ? (
          <button
            type="button"
            className={styles.toggleButton}
            onClick={() => onToggle(path)}
            aria-label={expanded ? "Collapse node" : "Expand node"}
          >
            {expanded ? "▾" : "▸"}
          </button>
        ) : (
          <span className={styles.toggleSpacer} />
        )}
        {label !== null && (
          <>
            <span className={styles.nodeKey}>{inArray ? `[${label}]` : label}</span>
            <span className={styles.nodeColon}>:</span>
          </>
        )}
        {container ? <span className={styles.nodeSummary}>{summary}</span> : renderPrimitive(value)}
      </div>
      {container && expanded && (
        <div className={styles.nodeChildren}>
          {entries.length === 0 ? (
            <div className={styles.jsonRow} style={{ paddingLeft: `${(depth + 1) * 14}px` }}>
              <span className={styles.toggleSpacer} />
              <span className={styles.emptyNode}>(empty)</span>
            </div>
          ) : (
            entries.map(([key, child]) => (
              <JsonNode
                key={`${path}.${key}`}
                value={child}
                label={key}
                path={`${path}.${key}`}
                depth={depth + 1}
                inArray={Array.isArray(value)}
                expandedPaths={expandedPaths}
                onToggle={onToggle}
              />
            ))
          )}
        </div>
      )}
    </div>
  )
}

function StructuredValue({
  value,
  emptyText,
  resetKey,
}: {
  value: unknown
  emptyText: string
  resetKey: string
}) {
  const parsed = useMemo(() => parseStructuredValue(value), [value])
  const [expandedPaths, setExpandedPaths] = useState<Set<string>>(() => new Set(["$"]))

  useEffect(() => {
    setExpandedPaths(new Set(["$"]))
  }, [])

  if (!parsed) {
    return <pre>{toText(value, emptyText)}</pre>
  }

  const togglePath = (path: string) => {
    setExpandedPaths(prev => {
      const next = new Set(prev)
      if (next.has(path)) {
        next.delete(path)
      } else {
        next.add(path)
      }
      return next
    })
  }

  return (
    <div className={styles.treeContainer}>
      <JsonNode
        value={parsed}
        label={null}
        path="$"
        depth={0}
        inArray={false}
        expandedPaths={expandedPaths}
        onToggle={togglePath}
      />
    </div>
  )
}

export const LogDetailPage: React.FC = () => {
  const navigate = useNavigate()
  const { traceId } = useParams<{ traceId: string }>()
  const { t } = useTranslation()
  const { logs, refreshLogs, loading } = useProxyStore()

  const decodedTraceId = useMemo(() => (traceId ? decodeURIComponent(traceId) : ""), [traceId])

  useEffect(() => {
    refreshLogs()
  }, [refreshLogs])

  const log = useMemo(() => {
    if (!decodedTraceId) return null
    return logs.find(item => item.traceId === decodedTraceId) ?? null
  }, [decodedTraceId, logs])

  const getStatusText = (status: LogEntry["status"]) => t(`logs.state.${status}`)

  if (!decodedTraceId) {
    return (
      <div className={styles.logDetailPage}>
        <div className={styles.header}>
          <h2>{t("logs.detailTitle")}</h2>
        </div>
        <div className={styles.emptyState}>
          <p>{t("logs.logNotFound")}</p>
          <Button variant="default" icon={ArrowLeft} onClick={() => navigate("/logs")}>
            {t("logs.backToList")}
          </Button>
        </div>
      </div>
    )
  }

  return (
    <div className={styles.logDetailPage}>
      <div className={styles.header}>
        <div className={styles.headerTop}>
          <Button variant="default" size="small" icon={ArrowLeft} onClick={() => navigate("/logs")}>
            {t("logs.backToList")}
          </Button>
          <Button
            variant="default"
            size="small"
            icon={RefreshCw}
            onClick={() => refreshLogs()}
            loading={loading}
          >
            {t("logs.refresh")}
          </Button>
        </div>
        <h2>{t("logs.detailTitle")}</h2>
        <p className={styles.subtitle}>
          {t("logs.traceIdLabel")}: {decodedTraceId}
        </p>
      </div>

      {!log ? (
        <div className={styles.emptyState}>
          <p>{t("logs.logNotFound")}</p>
        </div>
      ) : (
        <>
          <div className={styles.metaGrid}>
            <div className={styles.metaItem}>
              <span>{t("logs.timeLabel")}</span>
              <strong>{formatTimestamp(log.timestamp)}</strong>
            </div>
            <div className={styles.metaItem}>
              <span>{t("logs.status")}</span>
              <strong>
                {t("logs.requestStatus", {
                  status: log.httpStatus ?? "---",
                  state: getStatusText(log.status),
                })}
              </strong>
            </div>
            <div className={styles.metaItem}>
              <span>{t("logs.request")}</span>
              <strong>
                {log.method} {log.requestPath}
              </strong>
            </div>
            <div className={styles.metaItem}>
              <span>{t("logs.duration")}</span>
              <strong>{log.durationMs}ms</strong>
            </div>
          </div>

          <div className={styles.dataArea}>
            <h3 className={styles.areaTitle}>{t("logs.requestDataSection")}</h3>
            <div className={styles.section}>
              <h3>{t("logs.requestHeaders")}</h3>
              <pre>{toText(log.requestHeaders, t("logs.emptyValue"))}</pre>
            </div>

            <div className={styles.section}>
              <h3>{t("logs.forwardRequestHeaders")}</h3>
              <pre>{toText(log.forwardRequestHeaders, t("logs.emptyValue"))}</pre>
            </div>

            <div className={styles.section}>
              <h3>{t("logs.responseHeaders")}</h3>
              <pre>{toText(log.responseHeaders, t("logs.emptyValue"))}</pre>
            </div>

            <div className={styles.section}>
              <h3>{t("logs.upstreamResponseHeaders")}</h3>
              <pre>{toText(log.upstreamResponseHeaders, t("logs.emptyValue"))}</pre>
            </div>

            <div className={styles.section}>
              <h3>{t("logs.requestBody")}</h3>
              <StructuredValue
                value={log.requestBody}
                emptyText={t("logs.emptyValue")}
                resetKey={`${decodedTraceId}:request`}
              />
            </div>

            <div className={styles.section}>
              <h3>{t("logs.responseBody")}</h3>
              <StructuredValue
                value={log.responseBody}
                emptyText={t("logs.emptyValue")}
                resetKey={`${decodedTraceId}:response`}
              />
            </div>

            {log.error && (
              <div className={styles.section}>
                <h3>{t("logs.errorDetail")}</h3>
                <pre>{toText(log.error, t("logs.emptyValue"))}</pre>
              </div>
            )}
          </div>

          <div className={styles.dataArea}>
            <h3 className={styles.areaTitle}>{t("logs.tokenDataSection")}</h3>
            <div className={styles.tokenGrid}>
              <div className={styles.tokenCard}>
                <span>{t("logs.tokenInput")}</span>
                <strong>{formatTokenMillions(log.tokenUsage?.inputTokens ?? 0)}</strong>
              </div>
              <div className={styles.tokenCard}>
                <span>{t("logs.tokenOutput")}</span>
                <strong>{formatTokenMillions(log.tokenUsage?.outputTokens ?? 0)}</strong>
              </div>
              <div className={styles.tokenCard}>
                <span>{t("logs.tokenCacheRead")}</span>
                <strong>{formatTokenMillions(log.tokenUsage?.cacheReadTokens ?? 0)}</strong>
              </div>
              <div className={styles.tokenCard}>
                <span>{t("logs.tokenCacheWrite")}</span>
                <strong>{formatTokenMillions(log.tokenUsage?.cacheWriteTokens ?? 0)}</strong>
              </div>
            </div>
          </div>
        </>
      )}
    </div>
  )
}

export default LogDetailPage
