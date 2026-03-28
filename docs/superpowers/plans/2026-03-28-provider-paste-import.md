# Provider Paste Import Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a paste-based import helper to the create-provider flow so supported Codex, Claude Code, and AOR snippets can auto-fill the Provider form without touching local integration configs.

**Architecture:** Keep parsing and form-application logic in a pure renderer utility so it can be tested independently from React. Add a small `ProviderImportCard` component for the UI, then wire it into `RuleFormPage` only when `mode === "create"` so the existing edit flow stays unchanged.

**Tech Stack:** React, TypeScript, Vite, existing form components, renderer-only utility parsing, `@iarna/toml` for Codex TOML parsing, Node `--test` unit suite.

---

### Task 1: Build and test the provider import parser utility

**Files:**
- Modify: `package.json`
- Modify: `package-lock.json`
- Modify: `tsconfig.unit.json`
- Create: `src/renderer/utils/providerImport.ts`
- Test: `tests/unit/providerImport.test.ts`

- [ ] **Step 1: Write the failing parser/apply tests**

Create `tests/unit/providerImport.test.ts` with coverage for:
- Codex TOML success
- Claude Code JSON success
- AOR JSON success
- auto-detect success
- invalid JSON failure
- invalid TOML failure
- unsupported `wire_api`
- partial apply that preserves existing form values

Use this test skeleton:

```ts
import assert from "node:assert/strict"
import { test } from "node:test"

import {
  applyProviderImportDraft,
  parseProviderImport,
  ProviderImportParseError,
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

test("parseProviderImport rejects invalid JSON payloads", () => {
  assert.throws(
    () => parseProviderImport({ format: "claude_code", raw: "{not-json}" }),
    (error: unknown) =>
      error instanceof ProviderImportParseError && error.code === "invalid_json"
  )
})

test("parseProviderImport rejects invalid TOML payloads", () => {
  assert.throws(
    () => parseProviderImport({ format: "codex", raw: "[model_providers.OpenAI" }),
    (error: unknown) =>
      error instanceof ProviderImportParseError && error.code === "invalid_toml"
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
```

- [ ] **Step 2: Run the targeted test to verify it fails**

Run:

```bash
rm -rf .tmp/unit-tests
npx tsc -p tsconfig.unit.json
node --test .tmp/unit-tests/tests/unit/providerImport.test.js
```

Expected: FAIL because `src/renderer/utils/providerImport.ts` does not exist yet.

- [ ] **Step 3: Add the TOML parser dependency**

Install a browser-safe TOML parser for Codex snippets:

```bash
npm install @iarna/toml
```

This should update:

```json
{
  "dependencies": {
    "@iarna/toml": "^2.2.5"
  }
}
```

- [ ] **Step 4: Implement the parser utility and form-apply helper**

Create `src/renderer/utils/providerImport.ts` with these types and functions:

```ts
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
```

Implement helpers in the same file:
- `detectProviderImportFormat(raw)`
- `parseCodexImport(raw)`
- `parseClaudeCodeImport(raw)`
- `parseAorImport(raw)`
- `buildMissingFields(draft)`
- `normalizeString(value)`

Codex-specific implementation target:

```ts
function parseCodexImport(raw: string): ProviderImportParseResult {
  let parsed: Record<string, unknown>
  try {
    parsed = TOML.parse(raw) as Record<string, unknown>
  } catch {
    throw new ProviderImportParseError("invalid_toml")
  }

  const selectedProvider = normalizeString(parsed.model_provider)
  const modelProviders = parsed.model_providers as Record<string, unknown> | undefined
  const selectedEntry =
    (selectedProvider && modelProviders?.[selectedProvider]) ||
    (modelProviders && Object.keys(modelProviders).length === 1
      ? modelProviders[Object.keys(modelProviders)[0]]
      : undefined)

  const providerEntry =
    selectedEntry && typeof selectedEntry === "object"
      ? (selectedEntry as Record<string, unknown>)
      : undefined

  const wireApi = normalizeString(providerEntry?.wire_api)
  const warnings: string[] = []
  let protocol: Provider["protocol"] | undefined

  if (wireApi === "responses") protocol = "openai"
  else if (wireApi === "chat_completions") protocol = "openai_completion"
  else if (wireApi) warnings.push(`Unsupported Codex wire_api: ${wireApi}`)

  const draft: ProviderImportDraft = {
    name: normalizeString(providerEntry?.name),
    protocol,
    apiAddress: normalizeString(providerEntry?.base_url),
    defaultModel: normalizeString(parsed.model),
  }

  return {
    format: "codex",
    draft: compactDraft(draft),
    missingFields: buildMissingFields(compactDraft(draft)),
    warnings,
  }
}
```

Update `tsconfig.unit.json` so the new utility compiles in unit runs:

```json
{
  "include": [
    "tests/unit/**/*.ts",
    "src/renderer/utils/providerImport.ts"
  ]
}
```

- [ ] **Step 5: Run the targeted parser tests to verify they pass**

Run:

```bash
rm -rf .tmp/unit-tests
npx tsc -p tsconfig.unit.json
node --test .tmp/unit-tests/tests/unit/providerImport.test.js
```

Expected: PASS for every `providerImport` test.

- [ ] **Step 6: Commit the parser utility**

```bash
git add package.json package-lock.json tsconfig.unit.json src/renderer/utils/providerImport.ts tests/unit/providerImport.test.ts
git commit -m "feat: add provider paste import parser"
```

### Task 2: Build the reusable Provider import card UI

**Files:**
- Create: `src/renderer/pages/RuleFormPage/ProviderImportCard.tsx`
- Modify: `src/renderer/pages/RuleFormPage/RuleFormPage.module.css`
- Modify: `src/renderer/i18n/en-US.ts`
- Modify: `src/renderer/i18n/zh-CN.ts`
- Modify: `tsconfig.unit.json`
- Test: `tests/unit/providerImportCard.test.ts`

- [ ] **Step 1: Write the failing import card render test**

Create `tests/unit/providerImportCard.test.ts` as a render-to-static-markup test with the same alias and CSS mock setup pattern used elsewhere in `tests/unit`:

```ts
import assert from "node:assert/strict"
import { existsSync } from "node:fs"
import path from "node:path"
import { test } from "node:test"
import React from "react"
import { renderToStaticMarkup } from "react-dom/server"

const Module = require("node:module") as {
  _resolveFilename: (
    request: string,
    parent: { filename?: string } | undefined,
    isMain: boolean,
    options?: unknown
  ) => string
}

// Reuse the same alias/css mock pattern used in tests/unit/agentEditPageLayout.test.ts.

function loadProviderImportCard() {
  return require("../../src/renderer/pages/RuleFormPage/ProviderImportCard") as typeof import("../../src/renderer/pages/RuleFormPage/ProviderImportCard")
}

test("renders import format selector, paste textarea, parse action, and preview", () => {
  const { ProviderImportCard } = loadProviderImportCard()
  const markup = renderToStaticMarkup(
    React.createElement(ProviderImportCard, {
      format: "auto",
      rawValue: '{"env":{"ANTHROPIC_BASE_URL":"https://supercodex.space/v1"}}',
      parseError: null,
      parseResult: {
        format: "claude_code",
        draft: {
          protocol: "anthropic",
          apiAddress: "https://supercodex.space/v1",
        },
        missingFields: ["token", "defaultModel"],
        warnings: [],
      },
      onFormatChange: () => {},
      onRawChange: () => {},
      onParse: () => {},
      onApply: () => {},
      onClear: () => {},
    })
  )

  assert.match(markup, /ruleForm\.importTitle/)
  assert.match(markup, /ruleForm\.importFormatAuto/)
  assert.match(markup, /ruleForm\.importFormatCodex/)
  assert.match(markup, /ruleForm\.importParse/)
  assert.match(markup, /ruleForm\.importApply/)
  assert.match(markup, /https:\/\/supercodex\.space\/v1/)
})
```

- [ ] **Step 2: Run the targeted component test to verify it fails**

Run:

```bash
rm -rf .tmp/unit-tests
npx tsc -p tsconfig.unit.json
node --test .tmp/unit-tests/tests/unit/providerImportCard.test.js
```

Expected: FAIL because `ProviderImportCard.tsx` does not exist yet.

- [ ] **Step 3: Implement the import card component, translations, and styles**

Create `src/renderer/pages/RuleFormPage/ProviderImportCard.tsx`:

```tsx
import type React from "react"
import { Button } from "@/components"
import { useTranslation } from "@/hooks"
import type {
  ProviderImportInputFormat,
  ProviderImportParseResult,
} from "@/utils/providerImport"
import styles from "./RuleFormPage.module.css"

export interface ProviderImportCardProps {
  format: ProviderImportInputFormat
  rawValue: string
  parseError: string | null
  parseResult: ProviderImportParseResult | null
  onFormatChange: (value: ProviderImportInputFormat) => void
  onRawChange: (value: string) => void
  onParse: () => void
  onApply: () => void
  onClear: () => void
}

export const ProviderImportCard: React.FC<ProviderImportCardProps> = ({
  format,
  rawValue,
  parseError,
  parseResult,
  onFormatChange,
  onRawChange,
  onParse,
  onApply,
  onClear,
}) => {
  const { t } = useTranslation()

  return (
    <section className={styles.importCard}>
      <div className={styles.importCardHeader}>
        <div>
          <h2 className={styles.sectionTitle}>{t("ruleForm.importTitle")}</h2>
          <p className={styles.fieldHint}>{t("ruleForm.importHint")}</p>
        </div>
        <div className={styles.importActions}>
          <Button type="button" variant="default" size="small" onClick={onClear}>
            {t("ruleForm.importClear")}
          </Button>
          <Button type="button" variant="primary" size="small" onClick={onParse}>
            {t("ruleForm.importParse")}
          </Button>
        </div>
      </div>

      <div className={styles.formGroup}>
        <label htmlFor="provider-import-format">{t("ruleForm.importFormat")}</label>
        <select
          id="provider-import-format"
          className={styles.nativeSelect}
          value={format}
          onChange={event => onFormatChange(event.target.value as ProviderImportInputFormat)}
        >
          <option value="auto">{t("ruleForm.importFormatAuto")}</option>
          <option value="codex">{t("ruleForm.importFormatCodex")}</option>
          <option value="claude_code">{t("ruleForm.importFormatClaudeCode")}</option>
          <option value="aor">{t("ruleForm.importFormatAor")}</option>
        </select>
      </div>

      <div className={styles.formGroup}>
        <label htmlFor="provider-import-raw">{t("ruleForm.importInputLabel")}</label>
        <textarea
          id="provider-import-raw"
          className={styles.importTextarea}
          value={rawValue}
          onChange={event => onRawChange(event.target.value)}
          placeholder={t("ruleForm.importInputPlaceholder")}
        />
      </div>

      {parseError ? <p className={styles.errorText}>{parseError}</p> : null}

      {parseResult ? (
        <div className={styles.importPreview}>
          <p className={styles.importDetectedFormat}>
            {t("ruleForm.importDetectedFormat")}:{" "}
            <strong>{t(`ruleForm.importDetectedFormatValue.${parseResult.format}`)}</strong>
          </p>
          <div className={styles.importPreviewGrid}>
            {Object.entries(parseResult.draft).map(([key, value]) => (
              <div key={key} className={styles.importPreviewItem}>
                <span>{key}</span>
                <strong>{String(value)}</strong>
              </div>
            ))}
          </div>
          {parseResult.missingFields.length > 0 ? (
            <p className={styles.fieldHint}>
              {t("ruleForm.importMissingFields", {
                fields: parseResult.missingFields.join(", "),
              })}
            </p>
          ) : null}
          <Button type="button" variant="primary" size="small" onClick={onApply}>
            {t("ruleForm.importApply")}
          </Button>
        </div>
      ) : null}
    </section>
  )
}
```

Add translation keys under `ruleForm` in both locale files:

```ts
importTitle: "Paste Config Import",
importHint: "Paste a supported config snippet to auto-fill provider fields.",
importFormat: "Import Format",
importFormatAuto: "Auto Detect",
importFormatCodex: "Codex",
importFormatClaudeCode: "Claude Code",
importFormatAor: "AOR",
importInputLabel: "Config Text",
importInputPlaceholder: "Paste Codex TOML, Claude Code JSON, or AOR JSON here",
importParse: "Parse",
importApply: "Apply To Form",
importClear: "Clear",
importDetectedFormat: "Detected Format",
importDetectedFormatValue: {
  codex: "Codex",
  claude_code: "Claude Code",
  aor: "AOR"
},
importMissingFields: "Missing fields: {{fields}}",
importErrorUnrecognizedFormat: "Unable to recognize import format",
importErrorInvalidJson: "Input is not valid JSON",
importErrorInvalidToml: "Input is not valid TOML",
importErrorNoSupportedFields: "No supported provider fields were found",
```

Add matching CSS in `RuleFormPage.module.css`:

```css
.importCard {
  border: 1px solid var(--border);
  border-radius: var(--radius-lg);
  padding: var(--sp-4);
  background: color-mix(in srgb, var(--surface) 88%, var(--bg-raised) 12%);
  display: grid;
  gap: var(--sp-4);
}

.importCardHeader {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: var(--sp-3);
  flex-wrap: wrap;
}

.importActions {
  display: flex;
  align-items: center;
  gap: var(--sp-2);
  flex-wrap: wrap;
}

.importTextarea {
  width: 100%;
  min-height: 140px;
  resize: vertical;
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  padding: var(--sp-3);
  background: var(--surface);
  color: var(--text-primary);
  font-family: var(--font-mono);
  font-size: var(--text-xs);
  line-height: 1.5;
}

.importPreview {
  display: grid;
  gap: var(--sp-3);
  padding-top: var(--sp-2);
  border-top: 1px solid var(--border);
}

.importPreviewGrid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
  gap: var(--sp-2);
}

.importPreviewItem {
  display: grid;
  gap: 4px;
  padding: var(--sp-2);
  border: 1px solid var(--border);
  border-radius: var(--radius-md);
  background: var(--bg-raised);
}

.importDetectedFormat {
  margin: 0;
  font-size: var(--text-sm);
  color: var(--text-secondary);
}
```

Update `tsconfig.unit.json` to include the new component:

```json
"src/renderer/pages/RuleFormPage/ProviderImportCard.tsx"
```

- [ ] **Step 4: Run the targeted import card test to verify it passes**

Run:

```bash
rm -rf .tmp/unit-tests
npx tsc -p tsconfig.unit.json
node --test .tmp/unit-tests/tests/unit/providerImportCard.test.js
```

Expected: PASS.

- [ ] **Step 5: Commit the import card UI**

```bash
git add tsconfig.unit.json src/renderer/pages/RuleFormPage/ProviderImportCard.tsx src/renderer/pages/RuleFormPage/RuleFormPage.module.css src/renderer/i18n/en-US.ts src/renderer/i18n/zh-CN.ts tests/unit/providerImportCard.test.ts
git commit -m "feat: add provider import card ui"
```

### Task 3: Integrate the import card into the create-provider flow

**Files:**
- Modify: `src/renderer/pages/RuleFormPage/RuleFormPage.tsx`
- Modify: `tsconfig.unit.json`
- Test: `tests/unit/ruleFormPageImport.test.ts`

- [ ] **Step 1: Write the failing RuleFormPage visibility test**

Create `tests/unit/ruleFormPageImport.test.ts` with the same alias and CSS mock strategy used in `tests/unit/serviceProviderList.test.ts`. Mock these modules:
- `react-router-dom`
- `@/hooks`
- `@/store`
- `@/utils/relax`
- `@/components`

Use this focused assertion:

```ts
import assert from "node:assert/strict"
import { test } from "node:test"
import React from "react"
import { renderToStaticMarkup } from "react-dom/server"

function loadRuleFormPage() {
  return require("../../src/renderer/pages/RuleFormPage/RuleFormPage") as typeof import("../../src/renderer/pages/RuleFormPage/RuleFormPage")
}

test("create mode renders the provider import card while edit mode hides it", () => {
  const { RuleFormPage } = loadRuleFormPage()

  const createMarkup = renderToStaticMarkup(React.createElement(RuleFormPage, { mode: "create" }))
  const editMarkup = renderToStaticMarkup(React.createElement(RuleFormPage, { mode: "edit" }))

  assert.match(createMarkup, /ruleForm\.importTitle/)
  assert.doesNotMatch(editMarkup, /ruleForm\.importTitle/)
})
```

Use a mocked config object that includes:

```ts
const mockConfig = {
  server: { port: 8899, authEnabled: false, localBearerToken: "" },
  compat: { strictMode: false, textToolCallFallbackEnabled: true },
  logging: { captureBody: false },
  ui: {
    launchOnStartup: false,
    autoStartServer: true,
    closeToTray: true,
    theme: "light",
    locale: "en-US",
    localeMode: "explicit",
    autoUpdateEnabled: true,
    quotaAutoRefreshMinutes: 5,
  },
  remoteGit: { enabled: false, repoUrl: "", token: "", branch: "main" },
  providers: [],
  groups: [],
}
```

- [ ] **Step 2: Run the targeted RuleFormPage test to verify it fails**

Run:

```bash
rm -rf .tmp/unit-tests
npx tsc -p tsconfig.unit.json
node --test .tmp/unit-tests/tests/unit/ruleFormPageImport.test.js
```

Expected: FAIL because `RuleFormPage` does not render the import card yet.

- [ ] **Step 3: Wire parser state and apply logic into `RuleFormPage`**

Update imports at the top of `RuleFormPage.tsx`:

```tsx
import {
  applyProviderImportDraft,
  parseProviderImport,
  ProviderImportFormFields,
  ProviderImportInputFormat,
  ProviderImportParseError,
  type ProviderImportParseResult,
} from "@/utils/providerImport"
import { ProviderImportCard } from "./ProviderImportCard"
```

Add create-only import state near the other local form state:

```tsx
const [importFormat, setImportFormat] = useState<ProviderImportInputFormat>("auto")
const [importText, setImportText] = useState("")
const [importResult, setImportResult] = useState<ProviderImportParseResult | null>(null)
const [importError, setImportError] = useState<string | null>(null)
```

Add helpers before `handleSubmit`:

```tsx
const getCurrentImportFormFields = (): ProviderImportFormFields => ({
  name,
  protocol,
  token,
  apiAddress,
  website,
  defaultModel,
})

const resolveImportErrorText = (error: unknown): string => {
  if (!(error instanceof ProviderImportParseError)) {
    return t("ruleForm.importErrorNoSupportedFields")
  }
  switch (error.code) {
    case "invalid_json":
      return t("ruleForm.importErrorInvalidJson")
    case "invalid_toml":
      return t("ruleForm.importErrorInvalidToml")
    case "unrecognized_format":
      return t("ruleForm.importErrorUnrecognizedFormat")
    case "no_supported_fields":
    default:
      return t("ruleForm.importErrorNoSupportedFields")
  }
}

const handleParseImport = () => {
  try {
    const result = parseProviderImport({
      format: importFormat,
      raw: importText,
    })
    setImportResult(result)
    setImportError(null)
  } catch (error) {
    setImportResult(null)
    setImportError(resolveImportErrorText(error))
  }
}

const handleApplyImport = () => {
  if (!importResult) return
  const applied = applyProviderImportDraft(getCurrentImportFormFields(), importResult.draft)

  setName(applied.name)
  setProtocol(applied.protocol)
  setToken(applied.token)
  setApiAddress(applied.apiAddress)
  setWebsite(applied.website)
  setDefaultModel(applied.defaultModel)
  setErrors(prev => ({
    ...prev,
    name: undefined,
    token: undefined,
    apiAddress: undefined,
    defaultModel: undefined,
  }))
}
```

Render the card at the top of the form, before the routing section:

```tsx
<form onSubmit={handleSubmit} className={styles.ruleForm}>
  {!isEditMode ? (
    <ProviderImportCard
      format={importFormat}
      rawValue={importText}
      parseError={importError}
      parseResult={importResult}
      onFormatChange={value => {
        setImportFormat(value)
        setImportResult(null)
        setImportError(null)
      }}
      onRawChange={value => {
        setImportText(value)
        setImportResult(null)
        setImportError(null)
      }}
      onParse={handleParseImport}
      onApply={handleApplyImport}
      onClear={() => {
        setImportText("")
        setImportResult(null)
        setImportError(null)
        setImportFormat("auto")
      }}
    />
  ) : null}
```

Update `tsconfig.unit.json` to ensure `RuleFormPage.tsx` and `ProviderImportCard.tsx` remain included together with the new test file pattern already in place.

- [ ] **Step 4: Run the focused page test, then the full renderer verification**

Run the focused page test:

```bash
rm -rf .tmp/unit-tests
npx tsc -p tsconfig.unit.json
node --test .tmp/unit-tests/tests/unit/ruleFormPageImport.test.js
```

Expected: PASS.

Then run the full renderer suite:

```bash
npm run test:unit:ts
```

Expected: PASS for all unit tests, including:
- `providerImport.test`
- `providerImportCard.test`
- `ruleFormPageImport.test`
- all existing renderer tests

Finally verify the app still builds:

```bash
npm run build
```

Expected: PASS with no TypeScript or Vite build errors.

- [ ] **Step 5: Commit the RuleFormPage integration**

```bash
git add tsconfig.unit.json src/renderer/pages/RuleFormPage/RuleFormPage.tsx tests/unit/ruleFormPageImport.test.ts
git commit -m "feat: wire provider paste import into rule form"
```

### Task 4: Perform manual smoke verification with real sample snippets

**Files:**
- No file changes required for this verification task

- [ ] **Step 1: Start the app and open the new Provider page**

Run:

```bash
npm run dev
```

Open the create-provider route and confirm the import card appears above the form.

- [ ] **Step 2: Verify Codex sample parsing**

Paste:

```toml
model_provider = "OpenAI"
model = "gpt-5.4"

[model_providers.OpenAI]
name = "OpenAI"
base_url = "https://supercodex.space/v1"
wire_api = "responses"
requires_openai_auth = true
```

Expected after `Parse`:
- detected format = `Codex`
- `apiAddress` preview = `https://supercodex.space/v1`
- `defaultModel` preview = `gpt-5.4`
- `protocol` preview = `openai`
- token remains missing

- [ ] **Step 3: Verify Claude Code sample parsing**

Paste:

```json
{
  "env": {
    "ANTHROPIC_BASE_URL": "https://supercodex.space/v1",
    "ANTHROPIC_AUTH_TOKEN": "sk-84fdb3b05882631d12b3bcafe4e44626b052eb97bb0cd5af53fb31108ca0ee5e",
    "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC": "1",
    "CLAUDE_CODE_ATTRIBUTION_HEADER": "0"
  }
}
```

Expected after `Parse`:
- detected format = `Claude Code`
- `protocol` preview = `anthropic`
- `apiAddress` preview = `https://supercodex.space/v1`
- `token` preview contains the provided token
- `defaultModel` remains missing

- [ ] **Step 4: Verify AOR sample parsing and apply behavior**

Paste:

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

Click `Parse`, then `Apply To Form`.

Expected:
- the corresponding form fields update
- untouched form fields stay unchanged
- save button enablement still follows the existing validation rules
