import { ArrowLeft, RefreshCw } from "lucide-react"
import type React from "react"
import { useEffect, useMemo } from "react"
import { useNavigate, useParams } from "react-router-dom"
import { Button } from "@/components"
import { useTranslation } from "@/hooks"
import { useProxyStore } from "@/store"
import type { LogEntry } from "@/types"
import styles from "./LogDetailPage.module.css"

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
              <pre>{toText(log.requestBody, t("logs.emptyValue"))}</pre>
            </div>

            <div className={styles.section}>
              <h3>{t("logs.responseBody")}</h3>
              <pre>{toText(log.responseBody, t("logs.emptyValue"))}</pre>
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
                <strong>{log.tokenUsage?.inputTokens ?? 0}</strong>
              </div>
              <div className={styles.tokenCard}>
                <span>{t("logs.tokenOutput")}</span>
                <strong>{log.tokenUsage?.outputTokens ?? 0}</strong>
              </div>
              <div className={styles.tokenCard}>
                <span>{t("logs.tokenCacheRead")}</span>
                <strong>{log.tokenUsage?.cacheReadTokens ?? 0}</strong>
              </div>
              <div className={styles.tokenCard}>
                <span>{t("logs.tokenCacheWrite")}</span>
                <strong>{log.tokenUsage?.cacheWriteTokens ?? 0}</strong>
              </div>
            </div>
          </div>
        </>
      )}
    </div>
  )
}

export default LogDetailPage
