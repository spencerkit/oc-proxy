import assert from "node:assert/strict"
import { existsSync } from "node:fs"
import path from "node:path"
import { test } from "node:test"
import React from "react"
import { renderToStaticMarkup } from "react-dom/server"

import type {
  ProviderImportField,
  ProviderImportParseResult,
} from "../../src/renderer/utils/providerImport"

const Module = require("node:module") as {
  _resolveFilename: (
    request: string,
    parent: { filename?: string } | undefined,
    isMain: boolean,
    options?: unknown
  ) => string
}

type CssModuleExports = Record<string, string>
type UnknownProps = Record<string, unknown>

const repoRoot = path.resolve(__dirname, "../../../..")
const unitOutDir = path.join(repoRoot, ".tmp/unit-tests")
const originalResolveFilename = Module._resolveFilename
const originalCssExtension = require.extensions[".css"]

const translations: Record<string, string> = {
  "ruleForm.importTitle": "Paste Config Import",
  "ruleForm.importHint": "Paste a supported config snippet to auto-fill provider fields.",
  "ruleForm.importFormat": "Import Format",
  "ruleForm.importFormatAuto": "Auto Detect",
  "ruleForm.importFormatCodex": "Codex",
  "ruleForm.importFormatClaudeCode": "Claude Code",
  "ruleForm.importFormatAor": "AOR",
  "ruleForm.importInputLabel": "Config Text",
  "ruleForm.importInputPlaceholder": "Paste Codex TOML, Claude Code JSON, or AOR JSON here",
  "ruleForm.importParse": "Parse",
  "ruleForm.importClear": "Clear",
  "ruleForm.importPreviewTitle": "Parsed Preview",
  "ruleForm.importDetectedFormat": "Detected Format",
  "ruleForm.importDetectedFormatValue.codex": "Codex",
  "ruleForm.importDetectedFormatValue.claude_code": "Claude Code",
  "ruleForm.importDetectedFormatValue.aor": "AOR",
  "ruleForm.importApply": "Apply To Form",
  "ruleForm.importField.name": "Name",
  "ruleForm.importField.protocol": "Protocol",
  "ruleForm.importField.token": "Token",
  "ruleForm.importField.apiAddress": "API Address",
  "ruleForm.importField.website": "Website",
  "ruleForm.importField.defaultModel": "Default Model",
  "ruleForm.importWarnings": "Warnings",
}

function translate(key: string, options?: Record<string, unknown>): string {
  if (key in translations) {
    return translations[key]
  }

  if (key === "ruleForm.importMissingFields" && typeof options?.fields === "string") {
    return `Missing: ${String(options.fields)}`
  }

  return key
}

function resolveCompiledAlias(request: string): string | null {
  const aliasPrefixes = [
    { prefix: "@/components/", target: "src/renderer/components/" },
    { prefix: "@/hooks/", target: "src/renderer/hooks/" },
    { prefix: "@/types/", target: "src/renderer/types/" },
    { prefix: "@/utils/", target: "src/renderer/utils/" },
    { prefix: "@/contexts/", target: "src/renderer/contexts/" },
    { prefix: "@/i18n/", target: "src/renderer/i18n/" },
    { prefix: "@/pages/", target: "src/renderer/pages/" },
    { prefix: "@/renderer/", target: "src/renderer/" },
    { prefix: "@/", target: "src/" },
  ] as const

  for (const { prefix, target } of aliasPrefixes) {
    if (!request.startsWith(prefix)) continue

    const relativeModulePath = request.slice(prefix.length)
    const candidates = [
      path.join(unitOutDir, target, `${relativeModulePath}.js`),
      path.join(unitOutDir, target, relativeModulePath, "index.js"),
    ]

    const resolved = candidates.find(candidate => existsSync(candidate))
    if (resolved) return resolved
  }

  return null
}

Module._resolveFilename = (request, parent, isMain, options) => {
  if (request === "@/hooks" || request === "@/components") {
    return request
  }

  const compiledAliasPath = resolveCompiledAlias(request)
  if (compiledAliasPath) {
    return compiledAliasPath
  }

  if (request.endsWith(".css") && parent?.filename) {
    const compiledCssPath = path.resolve(path.dirname(parent.filename), request)
    const sourceCssPath = compiledCssPath.replace(
      `${unitOutDir}${path.sep}`,
      `${repoRoot}${path.sep}`
    )
    if (existsSync(sourceCssPath)) {
      return sourceCssPath
    }
  }

  return originalResolveFilename(request, parent, isMain, options)
}

require.extensions[".css"] = module => {
  module.exports = new Proxy<CssModuleExports>({} as CssModuleExports, {
    get: (_target, property) => String(property),
  })
}

require.cache["@/hooks"] = {
  exports: {
    useTranslation: () => ({
      t: translate,
    }),
  },
  filename: "@/hooks",
  id: "@/hooks",
  loaded: true,
} as NodeModule

require.cache["@/components"] = {
  exports: {
    Button: ({ children, title, ...props }: UnknownProps) =>
      React.createElement(
        "button",
        { type: "button", ...(props as Record<string, unknown>), title },
        children as React.ReactNode
      ),
  },
  filename: "@/components",
  id: "@/components",
  loaded: true,
} as NodeModule

function loadProviderImportCard() {
  return require("../../src/renderer/pages/RuleFormPage/ProviderImportCard") as typeof import("../../src/renderer/pages/RuleFormPage/ProviderImportCard")
}

function createParseResult(
  overrides: Partial<ProviderImportParseResult> = {}
): ProviderImportParseResult {
  return {
    format: overrides.format ?? "codex",
    draft: overrides.draft ?? {
      name: "OpenAI",
      protocol: "openai",
      apiAddress: "https://supercodex.space/v1",
      defaultModel: "gpt-5.4",
    },
    missingFields: overrides.missingFields ?? (["token", "website"] as ProviderImportField[]),
    warnings: overrides.warnings ?? [],
  }
}

test("ProviderImportCard renders parser warnings when present", () => {
  const { ProviderImportCard } = loadProviderImportCard()

  const markup = renderToStaticMarkup(
    React.createElement(ProviderImportCard, {
      format: "auto",
      rawValue: 'wire_api = "chat"',
      parseError: null,
      parseResult: createParseResult({
        warnings: ["Unsupported Codex wire_api: chat"],
      }),
      onFormatChange: () => {},
      onRawChange: () => {},
      onParse: () => {},
      onClear: () => {},
      onApply: () => {},
    })
  )

  assert.match(markup, /Warnings/)
  assert.match(markup, /Unsupported Codex wire_api: chat/)
})

test("ProviderImportCard renders localized labels in missing fields summary", () => {
  const { ProviderImportCard } = loadProviderImportCard()

  const markup = renderToStaticMarkup(
    React.createElement(ProviderImportCard, {
      format: "auto",
      rawValue: 'model_provider = "OpenAI"',
      parseError: null,
      parseResult: createParseResult({
        missingFields: ["token", "website"],
      }),
      onFormatChange: () => {},
      onRawChange: () => {},
      onParse: () => {},
      onClear: () => {},
      onApply: () => {},
    })
  )

  assert.match(markup, /Missing: Token, Website/)
  assert.doesNotMatch(markup, /Missing: token, website/)
})

test("ProviderImportCard renders parse error without preview when parse result is absent", () => {
  const { ProviderImportCard } = loadProviderImportCard()

  const markup = renderToStaticMarkup(
    React.createElement(ProviderImportCard, {
      format: "auto",
      rawValue: 'model_provider = "OpenAI"',
      parseError: "Unable to parse snippet",
      parseResult: null,
      onFormatChange: () => {},
      onRawChange: () => {},
      onParse: () => {},
      onClear: () => {},
      onApply: () => {},
    })
  )

  assert.match(markup, /Paste Config Import/)
  assert.match(markup, /Auto Detect/)
  assert.match(markup, /Codex/)
  assert.match(markup, /Claude Code/)
  assert.match(markup, /AOR/)
  assert.match(markup, /Import Format/)
  assert.match(markup, /Config Text/)
  assert.match(markup, /Parse/)
  assert.match(markup, /Clear/)
  assert.match(markup, /Unable to parse snippet/)
  assert.doesNotMatch(markup, /Parsed Preview/)
  assert.doesNotMatch(markup, /Detected Format/)
  assert.doesNotMatch(markup, /Apply To Form/)
})

test.after(() => {
  Module._resolveFilename = originalResolveFilename
  require.extensions[".css"] = originalCssExtension
})
