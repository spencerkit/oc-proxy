# Provider Paste Import Design

## Summary

Add a paste-based import helper to the new-provider flow so users can paste configuration snippets in supported formats and have the Provider form auto-filled with recognized values such as `token`, `apiAddress`, `defaultModel`, `protocol`, and `website`.

This feature is a form assistant only. It does not read local config directories, does not create providers automatically, and does not depend on the existing Claude/Codex/OpenClaw/OpenCode integration target system.

## Problem

Creating a Provider currently requires users to manually copy values out of third-party config snippets. This is repetitive, error-prone, and slows down onboarding when a user already has a usable upstream configuration in another tool.

## Goals

- Let users paste a config snippet while creating a Provider.
- Support these input families:
  - Codex-style TOML snippets
  - Claude Code-style JSON snippets
  - AOR custom JSON snippets
- Parse recognized fields and apply them into the existing Provider create form.
- Leave missing fields untouched so users can complete them manually.
- Keep the implementation local to the Provider form and parser utilities.

## Non-Goals

- Reading files from `~/.codex`, `~/.claude`, or any local directory.
- Batch-importing multiple Providers at once.
- Auto-saving a Provider without user review.
- Replacing the existing integration target workflows.
- Backfilling unknown values by guessing from domain names or provider names.

## Entry Point

The feature lives only in the create-mode Provider form in [RuleFormPage.tsx](/home/spencer/workspace/oc-proxy/src/renderer/pages/RuleFormPage/RuleFormPage.tsx).

It appears as a compact card above the manual form fields:

- Format selector: `Auto Detect`, `Codex`, `Claude Code`, `AOR`
- Multi-line paste input
- `Parse` action
- Parse result preview
- `Apply To Form` action

This card is not shown in edit mode for the initial release.

## User Flow

1. User opens the new Provider page.
2. User pastes a config snippet into the import card.
3. User chooses a specific format or keeps `Auto Detect`.
4. User clicks `Parse`.
5. The UI shows:
   - recognized format
   - parsed field/value preview
   - fields still missing
   - parse errors when input is invalid
6. User clicks `Apply To Form`.
7. Parsed non-empty values populate the existing form fields.
8. User reviews and completes any remaining blanks before saving normally.

## Supported Formats

### Codex

The parser accepts TOML snippets shaped like:

```toml
model_provider = "OpenAI"
model = "gpt-5.4"

[model_providers.OpenAI]
name = "OpenAI"
base_url = "https://supercodex.space/v1"
wire_api = "responses"
requires_openai_auth = true
```

Field mapping:

- `model` -> `defaultModel`
- `model_provider` identifies which provider block to read under `model_providers`
- `model_providers.<selected>.base_url` -> `apiAddress`
- `model_providers.<selected>.wire_api = "responses"` -> `protocol = "openai"`
- `model_providers.<selected>.wire_api = "chat_completions"` -> `protocol = "openai_completion"`
- `model_providers.<selected>.name` -> name candidate if no better name is supplied

Codex snippets may not include a token. If no token is present, the form token field remains unchanged.

### Claude Code

The parser accepts JSON snippets shaped like:

```json
{
  "env": {
    "ANTHROPIC_BASE_URL": "https://supercodex.space/v1",
    "ANTHROPIC_AUTH_TOKEN": "sk-xxx"
  }
}
```

Field mapping:

- `env.ANTHROPIC_BASE_URL` -> `apiAddress`
- `env.ANTHROPIC_AUTH_TOKEN` -> `token`
- `env.ANTHROPIC_API_KEY` is accepted as an alias for `token`
- `protocol` is fixed to `anthropic`

Claude Code snippets may not include a model. If no model is present, the form model field remains unchanged.

### AOR Custom Format

The first AOR-specific format is a versioned JSON envelope:

```json
{
  "format": "aor-provider/v1",
  "name": "SuperCodex",
  "protocol": "openai",
  "apiAddress": "https://supercodex.space/v1",
  "token": "sk-xxx",
  "defaultModel": "gpt-5.4",
  "website": "https://supercodex.space"
}
```

Accepted aliases:

- `base_url` -> `apiAddress`
- `api_key` -> `token`
- `model` -> `defaultModel`

The `format: "aor-provider/v1"` marker is required so the custom format stays unambiguous and safely extensible.

## Auto-Detect Rules

When the selector is `Auto Detect`, the parser uses these ordered checks:

1. Valid JSON with `format = "aor-provider/v1"` or `format: "aor-provider/v1"` -> `AOR`
2. Valid JSON with an `env` object containing `ANTHROPIC_BASE_URL`, `ANTHROPIC_AUTH_TOKEN`, or `ANTHROPIC_API_KEY` -> `Claude Code`
3. Text containing `model_provider` and a `[model_providers.` TOML table -> `Codex`
4. Otherwise fail with `unrecognized format`

Auto-detect never silently falls back to a partial parse from the wrong format.

## Application Rules

Applying parsed data to the form follows these rules:

- Only non-empty parsed values are applied.
- Existing form values are preserved when the parsed payload omits a field.
- No field is cleared by import.
- The user can still edit all values after applying.

Field targets in the form:

- `name`
- `protocol`
- `token`
- `apiAddress`
- `website`
- `defaultModel`

The import helper does not populate quota, pricing, or model mappings in the first release.

## Validation and Error Handling

Parse-time validation:

- Codex parser rejects malformed TOML.
- Claude Code and AOR parsers reject malformed JSON.
- AOR parser rejects payloads missing `format: "aor-provider/v1"`.
- Protocol values outside `openai`, `openai_completion`, or `anthropic` are rejected.

UI error states:

- `Unable to recognize import format`
- `Input is not valid JSON`
- `Input is not valid TOML`
- `No supported provider fields were found`

Preview states:

- recognized fields
- missing recommended fields
- recognized format label

## Parser Architecture

Add a dedicated renderer utility module for pure parsing logic. The parser stays out of the component body so it is easy to test.

Suggested module responsibilities:

- detect input format
- parse one specific format into a normalized result
- report recognized values plus warnings

Suggested normalized shape:

```ts
type ProviderImportFormat = "codex" | "claude_code" | "aor"

type ProviderImportDraft = Partial<Pick<Provider, "name" | "protocol" | "token" | "apiAddress" | "website" | "defaultModel">>

interface ProviderImportParseResult {
  format: ProviderImportFormat
  draft: ProviderImportDraft
  missingFields: Array<keyof ProviderImportDraft>
  warnings: string[]
}
```

## UI Design Notes

- Keep the import card visibly secondary to the main form.
- Do not auto-parse on paste in the first release.
- Require an explicit `Parse` click so failure states are predictable.
- Show the preview before mutating the form.
- After `Apply To Form`, keep the pasted text visible until the user clears it manually.

## Testing Strategy

Unit tests should cover:

- Codex TOML parse success
- Claude Code JSON parse success
- AOR JSON parse success
- auto-detect success for all three formats
- malformed JSON failure
- malformed TOML failure
- unsupported `wire_api` handling
- missing-token partial parse behavior
- apply behavior that preserves existing values when parsed values are absent

Component tests should cover:

- create-mode import card visibility
- edit-mode import card hidden
- parse result preview rendering
- apply-to-form updates the expected fields
- parse error messaging

## Delivery Scope

Initial release scope:

- create-mode form import card
- format selector with auto-detect
- parsers for Codex, Claude Code, and AOR custom JSON
- preview + apply workflow
- parser and form tests

Deferred scope:

- edit-mode import
- batch import
- directory/file reading
- import of quota/pricing/model mappings
- clipboard auto-parse

## Acceptance Criteria

- A user can paste a supported config snippet into the new Provider form.
- The app recognizes the format or returns a clear parse error.
- Parsed values can be previewed before form mutation.
- Clicking `Apply To Form` fills only the recognized fields.
- Missing values remain available for manual entry.
- Existing Provider create/save validation continues to work unchanged.
