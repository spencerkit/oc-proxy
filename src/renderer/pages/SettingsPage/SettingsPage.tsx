import React, { useEffect, useState } from 'react';
import { useProxyStore } from '@/store';
import { Button, Input, Switch } from '@/components';
import { useTranslation, useLogs } from '@/hooks';
import type { CompatConfig, LocaleCode, ProxyConfig, ServerConfig, ThemeMode, UIConfig } from '@/types';
import styles from './SettingsPage.module.css';

/**
 * SettingsPage Component
 * Service settings configuration page
 */
export const SettingsPage: React.FC = () => {
  const { t } = useTranslation();
  const { config, saveConfig, loading } = useProxyStore();
  const { showToast } = useLogs();

  const [host, setHost] = useState('');
  const [portText, setPortText] = useState('8080');
  const [strictMode, setStrictMode] = useState(false);
  const [launchOnStartup, setLaunchOnStartup] = useState(false);
  const [theme, setTheme] = useState<ThemeMode>('light');
  const [locale, setLocale] = useState<LocaleCode>('en-US');
  const [portError, setPortError] = useState('');

  // Load initial values from config
  useEffect(() => {
    if (config) {
      setHost(config.server.host);
      setPortText(String(config.server.port));
      setStrictMode(config.compat.strictMode);
      setLaunchOnStartup(config.ui.launchOnStartup);
      setTheme(config.ui.theme);
      setLocale(config.ui.locale);
    }
  }, [config]);

  const validatePort = (value: string): boolean => {
    if (!/^\d+$/.test(value)) {
      setPortError(t('settings.portError'));
      return false;
    }

    const parsed = Number(value);
    if (!Number.isInteger(parsed) || parsed < 1 || parsed > 65535) {
      setPortError(t('settings.portError'));
      return false;
    }

    setPortError('');
    return true;
  };

  const handlePortChange = (value: string) => {
    setPortText(value);
    validatePort(value);
  };

  const focusInput = (id: string) => {
    const input = document.getElementById(id) as HTMLInputElement | null;
    input?.focus();
  };

  const parsedPort = /^\d+$/.test(portText) ? Number(portText) : NaN;
  const normalizedHost = host.trim();
  const previewHost = normalizedHost || '0.0.0.0';
  const previewPort = Number.isInteger(parsedPort) ? parsedPort : '----';

  const hasChanges = Boolean(
    config
    && (
      host !== config.server.host
      || String(config.server.port) !== portText
      || strictMode !== config.compat.strictMode
      || launchOnStartup !== config.ui.launchOnStartup
      || theme !== config.ui.theme
      || locale !== config.ui.locale
    )
  );

  const canSave = !loading && hasChanges && !portError && Number.isInteger(parsedPort);

  const handleSave = async () => {
    if (!config) return;
    if (!validatePort(portText)) {
      focusInput('port');
      return;
    }

    const port = Number(portText);
    const newServerConfig: ServerConfig = {
      host: normalizedHost || config.server.host,
      port,
      authEnabled: config.server.authEnabled ?? false,
      localBearerToken: config.server.localBearerToken ?? '',
    };

    const newCompatConfig: CompatConfig = {
      strictMode,
    };

    const newUIConfig: UIConfig = {
      launchOnStartup,
      theme,
      locale,
    };

    const newConfig: ProxyConfig = {
      ...config,
      server: newServerConfig,
      compat: newCompatConfig,
      ui: newUIConfig,
    };

    try {
      await saveConfig(newConfig);
      showToast(t('settings.saveSuccess'), 'success');
    } catch (error) {
      showToast(t('errors.saveFailed', { message: String(error) }), 'error');
    }
  };

  return (
    <div className={styles.settingsPage}>
      <div className={styles.header}>
        <h2>{t('settings.title')}</h2>
        <p className={styles.subtitle}>{t('settings.subtitle')}</p>
      </div>

      <div className={styles.layout}>
        <div className={styles.form}>
          <div className={styles.section}>
            <h3 className={styles.sectionTitle}>{t('settings.networkSection')}</h3>

            <div className={styles.formGroup}>
              <label htmlFor="host">{t('settings.listenHost')}</label>
              <Input
                id="host"
                value={host}
                onChange={(e) => setHost(e.target.value)}
                placeholder="0.0.0.0"
                hint={t('settings.hostHint')}
              />
            </div>

            <div className={styles.formGroup}>
              <label htmlFor="port">{t('settings.servicePort')}</label>
              <Input
                id="port"
                type="number"
                value={portText}
                onChange={(e) => handlePortChange(e.target.value)}
                placeholder="8080"
                min={1}
                max={65535}
                hint={!portError ? t('settings.portHint') : undefined}
                error={portError || undefined}
              />
            </div>
          </div>

          <div className={styles.section}>
            <h3 className={styles.sectionTitle}>{t('settings.behaviorSection')}</h3>

            <div className={styles.formGroupSwitch}>
              <div className={styles.switchLabel}>
                <label htmlFor="strictMode">{t('settings.strictMode')}</label>
                <p>{t('settings.strictModeHint')}</p>
              </div>
              <Switch
                id="strictMode"
                checked={strictMode}
                onChange={setStrictMode}
              />
            </div>

            <div className={styles.formGroupSwitch}>
              <div className={styles.switchLabel}>
                <label htmlFor="launchOnStartup">{t('settings.launchOnStartup')}</label>
                <p>{t('settings.launchOnStartupHint')}</p>
              </div>
              <Switch
                id="launchOnStartup"
                checked={launchOnStartup}
                onChange={setLaunchOnStartup}
              />
            </div>
          </div>

          <div className={styles.section}>
            <h3 className={styles.sectionTitle}>{t('settings.interfaceSection')}</h3>

            <div className={styles.formGroup}>
              <label>{t('settings.theme')}</label>
              <div className={styles.choiceGroup} role="radiogroup" aria-label={t('settings.theme')}>
                <button
                  type="button"
                  role="radio"
                  aria-checked={theme === 'light'}
                  className={`${styles.choiceButton} ${theme === 'light' ? styles.choiceButtonActive : ''}`}
                  onClick={() => setTheme('light' as ThemeMode)}
                >
                  <span className={styles.choiceTitle}>{t('settings.themeLight')}</span>
                  <span className={styles.choiceValue}>LIGHT</span>
                </button>
                <button
                  type="button"
                  role="radio"
                  aria-checked={theme === 'dark'}
                  className={`${styles.choiceButton} ${theme === 'dark' ? styles.choiceButtonActive : ''}`}
                  onClick={() => setTheme('dark' as ThemeMode)}
                >
                  <span className={styles.choiceTitle}>{t('settings.themeDark')}</span>
                  <span className={styles.choiceValue}>DARK</span>
                </button>
              </div>
              <p className={styles.fieldHint}>{t('settings.themeHint')}</p>
            </div>

            <div className={styles.formGroup}>
              <label>{t('settings.language')}</label>
              <div className={styles.choiceGroup} role="radiogroup" aria-label={t('settings.language')}>
                <button
                  type="button"
                  role="radio"
                  aria-checked={locale === 'en-US'}
                  className={`${styles.choiceButton} ${locale === 'en-US' ? styles.choiceButtonActive : ''}`}
                  onClick={() => setLocale('en-US' as LocaleCode)}
                >
                  <span className={styles.choiceTitle}>{t('settings.languageEnglish')}</span>
                  <span className={styles.choiceValue}>EN-US</span>
                </button>
                <button
                  type="button"
                  role="radio"
                  aria-checked={locale === 'zh-CN'}
                  className={`${styles.choiceButton} ${locale === 'zh-CN' ? styles.choiceButtonActive : ''}`}
                  onClick={() => setLocale('zh-CN' as LocaleCode)}
                >
                  <span className={styles.choiceTitle}>{t('settings.languageChinese')}</span>
                  <span className={styles.choiceValue}>ZH-CN</span>
                </button>
              </div>
              <p className={styles.fieldHint}>{t('settings.languageHint')}</p>
            </div>
          </div>

          <div className={styles.actions}>
            <span className={styles.changeHint}>
              {hasChanges ? t('settings.unsavedChanges') : t('settings.noChanges')}
            </span>
            <Button
              variant="primary"
              onClick={handleSave}
              loading={loading}
              disabled={!canSave}
            >
              {t('settings.save')}
            </Button>
          </div>
        </div>

        <aside className={styles.previewCard}>
          <h3>{t('settings.previewTitle')}</h3>
          <div className={styles.previewRow}>
            <span>{t('settings.previewAddress')}</span>
            <code>{`http://${previewHost}:${previewPort}`}</code>
          </div>
          <div className={styles.previewRow}>
            <span>{t('settings.previewMode')}</span>
            <strong>
              {strictMode ? t('settings.modeStrict') : t('settings.modeCompatible')}
            </strong>
          </div>
          <div className={styles.previewRow}>
            <span>{t('settings.previewTheme')}</span>
            <strong>{theme === 'dark' ? t('settings.themeDark') : t('settings.themeLight')}</strong>
          </div>
          <div className={styles.previewRow}>
            <span>{t('settings.previewLanguage')}</span>
            <strong>{locale === 'zh-CN' ? t('settings.languageChinese') : t('settings.languageEnglish')}</strong>
          </div>
          <div className={styles.previewRow}>
            <span>{t('settings.previewStartup')}</span>
            <strong>{launchOnStartup ? t('settings.startupEnabled') : t('settings.startupDisabled')}</strong>
          </div>
        </aside>
      </div>
    </div>
  );
};

export default SettingsPage;
