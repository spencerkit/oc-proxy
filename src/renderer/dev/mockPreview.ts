import type { AuthSessionStatus, ProxyConfig, ProxyStatus, StatsSummaryResult } from "@/types"

type MockPreviewWindow = Window & typeof globalThis

type MockPreviewState = {
  authSession: AuthSessionStatus
  config: ProxyConfig
  status: ProxyStatus
  stats: StatsSummaryResult
}

function clone<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T
}

function createMockPreviewState(): MockPreviewState {
  return {
    authSession: {
      authenticated: true,
      remoteRequest: false,
      passwordConfigured: false,
    },
    config: {
      server: {
        host: "0.0.0.0",
        port: 8899,
        authEnabled: false,
        localBearerToken: "",
      },
      compat: {
        strictMode: false,
        textToolCallFallbackEnabled: true,
      },
      logging: {
        captureBody: false,
      },
      ui: {
        theme: "light",
        locale: "zh-CN",
        localeMode: "manual",
        launchOnStartup: false,
        autoStartServer: true,
        closeToTray: true,
        quotaAutoRefreshMinutes: 5,
        autoUpdateEnabled: true,
      },
      remoteGit: {
        enabled: false,
        repoUrl: "",
        token: "",
        branch: "main",
      },
      providers: [],
      groups: [],
    },
    status: {
      running: false,
      address: null,
      lanAddress: null,
      metrics: {
        requests: 0,
        streamRequests: 0,
        errors: 0,
        avgLatencyMs: 0,
        inputTokens: 0,
        outputTokens: 0,
        cacheReadTokens: 0,
        cacheWriteTokens: 0,
        uptimeStartedAt: null,
      },
      groupRuntime: [],
    },
    stats: {
      dimension: "rule",
      hours: 24,
      ruleKey: null,
      ruleKeys: [],
      requests: 0,
      errors: 0,
      inputTokens: 0,
      outputTokens: 0,
      cacheReadTokens: 0,
      cacheWriteTokens: 0,
      totalCost: 0,
      costCurrency: "USD",
      inputTps: 0,
      outputTps: 0,
      peakInputTps: 0,
      peakOutputTps: 0,
      comparison: null,
      breakdowns: null,
      hourly: [],
      options: [],
    },
  }
}

export function isMockPreviewEnabled(search: string): boolean {
  const params = new URLSearchParams(search)
  const raw = params.get("mock")?.trim().toLowerCase()
  return raw === "1" || raw === "true" || raw === "yes"
}

export function installMockPreviewRuntime(targetWindow: MockPreviewWindow): boolean {
  if (!isMockPreviewEnabled(targetWindow.location.search)) {
    return false
  }

  if (targetWindow.__TAURI__?.core?.invoke || targetWindow.__TAURI_INTERNALS__?.invoke) {
    return false
  }

  const state = createMockPreviewState()

  targetWindow.__TAURI_INTERNALS__ = {
    invoke: async <T>(cmd: string, args?: Record<string, unknown>) => {
      switch (cmd) {
        case "auth_get_session_status":
          return clone(state.authSession) as T
        case "auth_login":
        case "auth_logout":
          return clone(state.authSession) as T
        case "app_get_info":
          return { name: "AI Open Router", version: "dev-mock-preview" } as T
        case "app_get_status":
          return clone(state.status) as T
        case "app_start_server":
          state.status.running = true
          return clone(state.status) as T
        case "app_stop_server":
          state.status.running = false
          return clone(state.status) as T
        case "app_renderer_ready":
        case "app_report_renderer_error":
          return undefined as T
        case "app_read_clipboard_text":
          return { text: "" } as T
        case "config_get":
          return clone(state.config) as T
        case "config_save":
          if (args?.nextConfig) {
            state.config = clone(args.nextConfig as ProxyConfig)
          }
          return {
            ok: true,
            config: clone(state.config),
            restarted: false,
            status: clone(state.status),
          } as T
        case "logs_list":
          return [] as T
        case "logs_clear":
        case "logs_stats_clear":
          return { ok: true } as T
        case "logs_stats_summary":
          return clone(state.stats) as T
        case "logs_stats_rule_cards":
        case "quota_get_group":
          return [] as T
        case "quota_get_rule":
          return {
            groupId: String(args?.groupId ?? ""),
            ruleId: String(args?.ruleId ?? ""),
            provider: "unknown",
            status: "unknown",
            remaining: null,
            total: null,
            percent: null,
            unit: null,
            resetAt: null,
            fetchedAt: new Date().toISOString(),
            message: null,
          } as T
        case "quota_test_draft":
          return {
            ok: false,
            message: "mock preview does not execute quota tests",
            rawResponse: null,
            snapshot: null,
          } as T
        case "integration_list_targets":
          return [] as T
        default:
          return {} as T
      }
    },
  }

  return true
}
