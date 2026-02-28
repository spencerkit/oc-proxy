/**
 * Hooks Module Exports
 *
 * Exports all custom hooks for the OA Proxy application.
 */

// Proxy configuration hooks
export {
  useProxyConfig,
  useConfigValue,
  useSaveConfigAction,
} from './useProxyConfig';

// Proxy status hooks
export {
  useProxyStatus,
  useProxyStatusAutoRefresh,
  useRunningState,
  useStatusValue,
} from './useProxyStatus';

// Logs hooks
export {
  useLogs,
  useLogsAutoRefresh,
  useLogsValue,
  useFilteredLogs,
  useLogCount,
} from './useLogs';

// Translation hooks
export {
  useTranslation,
  useT,
  initI18n,
  setLocale,
  getLocale,
  getAvailableLocales,
  type TranslateFunction,
  type Locale,
} from './useTranslation';

// Theme hooks
export {
  useTheme,
  useThemeValue,
  useIsDarkMode,
  useSystemThemeListener,
  setTheme,
  getTheme,
  getEffectiveTheme,
  applyThemeToDocument,
  type Theme,
} from './useTheme';
