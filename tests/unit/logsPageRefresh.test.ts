import assert from "node:assert/strict"
import { existsSync } from "node:fs"
import path from "node:path"
import { test } from "node:test"

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
type EffectCallback = () => undefined | (() => void)
type LogsTab = "stats" | "logs"
type MockProvider = { id: string; name?: string }
type MockGroup = { id: string; name?: string; providers?: MockProvider[] }

type HarnessWindow = {
  addEventListener: (event: string, handler: (...args: unknown[]) => void) => void
  removeEventListener: (event: string, handler: (...args: unknown[]) => void) => void
  setInterval: (handler: (...args: unknown[]) => void, timeout: number) => number
  clearInterval: (id: number) => void
  requestAnimationFrame: (handler: (...args: unknown[]) => void) => number
  cancelAnimationFrame: (id: number) => void
}

type HarnessDocument = {
  addEventListener: (event: string, handler: (...args: unknown[]) => void) => void
  removeEventListener: (event: string, handler: (...args: unknown[]) => void) => void
}

const repoRoot = path.resolve(__dirname, "../../../..")
const unitOutDir = path.join(repoRoot, ".tmp/unit-tests")
const originalResolveFilename = Module._resolveFilename
const originalCssExtension = require.extensions[".css"]
const originalWindow = (globalThis as { window?: HarnessWindow }).window
const originalDocument = (globalThis as { document?: HarnessDocument }).document

let currentPathname = "/logs"
let currentActiveTab: LogsTab = "stats"
let stateIndex = 0
let stateValues: unknown[] = []
let stateSetters: Array<((value: unknown) => void) | undefined> = []
let refIndex = 0
let refValues: unknown[] = []
let effectCallbacks: EffectCallback[] = []
let refreshLogsCalls = 0
let refreshLogsStatsCalls: Array<{
  hours: number
  ruleKeys: string[]
  dimension: string
  enableComparison: boolean
}> = []
let intervalCalls: number[] = []
let intervalId = 0
let mockConfigGroups: MockGroup[] | null = []

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
    request === "react" ||
    request === "react/jsx-runtime" ||
    request === "react-router-dom" ||
    request === "@/hooks" ||
    request === "@/store" ||
    request === "@/utils/relax" ||
    request === "@/components" ||
    request === "echarts" ||
    request === "lucide-react" ||
    request === "@/utils/tokenFormat"
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

const reactExports = {
  useEffect: (callback: EffectCallback) => {
    effectCallbacks.push(callback)
  },
  useMemo: <T>(factory: () => T) => factory(),
  useRef: <T>(value: T) => {
    const currentIndex = refIndex
    refIndex += 1
    if (!(currentIndex in refValues)) {
      refValues[currentIndex] = { current: value }
    }
    return refValues[currentIndex] as { current: T }
  },
  useState: <T>(initialValue: T | (() => T)) => {
    const currentIndex = stateIndex
    stateIndex += 1

    const resolvedInitialValue =
      typeof initialValue === "function" ? (initialValue as () => T)() : initialValue

    if (currentIndex === 0) {
      stateValues[currentIndex] = currentActiveTab
    } else if (!(currentIndex in stateValues)) {
      stateValues[currentIndex] = resolvedInitialValue
    }

    const setValue = (nextValue: T | ((previousValue: T) => T)) => {
      const previousValue = stateValues[currentIndex] as T
      stateValues[currentIndex] =
        typeof nextValue === "function"
          ? (nextValue as (previousValue: T) => T)(previousValue)
          : nextValue
    }

    stateSetters[currentIndex] = setValue as (value: unknown) => void

    return [stateValues[currentIndex] as T, setValue] as const
  },
}

require.cache.react = {
  exports: {
    ...reactExports,
    default: reactExports,
  },
  filename: "react",
  id: "react",
  loaded: true,
} as NodeModule

require.cache["react/jsx-runtime"] = {
  exports: {
    jsx: () => null,
    jsxs: () => null,
    Fragment: Symbol.for("react.fragment"),
  },
  filename: "react/jsx-runtime",
  id: "react/jsx-runtime",
  loaded: true,
} as NodeModule

require.cache["react-router-dom"] = {
  exports: {
    useLocation: () => ({ pathname: currentPathname }),
    useNavigate: () => () => {},
  },
  filename: "react-router-dom",
  id: "react-router-dom",
  loaded: true,
} as NodeModule

require.cache["@/hooks"] = {
  exports: {
    resolveLogsRefreshPlan: (pathname: string, activeTab: LogsTab) => ({
      pollLogs: pathname === "/logs" && activeTab === "logs",
      pollStats: pathname === "/logs",
    }),
    useLogs: () => ({
      showToast: () => {},
    }),
    useTranslation: () => ({
      t: (key: string) => key,
    }),
  },
  filename: "@/hooks",
  id: "@/hooks",
  loaded: true,
} as NodeModule

require.cache["@/store"] = {
  exports: {
    clearLogsAction: { key: "clearLogsAction" },
    clearLogsStatsAction: { key: "clearLogsStatsAction" },
    configState: { key: "configState" },
    logsState: { key: "logsState" },
    logsStatsState: { key: "logsStatsState" },
    refreshLogsAction: { key: "refreshLogsAction" },
    refreshLogsStatsAction: { key: "refreshLogsStatsAction" },
  },
  filename: "@/store",
  id: "@/store",
  loaded: true,
} as NodeModule

require.cache["@/utils/relax"] = {
  exports: {
    useActions: () => [
      () => {
        refreshLogsCalls += 1
        return Promise.resolve()
      },
      (payload: {
        hours: number
        ruleKeys: string[]
        dimension: string
        enableComparison: boolean
      }) => {
        refreshLogsStatsCalls.push(payload)
        return Promise.resolve()
      },
      () => Promise.resolve(),
      () => Promise.resolve(),
    ],
    useRelaxValue: (state: { key?: string }) => {
      if (state?.key === "configState") {
        return mockConfigGroups === null
          ? null
          : {
              groups: mockConfigGroups,
            }
      }
      if (state?.key === "logsState") {
        return []
      }
      if (state?.key === "logsStatsState") {
        return null
      }
      return null
    },
  },
  filename: "@/utils/relax",
  id: "@/utils/relax",
  loaded: true,
} as NodeModule

require.cache["@/components"] = {
  exports: {
    Button: (_props: UnknownProps) => null,
    Modal: (_props: UnknownProps) => null,
  },
  filename: "@/components",
  id: "@/components",
  loaded: true,
} as NodeModule

require.cache.echarts = {
  exports: {
    init: () => ({
      setOption: () => {},
      resize: () => {},
      dispose: () => {},
    }),
    graphic: {
      LinearGradient: function LinearGradient() {
        return {}
      },
    },
  },
  filename: "echarts",
  id: "echarts",
  loaded: true,
} as NodeModule

require.cache["lucide-react"] = {
  exports: {
    Check: () => null,
    ChevronLeft: () => null,
    ChevronRight: () => null,
    RotateCcw: () => null,
    Trash2: () => null,
    X: () => null,
  },
  filename: "lucide-react",
  id: "lucide-react",
  loaded: true,
} as NodeModule

require.cache["@/utils/tokenFormat"] = {
  exports: {
    formatTokenMillions: (value: number) => String(value),
  },
  filename: "@/utils/tokenFormat",
  id: "@/utils/tokenFormat",
  loaded: true,
} as NodeModule

function loadLogsPage() {
  return require("../../src/renderer/pages/LogsPage/LogsPage") as typeof import("../../src/renderer/pages/LogsPage/LogsPage")
}

function mountLogsPage(input: {
  pathname: string
  activeTab: LogsTab
  groups?: MockGroup[] | null
}) {
  currentPathname = input.pathname
  currentActiveTab = input.activeTab
  mockConfigGroups = input.groups === undefined ? [] : input.groups
  stateIndex = 0
  stateValues = []
  stateSetters = []
  refIndex = 0
  refValues = []
  effectCallbacks = []
  refreshLogsCalls = 0
  refreshLogsStatsCalls = []
  intervalCalls = []
  intervalId = 0

  ;(globalThis as { window?: HarnessWindow }).window = {
    addEventListener: () => {},
    removeEventListener: () => {},
    setInterval: (_handler, timeout) => {
      intervalId += 1
      intervalCalls.push(timeout)
      return intervalId
    },
    clearInterval: () => {},
    requestAnimationFrame: () => 1,
    cancelAnimationFrame: () => {},
  }

  ;(globalThis as { document?: HarnessDocument }).document = {
    addEventListener: () => {},
    removeEventListener: () => {},
  }

  const { LogsPage } = loadLogsPage()
  LogsPage({})

  for (const callback of effectCallbacks) {
    callback()
  }

  return {
    refreshLogsCalls,
    refreshLogsStatsCalls,
    intervalCalls,
    stateValues,
    stateSetters,
  }
}

test("LogsPage initializes all groups after config loads asynchronously", () => {
  mountLogsPage({ pathname: "/logs", activeTab: "stats", groups: null })
  assert.deepEqual(stateValues[3], [])

  mockConfigGroups = [
    {
      id: "group-a",
      name: "Group A",
      providers: [{ id: "provider-a", name: "Provider A" }],
    },
    {
      id: "group-b",
      name: "Group B",
      providers: [{ id: "provider-b", name: "Provider B" }],
    },
  ]

  stateIndex = 0
  refIndex = 0
  effectCallbacks = []
  const { LogsPage } = loadLogsPage()
  LogsPage({})
  for (const callback of effectCallbacks) {
    callback()
  }

  assert.deepEqual(stateValues[3], ["group-a", "group-b"])
})

test("LogsPage preserves explicit group selections when provider filtering changes options", () => {
  const result = mountLogsPage({
    pathname: "/logs",
    activeTab: "stats",
    groups: [
      {
        id: "group-a",
        name: "Group A",
        providers: [{ id: "provider-shared", name: "Shared" }],
      },
      {
        id: "group-b",
        name: "Group B",
        providers: [{ id: "provider-shared", name: "Shared" }],
      },
      {
        id: "group-c",
        name: "Group C",
        providers: [{ id: "provider-only-c", name: "Only C" }],
      },
    ],
  })

  assert.deepEqual(result.stateValues[3], ["group-a", "group-b", "group-c"])

  const setSelectedProviderId = result.stateSetters[2] as
    | ((value: string | ((previous: string) => string)) => void)
    | undefined
  const setSelectedGroupIds = result.stateSetters[3] as
    | ((value: string[] | ((previous: string[]) => string[])) => void)
    | undefined

  assert.equal(typeof setSelectedProviderId, "function")
  assert.equal(typeof setSelectedGroupIds, "function")

  setSelectedGroupIds?.(["group-a"])
  setSelectedProviderId?.("provider-shared")

  stateIndex = 0
  refIndex = 0
  effectCallbacks = []
  const { LogsPage } = loadLogsPage()
  LogsPage({})
  for (const callback of effectCallbacks) {
    callback()
  }

  assert.deepEqual(stateValues[3], ["group-a"])
})

test("LogsPage only starts stats refresh on the stats tab", () => {
  const result = mountLogsPage({ pathname: "/logs", activeTab: "stats" })

  assert.equal(result.refreshLogsCalls, 0)
  assert.equal(result.refreshLogsStatsCalls.length, 1)
  assert.deepEqual(result.refreshLogsStatsCalls[0], {
    hours: 24,
    ruleKeys: [],
    dimension: "rule",
    enableComparison: false,
  })
  assert.deepEqual(result.intervalCalls, [3000])
})

test("LogsPage starts both log and stats refresh on the logs tab", () => {
  const result = mountLogsPage({ pathname: "/logs", activeTab: "logs" })

  assert.equal(result.refreshLogsCalls, 1)
  assert.equal(result.refreshLogsStatsCalls.length, 1)
  assert.deepEqual(result.intervalCalls, [3000, 3000])
})

test("LogsPage does not start polling on log detail routes", () => {
  const result = mountLogsPage({ pathname: "/logs/trace-123", activeTab: "logs" })

  assert.equal(result.refreshLogsCalls, 0)
  assert.equal(result.refreshLogsStatsCalls.length, 0)
  assert.deepEqual(result.intervalCalls, [])
})

test.after(() => {
  Module._resolveFilename = originalResolveFilename
  require.extensions[".css"] = originalCssExtension
  ;(globalThis as { window?: HarnessWindow }).window = originalWindow
  ;(globalThis as { document?: HarnessDocument }).document = originalDocument
  delete require.cache.react
  delete require.cache["react/jsx-runtime"]
  delete require.cache["react-router-dom"]
  delete require.cache["@/hooks"]
  delete require.cache["@/store"]
  delete require.cache["@/utils/relax"]
  delete require.cache["@/components"]
  delete require.cache.echarts
  delete require.cache["lucide-react"]
  delete require.cache["@/utils/tokenFormat"]
})
