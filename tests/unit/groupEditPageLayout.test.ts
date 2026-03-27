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
type SaveConfigHandler = (config: ProxyConfig) => Promise<unknown>
type TestState<T> = { current: T }
type UnknownProps = Record<string, unknown>

type ReactElementNode = React.ReactElement<UnknownProps>
type InputElementNode = React.ReactElement<React.ComponentProps<"input">>
type FormElementNode = React.ReactElement<React.ComponentProps<"form">>

const unitOutDir = path.join(process.cwd(), ".tmp/unit-tests")
const originalResolveFilename = Module._resolveFilename
const originalCssExtension = require.extensions[".css"]

const configStateValue: TestState<ProxyConfig | null> = { current: null }
const saveConfigCalls: ProxyConfig[] = []
const navigationCalls: string[] = []
const toastCalls: Array<{ message: string; type: string }> = []
let saveConfigImpl: SaveConfigHandler = async config => config
let currentGroupId = "dev"
const navigateImpl = (to: string) => navigationCalls.push(to)
const showToastImpl = (message: string, type: string) => {
  toastCalls.push({ message, type })
}
const translateImpl = (key: string, options?: Record<string, unknown>) => {
  if ((key === "validation.required" || key === "validation.invalidFormat") && options?.field) {
    return `${key}:${String(options.field)}`
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
    useNavigate: () => navigateImpl,
    useParams: () => ({ groupId: currentGroupId }),
  },
  filename: "react-router-dom",
  id: "react-router-dom",
  loaded: true,
} as NodeModule

require.cache["@/hooks"] = {
  exports: {
    useLogs: () => ({
      showToast: showToastImpl,
    }),
    useTranslation: () => ({
      t: translateImpl,
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
    useActions: () => [
      async (config: ProxyConfig) => {
        saveConfigCalls.push(config)
        return saveConfigImpl(config)
      },
    ],
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
  },
  filename: "@/components",
  id: "@/components",
  loaded: true,
} as NodeModule

function loadGroupEditPage() {
  return require("../../src/renderer/pages/GroupEditPage/GroupEditPage") as typeof import("../../src/renderer/pages/GroupEditPage/GroupEditPage")
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
    providers: [],
    groups: [
      {
        id: "dev",
        name: "Dev",
        models: ["claude-sonnet-4-5"],
        providerIds: ["p1", "p2"],
        activeProviderId: "p1",
        providers: [],
        failover: {
          enabled: false,
          failureThreshold: 3,
          cooldownSeconds: 300,
        },
      },
    ],
  }
}

function resetHarness(config: ProxyConfig = createConfig()) {
  configStateValue.current = config
  saveConfigCalls.length = 0
  navigationCalls.length = 0
  toastCalls.length = 0
  saveConfigImpl = async nextConfig => nextConfig
  currentGroupId = "dev"
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

function createComponentHarness() {
  let slots: unknown[] = []
  let effectDependencySlots: Array<readonly unknown[] | undefined> = []

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
      const { GroupEditPage } = loadGroupEditPage()
      return resolveRenderedTree(GroupEditPage({}))
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

  return {
    renderReady,
    reset() {
      slots = []
      effectDependencySlots = []
    },
  }
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

function createCheckboxChangeEvent(checked: boolean): React.ChangeEvent<HTMLInputElement> {
  return { target: { checked } } as unknown as React.ChangeEvent<HTMLInputElement>
}

function createInputChangeEvent(value: string): React.ChangeEvent<HTMLInputElement> {
  return { target: { value } } as unknown as React.ChangeEvent<HTMLInputElement>
}

function createFormSubmitEvent(): React.FormEvent<HTMLFormElement> {
  return { preventDefault() {} } as unknown as React.FormEvent<HTMLFormElement>
}

function findInputById(tree: React.ReactNode, id: string): InputElementNode {
  const element = findElement(tree, node => node.type === "input" && node.props.id === id)
  assert.ok(element)
  return element as InputElementNode
}

function findForm(tree: React.ReactNode): FormElementNode {
  const element = findElement(tree, node => node.type === "form")
  assert.ok(element)
  return element as FormElementNode
}

test("renders group failover controls on group edit page", () => {
  resetHarness()
  const harness = createComponentHarness()

  const markup = renderToStaticMarkup(harness.renderReady() as React.ReactElement)

  assert.match(markup, /groupEditPage\.sectionFailover/)
  assert.match(markup, /groupEditPage\.failoverEnabled/)
  assert.match(markup, /groupEditPage\.failoverFailureThreshold/)
  assert.match(markup, /groupEditPage\.failoverCooldownSeconds/)
})

test("saves edited failover settings into group config", async () => {
  const initialConfig = createConfig()
  resetHarness(initialConfig)
  const harness = createComponentHarness()

  let tree = harness.renderReady()
  const thresholdInput = findInputById(tree, "failoverFailureThreshold")
  const cooldownInput = findInputById(tree, "failoverCooldownSeconds")
  const enabledInput = findInputById(tree, "failoverEnabled")

  assert.equal(thresholdInput.props.value, "3")
  assert.equal(cooldownInput.props.value, "300")
  assert.equal(enabledInput.props.checked, false)

  enabledInput.props.onChange?.(createCheckboxChangeEvent(true))
  thresholdInput.props.onChange?.(createInputChangeEvent("5"))
  cooldownInput.props.onChange?.(createInputChangeEvent("90"))

  tree = harness.renderReady()
  const form = findForm(tree)
  await form.props.onSubmit?.(createFormSubmitEvent())

  assert.equal(saveConfigCalls.length, 1)
  assert.deepEqual(saveConfigCalls[0]?.groups[0]?.failover, {
    enabled: true,
    failureThreshold: 5,
    cooldownSeconds: 90,
  })
  assert.deepEqual(initialConfig.groups[0]?.failover, {
    enabled: false,
    failureThreshold: 3,
    cooldownSeconds: 300,
  })
})

test("rejects invalid failover failure threshold input", async () => {
  resetHarness()
  const harness = createComponentHarness()

  let tree = harness.renderReady()
  const thresholdInput = findInputById(tree, "failoverFailureThreshold")
  thresholdInput.props.onChange?.(createInputChangeEvent("0"))

  tree = harness.renderReady()
  const form = findForm(tree)
  await form.props.onSubmit?.(createFormSubmitEvent())

  assert.equal(saveConfigCalls.length, 0)
  assert.deepEqual(toastCalls.at(-1), {
    message: "validation.invalidFormat:groupEditPage.failoverFailureThreshold",
    type: "error",
  })
})

test("rejects malformed failover numeric input", async () => {
  resetHarness()
  const harness = createComponentHarness()

  let tree = harness.renderReady()
  const thresholdInput = findInputById(tree, "failoverFailureThreshold")
  thresholdInput.props.onChange?.(createInputChangeEvent("1.5"))

  tree = harness.renderReady()
  let form = findForm(tree)
  await form.props.onSubmit?.(createFormSubmitEvent())

  assert.equal(saveConfigCalls.length, 0)
  assert.deepEqual(toastCalls.at(-1), {
    message: "validation.invalidFormat:groupEditPage.failoverFailureThreshold",
    type: "error",
  })

  resetHarness()
  tree = harness.renderReady()
  const cooldownInput = findInputById(tree, "failoverCooldownSeconds")
  cooldownInput.props.onChange?.(createInputChangeEvent("3abc"))

  tree = harness.renderReady()
  form = findForm(tree)
  await form.props.onSubmit?.(createFormSubmitEvent())

  assert.equal(saveConfigCalls.length, 0)
  assert.deepEqual(toastCalls.at(-1), {
    message: "validation.invalidFormat:groupEditPage.failoverCooldownSeconds",
    type: "error",
  })
})

process.on("exit", () => {
  Module._resolveFilename = originalResolveFilename
  if (originalCssExtension) {
    require.extensions[".css"] = originalCssExtension
    return
  }
  delete require.extensions[".css"]
})
