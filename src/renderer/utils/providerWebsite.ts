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
