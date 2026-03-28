import * as TOML from "@iarna/toml"
import type { Provider } from "@/types"

export type ProviderImportInputFormat = "auto" | "codex" | "claude_code" | "aor"
export type ProviderImportFormat = Exclude<ProviderImportInputFormat, "auto">
export type ProviderImportField =
  | "name"
  | "protocol"
  | "token"
  | "apiAddress"
  | "website"
  | "defaultModel"

export interface ProviderImportFormFields {
  name: string
  protocol: Provider["protocol"]
  token: string
  apiAddress: string
  website: string
  defaultModel: string
}

export type ProviderImportDraft = Partial<ProviderImportFormFields>

export interface ProviderImportParseResult {
  format: ProviderImportFormat
  draft: ProviderImportDraft
  missingFields: ProviderImportField[]
  warnings: string[]
}

const PROVIDER_IMPORT_FIELDS: ProviderImportField[] = [
  "name",
  "protocol",
  "token",
  "apiAddress",
  "website",
  "defaultModel",
]

const SUPPORTED_PROTOCOLS: Provider["protocol"][] = ["openai", "openai_completion", "anthropic"]

export class ProviderImportParseError extends Error {
  constructor(
    public readonly code:
      | "unrecognized_format"
      | "invalid_json"
      | "invalid_toml"
      | "unsupported_protocol"
      | "no_supported_fields"
  ) {
    super(code)
  }
}

export function parseProviderImport(input: {
  format: ProviderImportInputFormat
  raw: string
}): ProviderImportParseResult {
  const raw = input.raw.trim()
  if (!raw) {
    throw new ProviderImportParseError("no_supported_fields")
  }

  const format = input.format === "auto" ? detectProviderImportFormat(raw) : input.format
  const result =
    format === "codex"
      ? parseCodexImport(raw)
      : format === "claude_code"
        ? parseClaudeCodeImport(raw)
        : parseAorImport(raw)

  if (Object.keys(result.draft).length === 0) {
    throw new ProviderImportParseError("no_supported_fields")
  }

  return result
}

export function applyProviderImportDraft(
  current: ProviderImportFormFields,
  parsed: ProviderImportDraft
): ProviderImportFormFields {
  return {
    name: parsed.name ?? current.name,
    protocol: parsed.protocol ?? current.protocol,
    token: parsed.token ?? current.token,
    apiAddress: parsed.apiAddress ?? current.apiAddress,
    website: parsed.website ?? current.website,
    defaultModel: parsed.defaultModel ?? current.defaultModel,
  }
}

function detectProviderImportFormat(raw: string): ProviderImportFormat {
  const firstCharacter = raw[0]
  if (firstCharacter === "{") {
    const parsed = parseJsonRecord(raw)
    if (normalizeString(parsed.format) === "aor-provider/v1") {
      return "aor"
    }

    const env = asRecord(parsed.env)
    if (
      env &&
      ("ANTHROPIC_BASE_URL" in env || "ANTHROPIC_AUTH_TOKEN" in env || "ANTHROPIC_API_KEY" in env)
    ) {
      return "claude_code"
    }

    throw new ProviderImportParseError("unrecognized_format")
  }

  const parsedToml = tryParseTomlRecord(raw)
  if (hasCodexImportShape(parsedToml)) {
    return "codex"
  }

  throw new ProviderImportParseError("unrecognized_format")
}

function parseCodexImport(raw: string): ProviderImportParseResult {
  let parsed: Record<string, unknown>
  try {
    parsed = TOML.parse(raw) as Record<string, unknown>
  } catch {
    throw new ProviderImportParseError("invalid_toml")
  }

  const selectedProvider = normalizeString(parsed.model_provider)
  const modelProviders = asRecord(parsed.model_providers)
  const selectedEntry =
    (selectedProvider && modelProviders?.[selectedProvider]) ||
    (modelProviders && Object.keys(modelProviders).length === 1
      ? modelProviders[Object.keys(modelProviders)[0]]
      : undefined)

  const providerEntry = asRecord(selectedEntry)
  const wireApi = normalizeString(providerEntry?.wire_api)
  const warnings: string[] = []
  let protocol: Provider["protocol"] | undefined

  if (wireApi === "responses") protocol = "openai"
  else if (wireApi === "chat_completions") protocol = "openai_completion"
  else if (wireApi) warnings.push(`Unsupported Codex wire_api: ${wireApi}`)

  const draft = compactDraft({
    name: normalizeString(providerEntry?.name),
    protocol,
    apiAddress: normalizeString(providerEntry?.base_url),
    defaultModel: normalizeString(parsed.model),
  })

  return {
    format: "codex",
    draft,
    missingFields: buildMissingFields(draft),
    warnings,
  }
}

function parseClaudeCodeImport(raw: string): ProviderImportParseResult {
  const parsed = parseJsonRecord(raw)
  const env = asRecord(parsed.env)

  const draft = compactDraft({
    protocol:
      env &&
      ("ANTHROPIC_BASE_URL" in env || "ANTHROPIC_AUTH_TOKEN" in env || "ANTHROPIC_API_KEY" in env)
        ? "anthropic"
        : undefined,
    apiAddress: normalizeString(env?.ANTHROPIC_BASE_URL),
    token: normalizeString(env?.ANTHROPIC_AUTH_TOKEN) ?? normalizeString(env?.ANTHROPIC_API_KEY),
  })

  return {
    format: "claude_code",
    draft,
    missingFields: buildMissingFields(draft),
    warnings: [],
  }
}

function parseAorImport(raw: string): ProviderImportParseResult {
  const parsed = parseJsonRecord(raw)
  if (normalizeString(parsed.format) !== "aor-provider/v1") {
    throw new ProviderImportParseError("unrecognized_format")
  }

  const protocol = normalizeProtocol(parsed.protocol)

  if (parsed.protocol !== undefined && !protocol) {
    throw new ProviderImportParseError("unsupported_protocol")
  }

  const draft = compactDraft({
    name: normalizeString(parsed.name),
    protocol,
    token: normalizeString(parsed.api_key) ?? normalizeString(parsed.token),
    apiAddress: normalizeString(parsed.base_url) ?? normalizeString(parsed.apiAddress),
    website: normalizeString(parsed.website),
    defaultModel: normalizeString(parsed.model) ?? normalizeString(parsed.defaultModel),
  })

  return {
    format: "aor",
    draft,
    missingFields: buildMissingFields(draft),
    warnings: [],
  }
}

function buildMissingFields(draft: ProviderImportDraft): ProviderImportField[] {
  return PROVIDER_IMPORT_FIELDS.filter(field => draft[field] === undefined)
}

function normalizeString(value: unknown): string | undefined {
  if (typeof value !== "string") {
    return undefined
  }

  const trimmed = value.trim()
  return trimmed ? trimmed : undefined
}

function compactDraft(draft: ProviderImportDraft): ProviderImportDraft {
  const compacted: ProviderImportDraft = {}

  if (draft.name !== undefined) compacted.name = draft.name
  if (draft.protocol !== undefined) compacted.protocol = draft.protocol
  if (draft.token !== undefined) compacted.token = draft.token
  if (draft.apiAddress !== undefined) compacted.apiAddress = draft.apiAddress
  if (draft.website !== undefined) compacted.website = draft.website
  if (draft.defaultModel !== undefined) compacted.defaultModel = draft.defaultModel

  return compacted
}

function parseJsonRecord(raw: string): Record<string, unknown> {
  try {
    const parsed = JSON.parse(raw)
    return asRecord(parsed) ?? {}
  } catch {
    throw new ProviderImportParseError("invalid_json")
  }
}

function tryParseTomlRecord(raw: string): Record<string, unknown> | undefined {
  try {
    return asRecord(TOML.parse(raw)) ?? {}
  } catch {
    return undefined
  }
}

function asRecord(value: unknown): Record<string, unknown> | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return undefined
  }

  return value as Record<string, unknown>
}

function normalizeProtocol(value: unknown): Provider["protocol"] | undefined {
  const protocol = normalizeString(value)
  if (!protocol) {
    return undefined
  }

  return SUPPORTED_PROTOCOLS.includes(protocol as Provider["protocol"])
    ? (protocol as Provider["protocol"])
    : undefined
}

function hasCodexImportShape(parsed: Record<string, unknown> | undefined): boolean {
  if (!parsed) {
    return false
  }

  return !!normalizeString(parsed.model_provider) && !!asRecord(parsed.model_providers)
}
