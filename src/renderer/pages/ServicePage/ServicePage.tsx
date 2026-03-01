import React, { useState } from 'react';
import { Copy, Trash2, Plus } from 'lucide-react';
import { useProxyStore } from '@/store';
import { Button, Modal, Input } from '@/components';
import { useTranslation, useLogs } from '@/hooks';
import { RuleList } from './RuleList';
import { RuleForm } from './RuleForm';
import type { Group, Rule, ProxyConfig } from '@/types';
import styles from './ServicePage.module.css';

/**
 * ServicePage Component
 * Main page for managing proxy groups and rules
 */
export const ServicePage: React.FC = () => {
  const { t } = useTranslation();
  const { config, saveConfig, status } = useProxyStore();
  const { showToast } = useLogs();
  const [activeGroupId, setActiveGroupId] = useState<string | null>(null);
  const [selectedRuleId, setSelectedRuleId] = useState<string | null>(null);
  const [showAddGroupModal, setShowAddGroupModal] = useState(false);
  const [showAddRuleForm, setShowAddRuleForm] = useState(false);
  const [showDeleteGroupModal, setShowDeleteGroupModal] = useState(false);
  const [newGroupName, setNewGroupName] = useState('');
  const [newGroupPath, setNewGroupPath] = useState('');

  const groups = config?.groups ?? [];
  const activeGroup = groups.find((g) => g.id === activeGroupId);
  const activeRule = activeGroup?.rules.find((r) => r.id === selectedRuleId) ?? null;

  // Auto-select first group if none selected
  React.useEffect(() => {
    if (!activeGroupId && groups.length > 0) {
      setActiveGroupId(groups[0].id);
    }
  }, [groups, activeGroupId]);

  const handleSelectGroup = (groupId: string) => {
    setActiveGroupId(groupId);
    setSelectedRuleId(null);
    setShowAddRuleForm(false);
  };

  const handleAddGroup = async () => {
    if (!newGroupName.trim() || !newGroupPath.trim()) return;

    const newGroup: Group = {
      id: crypto.randomUUID(),
      name: newGroupName.trim(),
      path: newGroupPath.trim().replace(/^\//, ''),
      activeRuleId: null,
      rules: [],
    };

    if (!config) return;

    const newConfig: ProxyConfig = {
      ...config,
      groups: [...(config.groups ?? []), newGroup],
    };

    await saveConfig(newConfig);
    setShowAddGroupModal(false);
    setNewGroupName('');
    setNewGroupPath('');
    setActiveGroupId(newGroup.id);
    showToast(t('toast.groupCreated'), 'success');
  };

  const handleDeleteGroup = async () => {
    if (!activeGroupId || !config) return;

    const newGroups = config.groups.filter((g) => g.id !== activeGroupId);
    const newConfig = { ...config, groups: newGroups };

    await saveConfig(newConfig);
    setActiveGroupId(newGroups.length > 0 ? newGroups[0].id : null);
    setShowDeleteGroupModal(false);
    showToast(t('toast.groupDeleted'), 'success');
  };

  const handleAddRule = () => {
    setShowAddRuleForm(true);
    setSelectedRuleId(null);
  };

  const handleSaveRule = async (ruleData: Omit<Rule, 'id'>) => {
    if (!activeGroupId || !config) return;

    const newRule: Rule = {
      id: crypto.randomUUID(),
      ...ruleData,
    };

    const newGroups = config.groups.map((group) => {
      if (group.id === activeGroupId) {
        return {
          ...group,
          rules: [...group.rules, newRule],
          activeRuleId: group.activeRuleId ?? newRule.id,
        };
      }
      return group;
    });

    const newConfig = { ...config, groups: newGroups };
    await saveConfig(newConfig);
    setShowAddRuleForm(false);
    setSelectedRuleId(newRule.id);
    showToast(t('toast.ruleCreated'), 'success');
  };

  const handleDeleteRule = async (ruleId: string) => {
    if (!activeGroupId || !config) return;

    const newGroups = config.groups.map((group) => {
      if (group.id === activeGroupId) {
        const newRules = group.rules.filter((r) => r.id !== ruleId);
        const newActiveId = group.activeRuleId === ruleId
          ? (newRules.length > 0 ? newRules[0].id : null)
          : group.activeRuleId;
        return { ...group, rules: newRules, activeRuleId: newActiveId };
      }
      return group;
    });

    const newConfig = { ...config, groups: newGroups };
    await saveConfig(newConfig);
    setSelectedRuleId(null);
    showToast(t('toast.ruleDeleted'), 'success');
  };

  const handleCopyEntryUrl = async () => {
    if (!activeGroup) return;

    const host = status?.address ?? 'localhost';
    const port = config?.server.port ?? 8080;
    const url = `http://${host}:${port}/oc/${activeGroup.path}`;

    try {
      await navigator.clipboard.writeText(url);
      showToast(t('toast.entryUrlCopied'), 'success');
    } catch {
      showToast(t('toast.copyFailed'), 'error');
    }
  };

  const getEntryUrl = () => {
    if (!activeGroup) return '';
    const host = status?.address ?? 'localhost';
    const port = config?.server.port ?? 8080;
    return `http://${host}:${port}/oc/${activeGroup.path}`;
  };

  return (
    <div className={styles.servicePage}>
      {/* Group List Sidebar */}
      <div className={styles.sidebar}>
        <div className={styles.groupList}>
          <div className={styles.groupListHeader}>
            <h3>{t('servicePage.groupPath')}</h3>
            <Button
              variant="ghost"
              size="small"
              icon={Plus}
              onClick={() => setShowAddGroupModal(true)}
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
                      onClick={() => handleSelectGroup(group.id)}
                    >
                      <span className={styles.groupName}>{group.name}</span>
                      <span className={styles.groupPath}>/{group.path}</span>
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </div>
        </div>
      </div>

      {/* Main Content */}
      <div className={styles.content}>
        {!activeGroup ? (
          <div className={styles.noSelection}>
            <p>{t('servicePage.noGroupSelected')}</p>
          </div>
        ) : (
          <>
            {/* Group Header */}
            <div className={styles.groupHeader}>
              <div className={styles.groupInfo}>
                <h2>{activeGroup.name}</h2>
                <div className={styles.entryUrl}>
                  <code>{getEntryUrl()}</code>
                  <Button
                    variant="ghost"
                    size="small"
                    icon={Copy}
                    onClick={handleCopyEntryUrl}
                    title={t('servicePage.copyEntryUrl')}
                  />
                </div>
              </div>
              <Button
                variant="danger"
                size="small"
                icon={Trash2}
                onClick={() => setShowDeleteGroupModal(true)}
                title={t('servicePage.deleteGroup')}
              />
            </div>

            {/* Rule List */}
            <RuleList
              rules={activeGroup.rules}
              activeRuleId={selectedRuleId ?? activeGroup.activeRuleId}
              onSelect={setSelectedRuleId}
              onAdd={handleAddRule}
              onDelete={handleDeleteRule}
              groupName={activeGroup.name}
              groupId={activeGroup.id}
            />

            {/* Rule Form (when adding new rule) */}
            {showAddRuleForm && (
              <RuleForm
                groupPath={activeGroup.path}
                onSave={handleSaveRule}
                onCancel={() => setShowAddRuleForm(false)}
              />
            )}

            {/* Rule Detail (when rule is selected) */}
            {selectedRuleId && activeRule && !showAddRuleForm && (
              <div className={styles.ruleDetail}>
                <h3>{activeRule.model}</h3>
                <div className={styles.ruleInfo}>
                  <div className={styles.ruleInfoItem}>
                    <span className={styles.label}>{t('servicePage.forwardDirection')}:</span>
                    <span>{t(`ruleDirection.${activeRule.direction}`)}</span>
                  </div>
                  <div className={styles.ruleInfoItem}>
                    <span className={styles.label}>{t('servicePage.apiAddress')}:</span>
                    <span>{activeRule.apiAddress}</span>
                  </div>
                </div>
                <div className={styles.ruleActions}>
                  <Button
                    variant="danger"
                    size="small"
                    onClick={() => handleDeleteRule(activeRule.id)}
                  >
                    {t('servicePage.deleteRule')}
                  </Button>
                </div>
              </div>
            )}
          </>
        )}
      </div>

      {/* Add Group Modal */}
      <Modal
        open={showAddGroupModal}
        onClose={() => setShowAddGroupModal(false)}
        title={t('modal.addGroupTitle')}
      >
        <div className={styles.modalContent}>
          <div className={styles.formGroup}>
            <label htmlFor="groupName">{t('modal.groupNameLabel')}</label>
            <Input
              id="groupName"
              value={newGroupName}
              onChange={(e) => setNewGroupName(e.target.value)}
              placeholder={t('modal.groupNamePlaceholder')}
            />
          </div>
          <div className={styles.formGroup}>
            <label htmlFor="groupPath">{t('modal.pathLabel')}</label>
            <Input
              id="groupPath"
              value={newGroupPath}
              onChange={(e) => setNewGroupPath(e.target.value)}
              placeholder={t('modal.pathPlaceholder')}
            />
            <p className={styles.formHint}>{t('modal.pathHint')}</p>
          </div>
          <div className={styles.modalActions}>
            <Button variant="default" onClick={() => setShowAddGroupModal(false)}>
              {t('common.cancel')}
            </Button>
            <Button
              variant="primary"
              onClick={handleAddGroup}
              disabled={!newGroupName.trim() || !newGroupPath.trim()}
            >
              {t('modal.create')}
            </Button>
          </div>
        </div>
      </Modal>

      {/* Delete Group Modal */}
      <Modal
        open={showDeleteGroupModal}
        onClose={() => setShowDeleteGroupModal(false)}
        title={t('deleteGroupModal.title')}
      >
        <div className={styles.modalContent}>
          <p>{t('deleteGroupModal.confirmText', {
            name: activeGroup?.name,
            path: activeGroup?.path,
          })}</p>
          <div className={styles.modalActions}>
            <Button variant="default" onClick={() => setShowDeleteGroupModal(false)}>
              {t('common.cancel')}
            </Button>
            <Button variant="danger" onClick={handleDeleteGroup}>
              {t('deleteGroupModal.confirmDelete')}
            </Button>
          </div>
        </div>
      </Modal>
    </div>
  );
};

export default ServicePage;
