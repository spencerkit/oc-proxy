import assert from "node:assert/strict"
import { existsSync } from "node:fs"
import path from "node:path"
import { test } from "node:test"
import React from "react"
import { renderToStaticMarkup } from "react-dom/server"

import type { TranslateFunction } from "../../src/renderer/hooks/useTranslation"
import type { AgentConfig, AgentSourceFile, IntegrationClientKind } from "../../src/renderer/types"

const Module = require("node:module") as {
  _resolveFilename: (
    request: string,
    parent: { filename?: string } | undefined,
    isMain: boolean,
    options?: unknown
  ) => string
}

type CssModuleExports = Record<string, string>

const unitOutDir = path.join(process.cwd(), ".tmp/unit-tests")
const originalResolveFilename = Module._resolveFilename
const originalCssExtension = require.extensions[".css"]

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
  const compiledAliasPath = resolveCompiledAlias(request)
  if (compiledAliasPath) {
    return compiledAliasPath
  }

  if (request.endsWith(".css") && parent?.filename) {
    const compiledCssPath = path.resolve(path.dirname(parent.filename), request)
    const sourceCssPath = compiledCssPath.replace(
      `${unitOutDir}${path.sep}`,
      `${process.cwd()}${path.sep}`
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

function loadAgentEditContent() {
  return require("../../src/renderer/pages/AgentEditPage/AgentEditContent") as typeof import("../../src/renderer/pages/AgentEditPage/AgentEditContent")
}

const t: TranslateFunction = (key, options) => {
  if (options && "format" in options) {
    return `${key}:${String(options.format)}`
  }
  return key
}

function renderContent(input: {
  kind: IntegrationClientKind
  editMode?: "form" | "source"
  formData?: AgentConfig
  fallbackModelsText?: string
  showApiToken?: boolean
  supportsTimeout?: boolean
  timeoutText?: string
  timeoutError?: string
  sourceFiles?: AgentSourceFile[]
  activeSourceFile?: AgentSourceFile
  sourceContent?: string
  sourcePlaceholder?: string
  metaFormat?: string
  dirtySourceIds?: string[]
}) {
  const sourceFiles = input.sourceFiles ?? [
    {
      sourceId: "primary",
      label: "openclaw.json",
      filePath: "/tmp/openclaw.json",
      content: "{}",
    },
  ]

  const { AgentEditContent } = loadAgentEditContent()

  return renderToStaticMarkup(
    React.createElement(AgentEditContent, {
      kind: input.kind,
      editMode: input.editMode ?? "form",
      formData: input.formData ?? {},
      fallbackModelsText: input.fallbackModelsText ?? "",
      showApiToken: input.showApiToken ?? false,
      supportsTimeout:
        input.supportsTimeout ?? (input.kind === "claude" || input.kind === "opencode"),
      timeoutText: input.timeoutText ?? "",
      timeoutError: input.timeoutError ?? "",
      sourceFiles,
      activeSourceFile: input.activeSourceFile ?? sourceFiles[0],
      sourceContent: input.sourceContent ?? sourceFiles[0]?.content ?? "",
      sourcePlaceholder: input.sourcePlaceholder ?? "{}",
      metaFormat: input.metaFormat ?? "config.json",
      dirtySourceIds: input.dirtySourceIds ?? [],
      t,
      onFormDataChange: () => {},
      onFallbackModelsTextChange: () => {},
      onToggleApiTokenVisibility: () => {},
      onTimeoutTextChange: () => {},
      onSourceSelect: () => {},
      onSourceChange: () => {},
      defaultOpenclawAgentId: "default",
      defaultOpenclawProviderId: "aor_shared",
      defaultOpenclawApiFormat: "openai-responses",
    })
  )
}

test("renders OpenClaw-specific form fields when kind is openclaw", () => {
  const markup = renderContent({
    kind: "openclaw",
    formData: {
      agentId: "workspace-alpha",
      providerId: "aor_shared",
      model: "gpt-4.1",
      apiFormat: "openai-responses",
      url: "http://127.0.0.1:8899/oc/dev/v1",
      apiToken: "secret",
    },
    fallbackModelsText: "gpt-4.1-mini",
  })

  assert.match(markup, /agentManagement\.openclawAgentId/)
  assert.match(markup, /agentManagement\.openclawProviderId/)
  assert.match(markup, /agentManagement\.openclawApiFormat/)
  assert.match(markup, /agentManagement\.openclawFallbackModels/)
  assert.doesNotMatch(markup, /agentManagement\.alwaysThinkingEnabled/)
})

test("renders masked generic token field with visibility toggle", () => {
  const markup = renderContent({
    kind: "claude",
    formData: {
      apiToken: "secret",
    },
  })

  assert.match(markup, /type="password"/)
  assert.match(markup, /agentManagement\.showToken/)
})

test("renders generic form fields for non-OpenClaw kinds", () => {
  const markup = renderContent({
    kind: "claude",
    formData: {
      model: "claude-sonnet-4-5-20250929",
      apiToken: "secret",
      url: "http://localhost:8080/oc/dev",
      alwaysThinkingEnabled: true,
    },
    timeoutText: "300000",
  })

  assert.match(markup, /agentManagement\.url/)
  assert.match(markup, /agentManagement\.apiToken/)
  assert.match(markup, /agentManagement\.model/)
  assert.match(markup, /agentManagement\.timeout/)
  assert.match(markup, /agentManagement\.alwaysThinkingEnabled/)
  assert.doesNotMatch(markup, /agentManagement\.openclawAgentId/)
})

test("shows format action in OpenClaw source mode", () => {
  const markup = renderContent({
    kind: "openclaw",
    editMode: "source",
    sourceFiles: [
      {
        sourceId: "primary",
        label: "openclaw.json",
        filePath: "/tmp/openclaw.json",
        content: "{}",
      },
    ],
    sourceContent: "{}",
    metaFormat: "openclaw.json + agent files",
  })

  assert.match(markup, /agentManagement\.sourceEditor/)
  assert.match(markup, /agentManagement\.formatCurrentFile/)
})

test("shows OpenClaw source hint about validating related files", () => {
  const markup = renderContent({
    kind: "openclaw",
    editMode: "source",
    sourceFiles: [
      {
        sourceId: "models",
        label: "models.json",
        filePath: "/tmp/agents/workspace-alpha/agent/models.json",
        content: "{}",
      },
    ],
    sourceContent: "{}",
    metaFormat: "openclaw.json + agent files",
  })

  assert.match(markup, /agentManagement\.openclawSourceValidationHint/)
})

test("marks dirty OpenClaw source tabs", () => {
  const markup = renderContent({
    kind: "openclaw",
    editMode: "source",
    sourceFiles: [
      {
        sourceId: "primary",
        label: "openclaw.json",
        filePath: "/tmp/openclaw.json",
        content: "{}",
      },
      {
        sourceId: "models",
        label: "models.json",
        filePath: "/tmp/agents/workspace-alpha/agent/models.json",
        content: "{}",
      },
    ],
    activeSourceFile: {
      sourceId: "primary",
      label: "openclaw.json",
      filePath: "/tmp/openclaw.json",
      content: "{}",
    },
    sourceContent: "{}",
    metaFormat: "openclaw.json + agent files",
    dirtySourceIds: ["models"],
  })

  assert.match(markup, /models\.json \*/)
})

test("shows OpenClaw source files and active source hint", () => {
  const markup = renderContent({
    kind: "openclaw",
    editMode: "source",
    sourceFiles: [
      {
        sourceId: "primary",
        label: "openclaw.json",
        filePath: "/tmp/openclaw.json",
        content: "{}",
      },
      {
        sourceId: "models",
        label: "models.json",
        filePath: "/tmp/agents/workspace-alpha/agent/models.json",
        content: "{}",
      },
    ],
    sourceContent: "{}",
    metaFormat: "openclaw.json + agent files",
  })

  assert.match(markup, /agentManagement\.sourceEditor/)
  assert.match(markup, /openclaw\.json/)
  assert.match(markup, /models\.json/)
  assert.match(markup, /agentManagement\.sourceHint:openclaw\.json/)
})

process.on("exit", () => {
  Module._resolveFilename = originalResolveFilename
  if (originalCssExtension) {
    require.extensions[".css"] = originalCssExtension
    return
  }
  delete require.extensions[".css"]
})
