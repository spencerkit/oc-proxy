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
  Provider,
  ProviderModelTestResult,
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
  providers?: Group["providers"]
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
  totalDurationMs: number
  totalCost: number
  inputTps: number
  outputTps: number
}

export type StatsDimension = "rule" | "protocol" | "status"

export interface ComparisonSummary {
  requestsDeltaPct: number
  errorsDeltaPct: number
  totalCostDeltaPct: number
  inputTpsDeltaPct: number
  outputTpsDeltaPct: number
}

export interface StatsCountBreakdownItem {
  key: string
  count: number
  ratio: number
}

export interface StatsTokenBreakdownItem {
  key: string
  tokens: number
  ratio: number
}

export interface StatsRuleCountBreakdownItem {
  key: string
  label: string
  count: number
  ratio: number
}

export interface StatsRuleTokenBreakdownItem {
  key: string
  label: string
  tokens: number
  ratio: number
}

export interface StatsBreakdowns {
  errorsByStatus: StatsCountBreakdownItem[]
  requestsByProtocol: StatsCountBreakdownItem[]
  tokensByProtocol: StatsTokenBreakdownItem[]
  requestsByRule: StatsRuleCountBreakdownItem[]
  tokensByRule: StatsRuleTokenBreakdownItem[]
}

export interface StatsSummaryResult {
  dimension: StatsDimension
  hours: number
  ruleKey?: string | null
  ruleKeys?: string[] | null
  requests: number
  errors: number
  inputTokens: number
  outputTokens: number
  cacheReadTokens: number
  cacheWriteTokens: number
  totalCost: number
  costCurrency?: string | null
  inputTps: number
  outputTps: number
  peakInputTps: number
  peakOutputTps: number
  comparison?: ComparisonSummary | null
  breakdowns?: StatsBreakdowns | null
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
  cacheReadTokens: number
  cacheWriteTokens: number
  tokens: number
  totalCost: number
  hourly: RuleCardHourlyPoint[]
}

export interface ClipboardTextResult {
  text: string
}

export interface AppInfo {
  name: string
  version: string
}

export type IntegrationClientKind = "claude" | "codex" | "opencode"

export interface AgentConfig {
  url?: string
  apiToken?: string
  model?: string
  timeout?: number
  alwaysThinkingEnabled?: boolean
  includeCoAuthoredBy?: boolean
  skipDangerousModePermissionPrompt?: boolean
}

export interface IntegrationTarget {
  id: string
  kind: IntegrationClientKind
  configDir: string
  config?: AgentConfig
  createdAt: string
  updatedAt: string
}

export interface AgentConfigFile {
  targetId: string
  kind: IntegrationClientKind
  configDir: string
  filePath: string
  content: string
  sourceFiles: AgentSourceFile[]
  updatedAt?: string
  parsedConfig?: AgentConfig
}

export interface AgentSourceFile {
  sourceId: string
  label: string
  filePath: string
  content: string
}

export interface WriteAgentConfigResult {
  ok: boolean
  targetId: string
  filePath: string
  message?: string
}

export interface IntegrationWriteItem {
  targetId: string
  kind?: IntegrationClientKind | null
  configDir: string
  filePath?: string | null
  ok: boolean
  message?: string | null
}

export interface IntegrationWriteResult {
  ok: boolean
  groupId: string
  entryUrl: string
  succeeded: number
  failed: number
  items: IntegrationWriteItem[]
}
