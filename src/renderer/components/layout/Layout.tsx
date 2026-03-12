import type React from "react"
import { useLocation } from "react-router-dom"
import { Header, type HeaderProps, type HeaderView } from "./Header"
import styles from "./Layout.module.css"

export interface LayoutProps {
  /**
   * Header props
   */
  header?: Omit<HeaderProps, "onViewChange"> & {
    onViewChange?: (view: HeaderView) => void
  }

  /**
   * Main content
   */
  children: React.ReactNode

  /**
   * Footer content
   */
  footer?: React.ReactNode

  /**
   * Whether to show centered content
   */
  centered?: boolean

  /**
   * Whether content should take full height
   */
  fullHeight?: boolean

  /**
   * Loading state
   */
  loading?: boolean

  /**
   * Error state
   */
  error?: string

  /**
   * Test ID for testing
   */
  testId?: string

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
   * Current server transition state
   */
  serverAction?: "starting" | "stopping" | null

  /**
   * Whether to show start/stop server control button.
   */
  showServerControlButton?: boolean
}

/**
 * Main application layout component
 */
export const Layout: React.FC<LayoutProps> = ({
  header,
  children,
  footer,
  centered = false,
  fullHeight = false,
  loading = false,
  error,
  testId,
  isRunning,
  serverAddress,
  onStartServer,
  onStopServer,
  serverAction,
  showServerControlButton = true,
}) => {
  const location = useLocation()

  // Determine current view from location
  const getCurrentView = (): HeaderView => {
    const path = location.pathname
    if (path === "/settings") return "settings"
    if (path.startsWith("/logs")) return "logs"
    if (path.startsWith("/agents")) return "agents"
    if (path.startsWith("/providers")) return "providers"
    return "service"
  }

  const currentView = header?.view ?? getCurrentView()

  const contentClasses = [
    styles.content,
    centered && styles.centered,
    fullHeight && styles.fullHeight,
  ]
    .filter(Boolean)
    .join(" ")

  return (
    <div className={styles.layout} data-testid={testId}>
      <Header
        {...header}
        view={currentView}
        isRunning={isRunning}
        serverAddress={serverAddress}
        serverAction={serverAction}
        showServerControlButton={showServerControlButton}
        onStartServer={onStartServer}
        onStopServer={onStopServer}
      />

      <main className={styles.main}>
        {loading ? (
          <div className={styles.loading}>
            <span>Loading...</span>
          </div>
        ) : error ? (
          <div className={styles.error}>
            <span className={styles.errorIcon}>⚠</span>
            <h2 className={styles.errorTitle}>Error</h2>
            <p className={styles.errorMessage}>{error}</p>
          </div>
        ) : (
          <div className={contentClasses}>{children}</div>
        )}
      </main>

      {footer && !loading && !error && <footer className={styles.footer}>{footer}</footer>}
    </div>
  )
}

/**
 * Empty state component
 */
export interface EmptyStateProps {
  /**
   * Icon to display
   */
  icon?: React.ReactNode

  /**
   * Title text
   */
  title?: string

  /**
   * Message/description text
   */
  message?: string

  /**
   * Actions to display
   */
  actions?: React.ReactNode

  /**
   * Test ID for testing
   */
  testId?: string
}

export const EmptyState: React.FC<EmptyStateProps> = ({
  icon,
  title,
  message,
  actions,
  testId,
}) => {
  return (
    <div className={styles.empty} data-testid={testId}>
      {icon && <span className={styles.emptyIcon}>{icon}</span>}
      {title && <h2 className={styles.emptyTitle}>{title}</h2>}
      {message && <p className={styles.emptyMessage}>{message}</p>}
      {actions}
    </div>
  )
}

/**
 * Error state component
 */
export interface ErrorStateProps {
  /**
   * Error message
   */
  message?: string

  /**
   * Title text
   */
  title?: string

  /**
   * Actions to display
   */
  actions?: React.ReactNode

  /**
   * Test ID for testing
   */
  testId?: string
}

export const ErrorState: React.FC<ErrorStateProps> = ({
  message,
  title = "Error",
  actions,
  testId,
}) => {
  return (
    <div className={styles.error} data-testid={testId}>
      <span className={styles.errorIcon}>⚠</span>
      <h2 className={styles.errorTitle}>{title}</h2>
      <p className={styles.errorMessage}>{message}</p>
      {actions}
    </div>
  )
}

/**
 * Loading state component
 */
export interface LoadingStateProps {
  /**
   * Loading message
   */
  message?: string

  /**
   * Test ID for testing
   */
  testId?: string
}

export const LoadingState: React.FC<LoadingStateProps> = ({ message = "Loading...", testId }) => {
  return (
    <div className={styles.loading} data-testid={testId}>
      <span>{message}</span>
    </div>
  )
}

export default Layout
