/**
 * Server configuration interface
 */
export interface ServerConfig {
  host: string
  port: number
  authEnabled: boolean
  localBearerToken: string
}

/**
 * Compatibility configuration interface
 */
export interface CompatConfig {
  strictMode: boolean
  textToolCallFallbackEnabled?: boolean
}

/**
 * Logging configuration interface
 */
export interface LoggingConfig {
  // Controls whether request/response bodies are captured in logs.
  captureBody: boolean
}

/**
 * UI configuration interface (reserved for future UI-specific settings)
 */
export type ThemeMode = "light" | "dark"
export type LocaleCode = "en-US" | "zh-CN"
export type LocaleMode = "auto" | "manual"

export interface UIConfig {
  theme: ThemeMode
  locale: LocaleCode
  localeMode: LocaleMode
  launchOnStartup: boolean
  autoStartServer: boolean
  closeToTray: boolean
  quotaAutoRefreshMinutes: number
}

export interface RemoteGitConfig {
  enabled: boolean
  repoUrl: string
  token: string
  branch: string
}
