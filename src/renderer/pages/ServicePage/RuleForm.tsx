import React, { useState, useEffect } from 'react';
import { useTranslation } from '@/hooks';
import { Button, Input, Switch } from '@/components';
import type { Rule, RuleDirection } from '@/types';
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
  const [model, setModel] = useState(rule?.model ?? '');
  const [direction, setDirection] = useState<RuleDirection>(rule?.direction ?? 'oc');
  const [token, setToken] = useState(rule?.token ?? '');
  const [apiAddress, setApiAddress] = useState(rule?.apiAddress ?? '');

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    onSave({
      model,
      direction,
      token,
      apiAddress,
    });
  };

  const isValid = model.trim() && token.trim() && apiAddress.trim();

  return (
    <div className={styles.ruleForm}>
      <h3>{rule ? t('common.edit') : t('servicePage.addRule')}</h3>
      <form onSubmit={handleSubmit}>
        <div className={styles.formGroup}>
          <label htmlFor="model">{t('servicePage.model')}</label>
          <Input
            id="model"
            value={model}
            onChange={(e) => setModel(e.target.value)}
            placeholder="e.g. claude-3-5-sonnet-20241022"
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
