import type { AuthSessionStatus } from "@/types"

const AUTH_SESSION_EVENT = "aor:auth-session-changed"

export function emitAuthSessionChanged(status: AuthSessionStatus): void {
  window.dispatchEvent(
    new CustomEvent<AuthSessionStatus>(AUTH_SESSION_EVENT, {
      detail: status,
    })
  )
}

export function subscribeAuthSessionChanged(
  listener: (status: AuthSessionStatus) => void
): () => void {
  const handleEvent = (event: Event) => {
    const detail = (event as CustomEvent<AuthSessionStatus>).detail
    if (detail) {
      listener(detail)
    }
  }

  window.addEventListener(AUTH_SESSION_EVENT, handleEvent)
  return () => {
    window.removeEventListener(AUTH_SESSION_EVENT, handleEvent)
  }
}
