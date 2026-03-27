import assert from "node:assert/strict"
import { existsSync } from "node:fs"
import path from "node:path"
import { test } from "node:test"
import React from "react"
import { renderToStaticMarkup } from "react-dom/server"

import type {
  GroupRuntimeStatus,
  Provider,
  ProviderModelHealthSnapshot,
} from "../../src/renderer/types"

const Module = require("node:module") as {
  _resolveFilename: (
    request: string,
    parent: { filename?: string } | undefined,
    isMain: boolean,
    options?: unknown
  ) => string
}

type CssModuleExports = Record<string, string>

type TranslateOptions = Record<string, unknown> | undefined

type UnknownProps = Record<string, unknown>

const repoRoot = path.resolve(__dirname, "../../../..")
const unitOutDir = path.join(repoRoot, ".tmp/unit-tests")
const originalResolveFilename = Module._resolveFilename
const originalCssExtension = require.extensions[".css"]

const translations: Record<string, string> = {
  "servicePage.ruleName": "PROVIDER",
  "servicePage.addRule": "Add Provider",
  "servicePage.deleteRule": "Delete",
  "servicePage.unlinkRule": "Unlink",
  "servicePage.editRule": "Edit Provider",
  "servicePage.activateRule": "Set Active",
  "servicePage.testModel": "Test Model",
  "servicePage.testingModel": "Testing",
  "servicePage.testAllProviders": "Test All",
  "servicePage.testingAllProviders": "Testing All",
  "servicePage.defaultModel": "Default Model",
  "servicePage.apiAddress": "API Address",
  "servicePage.noRulesHint": "No providers yet",
  "servicePage.current": "Current",
  "servicePage.preferred": "Preferred",
  "servicePage.failover": "Failover",
  "servicePage.availabilityAvailable": "Available",
  "servicePage.availabilityUnavailable": "Unavailable",
  "servicePage.availabilityUntested": "Untested",
  "ruleForm.officialWebsite": "Official Website",
  "ruleProtocol.openai": "OpenAI",
  "ruleProtocol.openai_completion": "OpenAI Completion",
  "ruleProtocol.anthropic": "Anthropic",
}

function translate(key: string, options?: TranslateOptions): string {
  if (key in translations) {
    return translations[key]
  }
  if (options?.count !== undefined) {
    return `${key}:${String(options.count)}`
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
  if (request === "react-router-dom" || request === "@/hooks" || request === "@/components") {
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

require.cache["react-router-dom"] = {
  exports: {
    useNavigate: () => () => {},
  },
  filename: "react-router-dom",
  id: "react-router-dom",
  loaded: true,
} as NodeModule

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

function loadProviderList() {
  return require("../../src/renderer/pages/ServicePage/ProviderList") as typeof import("../../src/renderer/pages/ServicePage/ProviderList")
}

function loadCatalogProviderList() {
  return require("../../src/renderer/pages/ProvidersPage/ProviderList") as typeof import("../../src/renderer/pages/ProvidersPage/ProviderList")
}

function createProvider(overrides: Partial<Provider> = {}): Provider {
  return {
    id: overrides.id ?? "provider-1",
    name: overrides.name ?? "Provider One",
    protocol: overrides.protocol ?? "openai",
    token: overrides.token ?? "secret",
    apiAddress: overrides.apiAddress ?? "https://provider.example.com/v1",
    website: overrides.website,
    defaultModel: overrides.defaultModel ?? "gpt-4.1-mini",
    modelMappings: overrides.modelMappings ?? {},
    quota: overrides.quota ?? {
      enabled: false,
      provider: "",
      endpoint: "",
      method: "GET",
      useRuleToken: false,
      customToken: "",
      authHeader: "Authorization",
      authScheme: "Bearer",
      customHeaders: {},
      unitType: "percentage",
      lowThresholdPercent: 20,
      response: {},
    },
    cost: overrides.cost,
  }
}

function renderProviderList(input: {
  providers: Provider[]
  activeProviderId: string | null
  groupRuntime?: GroupRuntimeStatus | null
  testingProviderIds?: Record<string, boolean | undefined>
  providerHealthByProviderId?: Record<string, ProviderModelHealthSnapshot | null | undefined>
}) {
  const { ProviderList } = loadProviderList()
  return renderToStaticMarkup(
    React.createElement(ProviderList, {
      providers: input.providers,
      activeProviderId: input.activeProviderId,
      groupRuntime: input.groupRuntime,
      onActivate: () => {},
      onDelete: () => {},
      onEdit: () => {},
      onAdd: () => {},
      onTestModel: () => {},
      testingProviderIds: input.testingProviderIds,
      providerHealthByProviderId: input.providerHealthByProviderId,
      testingAll: false,
    })
  )
}

function renderCatalogProviderList(input: {
  providers: Provider[]
  providerHealthByProviderId?: Record<string, ProviderModelHealthSnapshot | null | undefined>
}) {
  const { ProviderList } = loadCatalogProviderList()
  return renderToStaticMarkup(
    React.createElement(ProviderList, {
      providers: input.providers,
      providerHealthByProviderId: input.providerHealthByProviderId,
      onDelete: () => {},
      onEdit: () => {},
      onAdd: () => {},
    })
  )
}

test("shows preferred, current, failover, and availability badges when runtime provider differs", () => {
  const providers = [
    createProvider({ id: "provider-a", name: "Preferred Provider", defaultModel: "gpt-4.1" }),
    createProvider({ id: "provider-b", name: "Failover Provider", defaultModel: "gpt-4o-mini" }),
  ]

  const markup = renderProviderList({
    providers,
    activeProviderId: "provider-a",
    groupRuntime: {
      groupId: "dev",
      currentProviderId: "provider-b",
      failoverActiveProviderId: "provider-b",
      failoverActive: true,
    },
    providerHealthByProviderId: {
      "provider-b": {
        groupId: "dev",
        providerId: "provider-b",
        status: "available",
        latencyMs: 128,
        testedAt: "2026-03-27T10:00:00.000Z",
      },
    },
  })

  assert.match(markup, /Preferred Provider/)
  assert.match(markup, /Failover Provider/)
  assert.match(markup, />Preferred</)
  assert.match(markup, />Current</)
  assert.match(markup, />Failover</)
  assert.match(markup, />Available</)
  assert.doesNotMatch(markup, />128ms</)
})

test("shows only the preferred badge when runtime status is unavailable", () => {
  const markup = renderProviderList({
    providers: [createProvider({ id: "provider-a", name: "Preferred Provider" })],
    activeProviderId: "provider-a",
  })

  assert.match(markup, />Preferred</)
  assert.doesNotMatch(markup, />Current</)
})

test("keeps provider name, protocol, status, and compact metadata visible in service cards", () => {
  const markup = renderProviderList({
    providers: [
      createProvider({
        id: "provider-a",
        name: "Provider A",
        protocol: "openai",
        apiAddress: "https://api.openai.com/v1/responses",
        defaultModel: "gpt-4.1-mini",
      }),
    ],
    activeProviderId: "provider-a",
    providerHealthByProviderId: {
      "provider-a": {
        groupId: "dev",
        providerId: "provider-a",
        status: "available",
        latencyMs: 42,
        testedAt: "2026-03-27T10:00:00.000Z",
      },
    },
  })

  assert.match(markup, /Provider A/)
  assert.match(markup, />OpenAI</)
  assert.match(markup, />Available</)
  assert.match(markup, />Default Model</)
  assert.match(markup, />gpt-4\.1-mini</)
  assert.match(markup, />API Address</)
  assert.match(markup, />api\.openai\.com</)
  assert.match(markup, /Edit Provider: Provider A/)
  assert.match(markup, /Delete: Provider A/)
})

test("keeps provider name, protocol, status, default model, and compact API address visible in catalog cards", () => {
  const markup = renderCatalogProviderList({
    providers: [
      createProvider({
        id: "provider-a",
        name: "Provider A",
        protocol: "openai",
        apiAddress: "https://api.openai.com/v1/responses?foo=bar",
        defaultModel: "gpt-4.1-mini",
      }),
    ],
    providerHealthByProviderId: {
      "provider-a": {
        groupId: "dev",
        providerId: "provider-a",
        status: "available",
        latencyMs: 42,
        testedAt: "2026-03-27T10:00:00.000Z",
      },
    },
  })

  assert.match(markup, /Provider A/)
  assert.match(markup, />OpenAI</)
  assert.match(markup, />Available</)
  assert.match(markup, />Default Model</)
  assert.match(markup, />gpt-4\.1-mini</)
  assert.match(markup, />API Address</)
  assert.match(markup, />api\.openai\.com</)
})

test("shows testing badge while a provider test is in progress", () => {
  const markup = renderProviderList({
    providers: [createProvider({ id: "provider-a", name: "Provider A" })],
    activeProviderId: "provider-a",
    testingProviderIds: {
      "provider-a": true,
    },
  })

  assert.match(markup, />Testing</)
})

test.after(() => {
  Module._resolveFilename = originalResolveFilename
  require.extensions[".css"] = originalCssExtension
  delete require.cache["react-router-dom"]
  delete require.cache["@/hooks"]
  delete require.cache["@/components"]
})
