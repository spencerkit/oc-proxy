import React, { useState } from 'react';
import { Copy, Trash2, Plus, Pencil } from 'lucide-react';
import { useNavigate } from 'react-router-dom';
import { useProxyStore } from '@/store';
import { Button, Modal, Input } from '@/components';
import { useTranslation, useLogs } from '@/hooks';
import { RuleList } from './RuleList';
import type { Group, ProxyConfig } from '@/types';
import styles from './ServicePage.module.css';

/**
 * ServicePage Component
 * Main page for managing proxy groups and rules
 */
export const ServicePage: React.FC = () => {
  const navigate = useNavigate();
  const { t } = useTranslation();
  const { config, saveConfig, status } = useProxyStore();
  const { showToast } = useLogs();
  const [activeGroupId, setActiveGroupId] = useState<string | null>(null);
  const [selectedRuleId, setSelectedRuleId] = useState<string | null>(null);
  const [showAddGroupModal, setShowAddGroupModal] = useState(false);
  const [showDeleteGroupModal, setShowDeleteGroupModal] = useState(false);
  const [showDeleteRuleModal, setShowDeleteRuleModal] = useState(false);
  const [pendingDeleteRuleId, setPendingDeleteRuleId] = useState<string | null>(null);
  const [newGroupName, setNewGroupName] = useState('');
  const [newGroupId, setNewGroupId] = useState('');

  const groups = config?.groups ?? [];
  const activeGroup = groups.find((g) => g.id === activeGroupId);
  const activeGroupModels = Array.isArray(activeGroup?.models) ? activeGroup.models : [];
  const activeRule = activeGroup?.rules.find((r) => r.id === selectedRuleId) ?? null;
  const pendingDeleteRule = activeGroup?.rules.find((r) => r.id === pendingDeleteRuleId) ?? null;

  // Auto-select first group if none selected
  React.useEffect(() => {
    if (!activeGroupId && groups.length > 0) {
      setActiveGroupId(groups[0].id);
    }
  }, [groups, activeGroupId]);

  const handleSelectGroup = (groupId: string) => {
    setActiveGroupId(groupId);
    setSelectedRuleId(null);
    setShowDeleteRuleModal(false);
    setPendingDeleteRuleId(null);
  };

  const handleAddGroup = async () => {
    if (!newGroupName.trim() || !newGroupId.trim() || !config) return;
    const normalizedId = newGroupId.trim().replace(/^\/+/, '');
    if (!/^[a-zA-Z0-9_-]+$/.test(normalizedId)) {
      showToast(t('validation.invalidFormat', { field: t('modal.groupIdLabel') }), 'error');
      return;
    }
    if ((config.groups || []).some((group) => group.id === normalizedId)) {
      showToast(t('validation.alreadyExists', { field: t('modal.groupIdLabel') }), 'error');
      return;
    }

    const newGroup: Group = {
      id: normalizedId,
      name: newGroupName.trim(),
      models: [],
      activeRuleId: null,
      rules: [],
    };

    const newConfig: ProxyConfig = {
      ...config,
      groups: [...(config.groups ?? []), newGroup],
    };

    await saveConfig(newConfig);
    setShowAddGroupModal(false);
    setNewGroupName('');
    setNewGroupId('');
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

  const handleRequestDeleteRule = (ruleId: string) => {
    setPendingDeleteRuleId(ruleId);
    setShowDeleteRuleModal(true);
  };

  const handleDeleteRule = async () => {
    if (!activeGroupId || !config || !pendingDeleteRuleId) return;

    const newGroups = config.groups.map((group) => {
      if (group.id === activeGroupId) {
        const newRules = group.rules.filter((r) => r.id !== pendingDeleteRuleId);
        const newActiveId = group.activeRuleId === pendingDeleteRuleId
          ? (newRules.length > 0 ? newRules[0].id : null)
          : group.activeRuleId;
        return { ...group, rules: newRules, activeRuleId: newActiveId };
      }
      return group;
    });

    const newConfig = { ...config, groups: newGroups };
    await saveConfig(newConfig);
    setSelectedRuleId(null);
    setShowDeleteRuleModal(false);
    setPendingDeleteRuleId(null);
    showToast(t('toast.ruleDeleted'), 'success');
  };

  const handleCopyEntryUrl = async () => {
    if (!activeGroup) return;

    const url = `${getServerBaseUrl()}/oc/${activeGroup.id}`;

    try {
      await navigator.clipboard.writeText(url);
      showToast(t('toast.entryUrlCopied'), 'success');
    } catch {
      showToast(t('toast.copyFailed'), 'error');
    }
  };

  const getEntryUrl = () => {
    if (!activeGroup) return '';
    return `${getServerBaseUrl()}/oc/${activeGroup.id}`;
  };

  const getServerBaseUrl = () => {
    if (status?.address && /^https?:\/\//.test(status.address)) {
      return status.address.replace(/\/+$/, '');
    }
    const port = config?.server.port ?? 8899;
    return `http://localhost:${port}`;
  };

  return (
    <div className={styles.servicePage}>
      {/* Group List Sidebar */}
      <div className={styles.sidebar}>
        <div className={styles.groupList}>
          <div className={styles.groupListHeader}>
            <div className={styles.groupHeaderTitle}>
              <h3>{t('servicePage.groupPath')}</h3>
              <span className={styles.countBadge}>{groups.length}</span>
            </div>
            <Button
              variant="ghost"
              size="small"
              icon={Plus}
              onClick={() => setShowAddGroupModal(true)}
              title={t('header.addGroup')}
              aria-label={t('header.addGroup')}
            />
          </div>
          <div className={styles.groupListContent}>
            {groups.length === 0 ? (
              <div className={styles.emptyHint}>
                <p>{t('servicePage.noGroupsHint')}</p>
                <Button
                  variant="primary"
                  size="small"
                  icon={Plus}
                  onClick={() => setShowAddGroupModal(true)}
                >
                  {t('servicePage.createFirstGroup')}
                </Button>
              </div>
            ) : (
              <ul className={styles.groupItems}>
                {groups.map((group) => (
                  <li key={group.id}>
                    <button
                      type="button"
                      className={`${styles.groupItem} ${group.id === activeGroupId ? styles.active : ''}`}
                      onClick={() => handleSelectGroup(group.id)}
                    >
                      <span className={styles.groupName}>{group.name}</span>
                      <span className={styles.groupPath}>/{group.id}</span>
                      <span className={styles.groupRuleCount}>{group.rules.length}</span>
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </div>
        </div>
      </div>

      {/* Main Content */}
      <div className={styles.mainContent}>
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
                <div className={styles.groupMeta}>
                  <span className={styles.metaChip}>/{activeGroup.id}</span>
                  <span className={styles.metaChip}>
                    {t('servicePage.rulesCount', { count: activeGroup.rules.length })}
                  </span>
                  <span className={styles.metaChip}>
                    {t('servicePage.modelsCount', { count: activeGroupModels.length })}
                  </span>
                </div>
                <div className={styles.entryUrl}>
                  <code>{getEntryUrl()}</code>
                  <Button
                    variant="ghost"
                    size="small"
                    icon={Copy}
                    onClick={handleCopyEntryUrl}
                    title={t('servicePage.copyEntryUrl')}
                    aria-label={t('servicePage.copyEntryUrl')}
                  />
                </div>
              </div>
              <div className={styles.groupActions}>
                <Button
                  variant="default"
                  size="small"
                  icon={Pencil}
                  onClick={() => navigate(`/groups/${activeGroup.id}/edit`)}
                  title={t('servicePage.editGroup')}
                  aria-label={t('servicePage.editGroup')}
                />
                <Button
                  variant="danger"
                  size="small"
                  icon={Trash2}
                  onClick={() => setShowDeleteGroupModal(true)}
                  title={t('servicePage.deleteGroup')}
                  aria-label={t('servicePage.deleteGroup')}
                />
              </div>
            </div>

            {/* Rule List */}
            <RuleList
              rules={activeGroup.rules}
              activeRuleId={selectedRuleId ?? activeGroup.activeRuleId}
              onSelect={setSelectedRuleId}
              onDelete={handleRequestDeleteRule}
              groupName={activeGroup.name}
              groupId={activeGroup.id}
            />

            {/* Rule Detail (when rule is selected) */}
            {selectedRuleId && activeRule && (
              <div className={styles.ruleDetail}>
                <h3>{activeRule.name}</h3>
                <div className={styles.ruleInfo}>
                  <div className={styles.ruleInfoItem}>
                    <span className={styles.label}>{t('servicePage.ruleProtocol')}:</span>
                    <span>{t(`ruleProtocol.${activeRule.protocol}`)}</span>
                  </div>
                  <div className={styles.ruleInfoItem}>
                    <span className={styles.label}>{t('servicePage.apiAddress')}:</span>
                    <span>{activeRule.apiAddress}</span>
                  </div>
                  <div className={styles.ruleInfoItem}>
                    <span className={styles.label}>{t('servicePage.defaultModel')}:</span>
                    <span>{activeRule.defaultModel}</span>
                  </div>
                </div>
                <div className={styles.ruleActions}>
                  <Button
                    variant="danger"
                    size="small"
                    onClick={() => handleRequestDeleteRule(activeRule.id)}
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
            <label htmlFor="groupId">{t('modal.groupIdLabel')}</label>
            <Input
              id="groupId"
              value={newGroupId}
              onChange={(e) => setNewGroupId(e.target.value)}
              placeholder={t('modal.groupIdPlaceholder')}
            />
            <p className={styles.formHint}>{t('modal.groupIdHint', { id: newGroupId.trim() || 'group-id' })}</p>
          </div>
          <div className={styles.modalActions}>
            <Button variant="default" onClick={() => setShowAddGroupModal(false)}>
              {t('common.cancel')}
            </Button>
            <Button
              variant="primary"
              onClick={handleAddGroup}
              disabled={!newGroupName.trim() || !newGroupId.trim()}
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
            path: activeGroup?.id,
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

      {/* Delete Rule Modal */}
      <Modal
        open={showDeleteRuleModal}
        onClose={() => {
          setShowDeleteRuleModal(false);
          setPendingDeleteRuleId(null);
        }}
        title={t('deleteRuleModal.title')}
      >
        <div className={styles.modalContent}>
          <p>{t('deleteRuleModal.confirmText', {
            model: pendingDeleteRule?.name ?? '',
          })}</p>
          <div className={styles.modalActions}>
            <Button
              variant="default"
              onClick={() => {
                setShowDeleteRuleModal(false);
                setPendingDeleteRuleId(null);
              }}
            >
              {t('common.cancel')}
            </Button>
            <Button variant="danger" onClick={handleDeleteRule}>
              {t('deleteRuleModal.confirmDelete')}
            </Button>
          </div>
        </div>
      </Modal>
    </div>
  );
};

export default ServicePage;
