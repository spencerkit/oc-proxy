/**
 * Server configuration interface
 */
export interface ServerConfig {
  host: string;
  port: number;
  authEnabled: boolean;
  localBearerToken: string;
}

/**
 * Compatibility configuration interface
 */
export interface CompatConfig {
  strictMode: boolean;
}

/**
 * Logging configuration interface
 */
export interface LoggingConfig {
  level: string;
  captureBody: boolean;
  redactRules: string[];
}

/**
 * UI configuration interface (reserved for future UI-specific settings)
 */
export type ThemeMode = 'light' | 'dark';
export type LocaleCode = 'en-US' | 'zh-CN';

export interface UIConfig {
  theme: ThemeMode;
  locale: LocaleCode;
  launchOnStartup: boolean;
}
