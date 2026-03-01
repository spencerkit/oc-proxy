/**
 * i18n Configuration for OA Proxy
 *
 * This module initializes i18next with react-i18next integration
 * and provides translation support for the application.
 */

import i18n from 'i18next';
import { initReactI18next } from 'react-i18next';
import { enUS } from './en-US';
import { zhCN } from './zh-CN';

// Type definition for locale
export type Locale = 'en-US' | 'zh-CN';

// Default locale
const DEFAULT_LOCALE: Locale = 'en-US';

/**
 * Translation resources object
 * Maps locale codes to translation content
 */
const resources = {
  'en-US': {
    translation: enUS,
  },
  'zh-CN': {
    translation: zhCN,
  },
};

/**
 * Supported locales list
 */
export const SUPPORTED_LOCALES: Locale[] = ['en-US', 'zh-CN'];

/**
 * Locale display names
 */
export const LOCALE_NAMES: Record<Locale, string> = {
  'en-US': 'English',
  'zh-CN': '简体中文',
};

/**
 * Initialize i18next
 * Should be called once during app initialization
 *
 * @param locale - Initial locale to use (defaults to en-US)
 * @returns Promise that resolves when i18n is initialized
 */
export async function initI18n(locale: Locale = DEFAULT_LOCALE): Promise<void> {
  await i18n.use(initReactI18next).init({
    resources,
    lng: locale,
    fallbackLng: DEFAULT_LOCALE,
    keySeparator: '.',
    nsSeparator: false,
    interpolation: {
      escapeValue: false, // React already escapes values
    },
    react: {
      useSuspense: false, // Disable suspense for SSR compatibility
    },
  });
}

/**
 * Get the current locale
 *
 * @returns Current locale code
 */
export function getCurrentLocale(): Locale {
  return i18n.language as Locale;
}

/**
 * Change the current locale
 *
 * @param locale - New locale to set
 * @returns Promise that resolves when locale is changed
 */
export async function changeLocale(locale: Locale): Promise<void> {
  await i18n.changeLanguage(locale);
}

/**
 * Get display name for a locale
 *
 * @param locale - Locale code
 * @returns Display name for the locale
 */
export function getLocaleName(locale: Locale): string {
  return LOCALE_NAMES[locale] || locale;
}

/**
 * Get a localized message using a translation key
 * This is a wrapper around i18n.t() for convenience
 *
 * @param key - Translation key (e.g., 'app.title')
 * @param options - i18n translation options
 * @returns Translated string
 */
export function t(key: string, options?: any): string {
  const result = i18n.t(key, options);
  return typeof result === 'string' ? result : String(result);
}

/**
 * Export the i18n instance for use in non-React components
 */
export { i18n };
export default i18n;

/**
 * Re-export translation resources for type inference
 */
export { enUS, zhCN };
