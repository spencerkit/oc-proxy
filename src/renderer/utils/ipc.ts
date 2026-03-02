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
  SaveConfigResult,
  StatsSummaryResult,
} from "@/types"

type InvokeFn = <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>

function getInvoke(): InvokeFn {
  const invoke =
    (window.__TAURI__?.core?.invoke as InvokeFn | undefined) ??
    (window.__TAURI_INTERNALS__?.invoke as InvokeFn | undefined)
  if (!invoke) {
    throw new Error("Tauri invoke is unavailable. Run this app inside Tauri runtime.")
  }
  return invoke
}

export const ipc = {
  getAppInfo(): Promise<AppInfo> {
    return getInvoke()<AppInfo>("app_get_info")
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

  getLogsStatsSummary(hours?: number, ruleKey?: string): Promise<StatsSummaryResult> {
    const args: Record<string, unknown> = {}
    if (typeof hours === "number") args.hours = hours
    if (typeof ruleKey === "string") args.ruleKey = ruleKey
    return getInvoke()<StatsSummaryResult>("logs_stats_summary", args)
  },

  clearLogsStats(): Promise<{ ok: boolean }> {
    return getInvoke()<{ ok: boolean }>("logs_stats_clear")
  },
}
