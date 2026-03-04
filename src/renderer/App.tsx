import type React from "react"
import { useCallback, useEffect } from "react"
import { Navigate, Route, Routes } from "react-router-dom"
import { Layout } from "@/components"
import { useLogs, useTranslation } from "@/hooks"
import {
  GroupEditPage,
  LogDetailPage,
  LogsPage,
  RuleCreatePage,
  RuleEditPage,
  ServicePage,
  SettingsPage,
} from "@/pages"
import { useProxyStore } from "@/store"
import {
  formatServerAddressForDisplay,
  resolveReachableServerBaseUrls,
} from "@/utils/serverAddress"

/**
 * Main App Component
 * Sets up routing and initializes the store
 */
const App: React.FC = () => {
  console.log("[App] Rendering...")

  const store = useProxyStore()
  const { t } = useTranslation()
  const { showToast } = useLogs()

  // Fallback translation function
  const translate = useCallback(
    (key: string, options?: Record<string, string | number>) => {
      if (typeof t === "function") {
        try {
          return t(key, options)
        } catch {
          return key
        }
      }
      return key
    },
    [t]
  )

  const isPortInUseError = useCallback((message: string) => {
    const normalized = String(message || "").toLowerCase()
    return normalized.includes("eaddrinuse") || normalized.includes("address already in use")
  }, [])

  const { init, loading, error, status, startServer, stopServer, config } = store || {
    init: () => {},
    loading: false,
    error: null,
    status: null,
    config: null,
    startServer: () => {},
    stopServer: () => {},
  }

  const isRunning = status?.running ?? false
  const serverAddresses = status
    ? resolveReachableServerBaseUrls({
        statusAddress: status.address,
        statusLanAddress: status.lanAddress,
        configHost: config?.server.host,
        configPort: config?.server.port,
      })
    : []
  const serverAddress =
    serverAddresses.length > 0
      ? serverAddresses.map(address => formatServerAddressForDisplay(address)).join(" ; ")
      : undefined

  const handleStartServer = useCallback(async () => {
    try {
      await Promise.resolve(startServer())
      showToast(translate("toast.serviceStarted"), "success")
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error)
      if (isPortInUseError(message)) {
        showToast(translate("toast.serviceStartPortInUse"), "error")
        return
      }
      showToast(translate("errors.operationFailed", { message }), "error")
    }
  }, [isPortInUseError, showToast, startServer, translate])

  const handleStopServer = useCallback(async () => {
    try {
      await Promise.resolve(stopServer())
      showToast(translate("toast.serviceStopped"), "success")
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error)
      showToast(translate("errors.operationFailed", { message }), "error")
    }
  }, [showToast, stopServer, translate])

  console.log("[App] loading:", loading, "error:", error, "status:", status)

  useEffect(() => {
    console.log("[App] useEffect running, calling init()...")
    init()
  }, [init])

  console.log("[App] About to render layout")

  const isInitialLoading = loading && !error && !config && !status

  if (isInitialLoading) {
    console.log("[App] Showing loading screen")
    return (
      <div className="loading-screen">
        <p>{translate("app.statusLoading")}</p>
      </div>
    )
  }

  if (error) {
    console.log("[App] Showing error screen:", error)
    return (
      <div className="error-screen">
        <p>{error}</p>
      </div>
    )
  }

  console.log("[App] Rendering routes")
  return (
    <Layout
      isRunning={isRunning}
      serverAddress={serverAddress}
      onStartServer={handleStartServer}
      onStopServer={handleStopServer}
    >
      <Routes>
        <Route path="/" element={<ServicePage />} />
        <Route path="/settings" element={<SettingsPage />} />
        <Route path="/logs" element={<LogsPage />} />
        <Route path="/logs/:traceId" element={<LogDetailPage />} />
        <Route path="/groups/:groupId/edit" element={<GroupEditPage />} />
        <Route path="/groups/:groupId/providers/new" element={<RuleCreatePage />} />
        <Route path="/groups/:groupId/providers/:providerId/edit" element={<RuleEditPage />} />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
    </Layout>
  )
}

export default App
