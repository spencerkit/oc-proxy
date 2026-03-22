import { ArrowRight, LockKeyhole, Network, ShieldCheck } from "lucide-react"
import type React from "react"
import { useCallback, useState } from "react"
import { Button, Input } from "@/components/common"
import { useTranslation } from "@/hooks"
import styles from "./RemoteManagementLogin.module.css"

export interface RemoteManagementLoginProps {
  onSubmit: (password: string) => Promise<void>
}

export const RemoteManagementLogin: React.FC<RemoteManagementLoginProps> = ({ onSubmit }) => {
  const { t } = useTranslation()
  const [password, setPassword] = useState("")
  const [error, setError] = useState("")
  const [submitting, setSubmitting] = useState(false)

  const handleSubmit = useCallback(
    async (event: React.FormEvent<HTMLFormElement>) => {
      event.preventDefault()
      const trimmed = password.trim()
      if (!trimmed) {
        setError(t("auth.passwordRequired"))
        return
      }

      try {
        setSubmitting(true)
        setError("")
        await onSubmit(trimmed)
      } catch (submitError) {
        const message =
          submitError instanceof Error ? submitError.message : String(submitError || "")
        setError(message || t("auth.loginFailed"))
      } finally {
        setSubmitting(false)
      }
    },
    [onSubmit, password, t]
  )

  return (
    <div className={styles.page}>
      <div className={styles.backdrop} />

      <div className={styles.shell}>
        <section className={styles.story}>
          <div className={styles.storyHeader}>
            <span className={styles.eyebrow}>{t("auth.gateEyebrow")}</span>
            <h1 className={styles.title}>{t("auth.gateTitle")}</h1>
            <p className={styles.subtitle}>{t("auth.gateSubtitle")}</p>
          </div>

          <div className={styles.storyGrid}>
            <article className={styles.storyCard}>
              <span className={styles.storyIcon}>
                <ShieldCheck size={18} />
              </span>
              <div>
                <h2>{t("auth.protectedTitle")}</h2>
                <p>{t("auth.protectedHint")}</p>
              </div>
            </article>

            <article className={styles.storyCard}>
              <span className={styles.storyIcon}>
                <Network size={18} />
              </span>
              <div>
                <h2>{t("auth.protectedApiTitle")}</h2>
                <p>{t("auth.protectedApiHint")}</p>
              </div>
            </article>
          </div>

          <p className={styles.localBypass}>{t("auth.localBypass")}</p>
        </section>

        <section className={styles.panel}>
          <div className={styles.panelHeader}>
            <span className={styles.panelBadge}>
              <LockKeyhole size={14} />
              {t("auth.panelBadge")}
            </span>
            <p className={styles.panelCopy}>{t("auth.panelCopy")}</p>
          </div>

          <form className={styles.form} onSubmit={handleSubmit}>
            <Input
              id="remote-management-password"
              type="password"
              size="large"
              label={t("auth.passwordLabel")}
              value={password}
              onChange={event => setPassword(event.target.value)}
              placeholder={t("auth.passwordPlaceholder")}
              hint={!error ? t("auth.passwordHint") : undefined}
              error={error || undefined}
              autoFocus
              autoComplete="current-password"
            />

            <Button
              type="submit"
              size="large"
              variant="primary"
              loading={submitting}
              iconRight={ArrowRight}
              fullWidth
            >
              {submitting ? t("auth.submitLoading") : t("auth.submit")}
            </Button>
          </form>
        </section>
      </div>
    </div>
  )
}

export default RemoteManagementLogin
