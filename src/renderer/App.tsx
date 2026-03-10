import type React from "react"
import { useCallback, useEffect, useRef } from "react"
import { Navigate, Route, Routes } from "react-router-dom"
import { shallow } from "zustand/shallow"
import { Layout } from "@/components"
import { useLogs, useTranslation } from "@/hooks"
import {
  AgentEditPage,
  AgentListPage,
  GroupEditPage,
  LogDetailPage,
  LogsPage,
  ProvidersPage,
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

  const {
    init,
    bootstrapping,
    bootstrapError,
    status,
    startServer,
    stopServer,
    config,
    serverAction,
  } = useProxyStore(
    state => ({
      init: state.init,
      bootstrapping: state.bootstrapping,
      bootstrapError: state.bootstrapError,
      status: state.status,
      startServer: state.startServer,
      stopServer: state.stopServer,
      config: state.config,
      serverAction: state.serverAction,
    }),
    shallow
  )
  const { t } = useTranslation()
  const { showToast } = useLogs()
  const initStartedRef = useRef(false)

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

  console.log(
    "[App] bootstrapping:",
    bootstrapping,
    "bootstrapError:",
    bootstrapError,
    "status:",
    status
  )

  useEffect(() => {
    if (initStartedRef.current) {
      return
    }
    initStartedRef.current = true
    console.log("[App] useEffect running, calling init()...")
    void init()
  }, [init])

  console.log("[App] About to render layout")

  if (bootstrapping && !bootstrapError && !config && !status) {
    console.log("[App] Showing loading screen")
    return (
      <div className="loading-screen">
        <p>{translate("app.statusLoading")}</p>
      </div>
    )
  }

  if (bootstrapError) {
    console.log("[App] Showing error screen:", bootstrapError)
    return (
      <div className="error-screen">
        <p>{bootstrapError}</p>
      </div>
    )
  }

  console.log("[App] Rendering routes")
  return (
    <Layout
      isRunning={isRunning}
      serverAddress={serverAddress}
      serverAction={serverAction}
      onStartServer={handleStartServer}
      onStopServer={handleStopServer}
    >
      <Routes>
        <Route path="/" element={<ServicePage />} />
        <Route path="/settings" element={<SettingsPage />} />
        <Route path="/logs" element={<LogsPage />} />
        <Route path="/logs/:traceId" element={<LogDetailPage />} />
        <Route path="/agents" element={<AgentListPage />} />
        <Route path="/providers" element={<ProvidersPage />} />
        <Route path="/providers/new" element={<RuleCreatePage />} />
        <Route path="/providers/:providerId/edit" element={<RuleEditPage />} />
        <Route path="/agents/:targetId/edit" element={<AgentEditPage />} />
        <Route path="/groups/:groupId/edit" element={<GroupEditPage />} />
        <Route path="/groups/:groupId/providers/new" element={<RuleCreatePage />} />
        <Route path="/groups/:groupId/providers/:providerId/edit" element={<RuleEditPage />} />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
    </Layout>
  )
}

export default App
