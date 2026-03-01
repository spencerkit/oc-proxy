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

  const [name, setName] = useState('');
  const [protocol, setProtocol] = useState<Rule['protocol']>('anthropic');
  const [token, setToken] = useState('');
  const [apiAddress, setApiAddress] = useState('');
  const [defaultModel, setDefaultModel] = useState('');
  const [modelMappings, setModelMappings] = useState<Record<string, string>>({});
  const [loading, setLoading] = useState(true);
  const [errors, setErrors] = useState<{ name?: string; token?: string; apiAddress?: string; defaultModel?: string }>({});

  // Find the group and rule
  const group = config?.groups.find((g) => g.id === groupId);
  const rule = group?.rules.find((r) => r.id === ruleId);

  useEffect(() => {
    if (rule) {
      setName(rule.name);
      setProtocol(rule.protocol);
      setToken(rule.token);
      setApiAddress(rule.apiAddress);
      setDefaultModel(rule.defaultModel);
      setModelMappings(rule.modelMappings || {});
      setLoading(false);
    } else if (config) {
      setLoading(false);
      showToast(t('toast.ruleNotFound'), 'error');
      navigate('/');
    }
  }, [rule, config, t, showToast, navigate]);

  const focusField = (id: string) => {
    const input = document.getElementById(id) as HTMLInputElement | null;
    input?.focus();
  };

  const validateForm = () => {
    const nextErrors: { name?: string; token?: string; apiAddress?: string; defaultModel?: string } = {};

    if (!name.trim()) {
      nextErrors.name = t('validation.required', { field: t('servicePage.ruleName') });
    }
    if (!token.trim()) {
      nextErrors.token = t('validation.required', { field: t('servicePage.token') });
    }
    if (!apiAddress.trim()) {
      nextErrors.apiAddress = t('validation.required', { field: t('servicePage.apiAddress') });
    }
    if (!defaultModel.trim()) {
      nextErrors.defaultModel = t('validation.required', { field: t('servicePage.defaultModel') });
    }

    setErrors(nextErrors);

    if (nextErrors.name) {
      focusField('name');
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
    if (nextErrors.defaultModel) {
      focusField('defaultModel');
      return false;
    }
    return true;
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!config || !groupId || !ruleId) return;
    if (!validateForm()) return;

    const newConfig: ProxyConfig = {
      ...config,
      groups: config.groups.map((group) => {
        if (group.id === groupId) {
          return {
            ...group,
            rules: group.rules.map((r) =>
              r.id === ruleId
                ? {
                  ...r,
                  name: name.trim(),
                  protocol,
                  token,
                  apiAddress,
                  defaultModel: defaultModel.trim(),
                  modelMappings: Object.fromEntries(
                    Object.entries(modelMappings)
                      .map(([key, value]) => [key.trim(), value.trim()])
                      .filter(([key, value]) => key && value)
                  ),
                }
                : r
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

  const isValid = name.trim() && token.trim() && apiAddress.trim() && defaultModel.trim();
  const previewPath = `/oc/${group?.id ?? ''}`;
  const previewUpstream = apiAddress.trim() || 'https://...';

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
        <nav className={styles.breadcrumb} aria-label={t('header.backToService')}>
          <button
            type="button"
            onClick={() => navigate('/')}
            className={styles.breadcrumbButton}
          >
            {t('servicePage.groupPath')}
          </button>
          <span className={styles.breadcrumbSeparator}>/</span>
          <span className={styles.breadcrumbItem}>{group?.name}</span>
          <span className={styles.breadcrumbSeparator}>/</span>
          <span className={styles.breadcrumbItem}>{rule?.name}</span>
        </nav>
      </div>

      <div className={styles.formContainer}>
        <div className={styles.ruleGrid}>
          <form onSubmit={handleSubmit} className={styles.ruleForm}>
            <section className={styles.formSection}>
              <h2 className={styles.sectionTitle}>{t('ruleForm.sectionRouting')}</h2>

              <div className={styles.formGroup}>
                <label htmlFor="name">{t('servicePage.ruleName')}</label>
                <Input
                  id="name"
                  value={name}
                  onChange={(e) => {
                    setName(e.target.value);
                    if (errors.name) {
                      setErrors((prev) => ({ ...prev, name: undefined }));
                    }
                  }}
                  placeholder={t('ruleForm.ruleNamePlaceholder')}
                  className={styles.input}
                  error={errors.name}
                />
              </div>

              <div className={styles.formGroup}>
                <label>{t('servicePage.ruleProtocol')}</label>
                <div className={styles.directionOptions}>
                  <button
                    type="button"
                    className={`${styles.directionOption} ${protocol === 'anthropic' ? styles.active : ''}`}
                    onClick={() => setProtocol('anthropic')}
                  >
                    {t('ruleProtocol.anthropic')}
                  </button>
                  <button
                    type="button"
                    className={`${styles.directionOption} ${protocol === 'openai' ? styles.active : ''}`}
                    onClick={() => setProtocol('openai')}
                  >
                    {t('ruleProtocol.openai')}
                  </button>
                </div>
                <p className={styles.fieldHint}>{t('ruleForm.protocolHint')}</p>
              </div>
            </section>

            <section className={styles.formSection}>
              <h2 className={styles.sectionTitle}>{t('ruleForm.sectionModelSettings')}</h2>

              <div className={styles.formGroup}>
                <label htmlFor="defaultModel">{t('servicePage.defaultModel')}</label>
                <Input
                  id="defaultModel"
                  value={defaultModel}
                  onChange={(e) => {
                    setDefaultModel(e.target.value);
                    if (errors.defaultModel) {
                      setErrors((prev) => ({ ...prev, defaultModel: undefined }));
                    }
                  }}
                  placeholder={t('ruleForm.defaultModelPlaceholder')}
                  className={styles.input}
                  error={errors.defaultModel}
                  hint={t('ruleForm.defaultModelHint')}
                />
              </div>

              <div className={styles.formGroup}>
                <label>{t('ruleForm.modelMappings')}</label>
                <div className={styles.mappingList}>
                  {(group?.models || []).length === 0 ? (
                    <p className={styles.fieldHint}>{t('ruleForm.noGroupModels')}</p>
                  ) : (
                    (group?.models || []).map((modelName) => (
                      <div key={modelName} className={styles.mappingRow}>
                        <span className={styles.mappingLabel}>{modelName}</span>
                        <Input
                          value={modelMappings[modelName] ?? ''}
                          onChange={(e) => {
                            setModelMappings((prev) => ({ ...prev, [modelName]: e.target.value }));
                          }}
                          placeholder={t('ruleForm.mappingPlaceholder')}
                        />
                      </div>
                    ))
                  )}
                </div>
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
                {t('ruleEditPage.saveChanges')}
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
              <strong>{t(`ruleProtocol.${protocol}`)}</strong>
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

export default RuleEditPage;
