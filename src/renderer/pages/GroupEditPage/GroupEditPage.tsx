import { ArrowLeft } from "lucide-react"
import type React from "react"
import { useEffect, useState } from "react"
import { useNavigate, useParams } from "react-router-dom"
import { Button, Input } from "@/components"
import { useLogs, useTranslation } from "@/hooks"
import { configState, saveConfigAction } from "@/store"
import type { ProxyConfig } from "@/types"
import { normalizeGroupFailoverConfig } from "@/utils/groupFailover"
import { useActions, useRelaxValue } from "@/utils/relax"
import styles from "./GroupEditPage.module.css"

const GROUP_EDIT_ACTIONS = [saveConfigAction] as const

export const GroupEditPage: React.FC = () => {
  const { groupId } = useParams<{ groupId: string }>()
  const navigate = useNavigate()
  const { t } = useTranslation()
  const { showToast } = useLogs()
  const config = useRelaxValue(configState)
  const [saveConfig] = useActions(GROUP_EDIT_ACTIONS)

  const group = config?.groups.find(item => item.id === groupId)

  const [name, setName] = useState("")
  const [failoverEnabled, setFailoverEnabled] = useState(false)
  const [failoverFailureThreshold, setFailoverFailureThreshold] = useState("3")
  const [failoverCooldownSeconds, setFailoverCooldownSeconds] = useState("300")
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    if (!group) {
      if (!config) return
      setLoading(false)
      showToast(t("toast.groupNotFound"), "error")
      navigate("/")
      return
    }
    setName(group.name)
    const failover = normalizeGroupFailoverConfig(group.failover)
    setFailoverEnabled(failover.enabled)
    setFailoverFailureThreshold(String(failover.failureThreshold))
    setFailoverCooldownSeconds(String(failover.cooldownSeconds))
    setLoading(false)
  }, [group, config, navigate, showToast, t])

  const handleSubmit = async (event: React.FormEvent) => {
    event.preventDefault()
    if (!config || !groupId || !group) return
    if (!name.trim()) {
      showToast(t("validation.required", { field: t("servicePage.groupName") }), "error")
      return
    }

    const parsedFailureThreshold = Number.parseInt(failoverFailureThreshold, 10)
    if (
      !/^\d+$/.test(failoverFailureThreshold) ||
      !Number.isInteger(parsedFailureThreshold) ||
      parsedFailureThreshold < 1
    ) {
      showToast(
        t("validation.invalidFormat", { field: t("groupEditPage.failoverFailureThreshold") }),
        "error"
      )
      return
    }

    const parsedCooldownSeconds = Number.parseInt(failoverCooldownSeconds, 10)
    if (
      !/^\d+$/.test(failoverCooldownSeconds) ||
      !Number.isInteger(parsedCooldownSeconds) ||
      parsedCooldownSeconds < 0
    ) {
      showToast(
        t("validation.invalidFormat", { field: t("groupEditPage.failoverCooldownSeconds") }),
        "error"
      )
      return
    }

    const nextConfig: ProxyConfig = {
      ...config,
      groups: config.groups.map(item => {
        if (item.id !== groupId) return item
        return {
          ...item,
          name: name.trim(),
          failover: {
            enabled: failoverEnabled,
            failureThreshold: parsedFailureThreshold,
            cooldownSeconds: parsedCooldownSeconds,
          },
        }
      }),
    }

    try {
      await saveConfig(nextConfig)
      showToast(t("toast.groupUpdated"), "success")
      navigate("/")
    } catch (error) {
      showToast(t("errors.saveFailed", { message: String(error) }), "error")
    }
  }

  if (loading) {
    return (
      <div className={styles.loading}>
        <p>{t("app.statusLoading")}</p>
      </div>
    )
  }

  return (
    <div className={styles.groupEditPage}>
      <div className="app-sub-header">
        <div className="app-sub-header-top">
          <button type="button" onClick={() => navigate("/")} className="app-sub-header-back">
            <ArrowLeft size={16} strokeWidth={2} />
            <span>{t("header.backToService")}</span>
          </button>
        </div>
        <div className="app-sub-header-main">
          <h1 className="app-sub-header-title">{t("groupEditPage.title")}</h1>
          <nav className="app-breadcrumb" aria-label={t("header.backToService")}>
            <button type="button" onClick={() => navigate("/")} className="app-breadcrumb-button">
              {t("servicePage.groupPath")}
            </button>
            <span className="app-breadcrumb-separator">/</span>
            <span className="app-breadcrumb-item">{group?.name}</span>
          </nav>
        </div>
      </div>

      <form className={styles.form} onSubmit={handleSubmit}>
        <div className={styles.section}>
          <h2 className={styles.sectionTitle}>{t("groupEditPage.sectionBasic")}</h2>

          <div className={styles.formGroup}>
            <label htmlFor="groupId">{t("modal.groupIdLabel")}</label>
            <Input id="groupId" value={group?.id ?? ""} disabled />
            <p className={styles.hint}>{t("groupEditPage.groupIdImmutable")}</p>
          </div>

          <div className={styles.formGroup}>
            <label htmlFor="groupName">{t("modal.groupNameLabel")}</label>
            <Input
              id="groupName"
              value={name}
              onChange={e => setName(e.target.value)}
              placeholder={t("modal.groupNamePlaceholder")}
            />
            <p className={styles.hint}>{t("groupEditPage.groupNameHint")}</p>
          </div>
        </div>

        <div className={styles.section}>
          <h2 className={styles.sectionTitle}>{t("groupEditPage.sectionFailover")}</h2>

          <div className={styles.checkboxRow}>
            <label className={styles.checkboxLabel} htmlFor="failoverEnabled">
              <input
                id="failoverEnabled"
                type="checkbox"
                checked={failoverEnabled}
                onChange={e => setFailoverEnabled(e.target.checked)}
              />
              <span>{t("groupEditPage.failoverEnabled")}</span>
            </label>
            <p className={styles.hint}>{t("groupEditPage.failoverEnabledHint")}</p>
          </div>

          <div className={styles.formGroup}>
            <label htmlFor="failoverFailureThreshold">
              {t("groupEditPage.failoverFailureThreshold")}
            </label>
            <Input
              id="failoverFailureThreshold"
              type="number"
              min={1}
              value={failoverFailureThreshold}
              onChange={e => setFailoverFailureThreshold(e.target.value)}
            />
          </div>

          <div className={styles.formGroup}>
            <label htmlFor="failoverCooldownSeconds">
              {t("groupEditPage.failoverCooldownSeconds")}
            </label>
            <Input
              id="failoverCooldownSeconds"
              type="number"
              min={0}
              value={failoverCooldownSeconds}
              onChange={e => setFailoverCooldownSeconds(e.target.value)}
            />
          </div>
        </div>

        <div className={styles.actions}>
          <Button type="button" variant="default" onClick={() => navigate("/")}>
            {t("common.cancel")}
          </Button>
          <Button type="submit" variant="primary">
            {t("common.save")}
          </Button>
        </div>
      </form>
    </div>
  )
}

export default GroupEditPage
