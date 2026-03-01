import React from 'react';
import { Plus, Folder, ChevronRight, Check, Trash2 } from 'lucide-react';
import { Button } from '@/components';
import { useTranslation } from '@/hooks';
import { useNavigate } from 'react-router-dom';
import type { Group } from '@/types';
import styles from './ServicePage.module.css';

export interface ServicePageProps {
  groups: Group[];
  activeGroupId: string | null;
  onSelectGroup: (groupId: string) => void;
  onAddGroup: () => void;
  onDeleteGroup: (groupId: string) => void;
}

/**
 * GroupList Component
 * Displays a list of groups in the sidebar
 */
export const GroupList: React.FC<{
  groups: Group[];
  activeGroupId: string | null;
  onSelect: (groupId: string) => void;
  onAdd: () => void;
}> = ({ groups, activeGroupId, onSelect, onAdd }) => {
  const { t } = useTranslation();

  return (
    <div className={styles.groupList}>
      <div className={styles.groupListHeader}>
        <h3>{t('servicePage.groupPath')}</h3>
        <Button
          variant="ghost"
          size="small"
          icon={Plus}
          onClick={onAdd}
          title={t('header.addGroup')}
        />
      </div>
      <div className={styles.groupListContent}>
        {groups.length === 0 ? (
          <p className={styles.emptyHint}>{t('servicePage.noGroupsHint')}</p>
        ) : (
          <ul className={styles.groupItems}>
            {groups.map((group) => (
              <li key={group.id}>
                <button
                  className={`${styles.groupItem} ${group.id === activeGroupId ? styles.active : ''}`}
                  onClick={() => onSelect(group.id)}
                >
                  <Folder size={16} className={styles.groupIcon} />
                  <span className={styles.groupName}>{group.name}</span>
                  <span className={styles.groupPath}>/{group.path}</span>
                  {group.id === activeGroupId && (
                    <Check size={14} className={styles.activeIcon} />
                  )}
                  <ChevronRight size={14} className={styles.chevron} />
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
};

/**
 * RuleList Component
 * Displays rules within a group
 */
export const RuleList: React.FC<{
  rules: Group['rules'];
  activeRuleId: string | null;
  onSelect: (ruleId: string) => void;
  onAdd: () => void;
  onDelete: (ruleId: string) => void;
  groupName: string;
  groupId: string;
}> = ({ rules, activeRuleId, onSelect, onAdd, onDelete, groupName, groupId }) => {
  const { t } = useTranslation();
  const navigate = useNavigate();

  const handleRuleClick = (ruleId: string) => {
    navigate(`/groups/${groupId}/rules/${ruleId}/edit`);
  };

  const handleAddRuleClick = () => {
    navigate(`/groups/${groupId}/rules/new`);
  };

  return (
    <div className={styles.ruleList}>
      <div className={styles.ruleListHeader}>
        <h3>{t('servicePage.model')} ({groupName})</h3>
        <Button
          variant="ghost"
          size="small"
          icon={Plus}
          onClick={handleAddRuleClick}
          title={t('servicePage.addRule')}
        />
      </div>
      <div className={styles.ruleListContent}>
        {rules.length === 0 ? (
          <p className={styles.emptyHint}>{t('servicePage.noRulesHint')}</p>
        ) : (
          <ul className={styles.ruleItems}>
            {rules.map((rule) => (
              <li key={rule.id} className={styles.ruleItemContainer}>
                <button
                  className={`${styles.ruleItem} ${rule.id === activeRuleId ? styles.active : ''}`}
                  onClick={() => handleRuleClick(rule.id)}
                >
                  <span className={styles.ruleModel}>{rule.model}</span>
                  <span className={styles.ruleDirection}>
                    {t(`ruleDirection.${rule.direction}`)}
                  </span>
                  {rule.id === activeRuleId && (
                    <span className={styles.currentBadge}>{t('servicePage.current')}</span>
                  )}
                </button>
                <button
                  className={styles.deleteButton}
                  onClick={(e) => {
                    e.stopPropagation();
                    onDelete(rule.id);
                  }}
                  title={t('servicePage.deleteRule')}
                >
                  <Trash2 size={14} />
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
};

export default ServicePageProps;
