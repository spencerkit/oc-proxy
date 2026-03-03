import { AlertCircle, CheckCircle, Eye, EyeOff, TestTube2 } from "lucide-react"
import type React from "react"
import { useEffect, useState } from "react"
import { useNavigate, useParams } from "react-router-dom"
import { Button, Input, JsonTreeView, Switch } from "@/components"
import { useLogs, useTranslation } from "@/hooks"
import { useProxyStore } from "@/store"
import type { ProxyConfig, Rule, RuleQuotaSnapshot, RuleQuotaTestResult } from "@/types"
import { ipc } from "@/utils/ipc"
import styles from "./RuleCreatePage.module.css"

const parseQuotaHeaders = (raw: string): Record<string, string> => {
  const trimmed = raw.trim()
  if (!trimmed) return {}

  const parsed = JSON.parse(trimmed) as unknown
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
    throw new Error("headers must be a JSON object")
  }

  const next: Record<string, string> = {}
  for (const [key, value] of Object.entries(parsed as Record<string, unknown>)) {
    if (!key.trim()) continue
    if (typeof value === "string") {
      next[key.trim()] = value
      continue
    }
    if (typeof value === "number" || typeof value === "boolean") {
      next[key.trim()] = String(value)
      continue
    }
    throw new Error(`header value must be string/number/boolean: ${key}`)
  }
  return next
}

const buildRemainingMapping = (expr: string) => {
  const nextExpr = expr.trim()
  if (nextExpr) {
    return { expr: nextExpr }
  }
  return null
}

const normalizeNumericInput = (raw: string) => {
  const normalized = raw.replace(/[^0-9.]/g, "")
  const firstDot = normalized.indexOf(".")
  if (firstDot === -1) {
    return normalized
  }
  return `${normalized.slice(0, firstDot + 1)}${normalized.slice(firstDot + 1).replace(/\./g, "")}`
}

const buildQuotaConfig = ({
  enabled,
  provider,
  endpoint,
  method,
  useRuleToken,
  customToken,
  authHeader,
  authScheme,
  customHeaders,
  unitType,
  lowThresholdPercent,
  remainingExpr,
  unitPath,
  resetAtPath,
}: {
  enabled: boolean
  provider: string
  endpoint: string
  method: string
  useRuleToken: boolean
  customToken: string
  authHeader: string
  authScheme: string
  customHeaders: Record<string, string>
  unitType: Rule["quota"]["unitType"]
  lowThresholdPercent: number
  remainingExpr: string
  unitPath: string
  resetAtPath: string
}): Rule["quota"] => ({
  enabled,
  provider: provider.trim() || "custom",
  endpoint: endpoint.trim(),
  method,
  useRuleToken,
  customToken: useRuleToken ? "" : customToken.trim(),
  authHeader: authHeader.trim(),
  authScheme: authScheme.trim(),
  customHeaders,
  unitType,
  lowThresholdPercent:
    Number.isFinite(lowThresholdPercent) && lowThresholdPercent >= 0 ? lowThresholdPercent : 10,
  response: {
    remaining: buildRemainingMapping(remainingExpr),
    unit: unitPath.trim() || null,
    total: null,
    resetAt: resetAtPath.trim() || null,
  },
})

const formatQuotaValue = (value?: number | null): string => {
  if (value === null || value === undefined || Number.isNaN(value)) {
    return "-"
  }
  const abs = Math.abs(value)
  if (abs >= 1) {
    return value.toFixed(2).replace(/\\.00$/, "")
  }
  return value.toFixed(4).replace(/0+$/, "").replace(/\\.$/, "")
}

const formatTokenQuotaValue = (value?: number | null): string => {
  if (value === null || value === undefined || Number.isNaN(value)) {
    return "-"
  }
  const abs = Math.abs(value)
  if (abs >= 1_000_000) {
    return `${(value / 1_000_000).toFixed(2).replace(/\\.00$/, "")}M`
  }
  return Number.isInteger(value)
    ? String(value)
    : value
        .toFixed(2)
        .replace(/\\.00$/, "")
        .replace(/\\.$/, "")
}

const formatQuotaPreviewByUnitType = (
  unitType: Rule["quota"]["unitType"],
  snapshot?: RuleQuotaSnapshot | null
): string => {
  if (!snapshot) return "-"
  if (unitType === "percentage") {
    const basis =
      snapshot.percent !== null && snapshot.percent !== undefined
        ? snapshot.percent
        : snapshot.remaining !== null && snapshot.remaining !== undefined
          ? snapshot.remaining
          : null
    if (basis === null || Number.isNaN(basis)) return "-"
    const value = formatQuotaValue(basis)
    return value.endsWith("%") ? value : `${value}%`
  }
  if (unitType === "amount") {
    return formatQuotaValue(snapshot.remaining)
  }
  if (snapshot.unit?.trim()) {
    return `${formatQuotaValue(snapshot.remaining)} ${snapshot.unit.trim()}`
  }
  return formatTokenQuotaValue(snapshot.remaining)
}

/**
 * RuleCreatePage Component
 * Page for creating a new rule
 */
export const RuleCreatePage: React.FC = () => {
  const { groupId } = useParams<{ groupId: string }>()
  const navigate = useNavigate()
  const { t } = useTranslation()
  const { config, saveConfig } = useProxyStore()
  const { showToast } = useLogs()

  const [name, setName] = useState("")
  const [protocol, setProtocol] = useState<Rule["protocol"]>("anthropic")
  const [token, setToken] = useState("")
  const [showToken, setShowToken] = useState(false)
  const [apiAddress, setApiAddress] = useState("")
  const [defaultModel, setDefaultModel] = useState("")
  const [modelMappings, setModelMappings] = useState<Record<string, string>>({})

  const [quotaEnabled, setQuotaEnabled] = useState(false)
  const [quotaProvider, setQuotaProvider] = useState("custom")
  const [quotaEndpoint, setQuotaEndpoint] = useState("")
  const [quotaMethod, setQuotaMethod] = useState("GET")
  const [quotaUseRuleToken, setQuotaUseRuleToken] = useState(true)
  const [quotaCustomToken, setQuotaCustomToken] = useState("")
  const [quotaAuthHeader, setQuotaAuthHeader] = useState("Authorization")
  const [quotaAuthScheme, setQuotaAuthScheme] = useState("Bearer")
  const [quotaHeadersText, setQuotaHeadersText] = useState("{}")
  const [quotaUnitType, setQuotaUnitType] = useState<Rule["quota"]["unitType"]>("percentage")
  const [quotaRemainingExpr, setQuotaRemainingExpr] = useState("")
  const quotaUnitPath = ""
  const [quotaResetAtPath, setQuotaResetAtPath] = useState("")
  const [quotaLowThresholdPercent, setQuotaLowThresholdPercent] = useState("10")
  const [quotaTestLoading, setQuotaTestLoading] = useState(false)
  const [quotaTestResult, setQuotaTestResult] = useState<RuleQuotaTestResult | null>(null)
  const [quotaTestFingerprint, setQuotaTestFingerprint] = useState<string | null>(null)

  const [errors, setErrors] = useState<{
    name?: string
    token?: string
    apiAddress?: string
    defaultModel?: string
    quotaEndpoint?: string
    quotaHeaders?: string
    quotaRemaining?: string
    quotaThreshold?: string
  }>({})

  const group = config?.groups.find(g => g.id === groupId)

  const quotaDraftFingerprint = JSON.stringify({
    token,
    name,
    quotaEnabled,
    quotaProvider,
    quotaEndpoint,
    quotaMethod,
    quotaUseRuleToken,
    quotaCustomToken,
    quotaAuthHeader,
    quotaAuthScheme,
    quotaHeadersText,
    quotaUnitType,
    quotaRemainingExpr,
    quotaUnitPath,
    quotaResetAtPath,
    quotaLowThresholdPercent,
  })
  const quotaTestDirty =
    !!quotaTestResult && !!quotaTestFingerprint && quotaTestFingerprint !== quotaDraftFingerprint

  useEffect(() => {
    if (!config || group) return
    showToast(t("toast.groupNotFound"), "error")
    navigate("/")
  }, [config, group, navigate, showToast, t])

  useEffect(() => {
    if (quotaEnabled) return
    setQuotaTestLoading(false)
    setQuotaTestResult(null)
    setQuotaTestFingerprint(null)
  }, [quotaEnabled])

  if (!group) return null

  const focusField = (id: string) => {
    const input = document.getElementById(id) as HTMLInputElement | HTMLTextAreaElement | null
    input?.focus()
  }

  const validateForm = () => {
    const nextErrors: {
      name?: string
      token?: string
      apiAddress?: string
      defaultModel?: string
      quotaEndpoint?: string
      quotaHeaders?: string
      quotaRemaining?: string
      quotaThreshold?: string
    } = {}

    if (!name.trim()) {
      nextErrors.name = t("validation.required", { field: t("servicePage.ruleName") })
    }
    if (!token.trim()) {
      nextErrors.token = t("validation.required", { field: t("servicePage.token") })
    }
    if (!apiAddress.trim()) {
      nextErrors.apiAddress = t("validation.required", { field: t("servicePage.apiAddress") })
    }
    if (!defaultModel.trim()) {
      nextErrors.defaultModel = t("validation.required", { field: t("servicePage.defaultModel") })
    }

    if (quotaEnabled) {
      if (!quotaEndpoint.trim()) {
        nextErrors.quotaEndpoint = t("validation.required", { field: t("ruleForm.quotaEndpoint") })
      }
      if (!quotaRemainingExpr.trim()) {
        nextErrors.quotaRemaining = t("validation.required", {
          field: t("ruleForm.quotaRemainingMapping"),
        })
      }
      try {
        parseQuotaHeaders(quotaHeadersText)
      } catch {
        nextErrors.quotaHeaders = t("ruleForm.quotaHeadersError")
      }

      const threshold = Number(quotaLowThresholdPercent)
      if (!Number.isFinite(threshold) || threshold < 0) {
        nextErrors.quotaThreshold = t("ruleForm.quotaThresholdError")
      }
    }

    setErrors(nextErrors)

    if (nextErrors.name) {
      focusField("name")
      return false
    }
    if (nextErrors.token) {
      focusField("token")
      return false
    }
    if (nextErrors.apiAddress) {
      focusField("apiAddress")
      return false
    }
    if (nextErrors.defaultModel) {
      focusField("defaultModel")
      return false
    }
    if (nextErrors.quotaEndpoint) {
      focusField("quota-endpoint")
      return false
    }
    if (nextErrors.quotaRemaining) {
      focusField("quota-remaining-expr")
      return false
    }
    if (nextErrors.quotaHeaders) {
      focusField("quota-headers")
      return false
    }
    if (nextErrors.quotaThreshold) {
      focusField("quota-threshold")
      return false
    }
    return true
  }

  const resolveQuotaTestStatusText = (snapshot?: RuleQuotaSnapshot | null): string => {
    if (!snapshot) return "-"
    return t(`ruleQuota.${snapshot.status}`)
  }

  const handleTestQuota = async () => {
    if (!groupId) return

    if (!quotaEndpoint.trim()) {
      setQuotaTestResult({
        ok: false,
        message: t("validation.required", { field: t("ruleForm.quotaEndpoint") }),
      })
      return
    }
    if (!quotaRemainingExpr.trim()) {
      setQuotaTestResult({
        ok: false,
        message: t("validation.required", { field: t("ruleForm.quotaRemainingMapping") }),
      })
      return
    }

    let quotaHeaders: Record<string, string>
    try {
      quotaHeaders = parseQuotaHeaders(quotaHeadersText)
    } catch {
      setQuotaTestResult({
        ok: false,
        message: t("ruleForm.quotaHeadersError"),
      })
      return
    }

    const threshold = Number(quotaLowThresholdPercent)
    if (!Number.isFinite(threshold) || threshold < 0) {
      setQuotaTestResult({
        ok: false,
        message: t("ruleForm.quotaThresholdError"),
      })
      return
    }

    const quotaConfig = buildQuotaConfig({
      enabled: true,
      provider: quotaProvider,
      endpoint: quotaEndpoint,
      method: quotaMethod,
      useRuleToken: quotaUseRuleToken,
      customToken: quotaCustomToken,
      authHeader: quotaAuthHeader,
      authScheme: quotaAuthScheme,
      customHeaders: quotaHeaders,
      unitType: quotaUnitType,
      lowThresholdPercent: threshold,
      remainingExpr: quotaRemainingExpr,
      unitPath: quotaUnitPath,
      resetAtPath: quotaResetAtPath,
    })

    setQuotaTestLoading(true)
    try {
      const result = await ipc.testRuleQuotaDraft(
        groupId,
        name.trim() || "Draft Rule",
        token,
        apiAddress,
        defaultModel,
        quotaConfig
      )
      setQuotaTestResult(result)
      setQuotaTestFingerprint(quotaDraftFingerprint)
    } catch (error) {
      setQuotaTestResult({
        ok: false,
        message: String(error),
      })
      setQuotaTestFingerprint(quotaDraftFingerprint)
    } finally {
      setQuotaTestLoading(false)
    }
  }

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!config || !groupId) return
    if (!validateForm()) return

    const quotaHeaders = parseQuotaHeaders(quotaHeadersText)
    const threshold = Number(quotaLowThresholdPercent)
    const quotaConfig = buildQuotaConfig({
      enabled: quotaEnabled,
      provider: quotaProvider,
      endpoint: quotaEndpoint,
      method: quotaMethod,
      useRuleToken: quotaUseRuleToken,
      customToken: quotaCustomToken,
      authHeader: quotaAuthHeader,
      authScheme: quotaAuthScheme,
      customHeaders: quotaHeaders,
      unitType: quotaUnitType,
      lowThresholdPercent: threshold,
      remainingExpr: quotaRemainingExpr,
      unitPath: quotaUnitPath,
      resetAtPath: quotaResetAtPath,
    })

    const newRule: Rule = {
      id: crypto.randomUUID(),
      name: name.trim(),
      protocol,
      token,
      apiAddress,
      defaultModel: defaultModel.trim(),
      modelMappings: Object.fromEntries(
        Object.entries(modelMappings)
          .map(([key, value]) => [key.trim(), value.trim()])
          .filter(([key, value]) => key && value)
      ),
      quota: quotaConfig,
    }

    const newConfig: ProxyConfig = {
      ...config,
      groups: config.groups.map(group => {
        if (group.id === groupId) {
          return {
            ...group,
            rules: [...group.rules, newRule],
            activeRuleId: group.activeRuleId ?? newRule.id,
          }
        }
        return group
      }),
    }

    try {
      await saveConfig(newConfig)
      showToast(t("toast.ruleCreated"), "success")
      navigate("/")
    } catch (error) {
      showToast(t("errors.saveFailed", { message: String(error) }), "error")
    }
  }

  const handleCancel = () => {
    navigate("/")
  }

  const isValid =
    name.trim() &&
    token.trim() &&
    apiAddress.trim() &&
    defaultModel.trim() &&
    (!quotaEnabled || (quotaEndpoint.trim() && quotaRemainingExpr.trim()))

  return (
    <div className={styles.ruleCreatePage}>
      <div className={styles.header}>
        <h1>{t("ruleCreatePage.title")}</h1>
        <nav className={styles.breadcrumb} aria-label={t("header.backToService")}>
          <button type="button" onClick={() => navigate("/")} className={styles.breadcrumbButton}>
            {t("servicePage.groupPath")}
          </button>
          <span className={styles.breadcrumbSeparator}>/</span>
          <span className={styles.breadcrumbItem}>{group.name}</span>
          <span className={styles.breadcrumbSeparator}>/</span>
          <span className={styles.breadcrumbItem}>{t("ruleCreatePage.newRule")}</span>
        </nav>
      </div>

      <div className={styles.formContainer}>
        <div className={styles.ruleGrid}>
          <form onSubmit={handleSubmit} className={styles.ruleForm}>
            <section className={styles.formSection}>
              <h2 className={styles.sectionTitle}>{t("ruleForm.sectionRouting")}</h2>

              <div className={styles.formGroup}>
                <label htmlFor="name">{t("servicePage.ruleName")}</label>
                <Input
                  id="name"
                  value={name}
                  onChange={e => {
                    setName(e.target.value)
                    if (errors.name) {
                      setErrors(prev => ({ ...prev, name: undefined }))
                    }
                  }}
                  placeholder={t("ruleForm.ruleNamePlaceholder")}
                  className={styles.input}
                  error={errors.name}
                />
              </div>

              <div className={styles.formGroup}>
                <label htmlFor="create-rule-protocol-anthropic">
                  {t("servicePage.ruleProtocol")}
                </label>
                <div className={styles.directionOptions}>
                  <button
                    id="create-rule-protocol-anthropic"
                    type="button"
                    className={`${styles.directionOption} ${protocol === "anthropic" ? styles.active : ""}`}
                    onClick={() => setProtocol("anthropic")}
                  >
                    {t("ruleProtocol.anthropic")}
                  </button>
                  <button
                    type="button"
                    className={`${styles.directionOption} ${protocol === "openai_completion" ? styles.active : ""}`}
                    onClick={() => setProtocol("openai_completion")}
                  >
                    {t("ruleProtocol.openai_completion")}
                  </button>
                  <button
                    type="button"
                    className={`${styles.directionOption} ${protocol === "openai" ? styles.active : ""}`}
                    onClick={() => setProtocol("openai")}
                  >
                    {t("ruleProtocol.openai")}
                  </button>
                </div>
                <p className={styles.fieldHint}>{t("ruleForm.protocolHint")}</p>
              </div>
            </section>

            <section className={styles.formSection}>
              <h2 className={styles.sectionTitle}>{t("ruleForm.sectionModelSettings")}</h2>

              <div className={styles.formGroup}>
                <label htmlFor="defaultModel">{t("servicePage.defaultModel")}</label>
                <Input
                  id="defaultModel"
                  value={defaultModel}
                  onChange={e => {
                    setDefaultModel(e.target.value)
                    if (errors.defaultModel) {
                      setErrors(prev => ({ ...prev, defaultModel: undefined }))
                    }
                  }}
                  placeholder={t("ruleForm.defaultModelPlaceholder")}
                  className={styles.input}
                  error={errors.defaultModel}
                  hint={t("ruleForm.defaultModelHint")}
                />
              </div>

              <div className={styles.formGroup}>
                <label htmlFor="create-rule-mapping-first">{t("ruleForm.modelMappings")}</label>
                <div className={styles.mappingList}>
                  {(group.models || []).length === 0 ? (
                    <p className={styles.fieldHint}>{t("ruleForm.noGroupModels")}</p>
                  ) : (
                    (group.models || []).map(modelName => (
                      <div key={modelName} className={styles.mappingRow}>
                        <span className={styles.mappingLabel}>{modelName}</span>
                        <Input
                          id={
                            modelName === (group.models || [])[0]
                              ? "create-rule-mapping-first"
                              : undefined
                          }
                          value={modelMappings[modelName] ?? ""}
                          onChange={e => {
                            setModelMappings(prev => ({ ...prev, [modelName]: e.target.value }))
                          }}
                          placeholder={t("ruleForm.mappingPlaceholder")}
                        />
                      </div>
                    ))
                  )}
                </div>
              </div>
            </section>

            <section className={styles.formSection}>
              <h2 className={styles.sectionTitle}>{t("ruleForm.sectionSecurity")}</h2>

              <div className={styles.formGroup}>
                <label htmlFor="token">{t("servicePage.token")}</label>
                <Input
                  id="token"
                  type={showToken ? "text" : "password"}
                  value={token}
                  onChange={e => {
                    setToken(e.target.value)
                    if (errors.token) {
                      setErrors(prev => ({ ...prev, token: undefined }))
                    }
                  }}
                  placeholder="sk-..."
                  className={styles.input}
                  error={errors.token}
                  hint={t("ruleForm.tokenHint")}
                  endAdornment={
                    <button
                      type="button"
                      className={styles.tokenVisibilityButton}
                      onClick={() => setShowToken(prev => !prev)}
                      aria-label={showToken ? t("ruleForm.hideToken") : t("ruleForm.showToken")}
                      title={showToken ? t("ruleForm.hideToken") : t("ruleForm.showToken")}
                    >
                      {showToken ? <EyeOff size={16} /> : <Eye size={16} />}
                    </button>
                  }
                />
              </div>

              <div className={styles.formGroup}>
                <label htmlFor="apiAddress">{t("servicePage.apiAddress")}</label>
                <Input
                  id="apiAddress"
                  value={apiAddress}
                  onChange={e => {
                    setApiAddress(e.target.value)
                    if (errors.apiAddress) {
                      setErrors(prev => ({ ...prev, apiAddress: undefined }))
                    }
                  }}
                  placeholder="https://api.anthropic.com"
                  className={styles.input}
                  error={errors.apiAddress}
                  hint={t("ruleForm.endpointHint")}
                />
              </div>
            </section>

            <section className={styles.formSection}>
              <h2 className={styles.sectionTitle}>{t("ruleForm.sectionQuota")}</h2>

              <div className={styles.switchRow}>
                <div>
                  <label htmlFor="quota-enabled">{t("ruleForm.quotaEnabled")}</label>
                  <p className={styles.fieldHint}>{t("ruleForm.quotaEnabledHint")}</p>
                </div>
                <Switch
                  id="quota-enabled"
                  checked={quotaEnabled}
                  onChange={next => setQuotaEnabled(next)}
                />
              </div>

              {quotaEnabled && (
                <>
                  <div className={styles.formGroup}>
                    <label htmlFor="quota-provider">{t("ruleForm.quotaProvider")}</label>
                    <Input
                      id="quota-provider"
                      value={quotaProvider}
                      onChange={e => setQuotaProvider(e.target.value)}
                      placeholder="custom"
                    />
                  </div>

                  <div className={styles.formGroup}>
                    <label htmlFor="quota-endpoint">{t("ruleForm.quotaEndpoint")}</label>
                    <Input
                      id="quota-endpoint"
                      value={quotaEndpoint}
                      onChange={e => {
                        setQuotaEndpoint(e.target.value)
                        if (errors.quotaEndpoint) {
                          setErrors(prev => ({ ...prev, quotaEndpoint: undefined }))
                        }
                      }}
                      placeholder="https://provider.example.com/quota"
                      error={errors.quotaEndpoint}
                      hint={t("ruleForm.quotaEndpointHint")}
                    />
                  </div>

                  <div className={styles.formGroup}>
                    <label htmlFor="quota-method-get">{t("ruleForm.quotaMethod")}</label>
                    <div className={styles.directionOptions}>
                      <button
                        id="quota-method-get"
                        type="button"
                        className={`${styles.directionOption} ${quotaMethod === "GET" ? styles.active : ""}`}
                        onClick={() => setQuotaMethod("GET")}
                      >
                        GET
                      </button>
                      <button
                        type="button"
                        className={`${styles.directionOption} ${quotaMethod === "POST" ? styles.active : ""}`}
                        onClick={() => setQuotaMethod("POST")}
                      >
                        POST
                      </button>
                    </div>
                  </div>

                  <div className={styles.switchRow}>
                    <div>
                      <label htmlFor="quota-use-rule-token">
                        {t("ruleForm.quotaUseRuleToken")}
                      </label>
                      <p className={styles.fieldHint}>{t("ruleForm.quotaUseRuleTokenHint")}</p>
                    </div>
                    <Switch
                      id="quota-use-rule-token"
                      checked={quotaUseRuleToken}
                      onChange={next => setQuotaUseRuleToken(next)}
                    />
                  </div>

                  {!quotaUseRuleToken && (
                    <div className={styles.formGroup}>
                      <label htmlFor="quota-custom-token">{t("ruleForm.quotaCustomToken")}</label>
                      <Input
                        id="quota-custom-token"
                        type="password"
                        value={quotaCustomToken}
                        onChange={e => setQuotaCustomToken(e.target.value)}
                        placeholder="token..."
                      />
                    </div>
                  )}

                  <div className={styles.dualColumnRow}>
                    <div className={styles.formGroup}>
                      <label htmlFor="quota-auth-header">{t("ruleForm.quotaAuthHeader")}</label>
                      <Input
                        id="quota-auth-header"
                        value={quotaAuthHeader}
                        onChange={e => setQuotaAuthHeader(e.target.value)}
                        placeholder="Authorization"
                      />
                    </div>
                    <div className={styles.formGroup}>
                      <label htmlFor="quota-auth-scheme">{t("ruleForm.quotaAuthScheme")}</label>
                      <Input
                        id="quota-auth-scheme"
                        value={quotaAuthScheme}
                        onChange={e => setQuotaAuthScheme(e.target.value)}
                        placeholder="Bearer"
                      />
                    </div>
                  </div>

                  <div className={styles.formGroup}>
                    <label htmlFor="quota-headers">{t("ruleForm.quotaHeaders")}</label>
                    <textarea
                      id="quota-headers"
                      className={styles.jsonTextarea}
                      value={quotaHeadersText}
                      onChange={e => {
                        setQuotaHeadersText(e.target.value)
                        if (errors.quotaHeaders) {
                          setErrors(prev => ({ ...prev, quotaHeaders: undefined }))
                        }
                      }}
                      placeholder='{"x-api-key":"{{rule.token}}"}'
                    />
                    {errors.quotaHeaders ? (
                      <p className={styles.errorText}>{errors.quotaHeaders}</p>
                    ) : (
                      <p className={styles.fieldHint}>{t("ruleForm.quotaHeadersHint")}</p>
                    )}
                  </div>

                  <div className={styles.formGroup}>
                    <label htmlFor="quota-remaining-expr">{t("ruleForm.quotaRemainingExpr")}</label>
                    <Input
                      id="quota-remaining-expr"
                      value={quotaRemainingExpr}
                      onChange={e => {
                        setQuotaRemainingExpr(e.target.value)
                        if (errors.quotaRemaining) {
                          setErrors(prev => ({ ...prev, quotaRemaining: undefined }))
                        }
                      }}
                      placeholder="$.data.remaining_balance/$.data.remaining_total"
                      hint={
                        errors.quotaRemaining ? undefined : t("ruleForm.quotaRemainingExprHint")
                      }
                      error={errors.quotaRemaining}
                    />
                  </div>

                  <div className={styles.dualColumnRow}>
                    <div className={styles.formGroup}>
                      <label htmlFor="quota-unit-type">{t("ruleForm.quotaUnitType")}</label>
                      <select
                        id="quota-unit-type"
                        className={styles.nativeSelect}
                        value={quotaUnitType}
                        onChange={e =>
                          setQuotaUnitType(e.target.value as Rule["quota"]["unitType"])
                        }
                      >
                        <option value="percentage">{t("ruleForm.quotaUnitTypePercentage")}</option>
                        <option value="amount">{t("ruleForm.quotaUnitTypeAmount")}</option>
                        <option value="tokens">{t("ruleForm.quotaUnitTypeTokens")}</option>
                      </select>
                    </div>
                    <div className={styles.formGroup}>
                      <label htmlFor="quota-reset-at-path">{t("ruleForm.quotaResetAtPath")}</label>
                      <Input
                        id="quota-reset-at-path"
                        value={quotaResetAtPath}
                        onChange={e => setQuotaResetAtPath(e.target.value)}
                        placeholder="$.data.reset_at"
                      />
                    </div>
                  </div>

                  <div className={styles.formGroup}>
                    <label htmlFor="quota-threshold">{t("ruleForm.quotaLowThreshold")}</label>
                    <Input
                      id="quota-threshold"
                      type="number"
                      inputMode="decimal"
                      min="0"
                      step="0.01"
                      value={quotaLowThresholdPercent}
                      onChange={e => {
                        setQuotaLowThresholdPercent(normalizeNumericInput(e.target.value))
                        if (errors.quotaThreshold) {
                          setErrors(prev => ({ ...prev, quotaThreshold: undefined }))
                        }
                      }}
                      placeholder="10"
                      error={errors.quotaThreshold}
                      hint={
                        !errors.quotaThreshold ? t("ruleForm.quotaLowThresholdHint") : undefined
                      }
                    />
                  </div>

                  <div className={styles.quotaTestActions}>
                    <Button
                      type="button"
                      size="small"
                      variant="primary"
                      icon={TestTube2}
                      loading={quotaTestLoading}
                      onClick={() => void handleTestQuota()}
                    >
                      {quotaTestLoading ? t("ruleForm.quotaTesting") : t("ruleForm.quotaTest")}
                    </Button>
                    <p className={styles.fieldHint}>{t("ruleForm.quotaTestHint")}</p>
                  </div>

                  {quotaTestResult && (
                    <div
                      className={`${styles.quotaTestResult} ${quotaTestResult.ok ? styles.quotaTestResultSuccess : styles.quotaTestResultError}`}
                    >
                      <div className={styles.quotaTestHeader}>
                        {quotaTestResult.ok ? (
                          <CheckCircle size={16} className={styles.quotaTestIconSuccess} />
                        ) : (
                          <AlertCircle size={16} className={styles.quotaTestIconError} />
                        )}
                        <strong>
                          {quotaTestResult.ok
                            ? t("ruleForm.quotaTestSuccess")
                            : t("ruleForm.quotaTestFailed")}
                        </strong>
                      </div>

                      {quotaTestDirty && (
                        <p className={styles.quotaTestDirty}>{t("ruleForm.quotaTestDirty")}</p>
                      )}

                      {quotaTestResult.message && (
                        <p className={styles.quotaTestMessage}>{quotaTestResult.message}</p>
                      )}

                      {quotaTestResult.snapshot && (
                        <div className={styles.quotaTestGrid}>
                          <div>
                            <span>{t("ruleForm.quotaTestStatus")}</span>
                            <strong>{resolveQuotaTestStatusText(quotaTestResult.snapshot)}</strong>
                          </div>
                          <div>
                            <span>{t("ruleForm.quotaTestRemaining")}</span>
                            <strong>
                              {formatQuotaPreviewByUnitType(
                                quotaUnitType,
                                quotaTestResult.snapshot
                              )}
                            </strong>
                          </div>
                          <div>
                            <span>{t("ruleForm.quotaTestResponseUnit")}</span>
                            <strong>{quotaTestResult.snapshot.unit || "-"}</strong>
                          </div>
                          <div>
                            <span>{t("ruleForm.quotaTestPercent")}</span>
                            <strong>
                              {quotaTestResult.snapshot.percent === null ||
                              quotaTestResult.snapshot.percent === undefined
                                ? "-"
                                : `${quotaTestResult.snapshot.percent.toFixed(2)}%`}
                            </strong>
                          </div>
                          <div>
                            <span>{t("ruleForm.quotaResetAtPath")}</span>
                            <strong>{quotaTestResult.snapshot.resetAt || "-"}</strong>
                          </div>
                        </div>
                      )}

                      <div className={styles.quotaRawSection}>
                        <p className={styles.quotaRawTitle}>{t("ruleForm.quotaTestRawResponse")}</p>
                        <JsonTreeView
                          value={quotaTestResult.rawResponse}
                          emptyText={t("logs.emptyValue")}
                          resetKey={`${quotaTestFingerprint ?? ""}-${quotaTestResult.snapshot?.fetchedAt ?? ""}`}
                        />
                      </div>
                    </div>
                  )}
                </>
              )}
            </section>

            <div className={styles.formActions}>
              <Button variant="default" onClick={handleCancel} className={styles.button}>
                {t("common.cancel")}
              </Button>
              <Button type="submit" variant="primary" disabled={!isValid} className={styles.button}>
                {t("ruleCreatePage.createRule")}
              </Button>
            </div>
          </form>
        </div>
      </div>
    </div>
  )
}

export default RuleCreatePage
