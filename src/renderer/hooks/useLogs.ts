/**
 * useLogs Hook
 *
 * Custom hook for managing proxy server logs.
 * Provides logs selector and auto-refresh effect.
 */

import { useCallback, useEffect } from "react"
import { useToast } from "@/contexts/ToastContext"
import { useProxyStore } from "@/store"
import type { LogEntry } from "@/types"

/**
 * Auto-refresh interval for logs (in milliseconds)
 */
const LOGS_REFRESH_INTERVAL = 3000

/**
 * Hook for accessing server logs with auto-refresh
 *
 * @returns Object containing logs, refresh function, and clear function
 *
 * @example
 * const { logs, refreshLogs, clearLogs } = useLogs();
 *
 * // Display logs: {logs.map(log => <div key={log.id}>{log.message}</div>)}
 *
 * // Clear all logs: await clearLogs();
 */
export function useLogs() {
  const logs = useProxyStore(state => state.logs)
  const refreshLogs = useProxyStore(state => state.refreshLogs)
  const clearLogs = useProxyStore(state => state.clearLogs)
  const error = useProxyStore(state => state.logsError)
  const { showToast } = useToast()

  const showToastMessage = useCallback(
    (message: string, type?: "success" | "error" | "info" | "warning") => {
      showToast(message, type)
    },
    [showToast]
  )

  return {
    logs,
    refreshLogs,
    clearLogs,
    error,
    showToast: showToastMessage,
  }
}

/**
 * Hook for accessing server logs with auto-refresh
 * Automatically refreshes logs at regular intervals
 *
 * @returns Object containing logs and clear function
 *
 * @example
 * const { logs, clearLogs } = useLogsAutoRefresh();
 *
 * // Logs automatically refresh every 3 seconds
 * return <LogList logs={logs} onClear={clearLogs} />;
 */
export function useLogsAutoRefresh() {
  const logs = useProxyStore(state => state.logs)
  const refreshLogs = useProxyStore(state => state.refreshLogs)
  const clearLogs = useProxyStore(state => state.clearLogs)
  const { showToast } = useToast()

  // Auto-refresh effect
  useEffect(() => {
    // Initial refresh
    refreshLogs()

    // Set up interval for auto-refresh
    const intervalId = setInterval(() => {
      if (document.visibilityState !== "visible") return
      refreshLogs()
    }, LOGS_REFRESH_INTERVAL)

    // Cleanup interval on unmount
    return () => {
      clearInterval(intervalId)
    }
  }, [refreshLogs])

  const showToastMessage = useCallback(
    (message: string, type?: "success" | "error" | "info" | "warning") => {
      showToast(message, type)
    },
    [showToast]
  )

  return {
    logs,
    clearLogs,
    showToast: showToastMessage,
  }
}

/**
 * Hook for accessing only the logs value
 * Useful when you only need to read logs
 *
 * @returns Array of log entries
 *
 * @example
 * const logs = useLogsValue();
 * const errorCount = logs.filter(log => log.status === 'error').length;
 */
export function useLogsValue(): LogEntry[] {
  return useProxyStore(state => state.logs)
}

/**
 * Hook for filtered logs based on a predicate function
 * Useful for displaying specific log types
 *
 * @param predicate - Function to filter logs
 * @returns Array of filtered log entries
 *
 * @example
 * // Get only error logs
 * const errorLogs = useFilteredLogs(log => log.status === 'error');
 *
 * // Get logs for a specific group
 * const groupLogs = useFilteredLogs(log => log.groupId === 'group-123');
 */
export function useFilteredLogs(predicate: (log: LogEntry) => boolean): LogEntry[] {
  const logs = useProxyStore(state => state.logs)
  return logs.filter(predicate)
}

/**
 * Hook for getting log count
 * Useful for badges or counters
 *
 * @returns Number of log entries
 *
 * @example
 * const logCount = useLogCount();
 * return <Badge count={logCount} />;
 */
export function useLogCount(): number {
  return useProxyStore(state => state.logs.length)
}
