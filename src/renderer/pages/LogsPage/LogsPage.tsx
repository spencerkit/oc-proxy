import React, { useEffect, useRef } from 'react';
import { RefreshCw, Trash2 } from 'lucide-react';
import { useProxyStore } from '@/store';
import { Button } from '@/components';
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

  // Auto-scroll to bottom when new logs arrive
  useEffect(() => {
    logsEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [logs]);

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
    } catch {
      showToast(t('errors.operationFailed'), 'error');
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
              state: log.status,
            })}
          </span>
        </div>
        <div className={styles.logDetails}>
          {log.groupPath && (
            <div className={styles.logDetail}>
              <span className={styles.label}>Group:</span>
              <span>{log.groupPath}</span>
            </div>
          )}
          {log.model && (
            <div className={styles.logDetail}>
              <span className={styles.label}>Model:</span>
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
              <span className={styles.label}>Duration:</span>
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
          {t('logs.recentLogs', { count: logs.length })}
        </p>
      </div>

      <div className={styles.toolbar}>
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
          onClick={handleClear}
          disabled={logs.length === 0}
        >
          {t('logs.clear')}
        </Button>
      </div>

      <div className={styles.logsContainer}>
        {logs.length === 0 ? (
          <div className={styles.emptyState}>
            <p>{t('logs.noLogs')}</p>
          </div>
        ) : (
          <>
            {logs.map((log, index) => renderLogEntry(log, index))}
            <div ref={logsEndRef} />
          </>
        )}
      </div>
    </div>
  );
};

export default LogsPage;
