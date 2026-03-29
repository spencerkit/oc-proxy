import assert from "node:assert/strict"
import { test } from "node:test"

import {
  applyBillingTemplateToCost,
  BILLING_TEMPLATES,
  canApplyBillingTemplate,
  doesCostMatchBillingTemplate,
  findBillingTemplate,
  searchBillingTemplates,
} from "../../src/renderer/utils/billingTemplates"

test("searchBillingTemplates matches vendor, model, and alias text", () => {
  assert.equal(
    searchBillingTemplates("gpt-4o").some(item => item.modelId === "gpt-4o"),
    true
  )
  assert.equal(
    searchBillingTemplates("claude sonnet").some(item => item.vendorId === "anthropic"),
    true
  )
  assert.equal(
    searchBillingTemplates("glm5").some(item => item.modelId === "glm-5"),
    true
  )
})

test("searchBillingTemplates returns detached arrays and exposes a readonly catalog shape", () => {
  const allTemplates = searchBillingTemplates("")
  const secondSearch = searchBillingTemplates("")

  assert.notEqual(allTemplates, BILLING_TEMPLATES)
  assert.notEqual(allTemplates, secondSearch)
  assert.deepEqual(allTemplates, BILLING_TEMPLATES)
  assert.deepEqual(secondSearch, BILLING_TEMPLATES)
})

test("applyBillingTemplateToCost fills missing official fields with zero for partial templates", () => {
  const template = findBillingTemplate("openai", "gpt-4o")
  assert.ok(template)

  const next = applyBillingTemplateToCost(
    {
      enabled: true,
      inputPricePerM: 9,
      outputPricePerM: 9,
      cacheInputPricePerM: 9,
      cacheOutputPricePerM: 7,
      currency: "USD",
    },
    template,
    "2026-03-29T00:00:00.000Z"
  )

  assert.equal(next.inputPricePerM, 2.5)
  assert.equal(next.outputPricePerM, 10)
  assert.equal(next.cacheInputPricePerM, 1.25)
  assert.equal(next.cacheOutputPricePerM, 0)
  assert.equal(next.template?.vendorId, "openai")
  assert.equal(next.template?.modifiedAfterApply, false)
})

test("applyBillingTemplateToCost overwrites all priced fields for full templates including Anthropic cache mapping", () => {
  const template = findBillingTemplate("anthropic", "claude-sonnet-4-5")
  assert.ok(template)

  const next = applyBillingTemplateToCost(
    {
      enabled: true,
      inputPricePerM: 999,
      outputPricePerM: 888,
      cacheInputPricePerM: 777,
      cacheOutputPricePerM: 666,
      currency: "CNY",
    },
    template,
    "2026-03-29T00:00:00.000Z"
  )

  assert.equal(next.currency, "USD")
  assert.equal(next.inputPricePerM, 3)
  assert.equal(next.outputPricePerM, 15)
  assert.equal(next.cacheInputPricePerM, 0.3)
  assert.equal(next.cacheOutputPricePerM, 3.75)
  assert.equal(next.template?.vendorId, "anthropic")
  assert.equal(next.template?.modelId, "claude-sonnet-4-5")
})

test("canApplyBillingTemplate returns true for official models that default missing pricing to zero", () => {
  const template = findBillingTemplate("zhipu", "glm-5")
  assert.ok(template)
  assert.equal(canApplyBillingTemplate(template), true)
})

test("applyBillingTemplateToCost applies zero-valued pricing for official placeholders without published pricing", () => {
  const template = findBillingTemplate("zhipu", "glm-5")
  assert.ok(template)

  const next = applyBillingTemplateToCost(
    {
      enabled: true,
      inputPricePerM: 9,
      outputPricePerM: 8,
      cacheInputPricePerM: 7,
      cacheOutputPricePerM: 6,
      currency: "USD",
    },
    template,
    "2026-03-29T00:00:00.000Z"
  )

  assert.equal(next.currency, "CNY")
  assert.equal(next.inputPricePerM, 0)
  assert.equal(next.outputPricePerM, 0)
  assert.equal(next.cacheInputPricePerM, 0)
  assert.equal(next.cacheOutputPricePerM, 0)
  assert.equal(next.template?.vendorId, "zhipu")
  assert.equal(next.template?.modelId, "glm-5")
})

test("doesCostMatchBillingTemplate detects modified pricing against the seeded template", () => {
  const template = findBillingTemplate("anthropic", "claude-sonnet-4-5")
  assert.ok(template)

  assert.equal(
    doesCostMatchBillingTemplate(
      {
        enabled: true,
        inputPricePerM: 3,
        outputPricePerM: 15,
        cacheInputPricePerM: 0.3,
        cacheOutputPricePerM: 3.75,
        currency: "USD",
      },
      template
    ),
    true
  )

  assert.equal(
    doesCostMatchBillingTemplate(
      {
        enabled: true,
        inputPricePerM: 4,
        outputPricePerM: 15,
        cacheInputPricePerM: 0.3,
        cacheOutputPricePerM: 3.75,
        currency: "USD",
      },
      template
    ),
    false
  )
})
