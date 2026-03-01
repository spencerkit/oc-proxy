/**
 * Direction for request translation
 * - 'oc': OpenAI format -> Anthropic format
 * - 'co': Anthropic format -> OpenAI format
 */
export type RuleDirection = "oc" | "co"

/**
 * Protocol family supported by proxy
 */
export type RuleProtocol = "openai" | "anthropic"

/**
 * Proxy rule interface
 * Defines a single translation rule between API formats
 */
export interface Rule {
  id: string
  name: string
  protocol: RuleProtocol
  token: string
  apiAddress: string
  defaultModel: string
  modelMappings: Record<string, string>
}

/**
 * Proxy group interface
 * Organizes rules under a single path endpoint
 */
export interface Group {
  id: string
  name: string
  models: string[]
  activeRuleId: string | null
  rules: Rule[]
}

/**
 * Proxy status interface
 * Represents the current status of the proxy server
 */
export interface ProxyStatus {
  running: boolean
  address: string | null
  metrics: ProxyMetrics
}

/**
 * Proxy metrics interface
 * Tracks server performance and request statistics
 */
export interface ProxyMetrics {
  requests: number
  streamRequests: number
  errors: number
  avgLatencyMs: number
  inputTokens: number
  outputTokens: number
  cacheReadTokens: number
  cacheWriteTokens: number
  uptimeStartedAt: string | null
}

export interface TokenUsage {
  inputTokens: number
  outputTokens: number
  cacheReadTokens: number
  cacheWriteTokens: number
}

/**
 * Log entry error interface
 */
export interface LogEntryError {
  message: string
  code: string
}

/**
 * Log entry status
 */
export type LogEntryStatus = "ok" | "error" | "processing" | "rejected"

/**
 * Log entry phase
 */
export type LogEntryPhase = "request_chain" | string

/**
 * Log entry interface
 * Represents a single request/response log entry
 */
export interface LogEntry {
  timestamp: string
  traceId: string
  phase: LogEntryPhase
  status: LogEntryStatus
  method: string
  requestPath: string
  requestAddress: string
  clientAddress?: string
  groupPath: string | null
  groupName: string | null
  ruleId: string | null
  direction: RuleDirection | null
  entryProtocol?: RuleProtocol | null
  downstreamProtocol?: RuleProtocol | null
  model: string | null
  forwardedModel?: string | null
  forwardingAddress: string | null
  requestHeaders?: Record<string, string>
  forwardRequestHeaders?: Record<string, string> | null
  upstreamResponseHeaders?: Record<string, string> | null
  responseHeaders?: Record<string, string> | null
  requestBody: unknown
  forwardRequestBody: unknown
  responseBody: unknown
  tokenUsage?: TokenUsage | null
  httpStatus: number | null
  upstreamStatus: number | null
  durationMs: number
  error: LogEntryError | null
}
