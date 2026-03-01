import React, { useState, useEffect } from 'react';
import { useProxyStore } from '@/store';
import { Button, Input, Switch } from '@/components';
import { useTranslation, useLogs } from '@/hooks';
import type { ServerConfig, CompatConfig, ProxyConfig } from '@/types';
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
  const [port, setPort] = useState(8080);
  const [strictMode, setStrictMode] = useState(false);
  const [portError, setPortError] = useState('');

  // Load initial values from config
  useEffect(() => {
    if (config) {
      setHost(config.server.host);
      setPort(config.server.port);
      setStrictMode(config.compat.strictMode);
    }
  }, [config]);

  const validatePort = (value: number): boolean => {
    if (!Number.isInteger(value) || value < 1 || value > 65535) {
      setPortError(t('settings.portError'));
      return false;
    }
    setPortError('');
    return true;
  };

  const handlePortChange = (value: string) => {
    const numPort = parseInt(value, 10);
    setPort(numPort);
    if (!isNaN(numPort)) {
      validatePort(numPort);
    }
  };

  const handleSave = async () => {
    if (!validatePort(port)) return;

    const newServerConfig: ServerConfig = {
      host,
      port,
      authEnabled: config?.server.authEnabled ?? false,
      localBearerToken: config?.server.localBearerToken ?? '',
    };

    const newCompatConfig: CompatConfig = {
      strictMode,
    };

    if (!config) return;

    const newConfig: ProxyConfig = {
      ...config,
      server: newServerConfig,
      compat: newCompatConfig,
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
      <h2>{t('settings.title')}</h2>

      <div className={styles.form}>
        <div className={styles.formGroup}>
          <label htmlFor="host">{t('settings.listenHost')}</label>
          <Input
            id="host"
            value={host}
            onChange={(e) => setHost(e.target.value)}
            placeholder="0.0.0.0"
          />
        </div>

        <div className={styles.formGroup}>
          <label htmlFor="port">{t('settings.servicePort')}</label>
          <Input
            id="port"
            type="number"
            value={port}
            onChange={(e) => handlePortChange(e.target.value)}
            placeholder="8080"
            min={1}
            max={65535}
          />
          {portError && <p className={styles.error}>{portError}</p>}
        </div>

        <div className={styles.formGroupSwitch}>
          <div className={styles.switchLabel}>
            <label htmlFor="strictMode">{t('settings.strictMode')}</label>
          </div>
          <Switch
            id="strictMode"
            checked={strictMode}
            onChange={setStrictMode}
          />
        </div>

        <div className={styles.actions}>
          <Button
            variant="primary"
            onClick={handleSave}
            loading={loading}
            disabled={!!portError}
          >
            {t('settings.save')}
          </Button>
        </div>
      </div>
    </div>
  );
};

export default SettingsPage;
