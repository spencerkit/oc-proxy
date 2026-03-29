import type { BillingTemplateAttribution, RuleCostConfig } from "@/types/proxy"

export type BillingTemplateCompleteness = "full" | "partial"
export type BillingTemplateAvailability = "ready" | "unpriced"

export interface BillingTemplate {
  readonly vendorId: string
  readonly vendorLabel: string
  readonly modelId: string
  readonly modelLabel: string
  readonly searchAliases: readonly string[]
  readonly currency: string
  readonly inputPricePerM?: number
  readonly outputPricePerM?: number
  readonly cacheInputPricePerM?: number
  readonly cacheOutputPricePerM?: number
  readonly completeness: BillingTemplateCompleteness
  readonly availability: BillingTemplateAvailability
  readonly sourceUrl: string
  readonly sourceNote: string
  readonly verifiedAt: string
}

const VERIFIED_AT = "2026-03-29"

function freezeBillingTemplate(template: BillingTemplate): BillingTemplate {
  return Object.freeze({
    ...template,
    searchAliases: Object.freeze([...template.searchAliases]),
  })
}

const BILLING_TEMPLATE_CATALOG: BillingTemplate[] = [
  {
    vendorId: "openai",
    vendorLabel: "OpenAI",
    modelId: "gpt-5.4",
    modelLabel: "GPT-5.4",
    searchAliases: ["gpt54", "gpt-5.4", "gpt 5.4"],
    currency: "USD",
    inputPricePerM: 2.5,
    outputPricePerM: 15,
    cacheInputPricePerM: 0.25,
    cacheOutputPricePerM: 0,
    completeness: "partial",
    availability: "ready",
    sourceUrl: "https://developers.openai.com/api/docs/models/gpt-5.4",
    sourceNote:
      "Official model page pricing per 1M text tokens; unspecified billing dimensions default to 0.",
    verifiedAt: VERIFIED_AT,
  },
  {
    vendorId: "openai",
    vendorLabel: "OpenAI",
    modelId: "gpt-5-mini",
    modelLabel: "GPT-5 mini",
    searchAliases: ["gpt5mini", "gpt-5 mini", "gpt 5 mini"],
    currency: "USD",
    inputPricePerM: 0.25,
    outputPricePerM: 2,
    cacheInputPricePerM: 0.025,
    cacheOutputPricePerM: 0,
    completeness: "partial",
    availability: "ready",
    sourceUrl: "https://developers.openai.com/api/docs/models/gpt-5-mini",
    sourceNote:
      "Official model page pricing per 1M text tokens; unspecified billing dimensions default to 0.",
    verifiedAt: VERIFIED_AT,
  },
  {
    vendorId: "openai",
    vendorLabel: "OpenAI",
    modelId: "gpt-4o",
    modelLabel: "GPT-4o",
    searchAliases: ["gpt4o", "gpt-4o", "gpt 4o"],
    currency: "USD",
    inputPricePerM: 2.5,
    outputPricePerM: 10,
    cacheInputPricePerM: 1.25,
    cacheOutputPricePerM: 0,
    completeness: "partial",
    availability: "ready",
    sourceUrl: "https://developers.openai.com/api/docs/models/gpt-4o",
    sourceNote:
      "Official model page pricing per 1M text tokens; unspecified billing dimensions default to 0.",
    verifiedAt: VERIFIED_AT,
  },
  {
    vendorId: "openai",
    vendorLabel: "OpenAI",
    modelId: "gpt-4o-mini",
    modelLabel: "GPT-4o mini",
    searchAliases: ["gpt4omini", "gpt-4o-mini", "gpt 4o mini"],
    currency: "USD",
    inputPricePerM: 0.15,
    outputPricePerM: 0.6,
    cacheInputPricePerM: 0.075,
    cacheOutputPricePerM: 0,
    completeness: "partial",
    availability: "ready",
    sourceUrl: "https://developers.openai.com/api/docs/models/gpt-4o-mini",
    sourceNote:
      "Official model page pricing per 1M text tokens; unspecified billing dimensions default to 0.",
    verifiedAt: VERIFIED_AT,
  },
  {
    vendorId: "anthropic",
    vendorLabel: "Anthropic",
    modelId: "claude-sonnet-4-6",
    modelLabel: "Claude Sonnet 4.6",
    searchAliases: ["sonnet46", "claude sonnet 4.6", "claude-sonnet-4.6"],
    currency: "USD",
    inputPricePerM: 3,
    outputPricePerM: 15,
    cacheInputPricePerM: 0.3,
    cacheOutputPricePerM: 3.75,
    completeness: "full",
    availability: "ready",
    sourceUrl: "https://platform.claude.com/docs/zh-CN/about-claude/pricing",
    sourceNote: "Uses base input, output, cache hits, and 5m cache write pricing.",
    verifiedAt: VERIFIED_AT,
  },
  {
    vendorId: "anthropic",
    vendorLabel: "Anthropic",
    modelId: "claude-sonnet-4-5",
    modelLabel: "Claude Sonnet 4.5",
    searchAliases: ["sonnet45", "claude sonnet 4.5", "claude-sonnet-4.5"],
    currency: "USD",
    inputPricePerM: 3,
    outputPricePerM: 15,
    cacheInputPricePerM: 0.3,
    cacheOutputPricePerM: 3.75,
    completeness: "full",
    availability: "ready",
    sourceUrl: "https://platform.claude.com/docs/zh-CN/about-claude/pricing",
    sourceNote: "Uses base input, output, cache hits, and 5m cache write pricing.",
    verifiedAt: VERIFIED_AT,
  },
  {
    vendorId: "anthropic",
    vendorLabel: "Anthropic",
    modelId: "claude-opus-4-1",
    modelLabel: "Claude Opus 4.1",
    searchAliases: ["opus41", "claude opus 4.1", "claude-opus-4.1"],
    currency: "USD",
    inputPricePerM: 15,
    outputPricePerM: 75,
    cacheInputPricePerM: 1.5,
    cacheOutputPricePerM: 18.75,
    completeness: "full",
    availability: "ready",
    sourceUrl: "https://platform.claude.com/docs/zh-CN/about-claude/pricing",
    sourceNote: "Uses base input, output, cache hits, and 5m cache write pricing.",
    verifiedAt: VERIFIED_AT,
  },
  {
    vendorId: "anthropic",
    vendorLabel: "Anthropic",
    modelId: "claude-haiku-4-5",
    modelLabel: "Claude Haiku 4.5",
    searchAliases: ["haiku45", "claude haiku 4.5", "claude-haiku-4.5"],
    currency: "USD",
    inputPricePerM: 1,
    outputPricePerM: 5,
    cacheInputPricePerM: 0.1,
    cacheOutputPricePerM: 1.25,
    completeness: "full",
    availability: "ready",
    sourceUrl: "https://platform.claude.com/docs/zh-CN/about-claude/pricing",
    sourceNote: "Uses base input, output, cache hits, and 5m cache write pricing.",
    verifiedAt: VERIFIED_AT,
  },
  {
    vendorId: "minimax",
    vendorLabel: "MiniMax",
    modelId: "minimax-m2.5",
    modelLabel: "MiniMax-M2.5",
    searchAliases: ["m2.5", "minimax m2.5", "m25"],
    currency: "USD",
    inputPricePerM: 0.3,
    outputPricePerM: 1.2,
    cacheInputPricePerM: 0.03,
    cacheOutputPricePerM: 0.375,
    completeness: "full",
    availability: "ready",
    sourceUrl: "https://platform.minimax.io/docs/guides/pricing-paygo",
    sourceNote:
      "Uses pay-as-you-go input, output, prompt caching read, and prompt caching write pricing.",
    verifiedAt: VERIFIED_AT,
  },
  {
    vendorId: "minimax",
    vendorLabel: "MiniMax",
    modelId: "minimax-m2.5-highspeed",
    modelLabel: "MiniMax-M2.5-highspeed",
    searchAliases: ["m2.5hs", "m2.5 highspeed", "minimax m2.5 highspeed"],
    currency: "USD",
    inputPricePerM: 0.6,
    outputPricePerM: 2.4,
    cacheInputPricePerM: 0.03,
    cacheOutputPricePerM: 0.375,
    completeness: "full",
    availability: "ready",
    sourceUrl: "https://platform.minimax.io/docs/guides/pricing-paygo",
    sourceNote:
      "Uses pay-as-you-go input, output, prompt caching read, and prompt caching write pricing.",
    verifiedAt: VERIFIED_AT,
  },
  {
    vendorId: "minimax",
    vendorLabel: "MiniMax",
    modelId: "minimax-m2.1",
    modelLabel: "MiniMax-M2.1",
    searchAliases: ["m2.1", "minimax m2.1", "m21"],
    currency: "USD",
    inputPricePerM: 0.3,
    outputPricePerM: 1.2,
    cacheInputPricePerM: 0.03,
    cacheOutputPricePerM: 0.375,
    completeness: "full",
    availability: "ready",
    sourceUrl: "https://platform.minimax.io/docs/guides/pricing-paygo",
    sourceNote:
      "Uses pay-as-you-go input, output, prompt caching read, and prompt caching write pricing.",
    verifiedAt: VERIFIED_AT,
  },
  {
    vendorId: "minimax",
    vendorLabel: "MiniMax",
    modelId: "minimax-m2.1-highspeed",
    modelLabel: "MiniMax-M2.1-highspeed",
    searchAliases: ["m2.1hs", "m2.1 highspeed", "minimax m2.1 highspeed"],
    currency: "USD",
    inputPricePerM: 0.6,
    outputPricePerM: 2.4,
    cacheInputPricePerM: 0.03,
    cacheOutputPricePerM: 0.375,
    completeness: "full",
    availability: "ready",
    sourceUrl: "https://platform.minimax.io/docs/guides/pricing-paygo",
    sourceNote:
      "Uses pay-as-you-go input, output, prompt caching read, and prompt caching write pricing.",
    verifiedAt: VERIFIED_AT,
  },
  {
    vendorId: "minimax",
    vendorLabel: "MiniMax",
    modelId: "minimax-m2.7",
    modelLabel: "MiniMax-M2.7",
    searchAliases: ["m2.7", "minimax m2.7", "m27"],
    currency: "USD",
    inputPricePerM: 0,
    outputPricePerM: 0,
    cacheInputPricePerM: 0,
    cacheOutputPricePerM: 0,
    completeness: "partial",
    availability: "ready",
    sourceUrl: "https://platform.minimax.io/docs/guides/pricing-token-plan",
    sourceNote:
      "Official model exists, but the reviewed page does not expose pay-as-you-go per-1M pricing; default unspecified pricing to 0.",
    verifiedAt: VERIFIED_AT,
  },
  {
    vendorId: "zhipu",
    vendorLabel: "智谱",
    modelId: "glm-4.5",
    modelLabel: "GLM-4.5",
    searchAliases: ["glm45", "glm-4.5", "glm 4.5"],
    currency: "CNY",
    inputPricePerM: 0.8,
    outputPricePerM: 2,
    cacheInputPricePerM: 0,
    cacheOutputPricePerM: 0,
    completeness: "partial",
    availability: "ready",
    sourceUrl: "https://docs.bigmodel.cn/cn/guide/models/text/glm-4.5",
    sourceNote:
      "Official page states input 0.8 CNY / 1M and output 2 CNY / 1M; unspecified billing dimensions default to 0.",
    verifiedAt: VERIFIED_AT,
  },
  {
    vendorId: "zhipu",
    vendorLabel: "智谱",
    modelId: "glm-4-plus",
    modelLabel: "GLM-4-Plus",
    searchAliases: ["glm4plus", "glm-4-plus", "glm 4 plus"],
    currency: "CNY",
    inputPricePerM: 5,
    outputPricePerM: 5,
    cacheInputPricePerM: 0,
    cacheOutputPricePerM: 0,
    completeness: "partial",
    availability: "ready",
    sourceUrl: "https://docs.bigmodel.cn/cn/guide/models/text/glm-4",
    sourceNote:
      "Official page lists a unified 5 CNY / 1M token price; apply it symmetrically to input/output and default unspecified billing dimensions to 0.",
    verifiedAt: VERIFIED_AT,
  },
  {
    vendorId: "zhipu",
    vendorLabel: "智谱",
    modelId: "glm-4-long",
    modelLabel: "GLM-4-Long",
    searchAliases: ["glm4long", "glm-4-long", "glm 4 long"],
    currency: "CNY",
    inputPricePerM: 1,
    outputPricePerM: 1,
    cacheInputPricePerM: 0,
    cacheOutputPricePerM: 0,
    completeness: "partial",
    availability: "ready",
    sourceUrl: "https://docs.bigmodel.cn/cn/guide/models/text/glm-4-long",
    sourceNote:
      "Official page lists a unified 1 CNY / 1M token price; apply it symmetrically to input/output and default unspecified billing dimensions to 0.",
    verifiedAt: VERIFIED_AT,
  },
  {
    vendorId: "zhipu",
    vendorLabel: "智谱",
    modelId: "glm-z1-air",
    modelLabel: "GLM-Z1-Air",
    searchAliases: ["glmz1air", "glm-z1-air", "glm z1 air"],
    currency: "CNY",
    inputPricePerM: 0.5,
    outputPricePerM: 0.5,
    cacheInputPricePerM: 0,
    cacheOutputPricePerM: 0,
    completeness: "partial",
    availability: "ready",
    sourceUrl: "https://docs.bigmodel.cn/cn/guide/models/text/glm-z1",
    sourceNote:
      "Official page lists a unified 0.5 CNY / 1M token price; apply it symmetrically to input/output and default unspecified billing dimensions to 0.",
    verifiedAt: VERIFIED_AT,
  },
  {
    vendorId: "zhipu",
    vendorLabel: "智谱",
    modelId: "glm-5",
    modelLabel: "GLM-5",
    searchAliases: ["glm5", "glm-5", "glm 5"],
    currency: "CNY",
    inputPricePerM: 0,
    outputPricePerM: 0,
    cacheInputPricePerM: 0,
    cacheOutputPricePerM: 0,
    completeness: "partial",
    availability: "ready",
    sourceUrl: "https://docs.bigmodel.cn/cn/guide/start/model-overview",
    sourceNote:
      "Official model overview lists GLM-5, but the reviewed docs did not expose directly verifiable per-1M API pricing; default unspecified pricing to 0.",
    verifiedAt: VERIFIED_AT,
  },
  {
    vendorId: "zhipu",
    vendorLabel: "智谱",
    modelId: "glm-5.1",
    modelLabel: "GLM-5.1",
    searchAliases: ["glm51", "glm-5.1", "glm 5.1"],
    currency: "CNY",
    inputPricePerM: 0,
    outputPricePerM: 0,
    cacheInputPricePerM: 0,
    cacheOutputPricePerM: 0,
    completeness: "partial",
    availability: "ready",
    sourceUrl: "https://docs.bigmodel.cn/cn/guide/start/model-overview",
    sourceNote:
      "Reserved official-family placeholder; direct per-1M API pricing is not published on the reviewed page, so unspecified pricing defaults to 0.",
    verifiedAt: VERIFIED_AT,
  },
]

export const BILLING_TEMPLATES: readonly BillingTemplate[] = Object.freeze(
  BILLING_TEMPLATE_CATALOG.map(template => freezeBillingTemplate(template))
)

export function searchBillingTemplates(query: string): readonly BillingTemplate[] {
  const normalized = query.trim().toLowerCase()
  if (!normalized) return [...BILLING_TEMPLATES]

  return BILLING_TEMPLATES.filter(template => {
    const haystacks = [
      template.vendorLabel,
      template.vendorId,
      template.modelLabel,
      template.modelId,
      ...template.searchAliases,
    ].map(value => value.toLowerCase())

    return haystacks.some(value => value.includes(normalized))
  })
}

export function findBillingTemplate(
  vendorId: string,
  modelId: string
): BillingTemplate | undefined {
  return BILLING_TEMPLATES.find(
    template => template.vendorId === vendorId && template.modelId === modelId
  )
}

export function canApplyBillingTemplate(template: BillingTemplate): boolean {
  return (
    template.availability === "ready" &&
    [
      template.inputPricePerM,
      template.outputPricePerM,
      template.cacheInputPricePerM,
      template.cacheOutputPricePerM,
    ].some(value => value !== undefined)
  )
}

function buildTemplateAttribution(
  template: BillingTemplate,
  appliedAt: string
): BillingTemplateAttribution {
  return {
    vendorId: template.vendorId,
    vendorLabel: template.vendorLabel,
    modelId: template.modelId,
    modelLabel: template.modelLabel,
    sourceUrl: template.sourceUrl,
    verifiedAt: template.verifiedAt,
    appliedAt,
    modifiedAfterApply: false,
  }
}

export function applyBillingTemplateToCost(
  current: RuleCostConfig,
  template: BillingTemplate,
  appliedAt: string
): RuleCostConfig {
  if (!canApplyBillingTemplate(template)) {
    throw new Error(`Cannot apply billing template for ${template.vendorId}/${template.modelId}`)
  }

  const next: RuleCostConfig = {
    ...current,
    currency: template.currency,
    template: buildTemplateAttribution(template, appliedAt),
  }

  if (template.inputPricePerM !== undefined) next.inputPricePerM = template.inputPricePerM
  if (template.outputPricePerM !== undefined) next.outputPricePerM = template.outputPricePerM
  if (template.cacheInputPricePerM !== undefined)
    next.cacheInputPricePerM = template.cacheInputPricePerM
  if (template.cacheOutputPricePerM !== undefined) {
    next.cacheOutputPricePerM = template.cacheOutputPricePerM
  }

  return next
}

export function doesCostMatchBillingTemplate(
  cost: RuleCostConfig,
  template: BillingTemplate
): boolean {
  if (cost.currency !== template.currency) return false
  if (template.inputPricePerM !== undefined && cost.inputPricePerM !== template.inputPricePerM) {
    return false
  }
  if (template.outputPricePerM !== undefined && cost.outputPricePerM !== template.outputPricePerM) {
    return false
  }
  if (
    template.cacheInputPricePerM !== undefined &&
    cost.cacheInputPricePerM !== template.cacheInputPricePerM
  ) {
    return false
  }
  if (
    template.cacheOutputPricePerM !== undefined &&
    cost.cacheOutputPricePerM !== template.cacheOutputPricePerM
  ) {
    return false
  }

  return true
}
