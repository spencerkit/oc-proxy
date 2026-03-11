import {
  FileText,
  Globe,
  Layers,
  Moon,
  Server,
  Settings as SettingsIcon,
  Sun,
  Users,
} from "lucide-react"
import type React from "react"
import { useCallback, useEffect } from "react"
import { useTranslation } from "react-i18next"
import { useLocation, useNavigate } from "react-router-dom"
import { configState, saveConfigAction, savingConfigState } from "@/store"
import type { LocaleCode, ThemeMode } from "@/types"
import { resolveEffectiveLocale } from "@/utils/locale"
import { useActions, useRelaxValue } from "@/utils/relax"
import brandLogo from "../../../../assets/logo-lockup.svg"
import { Button } from "../common"
import styles from "./Header.module.css"

const HEADER_ACTIONS = [saveConfigAction] as const

export type HeaderView = "service" | "settings" | "logs" | "agents" | "providers"

export interface HeaderProps {
  /**
   * Current view
   */
  view?: HeaderView

  /**
   * Callback when view changes
   */
  onViewChange?: (view: HeaderView) => void

  /**
   * Whether to show service status indicator
   */
  showStatus?: boolean

  /**
   * Service running state
   */
  isRunning?: boolean

  /**
   * Server address to display
   */
  serverAddress?: string

  /**
   * Callback to start the server
   */
  onStartServer?: () => void

  /**
   * Callback to stop the server
   */
  onStopServer?: () => void

  /**
   * Whether to show start/stop server control button.
   */
  showServerControlButton?: boolean

  /**
   * Current server transition state
   */
  serverAction?: "starting" | "stopping" | null

  /**
   * Error count badge value
   */
  errorCount?: number

  /**
   * Additional actions to render in the header
   */
  actions?: React.ReactNode

  /**
   * Test ID for testing
   */
  testId?: string
}

/**
 * Header component with navigation and theme/language controls
 */
export const Header: React.FC<HeaderProps> = ({
  view,
  onViewChange,
  showStatus: _showStatus,
  isRunning,
  serverAddress,
  onStartServer,
  onStopServer,
  showServerControlButton = true,
  serverAction,
  errorCount,
  actions,
  testId,
}) => {
  const navigate = useNavigate()
  const location = useLocation()
  const { t, i18n } = useTranslation()
  const config = useRelaxValue(configState)
  const savingConfig = useRelaxValue(savingConfigState)
  const [saveConfig] = useActions(HEADER_ACTIONS)

  const supportedLocales: LocaleCode[] = ["en-US", "zh-CN"]
  const documentTheme = document.documentElement.getAttribute("data-theme")
  const theme: ThemeMode =
    config?.ui?.theme === "dark"
      ? "dark"
      : config?.ui?.theme === "light"
        ? "light"
        : documentTheme === "dark"
          ? "dark"
          : "light"
  const currentLocale: LocaleCode = resolveEffectiveLocale({
    locale: config?.ui?.locale,
    localeMode: config?.ui?.localeMode,
    systemLanguage: navigator.language,
  })

  // Determine current view from location
  const getCurrentView = (): HeaderView => {
    const path = location.pathname
    if (path === "/settings") return "settings"
    if (path.startsWith("/logs")) return "logs"
    if (path.startsWith("/agents")) return "agents"
    if (path.startsWith("/providers")) return "providers"
    return "service"
  }

  const currentView = view ?? getCurrentView()

  useEffect(() => {
    document.documentElement.setAttribute("data-theme", theme)
  }, [theme])

  useEffect(() => {
    if (i18n.language !== currentLocale) {
      i18n.changeLanguage(currentLocale)
    }
  }, [currentLocale, i18n])

  // Toggle theme
  const handleThemeToggle = useCallback(() => {
    if (!config) return

    const nextTheme: ThemeMode = theme === "light" ? "dark" : "light"
    saveConfig({
      ...config,
      ui: {
        ...config.ui,
        theme: nextTheme,
      },
    })
  }, [config, saveConfig, theme])

  // Change language
  const handleLanguageChange = useCallback(
    (locale: LocaleCode) => {
      if (!config) return

      i18n.changeLanguage(locale)
      saveConfig({
        ...config,
        ui: {
          ...config.ui,
          locale,
          localeMode: "manual",
        },
      })
    },
    [config, i18n, saveConfig]
  )

  // Handle view change - navigates to the appropriate route
  const handleViewChange = useCallback(
    (newView: HeaderView) => {
      if (onViewChange) {
        onViewChange(newView)
      } else {
        switch (newView) {
          case "service":
            navigate("/")
            break
          case "settings":
            navigate("/settings")
            break
          case "logs":
            navigate("/logs")
            break
          case "agents":
            navigate("/agents")
            break
          case "providers":
            navigate("/providers")
            break
        }
      }
    },
    [navigate, onViewChange]
  )

  return (
    <header className={styles.header} data-testid={testId}>
      {/* Left section */}
      <div className={styles.left}>
        <div className={styles.brand}>
          <img className={styles.brandLogo} src={brandLogo} alt={t("app.title")} />
        </div>
        {isRunning !== undefined && (
          <div className={styles.serviceStatus}>
            <span
              className={`${styles.statusDot} ${isRunning ? styles.running : styles.stopped}`}
            />
            <span className={styles.statusText}>
              {isRunning ? t("header.serviceRunning") : t("header.serviceStopped")}
            </span>
            {serverAddress && <span className={styles.serverAddress}>{serverAddress}</span>}
            {showServerControlButton && (
              <Button
                variant={isRunning ? "danger" : "primary"}
                size="small"
                onClick={isRunning ? onStopServer : onStartServer}
                loading={
                  (isRunning && serverAction === "stopping") ||
                  (!isRunning && serverAction === "starting")
                }
                disabled={serverAction !== null}
              >
                {isRunning ? t("header.stop") : t("header.start")}
              </Button>
            )}
          </div>
        )}
      </div>

      {/* Center section - Navigation */}
      <div className={styles.center}>
        <button
          type="button"
          className={`${styles.navButton} ${currentView === "service" ? styles.active : ""}`}
          onClick={() => handleViewChange("service")}
          aria-current={currentView === "service" ? "page" : undefined}
        >
          <Server size={16} strokeWidth={2} className={styles.navIcon} />
          <span className={styles.navLabel}>{t("header.serviceSwitch")}</span>
        </button>
        <div className={styles.divider} />
        <button
          type="button"
          className={`${styles.navButton} ${currentView === "providers" ? styles.active : ""}`}
          onClick={() => handleViewChange("providers")}
          aria-current={currentView === "providers" ? "page" : undefined}
        >
          <Layers size={16} strokeWidth={2} className={styles.navIcon} />
          <span className={styles.navLabel}>{t("header.providers")}</span>
        </button>
        <button
          type="button"
          className={`${styles.navButton} ${currentView === "agents" ? styles.active : ""}`}
          onClick={() => handleViewChange("agents")}
          aria-current={currentView === "agents" ? "page" : undefined}
        >
          <Users size={16} strokeWidth={2} className={styles.navIcon} />
          <span className={styles.navLabel}>{t("header.agents")}</span>
        </button>
        <button
          type="button"
          className={`${styles.navButton} ${currentView === "logs" ? styles.active : ""}`}
          onClick={() => handleViewChange("logs")}
          aria-current={currentView === "logs" ? "page" : undefined}
        >
          <FileText size={16} strokeWidth={2} className={styles.navIcon} />
          <span className={styles.navLabel}>{t("header.logs")}</span>
          {errorCount !== undefined && errorCount > 0 && (
            <span className={styles.badge}>{errorCount}</span>
          )}
        </button>
        <button
          type="button"
          className={`${styles.navButton} ${currentView === "settings" ? styles.active : ""}`}
          onClick={() => handleViewChange("settings")}
          aria-current={currentView === "settings" ? "page" : undefined}
        >
          <SettingsIcon size={16} strokeWidth={2} className={styles.navIcon} />
          <span className={styles.navLabel}>{t("header.settings")}</span>
        </button>
      </div>

      {/* Right section - Actions */}
      <div className={styles.right}>
        {/* Theme toggle */}
        <button
          type="button"
          className={styles.themeToggle}
          onClick={handleThemeToggle}
          disabled={!config || savingConfig}
          aria-label={theme === "light" ? "Switch to dark theme" : "Switch to light theme"}
          title={theme === "light" ? "Dark mode" : "Light mode"}
        >
          {theme === "light" ? (
            <Moon size={18} strokeWidth={2} />
          ) : (
            <Sun size={18} strokeWidth={2} />
          )}
        </button>

        {/* Language segmented control */}
        <div className={styles.languageSegment} data-testid={`${testId}-language-selector`}>
          <Globe size={14} strokeWidth={2} className={styles.languageIcon} />
          {supportedLocales.map(locale => (
            <button
              key={locale}
              type="button"
              className={`${styles.languageSegmentButton} ${currentLocale === locale ? styles.languageSegmentButtonActive : ""}`}
              onClick={() => handleLanguageChange(locale)}
              disabled={!config || savingConfig}
              aria-pressed={currentLocale === locale}
            >
              {locale === "en-US" ? "EN" : "中文"}
            </button>
          ))}
        </div>

        {/* Additional actions */}
        {actions}
      </div>
    </header>
  )
}

export default Header
