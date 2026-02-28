/**
 * useTranslation Hook
 *
 * Custom hook for internationalization (i18n).
 * Provides t function from i18next.
 *
 * This hook integrates with the i18next library to provide
 * translation functionality throughout the application.
 */

import { useCallback } from 'react';

/**
 * Translation function type
 */
export type TranslateFunction = (key: string, params?: Record<string, string | number>) => string;

/**
 * Current locale type
 */
export type Locale = 'en' | 'zh' | 'ja' | 'ko';

/**
 * i18next instance (to be initialized)
 */
let i18n: any = null;
let currentLocale: Locale = 'en';

/**
 * Basic i18n setup
 * This is a simplified implementation that can be expanded
 * with full i18next integration when needed
 */
const translations: Record<Locale, Record<string, string>> = {
  en: {
    // Server status
    'status.running': 'Running',
    'status.stopped': 'Stopped',
    'status.start': 'Start',
    'status.stop': 'Stop',
    'status.port': 'Port',
    'status.host': 'Host',
    'status.requests': 'Requests',
    'status.errors': 'Errors',
    'status.uptime': 'Uptime',

    // Configuration
    'config.title': 'Configuration',
    'config.save': 'Save',
    'config.reset': 'Reset',
    'config.groups': 'Groups',
    'config.addGroup': 'Add Group',
    'config.removeGroup': 'Remove Group',

    // Logs
    'logs.title': 'Logs',
    'logs.clear': 'Clear',
    'logs.autoRefresh': 'Auto Refresh',
    'logs.filter': 'Filter',

    // Common
    'common.loading': 'Loading...',
    'common.error': 'Error',
    'common.success': 'Success',
    'common.cancel': 'Cancel',
    'common.confirm': 'Confirm',
  },
  zh: {
    'status.running': '运行中',
    'status.stopped': '已停止',
    'status.start': '启动',
    'status.stop': '停止',
    'status.port': '端口',
    'status.host': '主机',
    'status.requests': '请求数',
    'status.errors': '错误数',
    'status.uptime': '运行时间',

    'config.title': '配置',
    'config.save': '保存',
    'config.reset': '重置',
    'config.groups': '分组',
    'config.addGroup': '添加分组',
    'config.removeGroup': '删除分组',

    'logs.title': '日志',
    'logs.clear': '清空',
    'logs.autoRefresh': '自动刷新',
    'logs.filter': '过滤',

    'common.loading': '加载中...',
    'common.error': '错误',
    'common.success': '成功',
    'common.cancel': '取消',
    'common.confirm': '确认',
  },
  ja: {
    'status.running': '実行中',
    'status.stopped': '停止中',
    'status.start': '開始',
    'status.stop': '停止',
    'status.port': 'ポート',
    'status.host': 'ホスト',
    'status.requests': 'リクエスト数',
    'status.errors': 'エラー数',
    'status.uptime': '稼働時間',

    'config.title': '設定',
    'config.save': '保存',
    'config.reset': 'リセット',
    'config.groups': 'グループ',
    'config.addGroup': 'グループを追加',
    'config.removeGroup': 'グループを削除',

    'logs.title': 'ログ',
    'logs.clear': 'クリア',
    'logs.autoRefresh': '自動更新',
    'logs.filter': 'フィルター',

    'common.loading': '読み込み中...',
    'common.error': 'エラー',
    'common.success': '成功',
    'common.cancel': 'キャンセル',
    'common.confirm': '確認',
  },
  ko: {
    'status.running': '실행 중',
    'status.stopped': '중지됨',
    'status.start': '시작',
    'status.stop': '중지',
    'status.port': '포트',
    'status.host': '호스트',
    'status.requests': '요청 수',
    'status.errors': '오류 수',
    'status.uptime': '가동 시간',

    'config.title': '설정',
    'config.save': '저장',
    'config.reset': '재설정',
    'config.groups': '그룹',
    'config.addGroup': '그룹 추가',
    'config.removeGroup': '그룹 삭제',

    'logs.title': '로그',
    'logs.clear': '지우기',
    'logs.autoRefresh': '자동 새로고침',
    'logs.filter': '필터',

    'common.loading': '로딩 중...',
    'common.error': '오류',
    'common.success': '성공',
    'common.cancel': '취소',
    'common.confirm': '확인',
  },
};

/**
 * Initialize i18n with a specific locale
 * This should be called once during app initialization
 *
 * @param locale - The locale to initialize with
 */
export function initI18n(locale: Locale = 'en'): void {
  currentLocale = locale;
  // In a full implementation, this would initialize i18next
  // i18n = await initI18next({ locale });
}

/**
 * Set the current locale
 *
 * @param locale - The locale to set
 */
export function setLocale(locale: Locale): void {
  currentLocale = locale;
  // In a full implementation: i18n.changeLanguage(locale);
}

/**
 * Get the current locale
 *
 * @returns The current locale
 */
export function getLocale(): Locale {
  return currentLocale;
}

/**
 * Get available locales
 *
 * @returns Array of available locales
 */
export function getAvailableLocales(): Locale[] {
  return ['en', 'zh', 'ja', 'ko'];
}

/**
 * Translation function
 * Translates a key using the current locale
 *
 * @param key - The translation key
 * @param params - Optional parameters to interpolate into the translation
 * @returns The translated string
 *
 * @example
 * t('status.running') // 'Running' (in English)
 * t('config.save') // '保存' (in Chinese)
 * t('common.loading') // '読み込み中...' (in Japanese)
 */
export function t(key: string, params?: Record<string, string | number>): string {
  const localeTranslations = translations[currentLocale] || translations.en;
  let translation = localeTranslations[key] || key;

  // Interpolate parameters if provided
  if (params) {
    Object.entries(params).forEach(([paramKey, value]) => {
      translation = translation.replace(`{{${paramKey}}}`, String(value));
    });
  }

  return translation;
}

/**
 * Hook for accessing translation function
 * Provides the t function for component translations
 *
 * @returns Translation function and current locale
 *
 * @example
 * function MyComponent() {
 *   const { t, locale, setLocale } = useTranslation();
 *
 *   return (
 *     <div>
 *       <h1>{t('status.title')}</h1>
 *       <p>{t('status.requests', { count: 42 })}</p>
 *     </div>
 *   );
 * }
 */
export function useTranslation() {
  return {
    t,
    locale: currentLocale,
    setLocale,
    getAvailableLocales,
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
  return t;
}
