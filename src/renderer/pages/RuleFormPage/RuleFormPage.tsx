import { AlertCircle, ArrowLeft, CheckCircle, Eye, EyeOff, TestTube2 } from "lucide-react"
import type React from "react"
import { useEffect, useState } from "react"
import { useNavigate, useParams } from "react-router-dom"
import { Button, Input, JsonTreeView, Modal, Switch } from "@/components"
import { useLogs, useTranslation } from "@/hooks"
import { configState, saveConfigAction, testRuleQuotaDraftAction } from "@/store"
import type { Provider, ProxyConfig, RuleQuotaSnapshot, RuleQuotaTestResult } from "@/types"
import type { BillingTemplateAttribution } from "@/types/proxy"
import {
  applyBillingTemplateToCost,
  canApplyBillingTemplate,
  doesCostMatchBillingTemplate,
  findBillingTemplate,
  searchBillingTemplates,
} from "@/utils/billingTemplates"
import { createStableId } from "@/utils/id"
import {
  applyProviderImportDraft,
  type ProviderImportInputFormat,
  ProviderImportParseError,
  type ProviderImportParseResult,
  parseProviderImport,
} from "@/utils/providerImport"
import { useActions, useRelaxValue } from "@/utils/relax"
import ProviderImportCard from "./ProviderImportCard"
import styles from "./RuleFormPage.module.css"

const RULE_FORM_ACTIONS = [saveConfigAction, testRuleQuotaDraftAction] as const

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

const readMappingPath = (mapping: unknown): string => {
  if (typeof mapping === "string") return mapping
  if (mapping && typeof mapping === "object" && "path" in mapping) {
    const path = (mapping as { path?: unknown }).path
    return typeof path === "string" ? path : ""
  }
  return ""
}

const readMappingExpr = (mapping: unknown): string => {
  if (typeof mapping === "string") return mapping
  if (mapping && typeof mapping === "object" && "expr" in mapping) {
    const expr = (mapping as { expr?: unknown }).expr
    return typeof expr === "string" ? expr : ""
  }
  return ""
}

/** Builds remaining mapping. */
const buildRemainingMapping = (expr: string) => {
  const nextExpr = expr.trim()
  if (nextExpr) {
    return { expr: nextExpr }
  }
  return null
}

/** Normalizes numeric input. */
const normalizeNumericInput = (raw: string) => {
  const normalized = raw.replace(/[^0-9.]/g, "")
  const firstDot = normalized.indexOf(".")
  if (firstDot === -1) {
    return normalized
  }
  return `${normalized.slice(0, firstDot + 1)}${normalized.slice(firstDot + 1).replace(/\./g, "")}`
}

const COST_CURRENCY_OPTIONS = ["USD", "CNY", "EUR", "JPY", "HKD", "GBP", "SGD"] as const

const parseCostInputValue = (value: string): number => Number(value || "0")

const formatBillingTemplatePrice = (value: number | undefined, currency: string): string => {
  if (value === undefined) return "-"
  return `${value} ${currency}`
}

const PROVIDER_IMPORT_ERROR_KEYS: Record<
  ProviderImportParseError["code"],
  | "ruleForm.importErrorInvalidJson"
  | "ruleForm.importErrorInvalidToml"
  | "ruleForm.importErrorUnrecognizedFormat"
  | "ruleForm.importErrorUnsupportedProtocol"
  | "ruleForm.importErrorNoSupportedFields"
> = {
  invalid_json: "ruleForm.importErrorInvalidJson",
  invalid_toml: "ruleForm.importErrorInvalidToml",
  unrecognized_format: "ruleForm.importErrorUnrecognizedFormat",
  unsupported_protocol: "ruleForm.importErrorUnsupportedProtocol",
  no_supported_fields: "ruleForm.importErrorNoSupportedFields",
}

const normalizeQuotaUnitType = (raw: unknown): Provider["quota"]["unitType"] => {
  if (raw === "percentage" || raw === "amount" || raw === "tokens") {
    return raw
  }
  return "percentage"
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
  unitType: Provider["quota"]["unitType"]
  lowThresholdPercent: number
  remainingExpr: string
  unitPath: string
  resetAtPath: string
}): Provider["quota"] => ({
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
  unitType: Provider["quota"]["unitType"],
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

export interface RuleFormPageProps {
  mode: "create" | "edit"
}

/**
 * RuleFormPage Component
 * Shared page for creating and editing providers
 */
export const RuleFormPage: React.FC<RuleFormPageProps> = ({ mode }) => {
  const { groupId, providerId } = useParams<{ groupId?: string; providerId?: string }>()
  const navigate = useNavigate()
  const { t } = useTranslation()
  const config = useRelaxValue(configState)
  const [saveConfig, testRuleQuotaDraft] = useActions(RULE_FORM_ACTIONS)
  const { showToast } = useLogs()
  const isEditMode = mode === "edit"
  const anthropicProtocolHelp = t("ruleForm.protocolDescriptionAnthropic")
  const openaiChatProtocolHelp = t("ruleForm.protocolDescriptionOpenaiCompletion")
  const openaiResponsesProtocolHelp = t("ruleForm.protocolDescriptionOpenai")

  const [name, setName] = useState("")
  const [protocol, setProtocol] = useState<Provider["protocol"]>("anthropic")
  const [token, setToken] = useState("")
  const [showToken, setShowToken] = useState(false)
  const [apiAddress, setApiAddress] = useState("")
  const [website, setWebsite] = useState("")
  const [defaultModel, setDefaultModel] = useState("")
  const [modelMappings, setModelMappings] = useState<Record<string, string>>({})
  const [importFormat, setImportFormat] = useState<ProviderImportInputFormat>("auto")
  const [importText, setImportText] = useState("")
  const [importResult, setImportResult] = useState<ProviderImportParseResult | null>(null)
  const [importError, setImportError] = useState<string | null>(null)
  const [showImportModal, setShowImportModal] = useState(false)
  const [showBillingTemplateModal, setShowBillingTemplateModal] = useState(false)
  const [billingTemplateSearch, setBillingTemplateSearch] = useState("")
  const [selectedBillingVendorId, setSelectedBillingVendorId] = useState("")
  const [selectedBillingModelId, setSelectedBillingModelId] = useState("")

  const [quotaEnabled, setQuotaEnabled] = useState(false)
  const [quotaProvider, setQuotaProvider] = useState("custom")
  const [quotaEndpoint, setQuotaEndpoint] = useState("")
  const [quotaMethod, setQuotaMethod] = useState("GET")
  const [quotaUseRuleToken, setQuotaUseRuleToken] = useState(true)
  const [quotaCustomToken, setQuotaCustomToken] = useState("")
  const [quotaAuthHeader, setQuotaAuthHeader] = useState("Authorization")
  const [quotaAuthScheme, setQuotaAuthScheme] = useState("Bearer")
  const [quotaHeadersText, setQuotaHeadersText] = useState("{}")
  const [quotaUnitType, setQuotaUnitType] = useState<Provider["quota"]["unitType"]>("percentage")
  const [quotaRemainingExpr, setQuotaRemainingExpr] = useState("")
  const [quotaUnitPath, setQuotaUnitPath] = useState("")
  const [quotaResetAtPath, setQuotaResetAtPath] = useState("")
  const [quotaLowThresholdPercent, setQuotaLowThresholdPercent] = useState("10")
  const [quotaTestLoading, setQuotaTestLoading] = useState(false)
  const [quotaTestResult, setQuotaTestResult] = useState<RuleQuotaTestResult | null>(null)
  const [quotaTestFingerprint, setQuotaTestFingerprint] = useState<string | null>(null)
  const [costEnabled, setCostEnabled] = useState(false)
  const [inputPricePerM, setInputPricePerM] = useState("")
  const [outputPricePerM, setOutputPricePerM] = useState("")
  const [cacheInputPricePerM, setCacheInputPricePerM] = useState("")
  const [cacheOutputPricePerM, setCacheOutputPricePerM] = useState("")
  const [costCurrency, setCostCurrency] = useState("USD")
  const [costTemplate, setCostTemplate] = useState<BillingTemplateAttribution | null>(null)

  const [loading, setLoading] = useState(mode === "edit")
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

  const isGlobalMode = !groupId
  const group = groupId ? config?.groups.find(g => g.id === groupId) : null
  const provider = isGlobalMode
    ? ((config?.providers ?? []).find(item => item.id === providerId) ?? null)
    : (group?.providers.find(item => item.id === providerId) ?? null)
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
  const billingTemplateResults = searchBillingTemplates(billingTemplateSearch)
  const selectedBillingTemplate =
    billingTemplateResults.find(
      template =>
        template.vendorId === selectedBillingVendorId && template.modelId === selectedBillingModelId
    ) ?? null
  const currentCostDraft = {
    enabled: costEnabled,
    inputPricePerM: parseCostInputValue(inputPricePerM),
    outputPricePerM: parseCostInputValue(outputPricePerM),
    cacheInputPricePerM: parseCostInputValue(cacheInputPricePerM),
    cacheOutputPricePerM: parseCostInputValue(cacheOutputPricePerM),
    currency: costCurrency.trim() || "USD",
    template: costTemplate,
  }
  const appliedBillingTemplate =
    costTemplate && findBillingTemplate(costTemplate.vendorId, costTemplate.modelId)
      ? findBillingTemplate(costTemplate.vendorId, costTemplate.modelId) || null
      : null
  const billingTemplateMarkedModified =
    !!costTemplate &&
    (costTemplate.modifiedAfterApply ||
      (appliedBillingTemplate
        ? !doesCostMatchBillingTemplate(currentCostDraft, appliedBillingTemplate)
        : false))
  const billingTemplateSummaryText = costTemplate
    ? t(
        billingTemplateMarkedModified
          ? "ruleForm.billingTemplateSummaryModified"
          : "ruleForm.billingTemplateSummaryApplied",
        {
          vendor: costTemplate.vendorLabel,
          model: costTemplate.modelLabel,
        }
      )
    : t("ruleForm.billingTemplateSummaryNone")

  useEffect(() => {
    if (!config) return
    if (!isGlobalMode && !group) {
      showToast(t("toast.groupNotFound"), "error")
      navigate("/")
      return
    }

    if (!isEditMode) {
      setLoading(false)
      return
    }

    if (!provider) {
      setLoading(false)
      showToast(t("toast.ruleNotFound"), "error")
      navigate(isGlobalMode ? "/providers" : "/")
      return
    }

    setName(provider.name)
    setProtocol(provider.protocol)
    setToken(provider.token)
    setApiAddress(provider.apiAddress)
    setWebsite(provider.website || "")
    setDefaultModel(provider.defaultModel)
    setModelMappings(provider.modelMappings || {})

    const quota = provider.quota
    setQuotaEnabled(!!quota?.enabled)
    setQuotaProvider(quota?.provider || "custom")
    setQuotaEndpoint(quota?.endpoint || "")
    setQuotaMethod((quota?.method || "GET").toUpperCase())
    setQuotaUseRuleToken(quota?.useRuleToken ?? true)
    setQuotaCustomToken(quota?.customToken || "")
    setQuotaAuthHeader(quota?.authHeader || "Authorization")
    setQuotaAuthScheme(quota?.authScheme || "Bearer")
    setQuotaHeadersText(JSON.stringify(quota?.customHeaders ?? {}, null, 2))
    setQuotaUnitType(normalizeQuotaUnitType(quota?.unitType))
    setQuotaRemainingExpr(readMappingExpr(quota?.response?.remaining))
    setQuotaUnitPath(readMappingPath(quota?.response?.unit))
    setQuotaResetAtPath(readMappingPath(quota?.response?.resetAt))
    setQuotaLowThresholdPercent(String(quota?.lowThresholdPercent ?? 10))
    setCostEnabled(!!provider.cost?.enabled)
    setInputPricePerM(String(provider.cost?.inputPricePerM ?? ""))
    setOutputPricePerM(String(provider.cost?.outputPricePerM ?? ""))
    setCacheInputPricePerM(String(provider.cost?.cacheInputPricePerM ?? ""))
    setCacheOutputPricePerM(String(provider.cost?.cacheOutputPricePerM ?? ""))
    setCostCurrency(provider.cost?.currency || "USD")
    setCostTemplate(provider.cost?.template ?? null)

    setLoading(false)
  }, [config, group, isEditMode, isGlobalMode, navigate, provider, showToast, t])

  useEffect(() => {
    if (quotaEnabled) return
    setQuotaTestLoading(false)
    setQuotaTestResult(null)
    setQuotaTestFingerprint(null)
  }, [quotaEnabled])

  useEffect(() => {
    if (!isEditMode) {
      setLoading(false)
    }
  }, [isEditMode])

  useEffect(() => {
    if (costEnabled) return
    setShowBillingTemplateModal(false)
  }, [costEnabled])

  const resetImportFeedback = () => {
    setImportResult(null)
    setImportError(null)
  }

  const handleImportFormatChange = (value: ProviderImportInputFormat) => {
    setImportFormat(value)
    resetImportFeedback()
  }

  const handleImportTextChange = (value: string) => {
    setImportText(value)
    resetImportFeedback()
  }

  const handleImportClear = () => {
    setImportFormat("auto")
    setImportText("")
    resetImportFeedback()
  }

  const handleImportParse = () => {
    try {
      const result = parseProviderImport({
        format: importFormat,
        raw: importText,
      })
      setImportResult(result)
      setImportError(null)
    } catch (error) {
      setImportResult(null)
      if (error instanceof ProviderImportParseError) {
        setImportError(t(PROVIDER_IMPORT_ERROR_KEYS[error.code]))
        return
      }
      setImportError(t("ruleForm.importErrorUnrecognizedFormat"))
    }
  }

  const handleImportApply = () => {
    if (!importResult) return

    const nextFields = applyProviderImportDraft(
      {
        name,
        protocol,
        token,
        apiAddress,
        website,
        defaultModel,
      },
      importResult.draft
    )

    setName(nextFields.name)
    setProtocol(nextFields.protocol)
    setToken(nextFields.token)
    setApiAddress(nextFields.apiAddress)
    setWebsite(nextFields.website)
    setDefaultModel(nextFields.defaultModel)
    setErrors(prev => ({
      ...prev,
      name: undefined,
      token: undefined,
      apiAddress: undefined,
      defaultModel: undefined,
    }))
    setShowImportModal(false)
  }

  const markCostTemplateModified = () => {
    setCostTemplate((currentTemplate: BillingTemplateAttribution | null) => {
      if (!currentTemplate || currentTemplate.modifiedAfterApply) {
        return currentTemplate
      }
      return {
        ...currentTemplate,
        modifiedAfterApply: true,
      }
    })
  }

  const handleCostCurrencyChange = (value: string) => {
    setCostCurrency(value)
    markCostTemplateModified()
  }

  const handleCostFieldChange =
    (setter: (value: string) => void) => (event: React.ChangeEvent<HTMLInputElement>) => {
      setter(normalizeNumericInput(event.target.value))
      markCostTemplateModified()
    }

  const handleBillingTemplateApply = () => {
    if (!selectedBillingTemplate || !canApplyBillingTemplate(selectedBillingTemplate)) {
      return
    }

    const nextCost = applyBillingTemplateToCost(
      currentCostDraft,
      selectedBillingTemplate,
      new Date().toISOString()
    )

    setCostCurrency(nextCost.currency)
    setInputPricePerM(String(nextCost.inputPricePerM))
    setOutputPricePerM(String(nextCost.outputPricePerM))
    setCacheInputPricePerM(String(nextCost.cacheInputPricePerM))
    setCacheOutputPricePerM(String(nextCost.cacheOutputPricePerM))
    setCostTemplate(nextCost.template ?? null)
    setShowBillingTemplateModal(false)
  }

  const handleClearBillingTemplateAttribution = () => {
    setCostTemplate(null)
  }

  const focusField = (id: string) => {
    const input = document.getElementById(id) as HTMLInputElement | HTMLTextAreaElement | null
    input?.focus()
  }

  const normalizeProviderName = (value: string) => value.trim().toLowerCase()

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
    } else if (!isEditMode) {
      const nameKey = normalizeProviderName(name)
      const providerNameExists = (config?.providers ?? []).some(
        provider => normalizeProviderName(provider.name) === nameKey
      )
      if (providerNameExists) {
        nextErrors.name = t("validation.alreadyExists", { field: t("servicePage.ruleName") })
      }
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
    const quotaTestGroupId = groupId || config?.groups[0]?.id
    if (!quotaTestGroupId) {
      setQuotaTestResult({
        ok: false,
        message: t("toast.groupNotFound"),
      })
      return
    }

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
      const result = await testRuleQuotaDraft({
        groupId: quotaTestGroupId,
        name: name.trim() || "Draft Provider",
        token,
        apiAddress,
        defaultModel,
        quotaConfig,
      })
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
    if (!config) return
    if (isEditMode && !providerId) return
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

    const providerDraft: Provider = {
      id: isEditMode ? providerId || createStableId() : createStableId(),
      name: name.trim(),
      protocol,
      token,
      apiAddress,
      website: website.trim(),
      defaultModel: defaultModel.trim(),
      modelMappings: Object.fromEntries(
        Object.entries(modelMappings)
          .map(([key, value]) => [key.trim(), value.trim()])
          .filter(([key, value]) => key && value)
      ),
      quota: quotaConfig,
      cost: {
        enabled: costEnabled,
        inputPricePerM: parseCostInputValue(inputPricePerM),
        outputPricePerM: parseCostInputValue(outputPricePerM),
        cacheInputPricePerM: parseCostInputValue(cacheInputPricePerM),
        cacheOutputPricePerM: parseCostInputValue(cacheOutputPricePerM),
        currency: costCurrency.trim() || "USD",
        template: costTemplate,
      },
    }

    const currentProviders = config.providers ?? []
    const nextProviders = [...currentProviders]
    if (isEditMode) {
      const targetId = providerId || providerDraft.id
      const index = nextProviders.findIndex(provider => provider.id === targetId)
      if (index >= 0) {
        nextProviders[index] = providerDraft
      } else {
        nextProviders.push(providerDraft)
      }
    } else {
      nextProviders.push(providerDraft)
    }

    const nextGroups = config.groups.map(currentGroup => {
      if (currentGroup.id !== groupId) return currentGroup

      if (isEditMode) {
        return currentGroup
      }

      const currentProviderIds =
        currentGroup.providerIds ?? currentGroup.providers.map(rule => rule.id)
      const providerIds = currentProviderIds.includes(providerDraft.id)
        ? currentProviderIds
        : [...currentProviderIds, providerDraft.id]
      return {
        ...currentGroup,
        providerIds,
        activeProviderId: currentGroup.activeProviderId ?? providerDraft.id,
      }
    })

    const newConfig: ProxyConfig = {
      ...config,
      providers: nextProviders,
      groups: nextGroups,
    }

    try {
      await saveConfig(newConfig)
      showToast(t(isEditMode ? "toast.ruleUpdated" : "toast.ruleCreated"), "success")
      navigate(isGlobalMode ? "/providers" : "/")
    } catch (error) {
      showToast(t("errors.saveFailed", { message: String(error) }), "error")
    }
  }

  const handleCancel = () => {
    navigate(isGlobalMode ? "/providers" : "/")
  }

  const isValid =
    name.trim() &&
    token.trim() &&
    apiAddress.trim() &&
    defaultModel.trim() &&
    (!quotaEnabled || (quotaEndpoint.trim() && quotaRemainingExpr.trim()))
  const breadcrumbLabel = isEditMode && provider ? provider.name : t("ruleCreatePage.newRule")
  const backLabel = isGlobalMode ? t("header.providers") : t("header.backToService")

  if ((!isGlobalMode && !group) || (isEditMode && !provider)) {
    return null
  }

  if (loading) {
    return (
      <div className={styles.loading}>
        <p>{t("app.statusLoading")}</p>
      </div>
    )
  }

  return (
    <div className={styles.rulePage}>
      <div className="app-sub-header">
        <div className="app-sub-header-top">
          <button
            type="button"
            onClick={() => navigate(isGlobalMode ? "/providers" : "/")}
            className="app-sub-header-back"
          >
            <ArrowLeft size={16} strokeWidth={2} />
            <span>{backLabel}</span>
          </button>
        </div>
        <div className="app-sub-header-main">
          <h1 className="app-sub-header-title">
            {t(isEditMode ? "ruleEditPage.title" : "ruleCreatePage.title")}
          </h1>
          <nav className="app-breadcrumb" aria-label={t("header.backToService")}>
            <button
              type="button"
              onClick={() => navigate(isGlobalMode ? "/providers" : "/")}
              className="app-breadcrumb-button"
            >
              {isGlobalMode ? t("header.providers") : t("servicePage.groupPath")}
            </button>
            {!isGlobalMode && group ? (
              <>
                <span className="app-breadcrumb-separator">/</span>
                <span className="app-breadcrumb-item">{group.name}</span>
              </>
            ) : null}
            <span className="app-breadcrumb-separator">/</span>
            <span className="app-breadcrumb-item">{breadcrumbLabel}</span>
          </nav>
        </div>
      </div>

      <div className={styles.formContainer}>
        <div className={styles.ruleGrid}>
          <form onSubmit={handleSubmit} className={styles.ruleForm}>
            {!isEditMode ? (
              <section className={styles.importEntry}>
                <div className={styles.importEntryContent}>
                  <h2 className={styles.sectionTitle}>{t("ruleForm.importEntryTitle")}</h2>
                  <p className={styles.fieldHint}>{t("ruleForm.importHint")}</p>
                </div>
                <div className={styles.importEntryActions}>
                  <Button type="button" variant="default" onClick={() => setShowImportModal(true)}>
                    {t("ruleForm.importOpen")}
                  </Button>
                </div>
              </section>
            ) : null}

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
                <label htmlFor="edit-rule-protocol-anthropic">
                  {t("ruleForm.requestProtocol")}
                </label>
                <div className={styles.directionOptions}>
                  <button
                    id="edit-rule-protocol-anthropic"
                    type="button"
                    className={`${styles.directionOption} ${protocol === "anthropic" ? styles.active : ""}`}
                    onClick={() => setProtocol("anthropic")}
                    data-tooltip={anthropicProtocolHelp}
                    aria-label={`${t("ruleProtocol.anthropic")} - ${anthropicProtocolHelp}`}
                  >
                    {t("ruleProtocol.anthropic")}
                  </button>
                  <button
                    type="button"
                    className={`${styles.directionOption} ${protocol === "openai_completion" ? styles.active : ""}`}
                    onClick={() => setProtocol("openai_completion")}
                    data-tooltip={openaiChatProtocolHelp}
                    aria-label={`${t("ruleProtocol.openai_completion")} - ${openaiChatProtocolHelp}`}
                  >
                    {t("ruleProtocol.openai_completion")}
                  </button>
                  <button
                    type="button"
                    className={`${styles.directionOption} ${protocol === "openai" ? styles.active : ""}`}
                    onClick={() => setProtocol("openai")}
                    data-tooltip={openaiResponsesProtocolHelp}
                    aria-label={`${t("ruleProtocol.openai")} - ${openaiResponsesProtocolHelp}`}
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

              <div className={styles.formGroup}>
                <label htmlFor="website">{t("ruleForm.officialWebsite")}</label>
                <Input
                  id="website"
                  value={website}
                  onChange={e => setWebsite(e.target.value)}
                  placeholder={t("ruleForm.officialWebsitePlaceholder")}
                  className={styles.input}
                  hint={t("ruleForm.officialWebsiteHint")}
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
                          setQuotaUnitType(e.target.value as Provider["quota"]["unitType"])
                        }
                      >
                        <option value="percentage">{t("ruleForm.quotaUnitTypePercentage")}</option>
                        <option value="amount">{t("ruleForm.quotaUnitTypeAmount")}</option>
                        <option value="tokens">{t("ruleForm.quotaUnitTypeTokens")}</option>
                      </select>
                    </div>
                    <div className={styles.formGroup}>
                      <label htmlFor="quota-unit-path">{t("ruleForm.quotaUnitPath")}</label>
                      <Input
                        id="quota-unit-path"
                        value={quotaUnitPath}
                        onChange={e => setQuotaUnitPath(e.target.value)}
                        placeholder="$.data.currency"
                      />
                    </div>
                  </div>

                  <div className={styles.dualColumnRow}>
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

            <section className={styles.formSection}>
              <h2 className={styles.sectionTitle}>{t("ruleForm.sectionCost")}</h2>
              <div className={styles.switchRow}>
                <div>
                  <label htmlFor="cost-enabled">{t("ruleForm.costEnabled")}</label>
                  <p className={styles.fieldHint}>{t("ruleForm.costEnabledHint")}</p>
                </div>
                <Switch id="cost-enabled" checked={costEnabled} onChange={setCostEnabled} />
              </div>
              {costEnabled && (
                <>
                  <div className={styles.billingTemplateRow}>
                    <div className={styles.billingTemplateSummary}>
                      <strong>{billingTemplateSummaryText}</strong>
                      {costTemplate?.sourceUrl ? (
                        <a
                          className={styles.billingTemplateSource}
                          href={costTemplate.sourceUrl}
                          target="_blank"
                          rel="noreferrer"
                        >
                          {t("ruleForm.billingTemplateSource")}
                        </a>
                      ) : null}
                    </div>
                    <div className={styles.billingTemplateActions}>
                      {costTemplate ? (
                        <Button
                          type="button"
                          variant="ghost"
                          size="small"
                          onClick={handleClearBillingTemplateAttribution}
                        >
                          {t("ruleForm.billingTemplateClear")}
                        </Button>
                      ) : null}
                      <Button
                        type="button"
                        variant="default"
                        size="small"
                        onClick={() => setShowBillingTemplateModal(true)}
                      >
                        {t("ruleForm.billingTemplateOpen")}
                      </Button>
                    </div>
                  </div>
                  <div className={styles.formGroup}>
                    <label htmlFor="cost-currency">{t("ruleForm.costCurrency")}</label>
                    <select
                      id="cost-currency"
                      className={styles.nativeSelect}
                      value={costCurrency}
                      onChange={e => handleCostCurrencyChange(e.target.value)}
                    >
                      {!COST_CURRENCY_OPTIONS.includes(
                        costCurrency as (typeof COST_CURRENCY_OPTIONS)[number]
                      ) && <option value={costCurrency}>{costCurrency}</option>}
                      {COST_CURRENCY_OPTIONS.map(currency => (
                        <option key={currency} value={currency}>
                          {currency}
                        </option>
                      ))}
                    </select>
                  </div>
                  <div className={styles.dualColumnRow}>
                    <div className={styles.formGroup}>
                      <label htmlFor="cost-input">{t("ruleForm.costInputPerM")}</label>
                      <Input
                        id="cost-input"
                        type="number"
                        inputMode="decimal"
                        min="0"
                        step="0.0001"
                        value={inputPricePerM}
                        onChange={handleCostFieldChange(setInputPricePerM)}
                      />
                    </div>
                    <div className={styles.formGroup}>
                      <label htmlFor="cost-output">{t("ruleForm.costOutputPerM")}</label>
                      <Input
                        id="cost-output"
                        type="number"
                        inputMode="decimal"
                        min="0"
                        step="0.0001"
                        value={outputPricePerM}
                        onChange={handleCostFieldChange(setOutputPricePerM)}
                      />
                    </div>
                  </div>
                  <div className={styles.dualColumnRow}>
                    <div className={styles.formGroup}>
                      <label htmlFor="cost-cache-input">{t("ruleForm.costCacheInputPerM")}</label>
                      <Input
                        id="cost-cache-input"
                        type="number"
                        inputMode="decimal"
                        min="0"
                        step="0.0001"
                        value={cacheInputPricePerM}
                        onChange={handleCostFieldChange(setCacheInputPricePerM)}
                      />
                    </div>
                    <div className={styles.formGroup}>
                      <label htmlFor="cost-cache-output">{t("ruleForm.costCacheOutputPerM")}</label>
                      <Input
                        id="cost-cache-output"
                        type="number"
                        inputMode="decimal"
                        min="0"
                        step="0.0001"
                        value={cacheOutputPricePerM}
                        onChange={handleCostFieldChange(setCacheOutputPricePerM)}
                      />
                    </div>
                  </div>
                </>
              )}
            </section>

            <div className={styles.formActions}>
              <Button variant="default" onClick={handleCancel} className={styles.button}>
                {t("common.cancel")}
              </Button>
              <Button type="submit" variant="primary" disabled={!isValid} className={styles.button}>
                {t(isEditMode ? "ruleEditPage.saveChanges" : "ruleCreatePage.createRule")}
              </Button>
            </div>
          </form>
        </div>
      </div>

      {!isEditMode ? (
        <Modal
          open={showImportModal}
          onClose={() => setShowImportModal(false)}
          title={t("ruleForm.importTitle")}
          size="large"
        >
          <ProviderImportCard
            showHeader={false}
            format={importFormat}
            rawValue={importText}
            parseError={importError}
            parseResult={importResult}
            onFormatChange={handleImportFormatChange}
            onRawChange={handleImportTextChange}
            onParse={handleImportParse}
            onApply={handleImportApply}
            onClear={handleImportClear}
          />
        </Modal>
      ) : null}

      <Modal
        open={showBillingTemplateModal}
        onClose={() => setShowBillingTemplateModal(false)}
        title={t("ruleForm.billingTemplateModalTitle")}
        size="large"
      >
        <div className={styles.billingTemplateModal}>
          <div className={styles.formGroup}>
            <label htmlFor="billing-template-search">
              {t("ruleForm.billingTemplateSearchLabel")}
            </label>
            <textarea
              id="billing-template-search"
              className={styles.importTextarea}
              value={billingTemplateSearch}
              onChange={e => setBillingTemplateSearch(e.target.value)}
              placeholder={t("ruleForm.billingTemplateSearchPlaceholder")}
            />
          </div>

          <div className={styles.billingTemplateLayout}>
            <div className={styles.billingTemplateList}>
              {billingTemplateResults.map(template => {
                const isActive =
                  template.vendorId === selectedBillingVendorId &&
                  template.modelId === selectedBillingModelId

                return (
                  <div
                    key={`${template.vendorId}:${template.modelId}`}
                    className={`${styles.billingTemplateItem} ${isActive ? styles.billingTemplateItemActive : ""}`}
                  >
                    <button
                      type="button"
                      onClick={() => {
                        setSelectedBillingVendorId(template.vendorId)
                        setSelectedBillingModelId(template.modelId)
                      }}
                    >
                      {template.modelLabel}
                    </button>
                    <span>{template.vendorLabel}</span>
                  </div>
                )
              })}
            </div>

            <div className={styles.billingTemplateDetail}>
              {selectedBillingTemplate ? (
                <>
                  <div>
                    <h3>{selectedBillingTemplate.modelLabel}</h3>
                    <p>
                      {t("ruleForm.billingTemplateVendorLabel")}:{" "}
                      {selectedBillingTemplate.vendorLabel}
                    </p>
                  </div>

                  <div className={styles.billingTemplateBadgeRow}>
                    <span>
                      {selectedBillingTemplate.availability === "ready"
                        ? t("ruleForm.billingTemplateAvailabilityReady")
                        : t("ruleForm.billingTemplateAvailabilityUnpriced")}
                    </span>
                    <span>
                      {selectedBillingTemplate.completeness === "full"
                        ? t("ruleForm.billingTemplateCompletenessFull")
                        : t("ruleForm.billingTemplateCompletenessPartial")}
                    </span>
                  </div>

                  <div className={styles.importPreviewGrid}>
                    <div className={styles.importPreviewItem}>
                      <span className={styles.importPreviewLabel}>
                        {t("ruleForm.costInputPerM")}
                      </span>
                      <strong className={styles.importPreviewValue}>
                        {formatBillingTemplatePrice(
                          selectedBillingTemplate.inputPricePerM,
                          selectedBillingTemplate.currency
                        )}
                      </strong>
                    </div>
                    <div className={styles.importPreviewItem}>
                      <span className={styles.importPreviewLabel}>
                        {t("ruleForm.costOutputPerM")}
                      </span>
                      <strong className={styles.importPreviewValue}>
                        {formatBillingTemplatePrice(
                          selectedBillingTemplate.outputPricePerM,
                          selectedBillingTemplate.currency
                        )}
                      </strong>
                    </div>
                    <div className={styles.importPreviewItem}>
                      <span className={styles.importPreviewLabel}>
                        {t("ruleForm.costCacheInputPerM")}
                      </span>
                      <strong className={styles.importPreviewValue}>
                        {formatBillingTemplatePrice(
                          selectedBillingTemplate.cacheInputPricePerM,
                          selectedBillingTemplate.currency
                        )}
                      </strong>
                    </div>
                    <div className={styles.importPreviewItem}>
                      <span className={styles.importPreviewLabel}>
                        {t("ruleForm.costCacheOutputPerM")}
                      </span>
                      <strong className={styles.importPreviewValue}>
                        {formatBillingTemplatePrice(
                          selectedBillingTemplate.cacheOutputPricePerM,
                          selectedBillingTemplate.currency
                        )}
                      </strong>
                    </div>
                  </div>

                  <p>
                    {t("ruleForm.billingTemplateVerifiedAt")}: {selectedBillingTemplate.verifiedAt}
                  </p>
                  <a
                    className={styles.billingTemplateSource}
                    href={selectedBillingTemplate.sourceUrl}
                    target="_blank"
                    rel="noreferrer"
                  >
                    {t("ruleForm.billingTemplateSource")}
                  </a>
                  <p>
                    {selectedBillingTemplate.availability === "unpriced"
                      ? t("ruleForm.billingTemplateUnavailableHint")
                      : selectedBillingTemplate.completeness === "partial"
                        ? t("ruleForm.billingTemplatePartialHint")
                        : selectedBillingTemplate.sourceNote}
                  </p>
                  <div className={styles.billingTemplateActions}>
                    <Button
                      type="button"
                      variant="primary"
                      size="small"
                      disabled={!canApplyBillingTemplate(selectedBillingTemplate)}
                      onClick={handleBillingTemplateApply}
                    >
                      {t("ruleForm.billingTemplateApply")}
                    </Button>
                  </div>
                </>
              ) : (
                <p>{t("ruleForm.billingTemplateSearchPlaceholder")}</p>
              )}
            </div>
          </div>
        </div>
      </Modal>
    </div>
  )
}

export default RuleFormPage
