/**
 * OA Proxy Type Definitions
 *
 * This file exports all type definitions for the OA Proxy application.
 * These types are used across the renderer process to ensure type safety.
 */

// Config types
export type {
  ServerConfig,
  CompatConfig,
  LoggingConfig,
  UIConfig,
  ThemeMode,
  LocaleCode,
} from './config';

// Proxy types
export type {
  Rule,
  Group,
  RuleDirection,
  ProxyStatus,
  ProxyMetrics,
  LogEntry,
  LogEntryError,
  LogEntryStatus,
  LogEntryPhase,
} from './proxy';

import type { ServerConfig, CompatConfig, LoggingConfig, UIConfig } from './config';
import type { Group } from './proxy';

/**
 * Complete proxy configuration interface
 * Combines server, compat, logging, and groups configuration
 */
export interface ProxyConfig {
  server: ServerConfig;
  compat: CompatConfig;
  logging: LoggingConfig;
  ui: UIConfig;
  groups: Group[];
}

/**
 * Result from saving configuration
 */
export interface SaveConfigResult {
  ok: boolean;
  config: ProxyConfig;
  restarted: boolean;
  status: ProxyStatus;
}

import type { ProxyStatus } from './proxy';
