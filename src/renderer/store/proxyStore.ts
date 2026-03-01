/**
 * Proxy Store - Zustand State Management
 *
 * Central store for managing OA Proxy state including:
 * - Configuration management
 * - Server status tracking
 * - Request logging
 * - Active group selection
 */

import { create } from 'zustand';
import type {
  ProxyConfig,
  ProxyStatus,
  LogEntry,
  Group,
} from '@/types';
import { ipc } from '@/utils/ipc';

/**
 * Proxy State Interface
 */
interface ProxyState {
  // State properties
  config: ProxyConfig | null;
  status: ProxyStatus | null;
  logs: LogEntry[];
  activeGroupId: string | null;
  loading: boolean;
  error: string | null;

  // Polling interval IDs
  statusIntervalId: number | null;
  logsIntervalId: number | null;

  // Actions
  init: () => Promise<void>;
  refreshStatus: () => Promise<void>;
  refreshLogs: () => Promise<void>;
  saveConfig: (config: ProxyConfig) => Promise<void>;
  setActiveGroupId: (groupId: string | null) => void;
  clearLogs: () => Promise<void>;
  startPolling: () => void;
  stopPolling: () => void;
  startServer: () => Promise<void>;
  stopServer: () => Promise<void>;
}

/**
 * Polling intervals (in milliseconds)
 */
const STATUS_POLL_INTERVAL = 3000;
const LOGS_POLL_INTERVAL = 3000;
const MAX_LOGS = 100;

/**
 * Create Zustand store for proxy state management
 */
export const useProxyStore = create<ProxyState>((set, get) => ({
  // Initial state
  config: null,
  status: null,
  logs: [],
  activeGroupId: null,
  loading: false,
  error: null,
  statusIntervalId: null,
  logsIntervalId: null,

  /**
   * Initialize store with initial data from IPC
   * Fetches config and status, then starts polling
   */
  init: async () => {
    try {
      console.log('[Store] Initializing...');
      set({ loading: true, error: null });

      console.log('[Store] Fetching config and status...');
      // Fetch initial config and status in parallel
      const [config, status] = await Promise.all([
        ipc.getConfig(),
        ipc.getStatus(),
      ]);

      console.log('[Store] Config received:', config);
      console.log('[Store] Status received:', status);

      set({
        config,
        status,
        loading: false,
      });

      console.log('[Store] Initialization complete');

      // Start polling for status and logs
      get().startPolling();
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Failed to initialize';
      console.error('[Store] Initialization error:', errorMessage);
      set({
        error: errorMessage,
        loading: false,
      });
    }
  },

  /**
   * Refresh server status from IPC
   */
  refreshStatus: async () => {
    try {
      const status = await ipc.getStatus();
      set({ status, error: null });
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Failed to refresh status';
      set({ error: errorMessage });
    }
  },

  /**
   * Refresh logs from IPC
   */
  refreshLogs: async () => {
    try {
      const logs = await ipc.listLogs(MAX_LOGS);
      set({ logs, error: null });
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Failed to refresh logs';
      set({ error: errorMessage });
    }
  },

  /**
   * Save configuration via IPC
   * Updates local config with the result
   */
  saveConfig: async (config: ProxyConfig) => {
    try {
      set({ loading: true, error: null });

      const result = await ipc.saveConfig(config);

      set({
        config: result.config,
        status: result.status,
        loading: false,
      });
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Failed to save configuration';
      set({
        error: errorMessage,
        loading: false,
      });
    }
  },

  /**
   * Set the active group ID
   */
  setActiveGroupId: (groupId: string | null) => {
    set({ activeGroupId: groupId });
  },

  /**
   * Clear all logs via IPC and in state
   */
  clearLogs: async () => {
    try {
      await ipc.clearLogs();
      set({ logs: [], error: null });
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Failed to clear logs';
      set({ error: errorMessage });
    }
  },

  /**
   * Start polling for status and logs
   * Sets up interval timers to refresh data periodically
   */
  startPolling: () => {
    const state = get();

    // Clear existing intervals if any
    if (state.statusIntervalId !== null) {
      window.clearInterval(state.statusIntervalId);
    }
    if (state.logsIntervalId !== null) {
      window.clearInterval(state.logsIntervalId);
    }

    // Set up status polling
    const statusIntervalId = window.setInterval(() => {
      get().refreshStatus();
    }, STATUS_POLL_INTERVAL);

    // Set up logs polling
    const logsIntervalId = window.setInterval(() => {
      get().refreshLogs();
    }, LOGS_POLL_INTERVAL);

    set({ statusIntervalId, logsIntervalId });
  },

  /**
   * Stop polling for status and logs
   * Clears interval timers
   */
  stopPolling: () => {
    const state = get();

    if (state.statusIntervalId !== null) {
      window.clearInterval(state.statusIntervalId);
      set({ statusIntervalId: null });
    }

    if (state.logsIntervalId !== null) {
      window.clearInterval(state.logsIntervalId);
      set({ logsIntervalId: null });
    }
  },

  /**
   * Start the proxy server via IPC
   */
  startServer: async () => {
    try {
      set({ loading: true, error: null });
      const status = await ipc.startServer();
      set({ status, loading: false });
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Failed to start server';
      set({ error: errorMessage, loading: false });
    }
  },

  /**
   * Stop the proxy server via IPC
   */
  stopServer: async () => {
    try {
      set({ loading: true, error: null });
      const status = await ipc.stopServer();
      set({ status, loading: false });
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Failed to stop server';
      set({ error: errorMessage, loading: false });
    }
  },
}));

/**
 * Selectors for common state queries
 */
export const proxySelectors = {
  /**
   * Check if proxy server is running

   */
  isRunning: (state: ProxyState) => state.status?.running ?? false,

  /**
   * Get active group object
   */
  activeGroup: (state: ProxyState) =>
    state.config?.groups.find((group: Group) => group.id === state.activeGroupId) ?? null,

  /**
   * Get total number of requests from status metrics
   */
  totalRequests: (state: ProxyState) => state.status?.metrics.requests ?? 0,

  /**
   * Get error count from status metrics
   */
  errorCount: (state: ProxyState) => state.status?.metrics.errors ?? 0,

  /**
   * Get uptime string from status metrics
   */
  uptime: (state: ProxyState) => {
    const startedAt = state.status?.metrics.uptimeStartedAt;
    if (!startedAt) return 'Not running';

    const uptime = Date.now() - new Date(startedAt).getTime();
    const seconds = Math.floor(uptime / 1000);
    const minutes = Math.floor(seconds / 60);
    const hours = Math.floor(minutes / 60);

    if (hours > 0) {
      return `${hours}h ${minutes % 60}m`;
    } else if (minutes > 0) {
      return `${minutes}m ${seconds % 60}s`;
    } else {
      return `${seconds}s`;
    }
  },
};
