/**
 * useProxyConfig Hook
 *
 * Custom hook for managing proxy configuration.
 * Provides config selector and saveConfig action.
 */

import { configState, lastOperationErrorState, saveConfigAction, savingConfigState } from "@/store"
import type { ProxyConfig } from "@/types"
import { useActions, useRelaxValue } from "@/utils/relax"

const CONFIG_ACTIONS = [saveConfigAction] as const

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
  const config = useRelaxValue(configState)
  const savingConfig = useRelaxValue(savingConfigState)
  const error = useRelaxValue(lastOperationErrorState)
  const [saveConfig] = useActions(CONFIG_ACTIONS)

  return {
    config,
    saveConfig,
    loading: savingConfig,
    error,
    savingConfig,
  }
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
  return useRelaxValue(configState)
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
  const [saveConfig] = useActions(CONFIG_ACTIONS)
  return saveConfig
}
