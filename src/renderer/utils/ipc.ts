import type {
  AppInfo,
  ClipboardTextResult,
  GroupBackupExportResult,
  GroupBackupImportResult,
  LogEntry,
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
} from "@/types"

type InvokeFn = <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>

/** Returns optional invoke function for best-effort telemetry calls. */
function getOptionalInvoke(): InvokeFn | undefined {
  return (
    (window.__TAURI__?.core?.invoke as InvokeFn | undefined) ??
    (window.__TAURI_INTERNALS__?.invoke as InvokeFn | undefined)
  )
}

/** Implements invoke behavior. */
function getInvoke(): InvokeFn {
  const invoke = getOptionalInvoke()
  if (!invoke) {
    throw new Error("Tauri invoke is unavailable. Run this app inside Tauri runtime.")
  }
  return invoke
}

export const ipc = {
  getAppInfo(): Promise<AppInfo> {
    return getInvoke()<AppInfo>("app_get_info")
  },

  reportRendererReady(): Promise<void> {
    const invoke = getOptionalInvoke()
    if (!invoke) {
      return Promise.resolve()
    }
    return invoke<void>("app_renderer_ready")
  },

  reportRendererError(payload: {
    kind: string
    message: string
    stack?: string
    source?: string
  }): Promise<void> {
    const invoke = getOptionalInvoke()
    if (!invoke) {
      return Promise.resolve()
    }
    return invoke<void>("app_report_renderer_error", payload)
  },

  getStatus(): Promise<ProxyStatus> {
    return getInvoke()<ProxyStatus>("app_get_status")
  },

  readClipboardText(): Promise<ClipboardTextResult> {
    return getInvoke()<ClipboardTextResult>("app_read_clipboard_text")
  },

  startServer(): Promise<ProxyStatus> {
    return getInvoke()<ProxyStatus>("app_start_server")
  },

  stopServer(): Promise<ProxyStatus> {
    return getInvoke()<ProxyStatus>("app_stop_server")
  },

  getConfig(): Promise<ProxyConfig> {
    return getInvoke()<ProxyConfig>("config_get")
  },

  saveConfig(config: ProxyConfig): Promise<SaveConfigResult> {
    return getInvoke()<SaveConfigResult>("config_save", { nextConfig: config })
  },

  exportGroupsBackup(): Promise<GroupBackupExportResult> {
    return getInvoke()<GroupBackupExportResult>("config_export_groups")
  },

  exportGroupsToFolder(): Promise<GroupBackupExportResult> {
    return getInvoke()<GroupBackupExportResult>("config_export_groups_folder")
  },

  exportGroupsToClipboard(): Promise<GroupBackupExportResult> {
    return getInvoke()<GroupBackupExportResult>("config_export_groups_clipboard")
  },

  importGroupsBackup(): Promise<GroupBackupImportResult> {
    return getInvoke()<GroupBackupImportResult>("config_import_groups")
  },

  importGroupsFromJson(jsonText: string): Promise<GroupBackupImportResult> {
    return getInvoke()<GroupBackupImportResult>("config_import_groups_json", { jsonText })
  },

  remoteRulesUpload(force?: boolean): Promise<RemoteRulesUploadResult> {
    const args: Record<string, unknown> = {}
    if (typeof force === "boolean") args.force = force
    return getInvoke()<RemoteRulesUploadResult>("config_remote_rules_upload", args)
  },

  remoteRulesPull(force?: boolean): Promise<RemoteRulesPullResult> {
    const args: Record<string, unknown> = {}
    if (typeof force === "boolean") args.force = force
    return getInvoke()<RemoteRulesPullResult>("config_remote_rules_pull", args)
  },

  listLogs(max?: number): Promise<LogEntry[]> {
    if (typeof max === "number") {
      return getInvoke()<LogEntry[]>("logs_list", { max })
    }
    return getInvoke()<LogEntry[]>("logs_list")
  },

  clearLogs(): Promise<{ ok: boolean }> {
    return getInvoke()<{ ok: boolean }>("logs_clear")
  },

  getLogsStatsSummary(
    hours?: number,
    ruleKeys?: string[],
    ruleKey?: string,
    dimension?: StatsDimension,
    enableComparison?: boolean
  ): Promise<StatsSummaryResult> {
    const args: Record<string, unknown> = {}
    if (typeof hours === "number") args.hours = hours
    if (Array.isArray(ruleKeys)) args.ruleKeys = ruleKeys
    if (typeof ruleKey === "string") args.ruleKey = ruleKey
    if (typeof dimension === "string") args.dimension = dimension
    if (typeof enableComparison === "boolean") args.enableComparison = enableComparison
    return getInvoke()<StatsSummaryResult>("logs_stats_summary", args)
  },

  getRuleCardStats(groupId: string, hours?: number): Promise<RuleCardStatsItem[]> {
    const args: Record<string, unknown> = { groupId }
    if (typeof hours === "number") args.hours = hours
    return getInvoke()<RuleCardStatsItem[]>("logs_stats_rule_cards", args)
  },

  clearLogsStats(): Promise<{ ok: boolean }> {
    return getInvoke()<{ ok: boolean }>("logs_stats_clear")
  },

  getProviderQuota(groupId: string, providerId: string): Promise<RuleQuotaSnapshot> {
    return getInvoke()<RuleQuotaSnapshot>("quota_get_rule", { groupId, ruleId: providerId })
  },

  getGroupQuotas(groupId: string): Promise<RuleQuotaSnapshot[]> {
    return getInvoke()<RuleQuotaSnapshot[]>("quota_get_group", { groupId })
  },

  testRuleQuotaDraft(
    groupId: string,
    providerName: string,
    providerToken: string,
    providerApiAddress: string,
    providerDefaultModel: string,
    quota: RuleQuotaConfig
  ): Promise<RuleQuotaTestResult> {
    return getInvoke()<RuleQuotaTestResult>("quota_test_draft", {
      groupId,
      ruleName: providerName,
      ruleToken: providerToken,
      ruleApiAddress: providerApiAddress,
      ruleDefaultModel: providerDefaultModel,
      quota,
    })
  },
}
