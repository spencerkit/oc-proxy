const ACTIVE_GROUP_STORAGE_KEY = "ai-open-router.activeGroupId"

export const readPersistedActiveGroupId = (): string | null => {
  if (typeof window === "undefined") return null
  try {
    const raw = window.localStorage.getItem(ACTIVE_GROUP_STORAGE_KEY)
    const value = raw?.trim()
    return value ? value : null
  } catch {
    return null
  }
}

export const persistActiveGroupId = (groupId: string | null) => {
  if (typeof window === "undefined") return
  try {
    if (groupId?.trim()) {
      window.localStorage.setItem(ACTIVE_GROUP_STORAGE_KEY, groupId)
      return
    }
    window.localStorage.removeItem(ACTIVE_GROUP_STORAGE_KEY)
  } catch {}
}
