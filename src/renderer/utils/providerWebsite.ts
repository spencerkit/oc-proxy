type InvokeFn = <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>

function getBrowserWindow(): Window | undefined {
  return typeof window === "undefined" ? undefined : window
}

function getOptionalInvoke(targetWindow = getBrowserWindow()): InvokeFn | undefined {
  return (
    (targetWindow?.__TAURI__?.core?.invoke as InvokeFn | undefined) ??
    (targetWindow?.__TAURI_INTERNALS__?.invoke as InvokeFn | undefined)
  )
}

export function resolveProviderWebsiteHref(raw?: string): string | null {
  const trimmed = raw?.trim()
  if (!trimmed) return null

  const candidate = /^https?:\/\//i.test(trimmed) ? trimmed : `https://${trimmed}`
  try {
    return new URL(candidate).toString()
  } catch {
    return null
  }
}

export function formatProviderWebsiteLabel(raw?: string): string | null {
  const trimmed = raw?.trim()
  if (!trimmed) return null

  return trimmed.replace(/^https?:\/\//i, "").replace(/\/+$/, "")
}

export async function openProviderWebsite(raw?: string): Promise<boolean> {
  const href = resolveProviderWebsiteHref(raw)
  if (!href) return false

  const browserWindow = getBrowserWindow()
  const invoke = getOptionalInvoke(browserWindow)
  if (invoke) {
    try {
      await invoke("app_open_external_url", { url: href })
      return true
    } catch {
      // Fall through to browser open when desktop IPC is unavailable.
    }
  }

  if (typeof browserWindow?.open === "function") {
    browserWindow.open(href, "_blank", "noopener,noreferrer")
    return true
  }

  return false
}
