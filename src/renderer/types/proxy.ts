/**
 * Direction for request translation
 * - 'oc': OpenAI format -> Anthropic format
 * - 'co': Anthropic format -> OpenAI format
 */
export type RuleDirection = "oc" | "co"

/**
 * Protocol family supported by proxy
 */
export type RuleProtocol = "openai" | "openai_completion" | "anthropic"

export type QuotaStatus = "ok" | "low" | "empty" | "unknown" | "unsupported" | "error"
export type QuotaUnitType = "percentage" | "amount" | "tokens"

export interface QuotaMappingObject {
  path?: string
  expr?: string
  value?: string | number | boolean
}

export type QuotaMappingValue = string | QuotaMappingObject

export interface RuleQuotaResponseMapping {
  remaining?: QuotaMappingValue | null
  unit?: QuotaMappingValue | null
  total?: QuotaMappingValue | null
  resetAt?: QuotaMappingValue | null
}

export interface RuleQuotaConfig {
  enabled: boolean
  provider: string
  endpoint: string
  method: string
  useRuleToken: boolean
  customToken: string
  authHeader: string
  authScheme: string
  customHeaders: Record<string, string>
  unitType: QuotaUnitType
  lowThresholdPercent: number
  response: RuleQuotaResponseMapping
}

export interface RuleQuotaSnapshot {
  groupId: string
  ruleId: string
  provider: string
  status: QuotaStatus
  remaining?: number | null
  total?: number | null
  percent?: number | null
  unit?: string | null
  resetAt?: string | null
  fetchedAt: string
  message?: string | null
}

export interface RuleQuotaTestResult {
  ok: boolean
  snapshot?: RuleQuotaSnapshot | null
  rawResponse?: unknown | null
  message?: string | null
}

export interface ProviderModelTestResult {
  ok: boolean
  resolvedModel?: string | null
  rawText?: string | null
  message?: string | null
}

export interface RuleCostConfig {
  enabled: boolean
  inputPricePerM: number
  outputPricePerM: number
  cacheInputPricePerM: number
  cacheOutputPricePerM: number
  currency: string
}

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
  quota: RuleQuotaConfig
  cost?: RuleCostConfig
}

export type Provider = Rule

/**
 * Proxy group interface
 * Organizes providers under a single path endpoint
 */
export interface Group {
  id: string
  name: string
  models: string[]
  activeProviderId: string | null
  providers: Provider[]
  activeRuleId?: string | null
  rules?: Provider[]
}

/**
 * Proxy status interface
 * Represents the current status of the proxy server
 */
export interface ProxyStatus {
  running: boolean
  address: string | null
  lanAddress?: string | null
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

export interface CostSnapshot {
  enabled: boolean
  currency: string
  inputPricePerM: number
  outputPricePerM: number
  cacheInputPricePerM: number
  cacheOutputPricePerM: number
  totalCost: number
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
  costSnapshot?: CostSnapshot | null
  httpStatus: number | null
  upstreamStatus: number | null
  durationMs: number
  error: LogEntryError | null
}
