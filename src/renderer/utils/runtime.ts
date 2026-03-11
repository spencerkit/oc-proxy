export function isTauriRuntime(): boolean {
  return Boolean(window.__TAURI__ || window.__TAURI_INTERNALS__)
}

export function isHeadlessHttpRuntime(): boolean {
  return !isTauriRuntime() && /^https?:$/.test(window.location.protocol)
}
