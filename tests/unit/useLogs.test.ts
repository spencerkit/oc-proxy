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

const repoRoot = path.resolve(__dirname, "../../../..")
const unitOutDir = path.join(repoRoot, ".tmp/unit-tests")
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
  if (
    request === "@/contexts/ToastContext" ||
    request === "@/store" ||
    request === "@/utils/relax"
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

require.cache["@/contexts/ToastContext"] = {
  exports: {
    useToast: () => ({
      showToast: () => {},
    }),
  },
  filename: "@/contexts/ToastContext",
  id: "@/contexts/ToastContext",
  loaded: true,
} as NodeModule

require.cache["@/store"] = {
  exports: {
    clearLogsAction: { key: "clearLogsAction" },
    logsErrorState: { key: "logsErrorState" },
    logsState: { key: "logsState" },
    refreshLogsAction: { key: "refreshLogsAction" },
  },
  filename: "@/store",
  id: "@/store",
  loaded: true,
} as NodeModule

require.cache["@/utils/relax"] = {
  exports: {
    useActions: () => [() => {}, () => {}],
    useRelaxValue: () => [],
  },
  filename: "@/utils/relax",
  id: "@/utils/relax",
  loaded: true,
} as NodeModule

function loadUseLogs() {
  return require("../../src/renderer/hooks/useLogs") as typeof import("../../src/renderer/hooks/useLogs")
}

test("resolveLogsRefreshPlan only polls raw logs on the active logs tab", () => {
  const { resolveLogsRefreshPlan } = loadUseLogs()

  assert.deepEqual(resolveLogsRefreshPlan("/logs", "logs"), {
    pollLogs: true,
    pollStats: true,
  })
  assert.deepEqual(resolveLogsRefreshPlan("/logs", "stats"), {
    pollLogs: false,
    pollStats: true,
  })
  assert.deepEqual(resolveLogsRefreshPlan("/settings", "logs"), {
    pollLogs: false,
    pollStats: false,
  })
  assert.deepEqual(resolveLogsRefreshPlan("/logs/trace-123", "logs"), {
    pollLogs: false,
    pollStats: false,
  })
})

test("does not export the legacy generic auto-refresh hook", () => {
  const hooks = loadUseLogs() as Record<string, unknown>

  assert.equal("useLogsAutoRefresh" in hooks, false)
})

test.after(() => {
  Module._resolveFilename = originalResolveFilename
  require.extensions[".css"] = originalCssExtension
  delete require.cache["@/contexts/ToastContext"]
  delete require.cache["@/store"]
  delete require.cache["@/utils/relax"]
})
