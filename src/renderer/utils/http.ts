import type {
  AgentConfig,
  AgentConfigFile,
  AppInfo,
  AuthSessionStatus,
  ClipboardTextResult,
  GroupBackupExportResult,
  GroupBackupImportResult,
  GroupsExportJsonResult,
  IntegrationClientKind,
  IntegrationTarget,
  IntegrationWriteResult,
  LogEntry,
  ProviderModelTestResult,
  ProxyConfig,
  ProxyStatus,
  RemoteRulesPullResult,
  RemoteRulesUploadResult,
  RuleCardStatsItem,
  RuleQuotaConfig,
  RuleQuotaSnapshot,
  RuleQuotaTestResult,
  SaveConfigResult,
  StatsDimension,
  StatsSummaryResult,
  WriteAgentConfigResult,
} from "@/types"

type HttpMethod = "GET" | "POST" | "PUT" | "DELETE"

const HTTP_PROTOCOLS = new Set(["http:", "https:"])

function normalizeBaseUrl(baseUrl: string): string {
  return baseUrl.replace(/\/+$/, "")
}

function resolveOverrideBaseUrl(): string | null {
  const override = window.__AOR_HTTP_BASE__
  if (typeof override !== "string") return null
  const trimmed = override.trim()
  if (!trimmed) return null
  try {
    if (!HTTP_PROTOCOLS.has(new URL(trimmed).protocol)) return null
  } catch {
    return null
  }
  return normalizeBaseUrl(trimmed)
}

function resolveOriginBaseUrl(): string | null {
  if (!HTTP_PROTOCOLS.has(window.location.protocol)) return null
  return normalizeBaseUrl(window.location.origin)
}

export function resolveHttpBaseUrl(): string | null {
  return resolveOverrideBaseUrl() ?? resolveOriginBaseUrl()
}

export function isHttpCandidate(): boolean {
  return resolveHttpBaseUrl() !== null
}

function buildQuery(params: Record<string, unknown>): string {
  const search = new URLSearchParams()
  for (const [key, value] of Object.entries(params)) {
    if (value === undefined || value === null) continue
    if (Array.isArray(value)) {
      for (const entry of value) {
        if (entry === undefined || entry === null) continue
        search.append(key, String(entry))
      }
      continue
    }
    search.set(key, String(value))
  }
  const text = search.toString()
  return text ? `?${text}` : ""
}

async function parseJsonSafely(text: string): Promise<unknown> {
  if (!text.trim()) return null
  try {
    return JSON.parse(text)
  } catch {
    return null
  }
}

async function resolveErrorMessage(response: Response, text: string): Promise<string> {
  const payload = await parseJsonSafely(text)
  if (payload && typeof payload === "object") {
    const maybeError =
      "error" in payload && typeof payload.error === "object" && payload.error
        ? (payload.error as { message?: unknown }).message
        : "message" in payload
          ? (payload as { message?: unknown }).message
          : null
    if (typeof maybeError === "string" && maybeError.trim()) {
      return maybeError
    }
  }
  if (text.trim()) return text
  return response.statusText || `HTTP ${response.status}`
}

async function request<T>(method: HttpMethod, path: string, body?: unknown): Promise<T> {
  const baseUrl = resolveHttpBaseUrl()
  if (!baseUrl) {
    throw new Error("HTTP base URL is unavailable")
  }
  const url = `${baseUrl}${path}`
  const headers: Record<string, string> = {}
  let payload: string | undefined
  if (body !== undefined) {
    headers["content-type"] = "application/json"
    payload = JSON.stringify(body)
  }
  const response = await fetch(url, {
    method,
    credentials: "include",
    headers,
    body: payload,
  })
  const text = await response.text()
  if (!response.ok) {
    const message = await resolveErrorMessage(response, text)
    throw new Error(message)
  }
  if (!text.trim()) {
    return undefined as T
  }
  const parsed = await parseJsonSafely(text)
  return parsed as T
}

async function requestExportJson(): Promise<GroupsExportJsonResult> {
  return request<GroupsExportJsonResult>("GET", "/api/config/groups/export-json")
}

function triggerDownload(text: string, fileName: string): void {
  const blob = new Blob([text], { type: "application/json" })
  const url = URL.createObjectURL(blob)
  const link = document.createElement("a")
  link.href = url
  link.download = fileName || "ai-open-router-groups-backup.json"
  document.body.appendChild(link)
  link.click()
  link.remove()
  URL.revokeObjectURL(url)
}

async function copyTextToClipboard(text: string): Promise<void> {
  if (navigator.clipboard?.writeText) {
    await navigator.clipboard.writeText(text)
    return
  }

  const textarea = document.createElement("textarea")
  textarea.value = text
  textarea.setAttribute("readonly", "true")
  textarea.style.position = "fixed"
  textarea.style.top = "-1000px"
  textarea.style.opacity = "0"
  document.body.appendChild(textarea)
  textarea.select()
  const ok = document.execCommand("copy")
  textarea.remove()
  if (!ok) {
    throw new Error("Clipboard write is unavailable")
  }
}

async function readClipboardTextFromBrowser(): Promise<string> {
  if (!navigator.clipboard?.readText) {
    throw new Error("Clipboard read is unavailable")
  }
  return navigator.clipboard.readText()
}

async function pickJsonFileText(): Promise<string | null> {
  type OpenFilePicker = (
    options?: unknown
  ) => Promise<Array<{ getFile: () => Promise<{ text: () => Promise<string> }> }>>
  const openFilePicker = (window as Window & { showOpenFilePicker?: OpenFilePicker })
    .showOpenFilePicker

  if (typeof openFilePicker === "function") {
    try {
      const [handle] = await openFilePicker({
        multiple: false,
        types: [
          {
            description: "JSON",
            accept: {
              "application/json": [".json"],
            },
          },
        ],
      })
      const file = await handle.getFile()
      return file.text()
    } catch (error) {
      if (error instanceof DOMException && error.name === "AbortError") {
        return null
      }
      throw error
    }
  }

  return new Promise((resolve, reject) => {
    const input = document.createElement("input")
    input.type = "file"
    input.accept = "application/json,.json"
    let settled = false

    const cleanup = () => {
      window.removeEventListener("focus", handleWindowFocus)
      input.remove()
    }

    const resolveOnce = (value: string | null) => {
      if (settled) return
      settled = true
      cleanup()
      resolve(value)
    }

    const rejectOnce = (error: Error) => {
      if (settled) return
      settled = true
      cleanup()
      reject(error)
    }

    const handleWindowFocus = () => {
      window.setTimeout(() => {
        if (!settled && !input.files?.length) {
          resolveOnce(null)
        }
      }, 0)
    }

    input.onchange = () => {
      const file = input.files?.[0]
      if (!file) {
        resolveOnce(null)
        return
      }
      const reader = new FileReader()
      reader.onload = () => {
        resolveOnce(typeof reader.result === "string" ? reader.result : "")
      }
      reader.onerror = () => {
        rejectOnce(new Error("Read file failed"))
      }
      reader.readAsText(file)
    }
    input.oncancel = () => resolveOnce(null)
    document.body.appendChild(input)
    window.addEventListener("focus", handleWindowFocus, { once: true })
    input.click()
  })
}

export const httpApi = {
  getAuthSession(): Promise<AuthSessionStatus> {
    return request<AuthSessionStatus>("GET", "/api/auth/session")
  },

  loginRemoteAdmin(password: string): Promise<AuthSessionStatus> {
    return request<AuthSessionStatus>("POST", "/api/auth/login", { password })
  },

  logoutRemoteAdmin(): Promise<AuthSessionStatus> {
    return request<AuthSessionStatus>("POST", "/api/auth/logout")
  },

  getAppInfo(): Promise<AppInfo> {
    return request<AppInfo>("GET", "/api/app/info")
  },

  reportRendererReady(): Promise<void> {
    return request<void>("POST", "/api/app/renderer-ready")
  },

  reportRendererError(payload: {
    kind: string
    message: string
    stack?: string
    source?: string
  }): Promise<void> {
    return request<void>("POST", "/api/app/renderer-error", payload)
  },

  getStatus(): Promise<ProxyStatus> {
    return request<ProxyStatus>("GET", "/api/app/status")
  },

  async readClipboardText(): Promise<ClipboardTextResult> {
    const text = await readClipboardTextFromBrowser()
    return { text }
  },

  startServer(): Promise<ProxyStatus> {
    return request<ProxyStatus>("POST", "/api/app/server/start")
  },

  stopServer(): Promise<ProxyStatus> {
    return request<ProxyStatus>("POST", "/api/app/server/stop")
  },

  getConfig(): Promise<ProxyConfig> {
    return request<ProxyConfig>("GET", "/api/config")
  },

  saveConfig(config: ProxyConfig): Promise<SaveConfigResult> {
    return request<SaveConfigResult>("PUT", "/api/config", { nextConfig: config })
  },

  setRemoteAdminPassword(password: string): Promise<AuthSessionStatus> {
    return request<AuthSessionStatus>("PUT", "/api/config/remote-admin-password", { password })
  },

  clearRemoteAdminPassword(): Promise<AuthSessionStatus> {
    return request<AuthSessionStatus>("DELETE", "/api/config/remote-admin-password")
  },

  async exportGroupsBackup(): Promise<GroupBackupExportResult> {
    const payload = await requestExportJson()
    triggerDownload(payload.text, payload.fileName)
    return {
      ok: true,
      canceled: false,
      source: "file",
      filePath: null,
      groupCount: payload.groupCount,
      charCount: payload.charCount,
    }
  },

  async exportGroupsToFolder(): Promise<GroupBackupExportResult> {
    const payload = await requestExportJson()
    triggerDownload(payload.text, payload.fileName)
    return {
      ok: true,
      canceled: false,
      source: "folder",
      filePath: null,
      groupCount: payload.groupCount,
      charCount: payload.charCount,
    }
  },

  async exportGroupsToClipboard(): Promise<GroupBackupExportResult> {
    const payload = await requestExportJson()
    await copyTextToClipboard(payload.text)
    return {
      ok: true,
      canceled: false,
      source: "clipboard",
      filePath: null,
      groupCount: payload.groupCount,
      charCount: payload.charCount,
    }
  },

  async importGroupsBackup(): Promise<GroupBackupImportResult> {
    const jsonText = await pickJsonFileText()
    if (jsonText === null) {
      return {
        ok: true,
        canceled: true,
        source: "file",
        importedGroupCount: undefined,
      }
    }
    return request<GroupBackupImportResult>("POST", "/api/config/groups/import-json", { jsonText })
  },

  importGroupsFromJson(jsonText: string): Promise<GroupBackupImportResult> {
    return request<GroupBackupImportResult>("POST", "/api/config/groups/import-json", { jsonText })
  },

  remoteRulesUpload(force?: boolean): Promise<RemoteRulesUploadResult> {
    return request<RemoteRulesUploadResult>("POST", "/api/config/remote-rules/upload", { force })
  },

  remoteRulesPull(force?: boolean): Promise<RemoteRulesPullResult> {
    return request<RemoteRulesPullResult>("POST", "/api/config/remote-rules/pull", { force })
  },

  listLogs(max?: number): Promise<LogEntry[]> {
    const query = buildQuery({ max })
    return request<LogEntry[]>("GET", `/api/logs${query}`)
  },

  clearLogs(): Promise<{ ok: boolean }> {
    return request<{ ok: boolean }>("DELETE", "/api/logs")
  },

  getLogsStatsSummary(
    hours?: number,
    ruleKeys?: string[],
    ruleKey?: string,
    dimension?: StatsDimension,
    enableComparison?: boolean
  ): Promise<StatsSummaryResult> {
    const query = buildQuery({
      hours,
      ruleKeys,
      ruleKey,
      dimension,
      enableComparison,
    })
    return request<StatsSummaryResult>("GET", `/api/logs/stats/summary${query}`)
  },

  getRuleCardStats(groupId: string, hours?: number): Promise<RuleCardStatsItem[]> {
    const query = buildQuery({ groupId, hours })
    return request<RuleCardStatsItem[]>("GET", `/api/logs/stats/rule-cards${query}`)
  },

  clearLogsStats(beforeEpochMs?: number): Promise<{ ok: boolean }> {
    const query = buildQuery({ beforeEpochMs })
    return request<{ ok: boolean }>("DELETE", `/api/logs/stats${query}`)
  },

  getProviderQuota(groupId: string, providerId: string): Promise<RuleQuotaSnapshot> {
    const query = buildQuery({ groupId, ruleId: providerId })
    return request<RuleQuotaSnapshot>("GET", `/api/quota/rule${query}`)
  },

  getGroupQuotas(groupId: string): Promise<RuleQuotaSnapshot[]> {
    const query = buildQuery({ groupId })
    return request<RuleQuotaSnapshot[]>("GET", `/api/quota/group${query}`)
  },

  testRuleQuotaDraft(
    groupId: string,
    providerName: string,
    providerToken: string,
    providerApiAddress: string,
    providerDefaultModel: string,
    quota: RuleQuotaConfig
  ): Promise<RuleQuotaTestResult> {
    return request<RuleQuotaTestResult>("POST", "/api/quota/test-draft", {
      groupId,
      ruleName: providerName,
      ruleToken: providerToken,
      ruleApiAddress: providerApiAddress,
      ruleDefaultModel: providerDefaultModel,
      quota,
    })
  },

  testProviderModel(
    groupId: string | undefined,
    providerId: string
  ): Promise<ProviderModelTestResult> {
    return request<ProviderModelTestResult>("POST", "/api/provider/test-model", {
      groupId,
      providerId,
    })
  },

  integrationListTargets(): Promise<IntegrationTarget[]> {
    return request<IntegrationTarget[]>("GET", "/api/integration/targets")
  },

  integrationPickDirectory(
    initialDir?: string,
    kind?: IntegrationClientKind
  ): Promise<string | null> {
    return request<string | null>("POST", "/api/integration/pick-directory", {
      initialDir,
      kind,
    })
  },

  integrationAddTarget(kind: IntegrationClientKind, configDir: string): Promise<IntegrationTarget> {
    return request<IntegrationTarget>("POST", "/api/integration/targets", {
      kind,
      configDir,
    })
  },

  integrationUpdateTarget(targetId: string, configDir: string): Promise<IntegrationTarget> {
    return request<IntegrationTarget>("PUT", "/api/integration/targets", {
      targetId,
      configDir,
    })
  },

  integrationRemoveTarget(targetId: string): Promise<{ ok: boolean; removed: boolean }> {
    const query = buildQuery({ targetId })
    return request<{ ok: boolean; removed: boolean }>("DELETE", `/api/integration/targets${query}`)
  },

  integrationWriteGroupEntry(
    groupId: string,
    targetIds: string[]
  ): Promise<IntegrationWriteResult> {
    return request<IntegrationWriteResult>("POST", "/api/integration/write-group-entry", {
      groupId,
      targetIds,
    })
  },

  integrationReadAgentConfig(targetId: string): Promise<AgentConfigFile> {
    const query = buildQuery({ targetId })
    return request<AgentConfigFile>("GET", `/api/integration/agent-config${query}`)
  },

  integrationWriteAgentConfig(
    targetId: string,
    config: AgentConfig
  ): Promise<WriteAgentConfigResult> {
    return request<WriteAgentConfigResult>("PUT", "/api/integration/agent-config", {
      targetId,
      config,
    })
  },

  integrationWriteAgentConfigSource(
    targetId: string,
    content: string,
    sourceId?: string
  ): Promise<WriteAgentConfigResult> {
    return request<WriteAgentConfigResult>("PUT", "/api/integration/agent-config/source", {
      targetId,
      content,
      sourceId,
    })
  },
}
