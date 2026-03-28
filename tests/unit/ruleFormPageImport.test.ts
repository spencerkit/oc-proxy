import assert from "node:assert/strict"
import { existsSync } from "node:fs"
import path from "node:path"
import { test } from "node:test"
import React from "react"
import { renderToStaticMarkup } from "react-dom/server"

import type { ProxyConfig } from "../../src/renderer/types"

const Module = require("node:module") as {
  _resolveFilename: (
    request: string,
    parent: { filename?: string } | undefined,
    isMain: boolean,
    options?: unknown
  ) => string
}

type CssModuleExports = Record<string, string>
type TestState<T> = { current: T }
type UnknownProps = Record<string, unknown>
type ReactElementNode = React.ReactElement<UnknownProps>
type InputElementNode = React.ReactElement<React.ComponentProps<"input">>
type TextAreaElementNode = React.ReactElement<React.ComponentProps<"textarea">>
type SelectElementNode = React.ReactElement<React.ComponentProps<"select">>
type ButtonElementNode = React.ReactElement<React.ComponentProps<"button">>
type FormElementNode = React.ReactElement<React.ComponentProps<"form">>

const unitOutDir = path.join(process.cwd(), ".tmp/unit-tests")
const originalResolveFilename = Module._resolveFilename
const originalCssExtension = require.extensions[".css"]
const originalDocument = globalThis.document

const configStateValue: TestState<ProxyConfig | null> = { current: null }
let currentParams: { groupId?: string; providerId?: string } = {}

Object.assign(globalThis, {
  document: {
    getElementById: () => null,
  },
})

function translate(key: string, options?: Record<string, unknown>): string {
  const translations: Record<string, string> = {
    "ruleCreatePage.title": "Create Rule",
    "ruleEditPage.title": "Edit Rule",
    "ruleCreatePage.newRule": "New Rule",
    "ruleForm.sectionRouting": "Routing",
    "ruleForm.importEntryTitle": "Import Config",
    "ruleForm.importTitle": "Paste Config Import",
    "ruleForm.importHint": "Paste a supported config snippet to auto-fill provider fields.",
    "ruleForm.importOpen": "Open Import",
    "ruleForm.importFormat": "Import Format",
    "ruleForm.importFormatAuto": "Auto Detect",
    "ruleForm.importFormatCodex": "Codex",
    "ruleForm.importFormatClaudeCode": "Claude Code",
    "ruleForm.importFormatAor": "AOR",
    "ruleForm.importInputLabel": "Config Text",
    "ruleForm.importInputPlaceholder": "Paste config here",
    "ruleForm.importParse": "Parse",
    "ruleForm.importClear": "Clear",
    "ruleForm.importApply": "Apply To Form",
    "ruleForm.importErrorUnsupportedProtocol": "Imported provider protocol is not supported",
    "header.providers": "Providers",
    "header.backToService": "Back",
    "servicePage.groupPath": "Group",
    "servicePage.ruleName": "Rule Name",
    "servicePage.token": "Token",
    "servicePage.apiAddress": "API Address",
    "servicePage.defaultModel": "Default Model",
    "app.statusLoading": "Loading",
    "validation.required": "{{field}} is required",
    "ruleProtocol.anthropic": "Anthropic",
    "ruleProtocol.openai_completion": "OpenAI Chat Completions",
    "ruleProtocol.openai": "OpenAI Responses",
  }

  if (key === "validation.required" && typeof options?.field === "string") {
    return `${String(options.field)} is required`
  }

  return translations[key] ?? key
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
  if (
    request === "react-router-dom" ||
    request === "@/hooks" ||
    request === "@/store" ||
    request === "@/utils/relax" ||
    request === "@/components"
  ) {
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

require.cache["react-router-dom"] = {
  exports: {
    useNavigate: () => () => {},
    useParams: () => currentParams,
  },
  filename: "react-router-dom",
  id: "react-router-dom",
  loaded: true,
} as NodeModule

require.cache["@/hooks"] = {
  exports: {
    useLogs: () => ({
      showToast: () => {},
    }),
    useTranslation: () => ({
      t: translate,
    }),
  },
  filename: "@/hooks",
  id: "@/hooks",
  loaded: true,
} as NodeModule

require.cache["@/store"] = {
  exports: {
    configState: { key: "configState" },
    saveConfigAction: { key: "saveConfigAction" },
    testRuleQuotaDraftAction: { key: "testRuleQuotaDraftAction" },
  },
  filename: "@/store",
  id: "@/store",
  loaded: true,
} as NodeModule

require.cache["@/utils/relax"] = {
  exports: {
    useRelaxValue: (state: { key?: string }) => {
      if (state?.key === "configState") {
        return configStateValue.current
      }
      return null
    },
    useActions: () => [async () => ({}), async () => ({})],
  },
  filename: "@/utils/relax",
  id: "@/utils/relax",
  loaded: true,
} as NodeModule

require.cache["@/components"] = {
  exports: {
    Button: ({ children, ...props }: UnknownProps) =>
      React.createElement(
        "button",
        { type: "button", ...(props as Record<string, unknown>) },
        children as React.ReactNode
      ),
    Input: ({ label, hint, error, endAdornment, ...props }: UnknownProps) =>
      React.createElement(
        React.Fragment,
        null,
        label
          ? React.createElement(
              "label",
              { htmlFor: props.id as string | undefined },
              label as React.ReactNode
            )
          : null,
        React.createElement("input", props),
        endAdornment ? React.createElement("span", null, endAdornment as React.ReactNode) : null,
        error ? React.createElement("p", null, error as React.ReactNode) : null,
        !error && hint ? React.createElement("p", null, hint as React.ReactNode) : null
      ),
    JsonTreeView: ({ data }: { data: unknown }) =>
      React.createElement("pre", null, JSON.stringify(data)),
    Switch: ({ checked, onChange, ...props }: UnknownProps) =>
      React.createElement("input", {
        type: "checkbox",
        checked,
        onChange,
        ...props,
      }),
    Modal: ({ open, title, children, footer }: UnknownProps) =>
      open
        ? React.createElement(
            "div",
            {
              role: "dialog",
            },
            title ? React.createElement("h2", null, title as React.ReactNode) : null,
            children as React.ReactNode,
            footer as React.ReactNode
          )
        : null,
  },
  filename: "@/components",
  id: "@/components",
  loaded: true,
} as NodeModule

function loadRuleFormPage() {
  return require("../../src/renderer/pages/RuleFormPage/RuleFormPage") as typeof import("../../src/renderer/pages/RuleFormPage/RuleFormPage")
}

function createConfig(): ProxyConfig {
  return {
    server: {
      host: "0.0.0.0",
      port: 8899,
      authEnabled: false,
      localBearerToken: "",
    },
    compat: {
      strictMode: false,
      textToolCallFallbackEnabled: true,
    },
    logging: {
      captureBody: false,
    },
    ui: {
      theme: "light",
      locale: "en-US",
      localeMode: "auto",
      launchOnStartup: false,
      autoStartServer: true,
      closeToTray: true,
      quotaAutoRefreshMinutes: 5,
      autoUpdateEnabled: true,
    },
    remoteGit: {
      enabled: false,
      repoUrl: "",
      token: "",
      branch: "main",
    },
    providers: [
      {
        id: "provider-1",
        name: "Existing Provider",
        protocol: "openai",
        token: "secret",
        apiAddress: "https://api.example.com/v1",
        website: "https://example.com",
        defaultModel: "gpt-test",
        modelMappings: {},
        quota: {
          enabled: false,
          provider: "custom",
          endpoint: "",
          method: "GET",
          useRuleToken: true,
          customToken: "",
          authHeader: "Authorization",
          authScheme: "Bearer",
          customHeaders: {},
          unitType: "percentage",
          lowThresholdPercent: 10,
          response: {},
        },
      },
    ],
    groups: [],
  }
}

function resetHarness() {
  configStateValue.current = createConfig()
  currentParams = {}
}

function resolveRenderedTree(node: React.ReactNode): React.ReactNode {
  if (Array.isArray(node)) {
    return node.map(child => resolveRenderedTree(child))
  }
  if (!React.isValidElement(node)) {
    return node
  }

  const element = node as ReactElementNode
  if (typeof element.type === "function") {
    return resolveRenderedTree(
      (element.type as (props: UnknownProps) => React.ReactNode)(element.props)
    )
  }

  const children = resolveRenderedTree(element.props.children as React.ReactNode)
  return React.cloneElement(element, element.props, children)
}

function createComponentHarness(mode: "create" | "edit") {
  const slots: unknown[] = []
  const effectDependencySlots: Array<readonly unknown[] | undefined> = []

  const renderOnce = () => {
    const originalUseState = React.useState
    const originalUseEffect = React.useEffect
    let stateCallIndex = 0
    let effectCallIndex = 0

    React.useState = ((initialState?: unknown) => {
      const slotIndex = stateCallIndex
      stateCallIndex += 1
      if (!(slotIndex in slots)) {
        slots[slotIndex] =
          typeof initialState === "function" ? (initialState as () => unknown)() : initialState
      }
      const setValue = (nextValue: unknown) => {
        slots[slotIndex] =
          typeof nextValue === "function"
            ? (nextValue as (previous: unknown) => unknown)(slots[slotIndex])
            : nextValue
      }
      return [slots[slotIndex], setValue]
    }) as unknown as typeof React.useState
    React.useEffect = ((effect: () => void, dependencies?: readonly unknown[]) => {
      const slotIndex = effectCallIndex
      effectCallIndex += 1

      const previousDependencies = effectDependencySlots[slotIndex]
      const shouldRun =
        !dependencies ||
        !previousDependencies ||
        dependencies.length !== previousDependencies.length ||
        dependencies.some(
          (dependency, index) => !Object.is(dependency, previousDependencies[index])
        )

      effectDependencySlots[slotIndex] = dependencies
      if (shouldRun) {
        effect()
      }
    }) as unknown as typeof React.useEffect

    try {
      const { RuleFormPage } = loadRuleFormPage()
      return resolveRenderedTree(RuleFormPage({ mode }))
    } finally {
      React.useState = originalUseState
      React.useEffect = originalUseEffect
    }
  }

  const renderReady = () => {
    let previousTree: React.ReactNode = null
    for (let attempt = 0; attempt < 5; attempt += 1) {
      const tree = renderOnce()
      if (tree === previousTree) {
        return tree
      }
      previousTree = tree
    }
    return previousTree
  }

  return { renderReady }
}

function findElement(
  node: React.ReactNode,
  predicate: (element: ReactElementNode) => boolean
): ReactElementNode | null {
  if (!node) return null
  if (Array.isArray(node)) {
    for (const child of node) {
      const match = findElement(child, predicate)
      if (match) return match
    }
    return null
  }
  if (!React.isValidElement(node)) {
    return null
  }

  const element = node as ReactElementNode
  if (predicate(element)) {
    return element
  }

  return findElement(element.props.children as React.ReactNode, predicate)
}

function createSelectChangeEvent(value: string): React.ChangeEvent<HTMLSelectElement> {
  return { target: { value } } as unknown as React.ChangeEvent<HTMLSelectElement>
}

function createInputChangeEvent(value: string): React.ChangeEvent<HTMLInputElement> {
  return { target: { value } } as unknown as React.ChangeEvent<HTMLInputElement>
}

function createTextAreaChangeEvent(value: string): React.ChangeEvent<HTMLTextAreaElement> {
  return { target: { value } } as unknown as React.ChangeEvent<HTMLTextAreaElement>
}

function createFormSubmitEvent(): React.FormEvent<HTMLFormElement> {
  return { preventDefault() {} } as unknown as React.FormEvent<HTMLFormElement>
}

function findInputById(tree: React.ReactNode, id: string): InputElementNode {
  const element = findElement(tree, node => node.type === "input" && node.props.id === id)
  assert.ok(element)
  return element as InputElementNode
}

function findTextareaById(tree: React.ReactNode, id: string): TextAreaElementNode {
  const element = findElement(tree, node => node.type === "textarea" && node.props.id === id)
  assert.ok(element)
  return element as TextAreaElementNode
}

function findImportFormatSelect(tree: React.ReactNode): SelectElementNode {
  const element = findElement(
    tree,
    node => node.type === "select" && node.props.id === "provider-import-format"
  )
  assert.ok(element)
  return element as SelectElementNode
}

function findButtonByText(tree: React.ReactNode, label: string): ButtonElementNode {
  const element = findElement(
    tree,
    node => node.type === "button" && String(node.props.children) === label
  )
  assert.ok(element)
  return element as ButtonElementNode
}

function findForm(tree: React.ReactNode): FormElementNode {
  const element = findElement(tree, node => node.type === "form")
  assert.ok(element)
  return element as FormElementNode
}

test("RuleFormPage shows provider import entry only in create mode above routing", () => {
  resetHarness()

  const createHarness = createComponentHarness("create")
  const createMarkup = renderToStaticMarkup(createHarness.renderReady() as React.ReactElement)

  assert.match(createMarkup, /Import Config/)
  assert.match(createMarkup, /Open Import/)
  assert.doesNotMatch(createMarkup, /Paste Config Import/)
  assert.match(createMarkup, /Routing/)
  assert.ok(createMarkup.indexOf("Import Config") < createMarkup.indexOf("Routing"))

  currentParams = { providerId: "provider-1" }
  const editHarness = createComponentHarness("edit")
  const editMarkup = renderToStaticMarkup(editHarness.renderReady() as React.ReactElement)

  assert.doesNotMatch(editMarkup, /Import Config/)
  assert.doesNotMatch(editMarkup, /Open Import/)
  assert.match(editMarkup, /Routing/)
})

test("RuleFormPage opens provider import popup on demand", () => {
  resetHarness()
  const harness = createComponentHarness("create")

  let tree = harness.renderReady()
  let markup = renderToStaticMarkup(tree as React.ReactElement)

  assert.doesNotMatch(markup, /Paste Config Import/)
  assert.doesNotMatch(markup, /Config Text/)

  const openButton = findButtonByText(tree, "Open Import")
  openButton.props.onClick?.({} as React.MouseEvent<HTMLButtonElement>)

  tree = harness.renderReady()
  markup = renderToStaticMarkup(tree as React.ReactElement)

  assert.match(markup, /Paste Config Import/)
  assert.match(markup, /Config Text/)
})

test("RuleFormPage clear import resets format back to auto", () => {
  resetHarness()
  const harness = createComponentHarness("create")

  let tree = harness.renderReady()
  const openButton = findButtonByText(tree, "Open Import")
  openButton.props.onClick?.({} as React.MouseEvent<HTMLButtonElement>)

  tree = harness.renderReady()
  let formatSelect = findImportFormatSelect(tree)
  assert.equal(formatSelect.props.value, "auto")

  formatSelect.props.onChange?.(createSelectChangeEvent("codex"))

  tree = harness.renderReady()
  formatSelect = findImportFormatSelect(tree)
  assert.equal(formatSelect.props.value, "codex")

  const clearButton = findButtonByText(tree, "Clear")
  clearButton.props.onClick?.({} as React.MouseEvent<HTMLButtonElement>)

  tree = harness.renderReady()
  formatSelect = findImportFormatSelect(tree)
  assert.equal(formatSelect.props.value, "auto")
})

test("RuleFormPage shows a precise error when imported AOR protocol is unsupported", () => {
  resetHarness()
  const harness = createComponentHarness("create")

  let tree = harness.renderReady()
  const openButton = findButtonByText(tree, "Open Import")
  openButton.props.onClick?.({} as React.MouseEvent<HTMLButtonElement>)

  tree = harness.renderReady()
  const importTextarea = findTextareaById(tree, "provider-import-raw")
  importTextarea.props.onChange?.(
    createTextAreaChangeEvent(
      JSON.stringify({
        format: "aor-provider/v1",
        protocol: "custom",
        name: "Unsupported",
        api_key: "secret",
        base_url: "https://unsupported.example.com/v1",
        model: "test-model",
      })
    )
  )

  tree = harness.renderReady()
  const parseButton = findButtonByText(tree, "Parse")
  parseButton.props.onClick?.({} as React.MouseEvent<HTMLButtonElement>)

  const markup = renderToStaticMarkup(harness.renderReady() as React.ReactElement)
  assert.match(markup, /Imported provider protocol is not supported/)
})

test("RuleFormPage parse and apply fills imported fields, preserves omitted website, and clears validation errors", async () => {
  resetHarness()
  const harness = createComponentHarness("create")

  let tree = harness.renderReady()
  const websiteInput = findInputById(tree, "website")
  websiteInput.props.onChange?.(createInputChangeEvent("https://preserved.example.com"))

  tree = harness.renderReady()
  const form = findForm(tree)
  await form.props.onSubmit?.(createFormSubmitEvent())

  let markup = renderToStaticMarkup(harness.renderReady() as React.ReactElement)
  assert.match(markup, /Rule Name is required/)
  assert.match(markup, /Token is required/)
  assert.match(markup, /API Address is required/)
  assert.match(markup, /Default Model is required/)

  tree = harness.renderReady()
  const openButton = findButtonByText(tree, "Open Import")
  openButton.props.onClick?.({} as React.MouseEvent<HTMLButtonElement>)

  tree = harness.renderReady()
  const importTextarea = findTextareaById(tree, "provider-import-raw")
  importTextarea.props.onChange?.(
    createTextAreaChangeEvent(
      JSON.stringify({
        format: "aor-provider/v1",
        protocol: "openai",
        name: "Imported Provider",
        api_key: "imported-secret",
        base_url: "https://imported.example.com/v1",
        model: "gpt-imported",
      })
    )
  )

  tree = harness.renderReady()
  const parseButton = findButtonByText(tree, "Parse")
  parseButton.props.onClick?.({} as React.MouseEvent<HTMLButtonElement>)

  tree = harness.renderReady()
  const applyButton = findButtonByText(tree, "Apply To Form")
  applyButton.props.onClick?.({} as React.MouseEvent<HTMLButtonElement>)

  tree = harness.renderReady()
  markup = renderToStaticMarkup(tree as React.ReactElement)

  assert.equal(findInputById(tree, "name").props.value, "Imported Provider")
  assert.equal(findInputById(tree, "token").props.value, "imported-secret")
  assert.equal(findInputById(tree, "apiAddress").props.value, "https://imported.example.com/v1")
  assert.equal(findInputById(tree, "defaultModel").props.value, "gpt-imported")
  assert.equal(findInputById(tree, "website").props.value, "https://preserved.example.com")
  assert.doesNotMatch(markup, /Paste Config Import/)
  assert.doesNotMatch(markup, /Rule Name is required/)
  assert.doesNotMatch(markup, /Token is required/)
  assert.doesNotMatch(markup, /API Address is required/)
  assert.doesNotMatch(markup, /Default Model is required/)
})

process.on("exit", () => {
  Module._resolveFilename = originalResolveFilename
  Object.assign(globalThis, {
    document: originalDocument,
  })
  if (originalCssExtension) {
    require.extensions[".css"] = originalCssExtension
    return
  }
  delete require.extensions[".css"]
})
