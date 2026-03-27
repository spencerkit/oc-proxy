import type { AgentConfig, IntegrationClientKind, OpenClawEditorConfig } from "../../types"

export function buildAgentEditFormState(
  kind: IntegrationClientKind,
  parsed?: AgentConfig | null,
  openclawEditor?: OpenClawEditorConfig | null
): AgentConfig {
  if (kind === "openclaw") {
    return {
      agentId: openclawEditor?.agentId ?? parsed?.agentId ?? "",
      providerId: openclawEditor?.providerId ?? parsed?.providerId ?? "",
      url: openclawEditor?.baseUrl ?? parsed?.url ?? "",
      apiToken: openclawEditor?.apiKey ?? parsed?.apiToken ?? "",
      apiFormat: openclawEditor?.apiFormat ?? parsed?.apiFormat ?? "",
      model: openclawEditor?.primaryModel ?? parsed?.model ?? "",
      fallbackModels: openclawEditor?.fallbackModels ?? parsed?.fallbackModels ?? [],
      timeout: parsed?.timeout,
      alwaysThinkingEnabled: false,
      includeCoAuthoredBy: false,
      skipDangerousModePermissionPrompt: false,
    }
  }

  return {
    agentId: parsed?.agentId ?? "",
    providerId: parsed?.providerId ?? "",
    url: parsed?.url ?? "",
    apiToken: parsed?.apiToken ?? "",
    apiFormat: parsed?.apiFormat ?? "",
    model: parsed?.model ?? "",
    fallbackModels: parsed?.fallbackModels ?? [],
    timeout: parsed?.timeout,
    alwaysThinkingEnabled: parsed?.alwaysThinkingEnabled ?? false,
    includeCoAuthoredBy: parsed?.includeCoAuthoredBy ?? false,
    skipDangerousModePermissionPrompt: parsed?.skipDangerousModePermissionPrompt ?? false,
  }
}
