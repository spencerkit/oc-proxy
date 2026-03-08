import { Plus, Trash2 } from "lucide-react"
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
  const [models, setModels] = useState<string[]>([])
  const [newModel, setNewModel] = useState("")
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
    setModels(group.models ?? [])
    setLoading(false)
  }, [group, config, navigate, showToast, t])

  const normalizeModels = (rawModels: string[]) => {
    const next: string[] = []
    const seen = new Set<string>()
    for (const item of rawModels) {
      const value = item.trim()
      if (!value || seen.has(value)) continue
      seen.add(value)
      next.push(value)
    }
    return next
  }

  const handleAddModel = () => {
    const value = newModel.trim()
    if (!value) return
    if (models.includes(value)) return
    setModels(prev => [...prev, value])
    setNewModel("")
  }

  const handleUpdateModel = (index: number, value: string) => {
    setModels(prev => prev.map((item, idx) => (idx === index ? value : item)))
  }

  const handleRemoveModel = (index: number) => {
    setModels(prev => prev.filter((_, idx) => idx !== index))
  }

  const handleSubmit = async (event: React.FormEvent) => {
    event.preventDefault()
    if (!config || !groupId || !group) return
    if (!name.trim()) {
      showToast(t("validation.required", { field: t("servicePage.groupName") }), "error")
      return
    }

    const nextModels = normalizeModels(models)
    const modelSet = new Set(nextModels)

    const nextConfig: ProxyConfig = {
      ...config,
      groups: config.groups.map(item => {
        if (item.id !== groupId) return item
        return {
          ...item,
          name: name.trim(),
          models: nextModels,
          providers: item.providers.map(provider => {
            const nextMappings: Record<string, string> = {}
            for (const [key, mapped] of Object.entries(provider.modelMappings || {})) {
              if (!modelSet.has(key)) continue
              nextMappings[key] = mapped
            }
            return {
              ...provider,
              modelMappings: nextMappings,
            }
          }),
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
      <div className={styles.header}>
        <h1>{t("groupEditPage.title")}</h1>
        <nav className={styles.breadcrumb} aria-label={t("header.backToService")}>
          <button type="button" onClick={() => navigate("/")} className={styles.breadcrumbButton}>
            {t("servicePage.groupPath")}
          </button>
          <span className={styles.breadcrumbSeparator}>/</span>
          <span className={styles.breadcrumbItem}>{group?.name}</span>
        </nav>
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
          <h2 className={styles.sectionTitle}>{t("groupEditPage.sectionModels")}</h2>
          <p className={styles.hint}>{t("groupEditPage.modelMatchHint")}</p>

          <div className={styles.modelList}>
            {models.length === 0 ? (
              <p className={styles.empty}>{t("groupEditPage.noModels")}</p>
            ) : (
              models.map((modelName, index) => (
                <div key={`${index}-${modelName}`} className={styles.modelRow}>
                  <Input
                    value={modelName}
                    onChange={e => handleUpdateModel(index, e.target.value)}
                    placeholder="e.g. a1"
                  />
                  <Button
                    type="button"
                    variant="danger"
                    size="small"
                    icon={Trash2}
                    onClick={() => handleRemoveModel(index)}
                    aria-label={`${t("common.delete")} ${modelName}`}
                  />
                </div>
              ))
            )}
          </div>

          <div className={styles.addModelRow}>
            <Input
              value={newModel}
              onChange={e => setNewModel(e.target.value)}
              placeholder={t("groupEditPage.newModelPlaceholder")}
            />
            <Button
              type="button"
              variant="default"
              size="small"
              icon={Plus}
              onClick={handleAddModel}
            >
              {t("common.add")}
            </Button>
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
