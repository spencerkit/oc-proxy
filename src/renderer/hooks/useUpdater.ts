import { useEffect, useRef } from "react"
import { useToast } from "@/contexts/ToastContext"
import { useTranslation } from "@/hooks"
import { configState } from "@/store"
import { useRelaxValue } from "@/utils/relax"
import { checkForUpdate, installUpdate } from "@/utils/updater"

const AUTO_UPDATE_INTERVAL_MS = 6 * 60 * 60 * 1000

export function useUpdater() {
  const config = useRelaxValue(configState)
  const { t } = useTranslation()
  const { showToast } = useToast()
  const runningRef = useRef(false)
  const lastInstalledVersionRef = useRef<string | null>(null)

  useEffect(() => {
    if (!window.__TAURI__ && !window.__TAURI_INTERNALS__) return
    if (!config?.ui?.autoUpdateEnabled) return

    let timerId: number | null = null

    const runCheck = async () => {
      if (runningRef.current) return
      runningRef.current = true
      try {
        const result = await checkForUpdate()
        if (result.available) {
          const versionLabel = result.info?.version ?? "unknown"
          if (lastInstalledVersionRef.current === versionLabel) {
            return
          }
          showToast(t("toast.updateAvailable", { version: versionLabel }), "info")
          showToast(t("toast.updateInstallStarted"), "info")
          const installed = await installUpdate()
          if (installed.installed) {
            lastInstalledVersionRef.current = installed.version ?? versionLabel
            showToast(
              t("toast.updateInstalled", {
                version: installed.version ? ` v${installed.version}` : "",
              }),
              "success"
            )
          }
        }
      } catch (error) {
        // Auto-update errors should be silent to avoid noisy toasts.
        console.warn("[updater] auto-update check failed", error)
      } finally {
        runningRef.current = false
      }
    }

    void runCheck()
    timerId = window.setInterval(runCheck, AUTO_UPDATE_INTERVAL_MS)

    return () => {
      if (timerId) window.clearInterval(timerId)
    }
  }, [config?.ui?.autoUpdateEnabled, showToast, t])
}
