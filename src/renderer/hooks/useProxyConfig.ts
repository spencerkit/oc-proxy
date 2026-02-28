/**
 * useProxyConfig Hook
 *
 * Custom hook for managing proxy configuration.
 * Provides config selector and saveConfig action.
 */

import { useEffect } from 'react';
import { useProxyStore } from '@/store';
import type { ProxyConfig } from '@/types';

/**
 * Hook for accessing proxy configuration
 *
 * @returns Object containing config and saveConfig action
 *
 * @example
 * const { config, saveConfig } = useProxyConfig();
 *
 * // Get config values
 * const serverHost = config?.server.host;
 *
 * // Save configuration
 * await saveConfig(updatedConfig);
 */
export function useProxyConfig() {
  const config = useProxyStore((state) => state.config);
  const saveConfig = useProxyStore((state) => state.saveConfig);
  const loading = useProxyStore((state) => state.loading);
  const error = useProxyStore((state) => state.error);

  return {
    config,
    saveConfig,
    loading,
    error,
  };
}

/**
 * Hook for accessing only the config value (selector-based)
 * Useful when you only need to read configuration
 *
 * @returns Current proxy configuration or null
 *
 * @example
 * const config = useConfigValue();
 * const port = config?.server.port;
 */
export function useConfigValue(): ProxyConfig | null {
  return useProxyStore((state) => state.config);
}

/**
 * Hook for accessing the saveConfig action only
 * Useful when you only need to save configuration
 *
 * @returns Function to save configuration
 *
 * @example
 * const saveConfig = useSaveConfigAction();
 * await saveConfig(updatedConfig);
 */
export function useSaveConfigAction(): (config: ProxyConfig) => Promise<void> {
  return useProxyStore((state) => state.saveConfig);
}
