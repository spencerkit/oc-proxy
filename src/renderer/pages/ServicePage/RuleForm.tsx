import React, { useState } from 'react';
import { useTranslation } from '@/hooks';
import { Button, Input } from '@/components';
import type { Rule } from '@/types';
import styles from './ServicePage.module.css';

export interface RuleFormProps {
  groupPath: string;
  rule?: Rule;
  onSave: (rule: Omit<Rule, 'id'>) => void;
  onCancel: () => void;
}

/**
 * RuleForm Component
 * Form for creating or editing a rule
 */
export const RuleForm: React.FC<RuleFormProps> = ({
  groupPath,
  rule,
  onSave,
  onCancel,
}) => {
  const { t } = useTranslation();
  const [name, setName] = useState(rule?.name ?? '');
  const [protocol, setProtocol] = useState<Rule['protocol']>(rule?.protocol ?? 'anthropic');
  const [token, setToken] = useState(rule?.token ?? '');
  const [apiAddress, setApiAddress] = useState(rule?.apiAddress ?? '');
  const [defaultModel, setDefaultModel] = useState(rule?.defaultModel ?? '');

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    onSave({
      name,
      protocol,
      token,
      apiAddress,
      defaultModel,
      modelMappings: {},
    });
  };

  const isValid = name.trim() && token.trim() && apiAddress.trim() && defaultModel.trim();

  return (
    <div className={styles.ruleForm}>
      <h3>{rule ? t('common.edit') : t('servicePage.addRule')}</h3>
      <form onSubmit={handleSubmit}>
        <div className={styles.formGroup}>
          <label htmlFor="name">{t('servicePage.ruleName')}</label>
          <Input
            id="name"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder={t('ruleForm.ruleNamePlaceholder')}
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
        </div>

        <div className={styles.formGroup}>
          <label htmlFor="defaultModel">{t('servicePage.defaultModel')}</label>
          <Input
            id="defaultModel"
            value={defaultModel}
            onChange={(e) => setDefaultModel(e.target.value)}
            placeholder={t('ruleForm.defaultModelPlaceholder')}
          />
        </div>

        <div className={styles.formGroup}>
          <label htmlFor="token">{t('servicePage.token')}</label>
          <Input
            id="token"
            type="password"
            value={token}
            onChange={(e) => setToken(e.target.value)}
            placeholder="sk-..."
          />
        </div>

        <div className={styles.formGroup}>
          <label htmlFor="apiAddress">{t('servicePage.apiAddress')}</label>
          <Input
            id="apiAddress"
            value={apiAddress}
            onChange={(e) => setApiAddress(e.target.value)}
            placeholder="https://api.anthropic.com"
          />
        </div>

        <div className={styles.formActions}>
          <Button type="button" variant="default" onClick={onCancel}>
            {t('common.cancel')}
          </Button>
          <Button type="submit" variant="primary" disabled={!isValid}>
            {t('servicePage.saveRule')}
          </Button>
        </div>
      </form>
    </div>
  );
};

export default RuleForm;
