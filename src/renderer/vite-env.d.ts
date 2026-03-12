/// <reference types="vite/client" />

declare global {
  interface ImportMetaEnv {
    readonly VITE_APP_TITLE?: string
    readonly VITE_API_URL?: string
  }

  interface ImportMeta {
    readonly env: ImportMetaEnv
  }

  interface TauriCoreNamespace {
    invoke: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>
  }

  interface TauriNamespace {
    core?: TauriCoreNamespace
  }

  interface TauriInternalsNamespace {
    invoke: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>
  }

  interface Window {
    __TAURI__?: TauriNamespace
    __TAURI_INTERNALS__?: TauriInternalsNamespace
    __AOR_HTTP_BASE__?: string
  }
}

export {}
