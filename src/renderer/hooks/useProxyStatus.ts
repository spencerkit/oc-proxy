/**
 * useProxyStatus Hook
 *
 * Custom hook for managing proxy server status.
 * Provides status, running selectors and auto-refresh effect.
 */

import { useEffect } from "react"
import { isRunningState, refreshStatusAction, statusErrorState, statusState } from "@/store"
import { useActions, useRelaxValue } from "@/utils/relax"

const STATUS_ACTIONS = [refreshStatusAction] as const

/**
 * Auto-refresh interval for status (in milliseconds)
 */
const STATUS_REFRESH_INTERVAL = 3000

/**
 * Hook for accessing proxy server status with auto-refresh
 *
 * @returns Object containing status, running state, and refresh function
 *
 * @example
 * const { status, running, refreshStatus } = useProxyStatus();
 *
 * if (running) {
 *   console.log('Server is running with', status?.metrics.requests, 'requests');
 * }
 */
export function useProxyStatus() {
  const status = useRelaxValue(statusState)
  const running = useRelaxValue(isRunningState)
  const error = useRelaxValue(statusErrorState)
  const [refreshStatus] = useActions(STATUS_ACTIONS)

  return {
    status,
    running,
    refreshStatus,
    error,
  }
}

/**
 * Hook for accessing proxy server status with auto-refresh
 * Automatically refreshes status at regular intervals
 *
 * @returns Object containing status and running state
 *
 * @example
 * const { status, running } = useProxyStatusAutoRefresh();
 *
 * // Status automatically refreshes every 3 seconds
 */
export function useProxyStatusAutoRefresh() {
  const status = useRelaxValue(statusState)
  const running = useRelaxValue(isRunningState)
  const [refreshStatus] = useActions(STATUS_ACTIONS)

  // Auto-refresh effect
  useEffect(() => {
    // Initial refresh
    refreshStatus()

    // Set up interval for auto-refresh
    const intervalId = setInterval(() => {
      if (document.visibilityState !== "visible") return
      refreshStatus()
    }, STATUS_REFRESH_INTERVAL)

    // Cleanup interval on unmount
    return () => {
      clearInterval(intervalId)
    }
  }, [refreshStatus])

  return {
    status,
    running,
  }
}

/**
 * Hook for accessing only the running state
 * Useful when you only need to know if the server is running
 *
 * @returns Boolean indicating if server is running
 *
 * @example
 * const running = useRunningState();
 *
 * if (running) {
 *   // Show running UI
 * }
 */
export function useRunningState(): boolean {
  return useRelaxValue(isRunningState)
}

/**
 * Hook for accessing only the status value
 * Useful when you only need to read status
 *
 * @returns Current proxy status or null
 *
 * @example
 * const status = useStatusValue();
 * const requests = status?.metrics.requests;
 */
export function useStatusValue() {
  return useRelaxValue(statusState)
}
