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
  RemoteGitConfig,
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
  QuotaStatus,
  QuotaUnitType,
  Rule,
  RuleDirection,
  RuleProtocol,
  RuleQuotaConfig,
  RuleQuotaSnapshot,
  RuleQuotaTestResult,
  TokenUsage,
} from "./proxy"

import type { CompatConfig, LoggingConfig, RemoteGitConfig, ServerConfig, UIConfig } from "./config"
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
  remoteGit: RemoteGitConfig
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
  source?: "file" | "json" | "remote"
  filePath?: string
  importedGroupCount?: number
  config?: ProxyConfig
  restarted?: boolean
  status?: ProxyStatus
}

export interface RemoteRulesUploadResult {
  ok: boolean
  changed: boolean
  branch: string
  filePath: string
  groupCount: number
  needsConfirmation: boolean
  warning?: string
  localUpdatedAt?: string
  remoteUpdatedAt?: string
}

export interface RemoteRulesPullResult {
  ok: boolean
  branch: string
  filePath: string
  importedGroupCount?: number
  config?: ProxyConfig
  restarted?: boolean
  status?: ProxyStatus
  needsConfirmation: boolean
  warning?: string
  localUpdatedAt?: string
  remoteUpdatedAt?: string
}

export interface StatsRuleOption {
  key: string
  label: string
  groupId: string
  ruleId: string
}

export interface HourlyStatsPoint {
  hour: string
  requests: number
  errors: number
  inputTokens: number
  outputTokens: number
  cacheReadTokens: number
  cacheWriteTokens: number
}

export interface StatsSummaryResult {
  hours: number
  ruleKey?: string | null
  ruleKeys?: string[] | null
  requests: number
  errors: number
  inputTokens: number
  outputTokens: number
  cacheReadTokens: number
  cacheWriteTokens: number
  rpm: number
  inputTpm: number
  outputTpm: number
  hourly: HourlyStatsPoint[]
  options: StatsRuleOption[]
}

export interface RuleCardHourlyPoint {
  hour: string
  requests: number
  inputTokens: number
  outputTokens: number
  tokens: number
}

export interface RuleCardStatsItem {
  groupId: string
  ruleId: string
  requests: number
  inputTokens: number
  outputTokens: number
  tokens: number
  hourly: RuleCardHourlyPoint[]
}

export interface ClipboardTextResult {
  text: string
}

export interface AppInfo {
  name: string
  version: string
}
