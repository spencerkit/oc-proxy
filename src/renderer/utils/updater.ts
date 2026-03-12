export type UpdateInfo = {
  version: string
  date?: string
  body?: string
}

export type UpdateCheckResult = {
  available: boolean
  info?: UpdateInfo
  checkedAt: string
}

export type UpdateInstallProgress = {
  event: string
  percent?: number
  transferred?: number
  total?: number
}

export type UpdateInstallResult = {
  installed: boolean
  version?: string
}

type UpdaterDownloadEvent = {
  event?: string
  data?: {
    contentLength?: number
    chunkLength?: number
  }
}

type UpdaterUpdate = {
  available?: boolean
  version: string
  date?: string
  body?: string
  downloadAndInstall: (cb?: (event: UpdaterDownloadEvent) => void) => Promise<void>
}

type UpdaterApi = {
  check: () => Promise<UpdaterUpdate | null>
}

type UpdateCache = {
  lastCheckedAt?: string
  available?: UpdateInfo | null
}

const UPDATE_CACHE_KEY = "aor.updater.cache"

function readCache(): UpdateCache {
  if (typeof localStorage === "undefined") return {}
  try {
    const raw = localStorage.getItem(UPDATE_CACHE_KEY)
    if (!raw) return {}
    const parsed = JSON.parse(raw) as UpdateCache
    return parsed ?? {}
  } catch {
    return {}
  }
}

function writeCache(next: UpdateCache) {
  if (typeof localStorage === "undefined") return
  try {
    localStorage.setItem(UPDATE_CACHE_KEY, JSON.stringify(next))
  } catch {
    // ignore cache write failures
  }
}

export function readUpdateCache(): UpdateCache {
  return readCache()
}

export function writeUpdateCache(update: UpdateCache) {
  writeCache(update)
}

function getUpdater(): UpdaterApi {
  const updater = (window as typeof window & { __TAURI__?: { updater?: UpdaterApi } }).__TAURI__
    ?.updater
  if (!updater?.check) {
    throw new Error("Updater is unavailable. Enable the updater plugin and permissions.")
  }
  return updater
}

export async function checkForUpdate(): Promise<UpdateCheckResult> {
  const updater = getUpdater()
  const checkedAt = new Date().toISOString()
  const update = await updater.check()
  const available = Boolean(update && update.available !== false)
  const info = available
    ? {
        version: update?.version ?? "unknown",
        date: update?.date,
        body: update?.body,
      }
    : undefined
  writeCache({ lastCheckedAt: checkedAt, available: info ?? null })
  return { available, info, checkedAt }
}

export async function installUpdate(
  onProgress?: (progress: UpdateInstallProgress) => void
): Promise<UpdateInstallResult> {
  const updater = getUpdater()
  const update = await updater.check()
  if (!update || update.available === false) {
    return { installed: false }
  }

  let total = 0
  let transferred = 0
  await update.downloadAndInstall(event => {
    if (!event) return
    const eventName = event.event ?? "unknown"
    const data = event.data ?? {}
    if (eventName === "Started") {
      total = data.contentLength ?? 0
      transferred = 0
    }
    if (eventName === "Progress") {
      transferred += data.chunkLength ?? 0
    }
    if (eventName === "Finished") {
      transferred = total || transferred
    }
    const percent = total > 0 ? Math.min(100, Math.round((transferred / total) * 100)) : undefined
    onProgress?.({
      event: eventName,
      percent,
      transferred,
      total: total || undefined,
    })
  })

  writeCache({ lastCheckedAt: new Date().toISOString(), available: null })
  return { installed: true, version: update.version }
}
