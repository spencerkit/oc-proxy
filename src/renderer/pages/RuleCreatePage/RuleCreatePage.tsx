import React, { useState } from 'react';
import { useParams, useNavigate } from 'react-router-dom';
import { useProxyStore } from '@/store';
import { Button, Input } from '@/components';
import { useTranslation, useLogs } from '@/hooks';
import type { Rule, ProxyConfig } from '@/types';
import styles from './RuleCreatePage.module.css';

/**
 * RuleCreatePage Component
 * Page for creating a new rule
 */
export const RuleCreatePage: React.FC = () => {
  const { groupId } = useParams<{ groupId: string }>();
  const navigate = useNavigate();
  const { t } = useTranslation();
  const { config, saveConfig } = useProxyStore();
  const { showToast } = useLogs();

  const [model, setModel] = useState('');
  const [direction, setDirection] = useState<Rule['direction']>('oc');
  const [token, setToken] = useState('');
  const [apiAddress, setApiAddress] = useState('');
  const [errors, setErrors] = useState<{ model?: string; token?: string; apiAddress?: string }>({});

  const group = config?.groups.find((g) => g.id === groupId);

  if (!group) {
    showToast(t('toast.groupNotFound'), 'error');
    navigate('/');
    return null;
  }

  const focusField = (id: string) => {
    const input = document.getElementById(id) as HTMLInputElement | null;
    input?.focus();
  };

  const validateForm = () => {
    const nextErrors: { model?: string; token?: string; apiAddress?: string } = {};

    if (!model.trim()) {
      nextErrors.model = t('validation.required', { field: t('servicePage.model') });
    }
    if (!token.trim()) {
      nextErrors.token = t('validation.required', { field: t('servicePage.token') });
    }
    if (!apiAddress.trim()) {
      nextErrors.apiAddress = t('validation.required', { field: t('servicePage.apiAddress') });
    }

    setErrors(nextErrors);

    if (nextErrors.model) {
      focusField('model');
      return false;
    }
    if (nextErrors.token) {
      focusField('token');
      return false;
    }
    if (nextErrors.apiAddress) {
      focusField('apiAddress');
      return false;
    }
    return true;
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!config || !groupId) return;
    if (!validateForm()) return;

    const newRule: Rule = {
      id: crypto.randomUUID(),
      model,
      direction,
      token,
      apiAddress,
    };

    const newConfig: ProxyConfig = {
      ...config,
      groups: config.groups.map((group) => {
        if (group.id === groupId) {
          return {
            ...group,
            rules: [...group.rules, newRule],
            activeRuleId: group.activeRuleId ?? newRule.id,
          };
        }
        return group;
      }),
    };

    await saveConfig(newConfig);
    showToast(t('toast.ruleCreated'), 'success');
    navigate('/');
  };

  const handleCancel = () => {
    navigate('/');
  };

  const isValid = model.trim() && token.trim() && apiAddress.trim();
  const previewPath = `/oc/${group.path}`;
  const previewUpstream = apiAddress.trim() || 'https://...';

  return (
    <div className={styles.ruleCreatePage}>
      <div className={styles.header}>
        <h1>{t('ruleCreatePage.title')}</h1>
        <nav className={styles.breadcrumb} aria-label={t('header.backToService')}>
          <button
            type="button"
            onClick={() => navigate('/')}
            className={styles.breadcrumbButton}
          >
            {t('servicePage.groupPath')}
          </button>
          <span className={styles.breadcrumbSeparator}>/</span>
          <span className={styles.breadcrumbItem}>{group.name}</span>
          <span className={styles.breadcrumbSeparator}>/</span>
          <span className={styles.breadcrumbItem}>{t('ruleCreatePage.newRule')}</span>
        </nav>
      </div>

      <div className={styles.formContainer}>
        <div className={styles.ruleGrid}>
          <form onSubmit={handleSubmit} className={styles.ruleForm}>
            <section className={styles.formSection}>
              <h2 className={styles.sectionTitle}>{t('ruleForm.sectionRouting')}</h2>

              <div className={styles.formGroup}>
                <label htmlFor="model">{t('servicePage.model')}</label>
                <Input
                  id="model"
                  value={model}
                  onChange={(e) => {
                    setModel(e.target.value);
                    if (errors.model) {
                      setErrors((prev) => ({ ...prev, model: undefined }));
                    }
                  }}
                  placeholder="e.g. claude-3-5-sonnet-20241022"
                  className={styles.input}
                  error={errors.model}
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
                <p className={styles.fieldHint}>{t('ruleForm.directionHint')}</p>
              </div>
            </section>

            <section className={styles.formSection}>
              <h2 className={styles.sectionTitle}>{t('ruleForm.sectionSecurity')}</h2>

              <div className={styles.formGroup}>
                <label htmlFor="token">{t('servicePage.token')}</label>
                <Input
                  id="token"
                  type="password"
                  value={token}
                  onChange={(e) => {
                    setToken(e.target.value);
                    if (errors.token) {
                      setErrors((prev) => ({ ...prev, token: undefined }));
                    }
                  }}
                  placeholder="sk-..."
                  className={styles.input}
                  error={errors.token}
                  hint={t('ruleForm.tokenHint')}
                />
              </div>

              <div className={styles.formGroup}>
                <label htmlFor="apiAddress">{t('servicePage.apiAddress')}</label>
                <Input
                  id="apiAddress"
                  value={apiAddress}
                  onChange={(e) => {
                    setApiAddress(e.target.value);
                    if (errors.apiAddress) {
                      setErrors((prev) => ({ ...prev, apiAddress: undefined }));
                    }
                  }}
                  placeholder="https://api.anthropic.com"
                  className={styles.input}
                  error={errors.apiAddress}
                  hint={t('ruleForm.endpointHint')}
                />
              </div>
            </section>

            <div className={styles.formActions}>
              <Button variant="default" onClick={handleCancel} className={styles.button}>
                {t('common.cancel')}
              </Button>
              <Button type="submit" variant="primary" disabled={!isValid} className={styles.button}>
                {t('ruleCreatePage.createRule')}
              </Button>
            </div>
          </form>

          <aside className={styles.previewCard}>
            <h3>{t('ruleForm.previewTitle')}</h3>
            <div className={styles.previewRow}>
              <span>{t('ruleForm.previewPath')}</span>
              <code>{previewPath}</code>
            </div>
            <div className={styles.previewRow}>
              <span>{t('ruleForm.previewDirection')}</span>
              <strong>{t(`ruleDirection.${direction}`)}</strong>
            </div>
            <div className={styles.previewRow}>
              <span>{t('ruleForm.previewUpstream')}</span>
              <code>{previewUpstream}</code>
            </div>
          </aside>
        </div>
      </div>
    </div>
  );
};

export default RuleCreatePage;
