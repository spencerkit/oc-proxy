import React from "react"
import ReactDOM from "react-dom/client"
import { I18nextProvider } from "react-i18next"
import { BrowserRouter, HashRouter } from "react-router-dom"
import App from "./App"
import { ToastContainer } from "./components/common/ToastContainer"
import { ToastProvider } from "./contexts/ToastContext"
import i18n, { initI18n, type Locale } from "./i18n"
import { ipc } from "./utils/ipc"
import { resolveEffectiveLocale } from "./utils/locale"
import "./styles.css"

/** Reports renderer telemetry safely without breaking boot flow. */
function reportRendererError(payload: {
  kind: string
  message: string
  stack?: string
  source?: string
}) {
  try {
    ipc.reportRendererError(payload).catch(error => {
      console.warn("[Main] reportRendererError rejected:", error)
    })
  } catch (error) {
    console.warn("[Main] reportRendererError threw:", error)
  }
}

/** Reports renderer ready state safely. */
function reportRendererReady() {
  try {
    ipc.reportRendererReady().catch(error => {
      console.warn("[Main] reportRendererReady rejected:", error)
    })
  } catch (error) {
    console.warn("[Main] reportRendererReady threw:", error)
  }
}

// 添加全局错误处理
window.onerror = (message, source, lineno, colno, error) => {
  console.error("Global error:", { message, source, lineno, colno, error })
  reportRendererError({
    kind: "window.onerror",
    message: String(message),
    source: source ? `${source}:${lineno}:${colno}` : undefined,
    stack: error instanceof Error ? error.stack : String(error ?? ""),
  })
  return false
}

window.onunhandledrejection = event => {
  console.error("Unhandled promise rejection:", event.reason)
  reportRendererError({
    kind: "unhandledrejection",
    message: String(event.reason ?? "unknown promise rejection"),
    stack: event.reason instanceof Error ? event.reason.stack : undefined,
  })
}

console.log("Renderer starting...")

/** Resolves theme. */
function resolveTheme(theme?: unknown): "light" | "dark" {
  if (theme === "light" || theme === "dark") {
    return theme
  }

  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light"
}

/** Resolves router. */
function resolveRouter() {
  if (window.__TAURI__) {
    return HashRouter
  }
  return window.location.protocol === "file:" ? HashRouter : BrowserRouter
}

// Initialize i18n before rendering
async function init() {
  let initialLocale: Locale = "en-US"
  let initialTheme: "light" | "dark" = resolveTheme()

  try {
    const config = await ipc.getConfig()
    initialLocale = resolveEffectiveLocale({
      locale: config?.ui?.locale,
      localeMode: config?.ui?.localeMode,
      systemLanguage: navigator.language,
    }) as Locale
    initialTheme = resolveTheme(config?.ui?.theme)
  } catch (error) {
    console.error("[Main] Failed to load config for bootstrap preferences:", error)
  }

  document.documentElement.setAttribute("data-theme", initialTheme)

  console.log("[Main] Initializing i18n...")
  await initI18n(initialLocale)
  console.log("[Main] i18n initialized")

  const rootElement = document.getElementById("root")
  if (!rootElement) {
    console.error("Root element not found!")
    throw new Error("Root element not found")
  }

  console.log("[Main] Root element found, rendering...")
  const Router = resolveRouter()

  ReactDOM.createRoot(rootElement).render(
    <React.StrictMode>
      <I18nextProvider i18n={i18n}>
        <ToastProvider>
          <Router>
            <App />
            <ToastContainer />
          </Router>
        </ToastProvider>
      </I18nextProvider>
    </React.StrictMode>
  )

  console.log("[Main] React rendered")
  reportRendererReady()
}

init().catch(error => {
  console.error(error)
  reportRendererError({
    kind: "bootstrap",
    message: error instanceof Error ? error.message : String(error),
    stack: error instanceof Error ? error.stack : undefined,
  })
})
