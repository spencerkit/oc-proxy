/**
 * Hooks Module Exports
 *
 * Exports all custom hooks for the AI Open Router application.
 */

// Logs hooks
export {
  useFilteredLogs,
  useLogCount,
  useLogs,
  useLogsAutoRefresh,
  useLogsValue,
} from "./useLogs"
// Proxy configuration hooks
export {
  useConfigValue,
  useProxyConfig,
  useSaveConfigAction,
} from "./useProxyConfig"
// Proxy status hooks
export {
  useProxyStatus,
  useProxyStatusAutoRefresh,
  useRunningState,
  useStatusValue,
} from "./useProxyStatus"
// Theme hooks
export {
  applyThemeToDocument,
  getEffectiveTheme,
  getTheme,
  setTheme,
  type Theme,
  useIsDarkMode,
  useSystemThemeListener,
  useTheme,
  useThemeValue,
} from "./useTheme"
// Translation hooks
export {
  initializeI18n,
  type TranslateFunction,
  useT,
  useTranslation,
} from "./useTranslation"
// Updater hook
export { useUpdater } from "./useUpdater"
