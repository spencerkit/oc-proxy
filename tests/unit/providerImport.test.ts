import assert from "node:assert/strict"
import { test } from "node:test"

import {
  applyProviderImportDraft,
  ProviderImportParseError,
  parseProviderImport,
} from "../../src/renderer/utils/providerImport"

test("parseProviderImport parses Codex TOML into a provider draft", () => {
  const result = parseProviderImport({
    format: "codex",
    raw: `
model_provider = "OpenAI"
model = "gpt-5.4"

[model_providers.OpenAI]
name = "OpenAI"
base_url = "https://supercodex.space/v1"
wire_api = "responses"
`.trim(),
  })

  assert.equal(result.format, "codex")
  assert.deepEqual(result.draft, {
    name: "OpenAI",
    protocol: "openai",
    apiAddress: "https://supercodex.space/v1",
    defaultModel: "gpt-5.4",
  })
})

test("parseProviderImport parses Claude Code JSON env payload", () => {
  const result = parseProviderImport({
    format: "claude_code",
    raw: JSON.stringify({
      env: {
        ANTHROPIC_BASE_URL: "https://supercodex.space/v1",
        ANTHROPIC_AUTH_TOKEN: "sk-test",
      },
    }),
  })

  assert.deepEqual(result.draft, {
    protocol: "anthropic",
    apiAddress: "https://supercodex.space/v1",
    token: "sk-test",
  })
})

test("parseProviderImport parses AOR JSON payload with aliases", () => {
  const result = parseProviderImport({
    format: "aor",
    raw: JSON.stringify({
      format: "aor-provider/v1",
      name: "SuperCodex",
      protocol: "openai",
      base_url: "https://supercodex.space/v1",
      api_key: "sk-123",
      model: "gpt-5.4",
      website: "https://supercodex.space",
    }),
  })

  assert.deepEqual(result.draft, {
    name: "SuperCodex",
    protocol: "openai",
    apiAddress: "https://supercodex.space/v1",
    token: "sk-123",
    defaultModel: "gpt-5.4",
    website: "https://supercodex.space",
  })
})

test("parseProviderImport rejects explicit AOR payloads missing the required marker", () => {
  assert.throws(
    () =>
      parseProviderImport({
        format: "aor",
        raw: JSON.stringify({
          name: "SuperCodex",
          protocol: "openai",
          base_url: "https://supercodex.space/v1",
        }),
      }),
    (error: unknown) =>
      error instanceof ProviderImportParseError && error.code === "unrecognized_format"
  )
})

test("parseProviderImport auto-detects Claude Code payloads", () => {
  const result = parseProviderImport({
    format: "auto",
    raw: JSON.stringify({
      env: {
        ANTHROPIC_BASE_URL: "https://supercodex.space/v1",
      },
    }),
  })

  assert.equal(result.format, "claude_code")
})

test("parseProviderImport auto-detects AOR payloads from the required marker", () => {
  const result = parseProviderImport({
    format: "auto",
    raw: JSON.stringify({
      format: "aor-provider/v1",
      protocol: "openai",
      base_url: "https://supercodex.space/v1",
    }),
  })

  assert.equal(result.format, "aor")
  assert.deepEqual(result.draft, {
    protocol: "openai",
    apiAddress: "https://supercodex.space/v1",
  })
})

test("parseProviderImport auto-detect rejects unrelated JSON", () => {
  assert.throws(
    () =>
      parseProviderImport({
        format: "auto",
        raw: JSON.stringify({
          hello: "world",
        }),
      }),
    (error: unknown) =>
      error instanceof ProviderImportParseError && error.code === "unrecognized_format"
  )
})

test("parseProviderImport auto-detect rejects unrelated TOML", () => {
  assert.throws(
    () =>
      parseProviderImport({
        format: "auto",
        raw: `
title = "hello"

[service]
url = "https://example.com"
`.trim(),
      }),
    (error: unknown) =>
      error instanceof ProviderImportParseError && error.code === "unrecognized_format"
  )
})

test("parseProviderImport auto-detect rejects AOR-like JSON without the required marker", () => {
  assert.throws(
    () =>
      parseProviderImport({
        format: "auto",
        raw: JSON.stringify({
          protocol: "openai",
          apiAddress: "https://supercodex.space/v1",
        }),
      }),
    (error: unknown) =>
      error instanceof ProviderImportParseError && error.code === "unrecognized_format"
  )
})

test("parseProviderImport auto-detect rejects AOR payloads with unsupported protocols", () => {
  assert.throws(
    () =>
      parseProviderImport({
        format: "auto",
        raw: JSON.stringify({
          format: "aor-provider/v1",
          protocol: "unsupported",
        }),
      }),
    (error: unknown) =>
      error instanceof ProviderImportParseError && error.code === "unsupported_protocol"
  )
})

test("parseProviderImport auto-detects Codex payloads when model_provider and model_providers table are both present", () => {
  const result = parseProviderImport({
    format: "auto",
    raw: `
model_provider = "OpenAI"
model = "gpt-5.4"

[model_providers.OpenAI]
name = "OpenAI"
base_url = "https://supercodex.space/v1"
wire_api = "responses"
`.trim(),
  })

  assert.equal(result.format, "codex")
  assert.deepEqual(result.draft, {
    name: "OpenAI",
    protocol: "openai",
    apiAddress: "https://supercodex.space/v1",
    defaultModel: "gpt-5.4",
  })
})

test("parseProviderImport auto-detects Codex payloads with quoted model_providers table names", () => {
  const result = parseProviderImport({
    format: "auto",
    raw: `
model_provider = "OpenAI"
model = "gpt-5.4"

["model_providers"."OpenAI"]
name = "OpenAI"
base_url = "https://supercodex.space/v1"
wire_api = "responses"
`.trim(),
  })

  assert.equal(result.format, "codex")
  assert.deepEqual(result.draft, {
    name: "OpenAI",
    protocol: "openai",
    apiAddress: "https://supercodex.space/v1",
    defaultModel: "gpt-5.4",
  })
})

test("parseProviderImport auto-detect rejects TOML and comments that only mention Codex markers", () => {
  assert.throws(
    () =>
      parseProviderImport({
        format: "auto",
        raw: `
# model_provider = "OpenAI"
title = "mentions [model_providers.OpenAI] in text"

[service]
description = "model_provider and [model_providers.OpenAI] are documentation examples"
url = "https://example.com"
`.trim(),
      }),
    (error: unknown) =>
      error instanceof ProviderImportParseError && error.code === "unrecognized_format"
  )
})

test("parseProviderImport auto-detect prefers AOR over Claude Code when both markers are present", () => {
  const result = parseProviderImport({
    format: "auto",
    raw: JSON.stringify({
      format: "aor-provider/v1",
      protocol: "openai",
      base_url: "https://supercodex.space/v1",
      env: {
        ANTHROPIC_BASE_URL: "https://anthropic.example/v1",
        ANTHROPIC_AUTH_TOKEN: "sk-anthropic",
      },
    }),
  })

  assert.equal(result.format, "aor")
  assert.deepEqual(result.draft, {
    protocol: "openai",
    apiAddress: "https://supercodex.space/v1",
  })
})

test("parseProviderImport rejects invalid JSON payloads", () => {
  assert.throws(
    () => parseProviderImport({ format: "claude_code", raw: "{not-json}" }),
    (error: unknown) => error instanceof ProviderImportParseError && error.code === "invalid_json"
  )
})

test("parseProviderImport rejects invalid TOML payloads", () => {
  assert.throws(
    () => parseProviderImport({ format: "codex", raw: "[model_providers.OpenAI" }),
    (error: unknown) => error instanceof ProviderImportParseError && error.code === "invalid_toml"
  )
})

test("parseProviderImport warns when Codex wire_api is unsupported", () => {
  const result = parseProviderImport({
    format: "codex",
    raw: `
model_provider = "OpenAI"

[model_providers.OpenAI]
base_url = "https://supercodex.space/v1"
wire_api = "unknown"
`.trim(),
  })

  assert.deepEqual(result.draft, {
    apiAddress: "https://supercodex.space/v1",
  })
  assert.match(result.warnings.join(" "), /wire_api/i)
})

test("applyProviderImportDraft preserves current values when parsed fields are missing", () => {
  const applied = applyProviderImportDraft(
    {
      name: "Existing",
      protocol: "anthropic",
      token: "keep-me",
      apiAddress: "https://existing.example/v1",
      website: "https://existing.example",
      defaultModel: "claude-3-7-sonnet",
    },
    {
      apiAddress: "https://supercodex.space/v1",
      defaultModel: "gpt-5.4",
    }
  )

  assert.deepEqual(applied, {
    name: "Existing",
    protocol: "anthropic",
    token: "keep-me",
    apiAddress: "https://supercodex.space/v1",
    website: "https://existing.example",
    defaultModel: "gpt-5.4",
  })
})
