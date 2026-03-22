import type React from "react"
import { useCallback, useEffect, useRef, useState } from "react"
import { Navigate, Route, Routes } from "react-router-dom"
import { Layout, RemoteManagementLogin } from "@/components"
import { useLogs, useTranslation, useUpdater } from "@/hooks"
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
import {
  bootstrapErrorState,
  bootstrappingState,
  configState,
  initAction,
  serverActionState,
  startServerAction,
  statusState,
  stopServerAction,
} from "@/store"
import type { AuthSessionStatus } from "@/types"
import { emitAuthSessionChanged, subscribeAuthSessionChanged } from "@/utils/authSession"
import { bridge } from "@/utils/bridge"
import { useActions, useRelaxValue } from "@/utils/relax"
import { isHeadlessHttpRuntime } from "@/utils/runtime"
import {
  formatServerAddressForDisplay,
  resolveReachableServerBaseUrls,
} from "@/utils/serverAddress"

const APP_ACTIONS = [initAction, startServerAction, stopServerAction] as const

/**
 * Main App Component
 * Sets up routing and initializes the store
 */
const App: React.FC = () => {
  console.log("[App] Rendering...")

  const bootstrapping = useRelaxValue(bootstrappingState)
  const bootstrapError = useRelaxValue(bootstrapErrorState)
  const status = useRelaxValue(statusState)
  const config = useRelaxValue(configState)
  const serverAction = useRelaxValue(serverActionState)
  const [init, startServer, stopServer] = useActions(APP_ACTIONS)
  const { t } = useTranslation()
  const { showToast } = useLogs()
  const initStartedRef = useRef(false)
  const [authSession, setAuthSession] = useState<AuthSessionStatus | null>(null)
  const [authSessionLoading, setAuthSessionLoading] = useState(false)
  const [authSessionError, setAuthSessionError] = useState<string | null>(null)

  useUpdater()

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
  const isHeadlessRuntime = isHeadlessHttpRuntime()
  const managementLocked = Boolean(
    isHeadlessRuntime &&
      authSession?.remoteRequest &&
      authSession.passwordConfigured &&
      !authSession.authenticated
  )
  const canInitialize =
    !isHeadlessRuntime ||
    Boolean(
      authSession &&
        (!authSession.remoteRequest || !authSession.passwordConfigured || authSession.authenticated)
    )
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
    status,
    "authSession:",
    authSession
  )

  const loadAuthSession = useCallback(async () => {
    if (!isHeadlessRuntime) {
      setAuthSession({
        authenticated: true,
        remoteRequest: false,
        passwordConfigured: false,
      })
      setAuthSessionError(null)
      return
    }

    try {
      setAuthSessionLoading(true)
      const session = await bridge.getAuthSession()
      setAuthSession(session)
      setAuthSessionError(null)
      emitAuthSessionChanged(session)
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error)
      setAuthSessionError(message || translate("auth.sessionLoadFailed"))
    } finally {
      setAuthSessionLoading(false)
    }
  }, [isHeadlessRuntime, translate])

  const handleRemoteAdminLogin = useCallback(async (password: string) => {
    const session = await bridge.loginRemoteAdmin(password)
    initStartedRef.current = false
    setAuthSession(session)
    setAuthSessionError(null)
    emitAuthSessionChanged(session)
  }, [])

  useEffect(() => {
    return subscribeAuthSessionChanged(session => {
      setAuthSession(session)
      setAuthSessionError(null)
    })
  }, [])

  useEffect(() => {
    void loadAuthSession()
  }, [loadAuthSession])

  useEffect(() => {
    if (initStartedRef.current || !canInitialize) {
      return
    }
    initStartedRef.current = true
    console.log("[App] useEffect running, calling init()...")
    void init()
  }, [canInitialize, init])

  console.log("[App] About to render layout")

  if (authSessionLoading && !authSession) {
    return (
      <div className="loading-screen">
        <p>{translate("auth.sessionChecking")}</p>
      </div>
    )
  }

  if (authSessionError && !authSession) {
    return (
      <div className="error-screen">
        <p>{translate("auth.sessionLoadFailed")}</p>
        <p>{authSessionError}</p>
      </div>
    )
  }

  if (managementLocked) {
    return <RemoteManagementLogin onSubmit={handleRemoteAdminLogin} />
  }

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
      showServerControlButton={!isHeadlessRuntime}
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
