/**
 * IPC Utility Module for OA Proxy
 *
 * Provides a centralized interface for all IPC communication with the main process.
 * Maps to window.proxyApp methods exposed via the preload script.
 */

import type {
  ProxyConfig,
  ProxyStatus,
  LogEntry,
  SaveConfigResult,
} from '@/types';

/**
 * IPC utility object containing all methods for main process communication
 */
export const ipc = {
  /**
   * Get the current proxy server status
   * @returns Promise resolving to server status with metrics
   */
  getStatus(): Promise<ProxyStatus> {
    console.log('[IPC] getStatus called');
    return window.proxyApp.getStatus();
  },

  /**
   * Start the proxy server
   * @returns Promise resolving to server status with metrics
   */
  startServer(): Promise<ProxyStatus> {
    console.log('[IPC] startServer called');
    return window.proxyApp.startServer();
  },

  /**
   * Stop the proxy server
   * @returns Promise resolving to server status with metrics
   */
  stopServer(): Promise<ProxyStatus> {
    console.log('[IPC] stopServer called');
    return window.proxyApp.stopServer();
  },

  /**
   * Get the current proxy configuration
   * @returns Promise resolving to proxy configuration
   */
  getConfig(): Promise<ProxyConfig> {
    console.log('[IPC] getConfig called');
    return window.proxyApp.getConfig();
  },

  /**
   * Save the proxy configuration
   * @param config - The proxy configuration to save
   * @returns Promise resolving to save result with status
   */
  saveConfig(config: ProxyConfig): Promise<SaveConfigResult> {
    console.log('[IPC] saveConfig called');
    return window.proxyApp.saveConfig(config);
  },

  /**
   * List log entries
   * @param max - Maximum number of log entries to return (optional)
   * @returns Promise resolving to array of log entries
   */
  listLogs(max?: number): Promise<LogEntry[]> {
    console.log('[IPC] listLogs called', max);
    return window.proxyApp.listLogs(max);
  },

  /**
   * Clear all log entries
   * @returns Promise resolving to success status
   */
  clearLogs(): Promise<{ ok: boolean }> {
    console.log('[IPC] clearLogs called');
    return window.proxyApp.clearLogs();
  },
};
