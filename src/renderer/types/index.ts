/**
 * AI Open Router Type Definitions
 *
 * This file exports all type definitions for the AI Open Router application.
 * These types are used across the renderer process to ensure type safety.
 */

// Config types
export type {
  CompatConfig,
  LocaleCode,
  LocaleMode,
  LoggingConfig,
  ServerConfig,
  ThemeMode,
  UIConfig,
} from "./config"

// Proxy types
export type {
  Group,
  LogEntry,
  LogEntryError,
  LogEntryPhase,
  LogEntryStatus,
  ProxyMetrics,
  ProxyStatus,
  Rule,
  RuleDirection,
  RuleProtocol,
  TokenUsage,
} from "./proxy"

import type { CompatConfig, LoggingConfig, ServerConfig, UIConfig } from "./config"
import type { Group, ProxyStatus } from "./proxy"

/**
 * Complete proxy configuration interface
 * Combines server, compat, logging, and groups configuration
 */
export interface ProxyConfig {
  server: ServerConfig
  compat: CompatConfig
  logging: LoggingConfig
  ui: UIConfig
  groups: Group[]
}

/**
 * Result from saving configuration
 */
export interface SaveConfigResult {
  ok: boolean
  config: ProxyConfig
  restarted: boolean
  status: ProxyStatus
}

export interface GroupBackupExportResult {
  ok: boolean
  canceled: boolean
  source?: "file" | "folder" | "clipboard"
  filePath?: string | null
  groupCount: number
  charCount?: number
}

export interface GroupBackupImportResult {
  ok: boolean
  canceled: boolean
  source?: "file" | "json"
  filePath?: string
  importedGroupCount?: number
  config?: ProxyConfig
  restarted?: boolean
  status?: ProxyStatus
}

export interface ClipboardTextResult {
  text: string
}

export interface AppInfo {
  name: string
  version: string
}
