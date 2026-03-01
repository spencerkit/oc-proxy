import React, { useEffect, useMemo, useRef, useState } from 'react';
import { RefreshCw, Trash2 } from 'lucide-react';
import { useProxyStore } from '@/store';
import { Button, Modal } from '@/components';
import { useTranslation, useLogs } from '@/hooks';
import type { LogEntry } from '@/types';
import styles from './LogsPage.module.css';

/**
 * LogsPage Component
 * Request log viewer page
 */
export const LogsPage: React.FC = () => {
  const { t } = useTranslation();
  const { logs, refreshLogs, clearLogs, loading } = useProxyStore();
  const { showToast } = useLogs();
  const logsEndRef = useRef<HTMLDivElement>(null);
  const [statusFilter, setStatusFilter] = useState<'all' | LogEntry['status']>('all');
  const [showClearConfirm, setShowClearConfirm] = useState(false);
  const statusFilters: Array<'all' | LogEntry['status']> = ['all', 'error', 'processing', 'rejected', 'ok'];

  // Auto-scroll to bottom when new logs arrive
  useEffect(() => {
    if (statusFilter === 'all') {
      logsEndRef.current?.scrollIntoView({ behavior: 'smooth' });
    }
  }, [statusFilter, logs]);

  const handleRefresh = async () => {
    try {
      await refreshLogs();
      showToast(t('logs.refreshSuccess'), 'success');
    } catch {
      showToast(t('logs.refreshError'), 'error');
    }
  };

  const handleClear = async () => {
    try {
      await clearLogs();
      showToast(t('logs.clearSuccess'), 'success');
    } catch (error) {
      showToast(t('errors.operationFailed', { message: String(error) }), 'error');
    }
  };

  const formatTimestamp = (timestamp: string) => {
    const date = new Date(timestamp);
    return date.toLocaleTimeString();
  };

  const getStatusClass = (status: LogEntry['status']) => {
    switch (status) {
      case 'ok':
        return styles.statusOk;
      case 'error':
        return styles.statusError;
      case 'processing':
        return styles.statusProcessing;
      case 'rejected':
        return styles.statusRejected;
      default:
        return '';
    }
  };

  const filteredLogs = useMemo(() => {
    if (statusFilter === 'all') return logs;
    return logs.filter((log) => log.status === statusFilter);
  }, [logs, statusFilter]);

  const logSummary = useMemo(() => {
    const total = logs.length;
    const errorCount = logs.filter((log) => log.status === 'error').length;
    const okCount = logs.filter((log) => log.status === 'ok').length;
    const completed = logs.filter((log) => log.durationMs > 0);
    const avgDuration = completed.length > 0
      ? Math.round(completed.reduce((sum, log) => sum + log.durationMs, 0) / completed.length)
      : 0;
    const successRate = total > 0 ? Math.round((okCount / total) * 100) : 0;

    return { total, errorCount, avgDuration, successRate };
  }, [logs]);

  const getStatusText = (status: LogEntry['status']) => t(`logs.state.${status}`);

  const getFilterLabel = (filter: 'all' | LogEntry['status']) => {
    if (filter === 'all') return t('logs.filterAll');
    return getStatusText(filter);
  };

  const renderLogEntry = (log: LogEntry, index: number) => {
    return (
      <div key={`${log.timestamp}-${index}`} className={styles.logEntry}>
        <div className={styles.logHeader}>
          <span className={styles.timestamp}>{formatTimestamp(log.timestamp)}</span>
          <span className={styles.method}>{log.method}</span>
          <span className={styles.path}>{log.requestPath}</span>
          <span className={`${styles.status} ${getStatusClass(log.status)}`}>
            {t('logs.requestStatus', {
              status: log.httpStatus ?? '---',
              state: getStatusText(log.status),
            })}
          </span>
        </div>
        <div className={styles.logDetails}>
          {log.groupPath && (
            <div className={styles.logDetail}>
              <span className={styles.label}>{t('logs.group')}:</span>
              <span>{log.groupPath}</span>
            </div>
          )}
          {log.model && (
            <div className={styles.logDetail}>
              <span className={styles.label}>{t('logs.model')}:</span>
              <span>{log.model}</span>
            </div>
          )}
          {log.forwardingAddress ? (
            <div className={styles.logDetail}>
              <span className={styles.label}>{t('logs.forwardingTo')}:</span>
              <span>{log.forwardingAddress}</span>
            </div>
          ) : (
            <div className={styles.logDetail}>
              <span className={styles.label}>{t('logs.notForwarding')}</span>
            </div>
          )}
          {log.error && (
            <div className={`${styles.logDetail} ${styles.error}`}>
              <span className={styles.label}>{t('logs.errorReason', { reason: log.error.message })}</span>
            </div>
          )}
          {log.durationMs > 0 && (
            <div className={styles.logDetail}>
              <span className={styles.label}>{t('logs.duration')}:</span>
              <span>{log.durationMs}ms</span>
            </div>
          )}
        </div>
      </div>
    );
  };

  return (
    <div className={styles.logsPage}>
      <div className={styles.header}>
        <h2>{t('logs.title')}</h2>
        <p className={styles.subtitle}>
          {statusFilter === 'all'
            ? t('logs.recentLogs', { count: logs.length })
            : t('logs.filteredLogs', { shown: filteredLogs.length, total: logs.length })}
        </p>
      </div>

      <div className={styles.toolbar}>
        <div className={styles.toolbarActions}>
          <Button
            variant="default"
            icon={RefreshCw}
            onClick={handleRefresh}
            loading={loading}
          >
            {t('logs.refresh')}
          </Button>
          <Button
            variant="danger"
            icon={Trash2}
            onClick={() => setShowClearConfirm(true)}
            disabled={logs.length === 0}
          >
            {t('logs.clear')}
          </Button>
        </div>
        <div className={styles.filterGroup} role="group" aria-label={t('logs.filterByStatus')}>
          {statusFilters.map((filter) => (
            <button
              key={filter}
              type="button"
              className={`${styles.filterButton} ${statusFilter === filter ? styles.filterButtonActive : ''}`}
              onClick={() => setStatusFilter(filter)}
              aria-pressed={statusFilter === filter}
            >
              {getFilterLabel(filter)}
            </button>
          ))}
        </div>
      </div>

      <div className={styles.summaryGrid}>
        <div className={styles.summaryCard}>
          <span className={styles.summaryLabel}>{t('logs.totalRequests')}</span>
          <strong className={styles.summaryValue}>{logSummary.total}</strong>
        </div>
        <div className={styles.summaryCard}>
          <span className={styles.summaryLabel}>{t('logs.errorsCount')}</span>
          <strong className={`${styles.summaryValue} ${styles.summaryValueDanger}`}>{logSummary.errorCount}</strong>
        </div>
        <div className={styles.summaryCard}>
          <span className={styles.summaryLabel}>{t('logs.successRate')}</span>
          <strong className={styles.summaryValue}>{logSummary.successRate}%</strong>
        </div>
        <div className={styles.summaryCard}>
          <span className={styles.summaryLabel}>{t('logs.avgDuration')}</span>
          <strong className={styles.summaryValue}>{logSummary.avgDuration}ms</strong>
        </div>
      </div>

      <div className={styles.logsContainer}>
        {logs.length === 0 ? (
          <div className={styles.emptyState}>
            <p>{t('logs.noLogs')}</p>
          </div>
        ) : filteredLogs.length === 0 ? (
          <div className={styles.emptyState}>
            <p>{t('logs.noFilteredLogs')}</p>
            <div className={styles.emptyActions}>
              <Button variant="default" size="small" onClick={() => setStatusFilter('all')}>
                {t('logs.resetFilter')}
              </Button>
            </div>
          </div>
        ) : (
          <>
            {filteredLogs.map((log, index) => renderLogEntry(log, index))}
            <div ref={logsEndRef} />
          </>
        )}
      </div>

      <Modal
        open={showClearConfirm}
        onClose={() => setShowClearConfirm(false)}
        title={t('clearLogsModal.title')}
      >
        <div className={styles.modalContent}>
          <p>{t('clearLogsModal.confirmText', { count: logs.length })}</p>
          <div className={styles.modalActions}>
            <Button variant="default" onClick={() => setShowClearConfirm(false)}>
              {t('common.cancel')}
            </Button>
            <Button
              variant="danger"
              onClick={async () => {
                await handleClear();
                setShowClearConfirm(false);
              }}
            >
              {t('clearLogsModal.confirmClear')}
            </Button>
          </div>
        </div>
      </Modal>
    </div>
  );
};

export default LogsPage;
