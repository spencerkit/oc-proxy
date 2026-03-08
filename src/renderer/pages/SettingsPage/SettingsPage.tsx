import type React from "react"
import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { shallow } from "zustand/shallow"
import { Button, Input, Modal, Switch } from "@/components"
import { useLogs, useTranslation } from "@/hooks"
import { useProxyStore } from "@/store"
import type { AppInfo, LocaleCode, ProxyConfig, ThemeMode } from "@/types"
import { ipc } from "@/utils/ipc"
import { resolveEffectiveLocale } from "@/utils/locale"
import styles from "./SettingsPage.module.css"

type ImportSource = "file" | "clipboard"
type ExportTarget = "folder" | "clipboard"
type RemoteSyncAction = "upload" | "pull" | null

type PendingRemoteConflict = {
  action: Exclude<RemoteSyncAction, null>
  localUpdatedAt?: string
  remoteUpdatedAt?: string
  warning?: string
} | null

const QUOTA_REFRESH_MINUTES_MIN = 1
const QUOTA_REFRESH_MINUTES_MAX = 1440
const QUOTA_REFRESH_MINUTES_DEFAULT = 5

/**
 * SettingsPage Component
 * Service settings configuration page
 */
export const SettingsPage: React.FC = () => {
  const { t } = useTranslation()
  const {
    config,
    saveConfig,
    savingConfig,
    exportGroupsToFolder,
    exportGroupsToClipboard,
    importGroupsBackup,
    importGroupsFromJson,
    remoteRulesPull,
    remoteRulesUpload,
    readClipboardText,
  } = useProxyStore(
    state => ({
      config: state.config,
      saveConfig: state.saveConfig,
      savingConfig: state.savingConfig,
      exportGroupsToFolder: state.exportGroupsToFolder,
      exportGroupsToClipboard: state.exportGroupsToClipboard,
      importGroupsBackup: state.importGroupsBackup,
      importGroupsFromJson: state.importGroupsFromJson,
      remoteRulesPull: state.remoteRulesPull,
      remoteRulesUpload: state.remoteRulesUpload,
      readClipboardText: state.readClipboardText,
    }),
    shallow
  )
  const { showToast } = useLogs()

  const [portText, setPortText] = useState("8080")
  const [strictMode, setStrictMode] = useState(false)
  const [textToolCallFallbackEnabled, setTextToolCallFallbackEnabled] = useState(true)
  const [detailedLogs, setDetailedLogs] = useState(false)
  const [launchOnStartup, setLaunchOnStartup] = useState(false)
  const [autoStartServer, setAutoStartServer] = useState(true)
  const [closeToTray, setCloseToTray] = useState(true)
  const [quotaAutoRefreshMinutesText, setQuotaAutoRefreshMinutesText] = useState(
    String(QUOTA_REFRESH_MINUTES_DEFAULT)
  )
  const [theme, setTheme] = useState<ThemeMode>("light")
  const [locale, setLocale] = useState<LocaleCode>("en-US")
  const [remoteSyncEnabled, setRemoteSyncEnabled] = useState(false)
  const [remoteRepoUrl, setRemoteRepoUrl] = useState("")
  const [remoteToken, setRemoteToken] = useState("")
  const [remoteBranch, setRemoteBranch] = useState("main")
  const [portError, setPortError] = useState("")
  const [quotaAutoRefreshMinutesError, setQuotaAutoRefreshMinutesError] = useState("")
  const [showImportModal, setShowImportModal] = useState(false)
  const [showExportModal, setShowExportModal] = useState(false)
  const [showAboutModal, setShowAboutModal] = useState(false)
  const [importSource, setImportSource] = useState<ImportSource>("file")
  const [exportTarget, setExportTarget] = useState<ExportTarget>("folder")
  const [importJsonText, setImportJsonText] = useState("")
  const [readingClipboard, setReadingClipboard] = useState(false)
  const [exporting, setExporting] = useState(false)
  const [remoteSyncAction, setRemoteSyncAction] = useState<RemoteSyncAction>(null)
  const [pendingRemoteConflict, setPendingRemoteConflict] = useState<PendingRemoteConflict>(null)
  const [aboutLoading, setAboutLoading] = useState(false)
  const [appInfo, setAppInfo] = useState<AppInfo | null>(null)
  const serverSnapshotRef = useRef("")
  const remoteSnapshotRef = useRef("")
  const quotaRefreshSnapshotRef = useRef("")
  const remoteSyncing = remoteSyncAction !== null

  useEffect(() => {
    if (!config) return

    const nextServerSnapshot = JSON.stringify({
      port: config.server.port,
    })
    if (nextServerSnapshot !== serverSnapshotRef.current) {
      serverSnapshotRef.current = nextServerSnapshot
      setPortText(String(config.server.port))
      setPortError("")
    }

    const nextRemoteSnapshot = JSON.stringify({
      enabled: config.remoteGit.enabled,
      repoUrl: config.remoteGit.repoUrl ?? "",
      token: config.remoteGit.token ?? "",
      branch: config.remoteGit.branch ?? "main",
    })
    if (nextRemoteSnapshot !== remoteSnapshotRef.current) {
      remoteSnapshotRef.current = nextRemoteSnapshot
      setRemoteSyncEnabled(!!config.remoteGit.enabled)
      setRemoteRepoUrl(config.remoteGit.repoUrl ?? "")
      setRemoteToken(config.remoteGit.token ?? "")
      setRemoteBranch(config.remoteGit.branch || "main")
    }

    setStrictMode(config.compat.strictMode)
    setTextToolCallFallbackEnabled(config.compat.textToolCallFallbackEnabled ?? true)
    setDetailedLogs(!!config.logging.captureBody)
    setLaunchOnStartup(config.ui.launchOnStartup)
    setAutoStartServer(config.ui.autoStartServer ?? true)
    setCloseToTray(config.ui.closeToTray ?? true)
    const nextQuotaRefreshText = String(
      config.ui.quotaAutoRefreshMinutes ?? QUOTA_REFRESH_MINUTES_DEFAULT
    )
    if (nextQuotaRefreshText !== quotaRefreshSnapshotRef.current) {
      quotaRefreshSnapshotRef.current = nextQuotaRefreshText
      setQuotaAutoRefreshMinutesText(nextQuotaRefreshText)
      setQuotaAutoRefreshMinutesError("")
    }
    setTheme(config.ui.theme)
    setLocale(
      resolveEffectiveLocale({
        locale: config.ui.locale,
        localeMode: config.ui.localeMode,
        systemLanguage: navigator.language,
      })
    )
  }, [config])

  const validatePort = useCallback(
    (value: string) => {
      if (!/^\d+$/.test(value)) {
        setPortError(t("settings.portError"))
        return false
      }

      const parsed = Number(value)
      if (!Number.isInteger(parsed) || parsed < 1 || parsed > 65535) {
        setPortError(t("settings.portError"))
        return false
      }

      setPortError("")
      return true
    },
    [t]
  )

  const validateQuotaAutoRefreshMinutes = useCallback(
    (value: string) => {
      if (!/^\d+$/.test(value)) {
        setQuotaAutoRefreshMinutesError(t("settings.quotaAutoRefreshMinutesError"))
        return false
      }

      const parsed = Number(value)
      if (
        !Number.isInteger(parsed) ||
        parsed < QUOTA_REFRESH_MINUTES_MIN ||
        parsed > QUOTA_REFRESH_MINUTES_MAX
      ) {
        setQuotaAutoRefreshMinutesError(t("settings.quotaAutoRefreshMinutesError"))
        return false
      }

      setQuotaAutoRefreshMinutesError("")
      return true
    },
    [t]
  )

  const applyImmediateConfig = useCallback(
    async (builder: (base: ProxyConfig) => ProxyConfig) => {
      if (!config) return
      try {
        await saveConfig(builder(config))
      } catch (error) {
        showToast(t("errors.saveFailed", { message: String(error) }), "error")
      }
    },
    [config, saveConfig, showToast, t]
  )

  const parsedPort = /^\d+$/.test(portText) ? Number(portText) : NaN
  const isServerDirty = Boolean(config && portText !== String(config.server.port))
  const canSaveServer = Boolean(
    config && isServerDirty && !savingConfig && !portError && Number.isInteger(parsedPort)
  )

  const remoteIsConfigured = useMemo(
    () => remoteRepoUrl.trim().length > 0 && remoteToken.trim().length > 0,
    [remoteRepoUrl, remoteToken]
  )

  const persistRemoteConfig = useCallback(
    async (
      overrides?: Partial<{
        enabled: boolean
        repoUrl: string
        token: string
        branch: string
      }>
    ) => {
      if (!config) return false

      const nextEnabled = overrides?.enabled ?? remoteSyncEnabled
      const nextRepoUrl = (overrides?.repoUrl ?? remoteRepoUrl).trim()
      const nextToken = (overrides?.token ?? remoteToken).trim()
      const nextBranch = (overrides?.branch ?? remoteBranch).trim() || "main"
      const changed =
        nextEnabled !== !!config.remoteGit.enabled ||
        nextRepoUrl !== (config.remoteGit.repoUrl ?? "") ||
        nextToken !== (config.remoteGit.token ?? "") ||
        nextBranch !== (config.remoteGit.branch ?? "main")

      if (!changed) return true

      try {
        await saveConfig({
          ...config,
          remoteGit: {
            enabled: nextEnabled,
            repoUrl: nextRepoUrl,
            token: nextToken,
            branch: nextBranch,
          },
        })
        return true
      } catch (error) {
        showToast(t("errors.saveFailed", { message: String(error) }), "error")
        return false
      }
    },
    [config, remoteBranch, remoteRepoUrl, remoteSyncEnabled, remoteToken, saveConfig, showToast, t]
  )

  const formatSyncTime = useCallback((value?: string) => {
    if (!value) return "-"
    const date = new Date(value)
    return Number.isNaN(date.getTime()) ? value : date.toLocaleString()
  }, [])

  const executeRemoteAction = useCallback(
    async (action: Exclude<RemoteSyncAction, null>, force = false) => {
      if (!remoteIsConfigured) {
        showToast(t("settings.remoteNotConfigured"), "error")
        return
      }
      if (!(await persistRemoteConfig())) return

      try {
        setRemoteSyncAction(action)
        if (action === "upload") {
          const result = await remoteRulesUpload(force)
          if (result.needsConfirmation && !force) {
            setPendingRemoteConflict({
              action,
              localUpdatedAt: result.localUpdatedAt,
              remoteUpdatedAt: result.remoteUpdatedAt,
              warning: result.warning,
            })
            return
          }
          showToast(
            result.changed ? t("settings.remoteUploadSuccess") : t("settings.remoteUploadNoChange"),
            "success"
          )
          setPendingRemoteConflict(null)
          return
        }

        const result = await remoteRulesPull(force)
        if (result.needsConfirmation && !force) {
          setPendingRemoteConflict({
            action,
            localUpdatedAt: result.localUpdatedAt,
            remoteUpdatedAt: result.remoteUpdatedAt,
            warning: result.warning,
          })
          return
        }
        showToast(
          t("settings.remotePullSuccess", { count: result.importedGroupCount || 0 }),
          "success"
        )
        setPendingRemoteConflict(null)
      } catch (error) {
        showToast(t("errors.operationFailed", { message: String(error) }), "error")
      } finally {
        setRemoteSyncAction(null)
      }
    },
    [persistRemoteConfig, remoteIsConfigured, remoteRulesPull, remoteRulesUpload, showToast, t]
  )

  const handlePortChange = (value: string) => {
    setPortText(value)
    validatePort(value)
  }

  const handleQuotaAutoRefreshMinutesChange = (value: string) => {
    setQuotaAutoRefreshMinutesText(value)
    validateQuotaAutoRefreshMinutes(value)
  }

  const handleSaveServer = async () => {
    if (!config) return

    const portValid = validatePort(portText)
    if (!portValid) {
      return
    }

    try {
      await saveConfig({
        ...config,
        server: {
          ...config.server,
          port: Number(portText),
        },
      })
      showToast(t("settings.networkSaveSuccess"), "success")
    } catch (error) {
      showToast(t("errors.saveFailed", { message: String(error) }), "error")
    }
  }

  const handleQuotaAutoRefreshMinutesBlur = async () => {
    if (!config) return
    if (!validateQuotaAutoRefreshMinutes(quotaAutoRefreshMinutesText)) return
    const nextMinutes = Number(quotaAutoRefreshMinutesText)
    if (nextMinutes === config.ui.quotaAutoRefreshMinutes) return

    await applyImmediateConfig(current => ({
      ...current,
      ui: {
        ...current.ui,
        quotaAutoRefreshMinutes: nextMinutes,
      },
    }))
  }

  const handleExportGroups = async () => {
    setExportTarget("folder")
    setShowExportModal(true)
  }

  const closeExportModal = () => {
    if (exporting) return
    setShowExportModal(false)
  }

  const handleConfirmExport = async () => {
    try {
      setExporting(true)
      if (exportTarget === "folder") {
        const result = await exportGroupsToFolder()
        if (!result.canceled) {
          showToast(
            t("settings.backupExportFolderSuccess", { count: result.groupCount }),
            "success"
          )
        }
      } else {
        const result = await exportGroupsToClipboard()
        if (!result.canceled) {
          showToast(
            t("settings.backupExportClipboardSuccess", { count: result.groupCount }),
            "success"
          )
        }
      }

      setShowExportModal(false)
    } catch (error) {
      showToast(t("errors.operationFailed", { message: String(error) }), "error")
    } finally {
      setExporting(false)
    }
  }

  const handleImportGroups = async () => {
    setImportSource("file")
    setShowImportModal(true)
  }

  const closeImportModal = () => {
    setShowImportModal(false)
  }

  const handleReadClipboard = async () => {
    try {
      setReadingClipboard(true)
      const result = await readClipboardText()
      setImportJsonText(result.text || "")
    } catch (error) {
      showToast(t("errors.operationFailed", { message: String(error) }), "error")
    } finally {
      setReadingClipboard(false)
    }
  }

  const handleConfirmImport = async () => {
    try {
      const result =
        importSource === "file"
          ? await importGroupsBackup()
          : await importGroupsFromJson(importJsonText)

      if (!result.canceled) {
        showToast(
          t("settings.backupImportSuccess", { count: result.importedGroupCount || 0 }),
          "success"
        )
      }
      closeImportModal()
    } catch (error) {
      showToast(t("errors.operationFailed", { message: String(error) }), "error")
    }
  }

  const handleOpenAbout = async () => {
    setShowAboutModal(true)
    if (appInfo) return

    try {
      setAboutLoading(true)
      const info = await ipc.getAppInfo()
      setAppInfo(info)
    } catch (error) {
      showToast(t("errors.operationFailed", { message: String(error) }), "error")
    } finally {
      setAboutLoading(false)
    }
  }

  const canConfirmImport = importSource === "file" || importJsonText.trim().length > 0

  return (
    <div className={styles.settingsPage}>
      <div className={styles.header}>
        <h2>{t("settings.title")}</h2>
        <p className={styles.subtitle}>{t("settings.subtitle")}</p>
      </div>

      <div className={styles.layout}>
        <div className={styles.form}>
          <div className={styles.section}>
            <h3 className={styles.sectionTitle}>{t("settings.networkSection")}</h3>

            <div className={styles.formGroup}>
              <label htmlFor="port">{t("settings.servicePort")}</label>
              <Input
                id="port"
                type="text"
                inputMode="numeric"
                pattern="[0-9]*"
                value={portText}
                onChange={e => handlePortChange(e.target.value)}
                placeholder="8080"
                hint={!portError ? t("settings.portHint") : undefined}
                error={portError || undefined}
                disabled={savingConfig}
              />
            </div>

            <div className={styles.actions}>
              <Button
                variant="primary"
                onClick={handleSaveServer}
                disabled={!canSaveServer}
                loading={savingConfig && isServerDirty}
                type="button"
              >
                {t("settings.savePort")}
              </Button>
            </div>
          </div>

          <div className={styles.section}>
            <h3 className={styles.sectionTitle}>{t("settings.behaviorSection")}</h3>

            <div className={styles.formGroupSwitch}>
              <div className={styles.switchLabel}>
                <label htmlFor="strictMode">{t("settings.strictMode")}</label>
                <p>{t("settings.strictModeHint")}</p>
              </div>
              <Switch
                id="strictMode"
                checked={strictMode}
                disabled={savingConfig}
                onChange={next => {
                  setStrictMode(next)
                  void applyImmediateConfig(current => ({
                    ...current,
                    compat: {
                      ...current.compat,
                      strictMode: next,
                    },
                  }))
                }}
              />
            </div>

            <div className={styles.formGroupSwitch}>
              <div className={styles.switchLabel}>
                <label htmlFor="textToolCallFallbackEnabled">
                  {t("settings.textToolCallFallbackEnabled")}
                </label>
                <p>{t("settings.textToolCallFallbackEnabledHint")}</p>
              </div>
              <Switch
                id="textToolCallFallbackEnabled"
                checked={textToolCallFallbackEnabled}
                disabled={savingConfig}
                onChange={next => {
                  setTextToolCallFallbackEnabled(next)
                  void applyImmediateConfig(current => ({
                    ...current,
                    compat: {
                      ...current.compat,
                      textToolCallFallbackEnabled: next,
                    },
                  }))
                }}
              />
            </div>

            <div className={styles.formGroupSwitch}>
              <div className={styles.switchLabel}>
                <label htmlFor="detailedLogs">{t("settings.detailedLogs")}</label>
                <p>{t("settings.detailedLogsHint")}</p>
              </div>
              <Switch
                id="detailedLogs"
                checked={detailedLogs}
                disabled={savingConfig}
                onChange={next => {
                  setDetailedLogs(next)
                  void applyImmediateConfig(current => ({
                    ...current,
                    logging: {
                      ...current.logging,
                      captureBody: next,
                    },
                  }))
                }}
              />
            </div>

            <div className={styles.formGroupSwitch}>
              <div className={styles.switchLabel}>
                <label htmlFor="launchOnStartup">{t("settings.launchOnStartup")}</label>
                <p>{t("settings.launchOnStartupHint")}</p>
              </div>
              <Switch
                id="launchOnStartup"
                checked={launchOnStartup}
                disabled={savingConfig}
                onChange={next => {
                  setLaunchOnStartup(next)
                  void applyImmediateConfig(current => ({
                    ...current,
                    ui: {
                      ...current.ui,
                      launchOnStartup: next,
                    },
                  }))
                }}
              />
            </div>

            <div className={styles.formGroupSwitch}>
              <div className={styles.switchLabel}>
                <label htmlFor="autoStartServer">{t("settings.autoStartServer")}</label>
                <p>{t("settings.autoStartServerHint")}</p>
              </div>
              <Switch
                id="autoStartServer"
                checked={autoStartServer}
                disabled={savingConfig}
                onChange={next => {
                  setAutoStartServer(next)
                  void applyImmediateConfig(current => ({
                    ...current,
                    ui: {
                      ...current.ui,
                      autoStartServer: next,
                    },
                  }))
                }}
              />
            </div>

            <div className={styles.formGroupSwitch}>
              <div className={styles.switchLabel}>
                <label htmlFor="closeToTray">{t("settings.closeToTray")}</label>
                <p>{t("settings.closeToTrayHint")}</p>
              </div>
              <Switch
                id="closeToTray"
                checked={closeToTray}
                disabled={savingConfig}
                onChange={next => {
                  setCloseToTray(next)
                  void applyImmediateConfig(current => ({
                    ...current,
                    ui: {
                      ...current.ui,
                      closeToTray: next,
                    },
                  }))
                }}
              />
            </div>

            <div className={styles.formGroup}>
              <label htmlFor="quota-auto-refresh-minutes">
                {t("settings.quotaAutoRefreshMinutes")}
              </label>
              <Input
                id="quota-auto-refresh-minutes"
                type="text"
                inputMode="numeric"
                pattern="[0-9]*"
                value={quotaAutoRefreshMinutesText}
                onChange={e => handleQuotaAutoRefreshMinutesChange(e.target.value)}
                onBlur={() => {
                  void handleQuotaAutoRefreshMinutesBlur()
                }}
                hint={
                  !quotaAutoRefreshMinutesError
                    ? t("settings.quotaAutoRefreshMinutesHint")
                    : undefined
                }
                error={quotaAutoRefreshMinutesError || undefined}
                disabled={savingConfig}
              />
            </div>
          </div>

          <div className={styles.section}>
            <h3 className={styles.sectionTitle}>{t("settings.interfaceSection")}</h3>

            <div className={styles.formGroup}>
              <label htmlFor="settings-theme-light">{t("settings.theme")}</label>
              <div className={styles.choiceGroup}>
                <button
                  id="settings-theme-light"
                  type="button"
                  aria-pressed={theme === "light"}
                  className={`${styles.choiceButton} ${theme === "light" ? styles.choiceButtonActive : ""}`}
                  onClick={() => {
                    setTheme("light")
                    void applyImmediateConfig(current => ({
                      ...current,
                      ui: {
                        ...current.ui,
                        theme: "light",
                      },
                    }))
                  }}
                  disabled={savingConfig}
                >
                  <span className={styles.choiceTitle}>{t("settings.themeLight")}</span>
                  <span className={styles.choiceValue}>LIGHT</span>
                </button>
                <button
                  type="button"
                  aria-pressed={theme === "dark"}
                  className={`${styles.choiceButton} ${theme === "dark" ? styles.choiceButtonActive : ""}`}
                  onClick={() => {
                    setTheme("dark")
                    void applyImmediateConfig(current => ({
                      ...current,
                      ui: {
                        ...current.ui,
                        theme: "dark",
                      },
                    }))
                  }}
                  disabled={savingConfig}
                >
                  <span className={styles.choiceTitle}>{t("settings.themeDark")}</span>
                  <span className={styles.choiceValue}>DARK</span>
                </button>
              </div>
              <p className={styles.fieldHint}>{t("settings.themeHint")}</p>
            </div>

            <div className={styles.formGroup}>
              <label htmlFor="settings-language-en">{t("settings.language")}</label>
              <div className={styles.choiceGroup}>
                <button
                  id="settings-language-en"
                  type="button"
                  aria-pressed={locale === "en-US"}
                  className={`${styles.choiceButton} ${locale === "en-US" ? styles.choiceButtonActive : ""}`}
                  onClick={() => {
                    setLocale("en-US")
                    void applyImmediateConfig(current => ({
                      ...current,
                      ui: {
                        ...current.ui,
                        locale: "en-US",
                        localeMode: "manual",
                      },
                    }))
                  }}
                  disabled={savingConfig}
                >
                  <span className={styles.choiceTitle}>{t("settings.languageEnglish")}</span>
                  <span className={styles.choiceValue}>EN-US</span>
                </button>
                <button
                  type="button"
                  aria-pressed={locale === "zh-CN"}
                  className={`${styles.choiceButton} ${locale === "zh-CN" ? styles.choiceButtonActive : ""}`}
                  onClick={() => {
                    setLocale("zh-CN")
                    void applyImmediateConfig(current => ({
                      ...current,
                      ui: {
                        ...current.ui,
                        locale: "zh-CN",
                        localeMode: "manual",
                      },
                    }))
                  }}
                  disabled={savingConfig}
                >
                  <span className={styles.choiceTitle}>{t("settings.languageChinese")}</span>
                  <span className={styles.choiceValue}>ZH-CN</span>
                </button>
              </div>
              <p className={styles.fieldHint}>{t("settings.languageHint")}</p>
            </div>
          </div>

          <div className={styles.section}>
            <h3 className={styles.sectionTitle}>{t("settings.backupSection")}</h3>

            <div className={styles.formGroup}>
              <label htmlFor="settings-backup-export">{t("settings.backupTitle")}</label>
              <div className={styles.backupActions}>
                <Button
                  id="settings-backup-export"
                  variant="default"
                  onClick={handleExportGroups}
                  disabled={savingConfig || exporting}
                >
                  {t("settings.backupExport")}
                </Button>
                <Button
                  variant="default"
                  onClick={handleImportGroups}
                  disabled={savingConfig || exporting}
                >
                  {t("settings.backupImport")}
                </Button>
              </div>
              <p className={styles.fieldHint}>{t("settings.backupHint")}</p>
            </div>
          </div>

          <div className={styles.section}>
            <h3 className={styles.sectionTitle}>{t("settings.remoteSection")}</h3>

            <div className={styles.formGroupSwitch}>
              <div className={styles.switchLabel}>
                <label htmlFor="remoteSyncEnabled">{t("settings.remoteSyncEnabled")}</label>
                <p>{t("settings.remoteSyncEnabledHint")}</p>
              </div>
              <Switch
                id="remoteSyncEnabled"
                checked={remoteSyncEnabled}
                disabled={savingConfig}
                onChange={next => {
                  setRemoteSyncEnabled(next)
                  void persistRemoteConfig({ enabled: next })
                }}
              />
            </div>

            {remoteSyncEnabled && (
              <>
                <div className={styles.formGroup}>
                  <label htmlFor="settings-remote-repo">{t("settings.remoteRepoUrl")}</label>
                  <Input
                    id="settings-remote-repo"
                    value={remoteRepoUrl}
                    onChange={e => setRemoteRepoUrl(e.target.value)}
                    onBlur={() => {
                      void persistRemoteConfig()
                    }}
                    placeholder={t("settings.remoteRepoUrlPlaceholder")}
                    disabled={savingConfig}
                  />
                </div>

                <div className={styles.formGroup}>
                  <label htmlFor="settings-remote-token">{t("settings.remoteToken")}</label>
                  <Input
                    id="settings-remote-token"
                    type="password"
                    value={remoteToken}
                    onChange={e => setRemoteToken(e.target.value)}
                    onBlur={() => {
                      void persistRemoteConfig()
                    }}
                    placeholder={t("settings.remoteTokenPlaceholder")}
                    disabled={savingConfig}
                  />
                </div>

                <div className={styles.formGroup}>
                  <label htmlFor="settings-remote-branch">{t("settings.remoteBranch")}</label>
                  <Input
                    id="settings-remote-branch"
                    value={remoteBranch}
                    onChange={e => setRemoteBranch(e.target.value)}
                    onBlur={() => {
                      void persistRemoteConfig()
                    }}
                    placeholder="main"
                    hint={t("settings.remoteBranchHint")}
                    disabled={savingConfig}
                  />
                </div>

                <div className={styles.backupActions}>
                  <Button
                    variant="default"
                    onClick={() => {
                      void executeRemoteAction("upload")
                    }}
                    disabled={savingConfig || remoteSyncing}
                    loading={remoteSyncAction === "upload"}
                  >
                    {t("settings.remoteRulesUpload")}
                  </Button>
                  <Button
                    variant="default"
                    onClick={() => {
                      void executeRemoteAction("pull")
                    }}
                    disabled={savingConfig || remoteSyncing}
                    loading={remoteSyncAction === "pull"}
                  >
                    {t("settings.remoteRulesUpdate")}
                  </Button>
                </div>
                <p className={styles.fieldHint}>{t("settings.remoteHint")}</p>
              </>
            )}
          </div>

          <div className={styles.section}>
            <h3 className={styles.sectionTitle}>{t("settings.aboutSection")}</h3>

            <div className={styles.formGroup}>
              <label htmlFor="settings-open-about">{t("settings.aboutTitle")}</label>
              <div className={styles.backupActions}>
                <Button
                  id="settings-open-about"
                  variant="default"
                  onClick={handleOpenAbout}
                  disabled={aboutLoading}
                >
                  {t("settings.openAbout")}
                </Button>
              </div>
              <p className={styles.fieldHint}>{t("settings.aboutHint")}</p>
            </div>
          </div>
        </div>
      </div>

      <Modal
        open={showExportModal}
        onClose={closeExportModal}
        title={t("settings.exportModalTitle")}
      >
        <div className={styles.importModalContent}>
          <p className={styles.importWarning}>{t("settings.exportModalHint")}</p>

          <div className={styles.formGroup}>
            <label htmlFor="settings-export-target-folder">{t("settings.exportTargetLabel")}</label>
            <div className={styles.importSourceGroup}>
              <button
                id="settings-export-target-folder"
                type="button"
                aria-pressed={exportTarget === "folder"}
                className={`${styles.choiceButton} ${exportTarget === "folder" ? styles.choiceButtonActive : ""}`}
                onClick={() => setExportTarget("folder")}
              >
                <span className={styles.choiceTitle}>{t("settings.exportTargetFolder")}</span>
              </button>
              <button
                type="button"
                aria-pressed={exportTarget === "clipboard"}
                className={`${styles.choiceButton} ${exportTarget === "clipboard" ? styles.choiceButtonActive : ""}`}
                onClick={() => setExportTarget("clipboard")}
              >
                <span className={styles.choiceTitle}>{t("settings.exportTargetClipboard")}</span>
              </button>
            </div>
          </div>

          <div className={styles.importModalActions}>
            <Button variant="default" onClick={closeExportModal} disabled={exporting}>
              {t("common.cancel")}
            </Button>
            <Button
              variant="primary"
              onClick={handleConfirmExport}
              loading={exporting}
              disabled={savingConfig}
            >
              {t("settings.exportConfirm")}
            </Button>
          </div>
        </div>
      </Modal>

      <Modal
        open={showImportModal}
        onClose={closeImportModal}
        title={t("settings.importModalTitle")}
      >
        <div className={styles.importModalContent}>
          <p className={styles.importWarning}>{t("settings.importModalWarning")}</p>

          <div className={styles.formGroup}>
            <label htmlFor="settings-import-source-file">{t("settings.importSourceLabel")}</label>
            <div className={styles.importSourceGroup}>
              <button
                id="settings-import-source-file"
                type="button"
                aria-pressed={importSource === "file"}
                className={`${styles.choiceButton} ${importSource === "file" ? styles.choiceButtonActive : ""}`}
                onClick={() => setImportSource("file")}
              >
                <span className={styles.choiceTitle}>{t("settings.importSourceFile")}</span>
              </button>
              <button
                type="button"
                aria-pressed={importSource === "clipboard"}
                className={`${styles.choiceButton} ${importSource === "clipboard" ? styles.choiceButtonActive : ""}`}
                onClick={() => setImportSource("clipboard")}
              >
                <span className={styles.choiceTitle}>{t("settings.importSourceClipboard")}</span>
              </button>
            </div>
          </div>

          {importSource === "clipboard" && (
            <div className={styles.formGroup}>
              <label htmlFor="import-json">{t("settings.importClipboardLabel")}</label>
              <textarea
                id="import-json"
                className={styles.importTextarea}
                value={importJsonText}
                onChange={e => setImportJsonText(e.target.value)}
                placeholder={t("settings.importClipboardPlaceholder")}
              />
              <div className={styles.importAuxActions}>
                <Button
                  variant="default"
                  onClick={handleReadClipboard}
                  loading={readingClipboard}
                  disabled={savingConfig}
                >
                  {t("settings.readClipboard")}
                </Button>
              </div>
            </div>
          )}

          <div className={styles.importModalActions}>
            <Button variant="default" onClick={closeImportModal}>
              {t("common.cancel")}
            </Button>
            <Button
              variant="danger"
              onClick={handleConfirmImport}
              disabled={!canConfirmImport}
              loading={savingConfig}
            >
              {t("settings.importConfirm")}
            </Button>
          </div>
        </div>
      </Modal>

      <Modal
        open={pendingRemoteConflict !== null}
        onClose={() => {
          if (remoteSyncing) return
          setPendingRemoteConflict(null)
        }}
        title={t("settings.remoteConflictTitle")}
      >
        <div className={styles.importModalContent}>
          {pendingRemoteConflict && (
            <>
              <p className={styles.importWarning}>
                {pendingRemoteConflict.action === "upload"
                  ? t("settings.remoteUploadConflict", {
                      local: formatSyncTime(pendingRemoteConflict.localUpdatedAt),
                      remote: formatSyncTime(pendingRemoteConflict.remoteUpdatedAt),
                    })
                  : t("settings.remotePullConflict", {
                      local: formatSyncTime(pendingRemoteConflict.localUpdatedAt),
                      remote: formatSyncTime(pendingRemoteConflict.remoteUpdatedAt),
                    })}
              </p>
              {pendingRemoteConflict.warning && (
                <p className={styles.importWarning}>{pendingRemoteConflict.warning}</p>
              )}
            </>
          )}

          <div className={styles.importModalActions}>
            <Button
              variant="default"
              onClick={() => setPendingRemoteConflict(null)}
              disabled={remoteSyncing}
            >
              {t("common.cancel")}
            </Button>
            <Button
              variant="danger"
              onClick={() => {
                if (!pendingRemoteConflict) return
                void executeRemoteAction(pendingRemoteConflict.action, true)
              }}
              loading={remoteSyncing}
            >
              {pendingRemoteConflict?.action === "upload"
                ? t("settings.remoteConflictConfirmUpload")
                : t("settings.remoteConflictConfirmPull")}
            </Button>
          </div>
        </div>
      </Modal>

      <Modal
        open={showAboutModal}
        onClose={() => setShowAboutModal(false)}
        title={t("settings.aboutModalTitle")}
      >
        <div className={styles.importModalContent}>
          {aboutLoading ? (
            <p className={styles.importWarning}>{t("common.loading")}</p>
          ) : (
            <div className={styles.formGroup}>
              <label htmlFor="settings-about-name">{t("settings.aboutName")}</label>
              <Input id="settings-about-name" value={appInfo?.name || "-"} readOnly />
              <label htmlFor="settings-about-version">{t("settings.aboutVersion")}</label>
              <Input id="settings-about-version" value={appInfo?.version || "-"} readOnly />
            </div>
          )}

          <div className={styles.importModalActions}>
            <Button variant="default" onClick={() => setShowAboutModal(false)}>
              {t("common.close")}
            </Button>
          </div>
        </div>
      </Modal>
    </div>
  )
}

export default SettingsPage
