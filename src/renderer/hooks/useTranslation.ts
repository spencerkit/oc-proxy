/**
 * useTranslation Hook
 *
 * Custom hook for internationalization (i18n).
 * Integrates with i18next for translation functionality.
 */

import { useTranslation as useI18nTranslation } from 'react-i18next';
import { initI18n, changeLocale, type Locale, SUPPORTED_LOCALES, LOCALE_NAMES } from '../i18n';

/**
 * Translation function type
 */
export type TranslateFunction = (key: string, options?: Record<string, string | number>) => string;

/**
 * Initialize i18n with a specific locale
 * This should be called once during app initialization
 *
 * @param locale - The locale to initialize with
 * @returns Promise that resolves when i18n is initialized
 */
export function initializeI18n(locale?: Locale): Promise<void> {
  return initI18n(locale);
}

/**
 * Hook for accessing translation function
 * Provides the t function and locale management for component translations
 *
 * @returns Object containing translation function and locale utilities
 *
 * @example
 * function MyComponent() {
 *   const { t, locale, changeLocale, getAvailableLocales, getLocaleName } = useTranslation();
 *
 *   return (
 *     <div>
 *       <h1>{t('app.title')}</h1>
 *       <p>{t('service.statusText', { running: 'Running', host: 'localhost', port: 8080, requests: 42, errors: 0, latency: 100 })}</p>
 *     </div>
 *   );
 * }
 */
export function useTranslation() {
  const { t, i18n } = useI18nTranslation();

  return {
    t,
    locale: i18n.language as Locale,
    changeLocale,
    getAvailableLocales: () => SUPPORTED_LOCALES,
    getLocaleName: (locale: Locale) => LOCALE_NAMES[locale] || locale,
  };
}

/**
 * Hook for accessing only the translation function
 * Useful when you only need to translate strings
 *
 * @returns Translation function
 *
 * @example
 * function Button() {
 *   const t = useT();
 *   return <button>{t('common.confirm')}</button>;
 * }
 */
export function useT(): TranslateFunction {
  const { t } = useI18nTranslation();
  return t;
}
