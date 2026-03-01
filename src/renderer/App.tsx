import React, { useEffect } from 'react';
import { Routes, Route, Navigate } from 'react-router-dom';
import { Layout } from '@/components';
import { ServicePage, SettingsPage, LogsPage, RuleEditPage, RuleCreatePage, GroupEditPage } from '@/pages';
import { useProxyStore } from '@/store';
import { useTranslation } from '@/hooks';

/**
 * Main App Component
 * Sets up routing and initializes the store
 */
const App: React.FC = () => {
  console.log('[App] Rendering...');

  let store;
  try {
    console.log('[App] Calling useProxyStore...');
    store = useProxyStore();
    console.log('[App] useProxyStore returned:', store);
  } catch (e) {
    console.error('[App] useProxyStore error:', e);
  }

  let t: ((key: string) => string) | undefined;
  try {
    console.log('[App] Calling useTranslation...');
    const result = useTranslation();
    t = result?.t;
    console.log('[App] useTranslation returned, t:', typeof t);
  } catch (e) {
    console.error('[App] useTranslation error:', e);
  }

  // Fallback translation function
  const translate = (key: string) => {
    if (typeof t === 'function') {
      try {
        return t(key);
      } catch {
        return key;
      }
    }
    return key;
  };

  const { init, loading, error, status, startServer, stopServer, config } = store || {
    init: () => {},
    loading: false,
    error: null,
    status: null,
    config: null,
    startServer: () => {},
    stopServer: () => {}
  };

  const isRunning = status?.running ?? false;
  const serverAddress = status?.address && config?.server.port
    ? `http://${status.address}:${config.server.port}`
    : undefined;

  console.log('[App] loading:', loading, 'error:', error, 'status:', status);

  useEffect(() => {
    console.log('[App] useEffect running, calling init()...');
    init();
  }, [init]);

  console.log('[App] About to render layout');

  if (loading && !error) {
    console.log('[App] Showing loading screen');
    return (
      <div className="loading-screen">
        <p>{translate('app.statusLoading')}</p>
      </div>
    );
  }

  if (error) {
    console.log('[App] Showing error screen:', error);
    return (
      <div className="error-screen">
        <p>{error}</p>
      </div>
    );
  }

  console.log('[App] Rendering routes');
  return (
    <Layout
      isRunning={isRunning}
      serverAddress={serverAddress}
      onStartServer={startServer}
      onStopServer={stopServer}
    >
      <Routes>
        <Route path="/" element={<ServicePage />} />
        <Route path="/settings" element={<SettingsPage />} />
        <Route path="/logs" element={<LogsPage />} />
        <Route path="/groups/:groupId/edit" element={<GroupEditPage />} />
        <Route path="/groups/:groupId/rules/new" element={<RuleCreatePage />} />
        <Route path="/groups/:groupId/rules/:ruleId/edit" element={<RuleEditPage />} />
        <Route path="*" element={<Navigate to="/" replace />} />
      </Routes>
    </Layout>
  );
};

export default App;
