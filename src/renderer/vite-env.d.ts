/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_APP_TITLE?: string;
  readonly VITE_API_URL?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}

// IPC types based on preload.js exposure
interface ProxyApp {
  // App status operations
  getStatus: () => Promise<{
    running: boolean;
    address: string | null;
    metrics: {
      requests: number;
      streamRequests: number;
      errors: number;
      avgLatencyMs: number;
      uptimeStartedAt: string | null;
    };
  }>;
  startServer: () => Promise<{
    running: boolean;
    address: string | null;
    metrics: {
      requests: number;
      streamRequests: number;
      errors: number;
      avgLatencyMs: number;
      uptimeStartedAt: string | null;
    };
  }>;
  stopServer: () => Promise<{
    running: boolean;
    address: string | null;
    metrics: {
      requests: number;
      streamRequests: number;
      errors: number;
      avgLatencyMs: number;
      uptimeStartedAt: string | null;
    };
  }>;

  // Config operations
  getConfig: () => Promise<ProxyConfig>;
  saveConfig: (config: ProxyConfig) => Promise<{
    ok: boolean;
    config: ProxyConfig;
    restarted: boolean;
    status?: {
      running: boolean;
      address: string | null;
      metrics: {
        requests: number;
        streamRequests: number;
        errors: number;
        avgLatencyMs: number;
        uptimeStartedAt: string | null;
      };
    };
  }>;

  // Logs operations
  listLogs: (max?: number) => Promise<LogEntry[]>;
  clearLogs: () => Promise<{ ok: boolean }>;
}

// Configuration types
interface ProxyConfig {
  server: {
    host: string;
    port: number;
    authEnabled: boolean;
    localBearerToken: string;
  };
  compat: {
    strictMode: boolean;
  };
  logging: {
    level: string;
    captureBody: boolean;
    redactRules: string[];
  };
  groups: ProxyGroup[];
}

interface ProxyGroup {
  id: string;
  name: string;
  path: string;
  activeRuleId: string | null;
  rules: ProxyRule[];
}

interface ProxyRule {
  id: string;
  model: string;
  token: string;
  apiAddress: string;
  direction: "oc" | "co";
}

// Log entry types
interface LogEntry {
  traceId: string;
  phase: "request_chain";
  status: string;
  method: string;
  requestPath: string;
  requestAddress: string;
  clientAddress?: string;
  groupPath?: string;
  groupName?: string;
  ruleId?: string;
  direction?: string;
  model?: string;
  forwardingAddress?: string;
  requestHeaders?: Record<string, string> | { omitted?: boolean };
  requestBody?: Record<string, unknown> | { omitted?: boolean };
  forwardRequestBody?: Record<string, unknown> | { omitted?: boolean };
  responseBody?: Record<string, unknown> | { omitted?: boolean };
  httpStatus?: number;
  upstreamStatus?: number;
  durationMs: number;
  error?: {
    message: string;
    code: string;
  };
}

// Extend Window interface with proxyApp
declare global {
  interface Window {
    proxyApp: ProxyApp;
  }
}

export {};
