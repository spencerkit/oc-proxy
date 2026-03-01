import React, { useState, useEffect } from 'react';
import { useParams, useNavigate } from 'react-router-dom';
import { useProxyStore } from '@/store';
import { Button, Input } from '@/components';
import { useTranslation, useLogs } from '@/hooks';
import type { Rule, ProxyConfig } from '@/types';
import styles from './RuleEditPage.module.css';

/**
 * RuleEditPage Component
 * Page for editing an existing rule
 */
export const RuleEditPage: React.FC = () => {
  const { groupId, ruleId } = useParams<{ groupId: string; ruleId: string }>();
  const navigate = useNavigate();
  const { t } = useTranslation();
  const { config, saveConfig } = useProxyStore();
  const { showToast } = useLogs();

  const [model, setModel] = useState('');
  const [direction, setDirection] = useState<Rule['direction']>('oc');
  const [token, setToken] = useState('');
  const [apiAddress, setApiAddress] = useState('');
  const [loading, setLoading] = useState(true);

  // Find the group and rule
  const group = config?.groups.find((g) => g.id === groupId);
  const rule = group?.rules.find((r) => r.id === ruleId);

  useEffect(() => {
    if (rule) {
      setModel(rule.model);
      setDirection(rule.direction);
      setToken(rule.token);
      setApiAddress(rule.apiAddress);
      setLoading(false);
    } else if (!loading) {
      showToast(t('toast.ruleNotFound'), 'error');
      navigate('/');
    }
  }, [rule, loading, t, showToast, navigate]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!config || !groupId || !ruleId) return;

    const newConfig: ProxyConfig = {
      ...config,
      groups: config.groups.map((group) => {
        if (group.id === groupId) {
          return {
            ...group,
            rules: group.rules.map((r) =>
              r.id === ruleId ? { ...r, model, direction, token, apiAddress } : r
            ),
          };
        }
        return group;
      }),
    };

    await saveConfig(newConfig);
    showToast(t('toast.ruleUpdated'), 'success');
    navigate('/');
  };

  const handleCancel = () => {
    navigate('/');
  };

  const isValid = model.trim() && token.trim() && apiAddress.trim();

  if (loading) {
    return (
      <div className={styles.loading}>
        <p>{t('app.statusLoading')}</p>
      </div>
    );
  }

  return (
    <div className={styles.ruleEditPage}>
      <div className={styles.header}>
        <h1>{t('ruleEditPage.title')}</h1>
        <div className={styles.breadcrumb}>
          <span onClick={() => navigate('/')} className={styles.breadcrumbItem}>
            {t('servicePage.groupPath')}
          </span>
          <span className={styles.breadcrumbSeparator}>/</span>
          <span className={styles.breadcrumbItem}>{group?.name}</span>
          <span className={styles.breadcrumbSeparator}>/</span>
          <span className={styles.breadcrumbItem}>{rule?.model}</span>
        </div>
      </div>

      <div className={styles.formContainer}>
        <form onSubmit={handleSubmit} className={styles.ruleForm}>
          <div className={styles.formGroup}>
            <label htmlFor="model">{t('servicePage.model')}</label>
            <Input
              id="model"
              value={model}
              onChange={(e) => setModel(e.target.value)}
              placeholder="e.g. claude-3-5-sonnet-20241022"
              className={styles.input}
            />
          </div>

          <div className={styles.formGroup}>
            <label>{t('servicePage.forwardDirection')}</label>
            <div className={styles.directionOptions}>
              <button
                type="button"
                className={`${styles.directionOption} ${direction === 'oc' ? styles.active : ''}`}
                onClick={() => setDirection('oc')}
              >
                {t('ruleDirection.oc')}
              </button>
              <button
                type="button"
                className={`${styles.directionOption} ${direction === 'co' ? styles.active : ''}`}
                onClick={() => setDirection('co')}
              >
                {t('ruleDirection.co')}
              </button>
            </div>
          </div>

          <div className={styles.formGroup}>
            <label htmlFor="token">{t('servicePage.token')}</label>
            <Input
              id="token"
              type="password"
              value={token}
              onChange={(e) => setToken(e.target.value)}
              placeholder="sk-..."
              className={styles.input}
            />
          </div>

          <div className={styles.formGroup}>
            <label htmlFor="apiAddress">{t('servicePage.apiAddress')}</label>
            <Input
              id="apiAddress"
              value={apiAddress}
              onChange={(e) => setApiAddress(e.target.value)}
              placeholder="https://api.anthropic.com"
              className={styles.input}
            />
          </div>

          <div className={styles.formActions}>
            <Button variant="default" onClick={handleCancel} className={styles.button}>
              {t('common.cancel')}
            </Button>
            <Button type="submit" variant="primary" disabled={!isValid} className={styles.button}>
              {t('ruleEditPage.saveChanges')}
            </Button>
          </div>
        </form>
      </div>
    </div>
  );
};

export default RuleEditPage;
