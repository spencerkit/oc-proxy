/**
 * Direction for request translation
 * - 'oc': OpenAI format -> Anthropic format
 * - 'co': Anthropic format -> OpenAI format
 */
export type RuleDirection = 'oc' | 'co';

/**
 * Proxy rule interface
 * Defines a single translation rule between API formats
 */
export interface Rule {
  id: string;
  model: string;
  direction: RuleDirection;
  token: string;
  apiAddress: string;
}

/**
 * Proxy group interface
 * Organizes rules under a single path endpoint
 */
export interface Group {
  id: string;
  name: string;
  path: string;
  activeRuleId: string | null;
  rules: Rule[];
}

/**
 * Proxy status interface
 * Represents the current status of the proxy server
 */
export interface ProxyStatus {
  running: boolean;
  address: string | null;
  metrics: ProxyMetrics;
}

/**
 * Proxy metrics interface
 * Tracks server performance and request statistics
 */
export interface ProxyMetrics {
  requests: number;
  streamRequests: number;
  errors: number;
  avgLatencyMs: number;
  uptimeStartedAt: string | null;
}

/**
 * Log entry error interface
 */
export interface LogEntryError {
  message: string;
  code: string;
}

/**
 * Log entry status
 */
export type LogEntryStatus = 'ok' | 'error' | 'processing' | 'rejected';

/**
 * Log entry phase
 */
export type LogEntryPhase = 'request_chain' | string;

/**
 * Log entry interface
 * Represents a single request/response log entry
 */
export interface LogEntry {
  timestamp: string;
  traceId: string;
  phase: LogEntryPhase;
  status: LogEntryStatus;
  method: string;
  requestPath: string;
  requestAddress: string;
  clientAddress?: string;
  groupPath: string | null;
  groupName: string | null;
  ruleId: string | null;
  direction: RuleDirection | null;
  model: string | null;
  forwardingAddress: string | null;
  requestHeaders?: Record<string, string>;
  requestBody: unknown;
  forwardRequestBody: unknown;
  responseBody: unknown;
  httpStatus: number | null;
  upstreamStatus: number | null;
  durationMs: number;
  error: LogEntryError | null;
}
