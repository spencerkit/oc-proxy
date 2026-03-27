/**
 * useLogs Hook
 *
 * Custom hook for managing proxy server logs.
 * Provides logs selector and auto-refresh effect.
 */

import { useCallback } from "react"
import { useToast } from "@/contexts/ToastContext"
import { clearLogsAction, logsErrorState, logsState, refreshLogsAction } from "@/store"
import type { LogEntry } from "@/types"
import { useActions, useRelaxValue } from "@/utils/relax"

const LOGS_ACTIONS = [refreshLogsAction, clearLogsAction] as const
const LOGS_ROUTE_PATH = "/logs"

type LogsRefreshTab = "stats" | "logs"

/**
 * Resolves whether logs and stats polling should run for the current route.
 */
export function resolveLogsRefreshPlan(
  pathname: string,
  activeTab: LogsRefreshTab
): {
  pollLogs: boolean
  pollStats: boolean
} {
  const isLogsRoute = pathname === LOGS_ROUTE_PATH
  return {
    pollLogs: isLogsRoute && activeTab === "logs",
    pollStats: isLogsRoute,
  }
}

/**
 * Hook for accessing server logs
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
  const logs = useRelaxValue(logsState)
  const error = useRelaxValue(logsErrorState)
  const [refreshLogs, clearLogs] = useActions(LOGS_ACTIONS)
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
  return useRelaxValue(logsState)
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
  const logs = useRelaxValue(logsState)
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
  return useRelaxValue(logsState).length
}
