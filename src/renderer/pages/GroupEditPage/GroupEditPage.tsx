import { ArrowLeft } from "lucide-react"
import type React from "react"
import { useEffect, useState } from "react"
import { useNavigate, useParams } from "react-router-dom"
import { shallow } from "zustand/shallow"
import { Button, Input } from "@/components"
import { useLogs, useTranslation } from "@/hooks"
import { useProxyStore } from "@/store"
import type { ProxyConfig } from "@/types"
import styles from "./GroupEditPage.module.css"

export const GroupEditPage: React.FC = () => {
  const { groupId } = useParams<{ groupId: string }>()
  const navigate = useNavigate()
  const { t } = useTranslation()
  const { showToast } = useLogs()
  const { config, saveConfig } = useProxyStore(
    state => ({
      config: state.config,
      saveConfig: state.saveConfig,
    }),
    shallow
  )

  const group = config?.groups.find(item => item.id === groupId)

  const [name, setName] = useState("")
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
    setLoading(false)
  }, [group, config, navigate, showToast, t])

  const handleSubmit = async (event: React.FormEvent) => {
    event.preventDefault()
    if (!config || !groupId || !group) return
    if (!name.trim()) {
      showToast(t("validation.required", { field: t("servicePage.groupName") }), "error")
      return
    }

    const nextConfig: ProxyConfig = {
      ...config,
      groups: config.groups.map(item => {
        if (item.id !== groupId) return item
        return {
          ...item,
          name: name.trim(),
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
