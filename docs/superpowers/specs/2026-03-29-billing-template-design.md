# Billing Template Design

## Summary

Add a built-in billing template library to the Provider form so users can choose a vendor/model pricing preset and apply it into the existing cost fields with one action.

The feature is a form assistant only. It does not change runtime billing math, does not fetch remote pricing data at runtime, and does not auto-update previously saved Providers.

## Problem

The current billing section in the Provider form requires users to manually type per-million token prices for every Provider. This is repetitive, slows down setup, and makes it harder to keep common vendor/model pricing consistent across Providers.

The project already supports template-like behavior in quota configuration, but billing has no preset or template workflow. Users want common vendor/model pricing, especially for OpenAI, Anthropic, MiniMax, and Zhipu, to be selectable from a maintained library instead of re-entered by hand.

## Goals

- Add a built-in billing template picker inside the existing Provider form.
- Organize templates by vendor and model.
- Support both vendor-first browsing and direct search.
- Apply pricing into the existing cost fields without changing runtime billing logic.
- Persist enough template metadata to show which template was used later.
- Seed the library with common vendor/model entries whose pricing can be verified from official sources.

## Non-Goals

- Changing how request cost is calculated at runtime.
- Pulling pricing from vendor APIs or websites dynamically.
- Auto-refreshing saved Provider prices when vendor pricing changes.
- Introducing expression-based billing formulas.
- Introducing template variables such as `{{rule.xxx}}` into billing fields.
- Replacing manual editing of billing fields after a template is applied.

## Current Constraints

The current system stores billing as numeric values only:

- `inputPricePerM`
- `outputPricePerM`
- `cacheInputPricePerM`
- `cacheOutputPricePerM`
- `currency`

Runtime cost snapshots are still computed directly from those numeric fields in `src-tauri/src/proxy/observability.rs`. This feature must preserve that behavior.

## Recommended Approach

Use a data-backed local template library that is bundled with the app and consumed by the Provider form.

This is preferred over a hard-coded UI-only list because:

- the template catalog will grow over time
- each template needs source metadata and verification date
- some vendors expose complete pricing while others only expose partial pricing
- the same template data should be testable independently from the form component

This is preferred over remote sync because the first release should stay deterministic, offline-safe, and low-risk.

## Entry Point

The feature lives in the billing section of the shared Provider form:

- [RuleFormPage.tsx](/home/spencer/workspace/oc-proxy/src/renderer/pages/RuleFormPage/RuleFormPage.tsx)

It appears inside the existing `sectionCost` block, below `Enable Cost Calculation` and above the numeric pricing fields.

## User Experience

### Billing Section

When cost calculation is enabled, the billing section shows:

- a compact template summary row
- a `Select Template` action
- a secondary action to clear template attribution without clearing manual prices

Summary states:

- `No template applied`
- `Applied OpenAI / GPT-5.4`
- `Applied Anthropic / Claude Sonnet 4.5, modified after apply`

### Template Picker Modal

Clicking `Select Template` opens a modal that follows the same general interaction style as the existing Provider import modal:

- top search input
- left vendor list
- right model list for the selected vendor
- detail panel for the currently highlighted template

The modal supports two navigation paths:

1. vendor-first browsing: select vendor, then select model
2. search-direct: type vendor or model name and jump directly to a result

The detail panel shows:

- vendor name
- model name
- currency
- input price
- output price
- cache input price
- cache output price
- template completeness: `full` or `partial`
- official source link
- verification date
- an explanation when only some fields will be applied

### Apply Behavior

When the user applies a template:

- template metadata is recorded in form state
- numeric cost fields are updated from the template
- the user can still manually edit all fields
- no confirmation dialog is shown because the form is not saved yet

Application rules:

- `full` template: overwrite all four numeric fields and currency
- `partial` template: overwrite only fields that the template explicitly provides
- missing template fields never clear existing user-entered values

### Post-Apply Editing

After a template is applied, manual edits remain allowed.

If the user changes any billing field after applying a template, the template summary changes to `modified after apply`, but template attribution is kept.

This preserves provenance without locking the form.

## Template Data Model

Create a dedicated renderer-side template module at `src/renderer/utils/billingTemplates.ts` so the catalog is testable and not embedded in the component body.

Suggested shape:

```ts
type BillingTemplateCompleteness = "full" | "partial"

interface BillingTemplate {
  vendorId: string
  vendorLabel: string
  modelId: string
  modelLabel: string
  searchAliases: string[]
  currency: string
  inputPricePerM?: number
  outputPricePerM?: number
  cacheInputPricePerM?: number
  cacheOutputPricePerM?: number
  completeness: BillingTemplateCompleteness
  sourceUrl: string
  sourceNote: string
  verifiedAt: string
}
```

Suggested saved attribution shape inside Provider cost config:

```ts
interface BillingTemplateAttribution {
  vendorId: string
  vendorLabel: string
  modelId: string
  modelLabel: string
  sourceUrl: string
  verifiedAt: string
  appliedAt: string
  modifiedAfterApply: boolean
}
```

Add this as an optional nested field under `cost` as `cost.template`.

## Config Compatibility

Older configs have no template attribution. They must continue to load without migration errors.

Compatibility rules:

- `cost.template` is optional
- absence of `cost.template` means manual pricing or legacy pricing
- old configs continue to behave exactly as before
- saving a Provider without using templates does not need to emit template metadata

Because the runtime only depends on numeric pricing fields, no runtime billing migration is required.

## Vendor and Model Coverage

### First Release Vendor Groups

The picker ships vendor sections for:

- OpenAI
- Anthropic
- MiniMax
- Zhipu

### Seed Catalog Policy

Only seed exact template pricing when the value can be verified from an official vendor source.

For every seeded template, store:

- official source URL
- verification date
- completeness flag

If an official source exposes only part of the pricing surface, ship a `partial` template instead of inventing missing numbers.

### OpenAI

OpenAI is a good fit for `full` templates because official model pages expose input, cached input, and output pricing in a shape that maps cleanly to the existing form.

Initial target models:

- `GPT-5.4`
- `GPT-5 mini`
- `GPT-5.4 nano`
- `GPT-4.1 mini`
- `GPT-4o`
- `GPT-4o mini`

### Anthropic

Anthropic is also a good fit, but its official pricing terminology differs from the current form.

Mapping rule:

- `cache hit` -> `cacheInputPricePerM`
- `cache write` -> `cacheOutputPricePerM`

Initial target models:

- `Claude Sonnet 4.5`
- `Claude Opus 4.1`
- `Claude Haiku 4.5`

`Claude Sonnet 4.6` is deferred from the initial seed catalog unless it has a directly verifiable official pricing page before implementation starts.

### MiniMax

MiniMax is a good fit for `full` templates because the official pricing guide exposes input, output, caching read, and caching write pricing.

Initial target models:

- `MiniMax-M2.7`
- `MiniMax-M2.7-HighSpeed`
- `MiniMax-M2.5`
- `MiniMax-M2.1`

### Zhipu

Zhipu coverage should prioritize newer families instead of centering the first release on older `GLM-4.x` models.

Coverage priority:

1. `GLM-5`
2. `GLM-5.1`
3. selected still-common older models when pricing is officially available

Because Zhipu pricing publication is split across docs and a dedicated pricing surface, the implementation must follow a stricter seed rule:

- ship exact Zhipu prices only when the price can be verified from an official, directly accessible source at implementation time
- if a model family exists in official docs but its current price cannot be verified from an official source on implementation day, do not ship a guessed price
- it is acceptable for the first release to include Zhipu vendor grouping and search aliases for `GLM-5` and `GLM-5.1` while only seeding exact numeric entries for the models whose official pricing is verifiable

This keeps vendor coverage aligned with user intent without introducing false price data.

## Search Rules

Search should match against:

- vendor label
- model label
- aliases

Examples:

- `openai`
- `gpt-5`
- `claude sonnet`
- `minimax m2.7`
- `glm5`
- `glm 5.1`
- `智谱`

Search is local only. No remote lookup is performed.

## Apply Semantics

Applying a template is a pure form mutation.

Rules:

- applying a template updates cost inputs in local state only
- no config save is triggered automatically
- a later manual save persists both numeric pricing and template attribution
- clearing attribution only removes `cost.template`; it does not zero out numeric prices
- disabling cost calculation keeps numeric values intact, matching current form behavior

## Error Handling

### UI Errors

Expected UI error and empty states:

- no search results
- template catalog missing a vendor section
- template detail unavailable because the selected template is malformed

These should fail safely:

- do not crash the form
- do not clear user-entered pricing
- show a lightweight inline message

### Data Validation

Template data validation should happen in tests and optionally through a small runtime guard in development builds.

Reject or flag templates that have:

- negative numeric prices
- missing `currency`
- missing `sourceUrl`
- missing `verifiedAt`
- `full` completeness with no numeric fields

## Implementation Notes

Likely touch points:

- renderer billing form state and modal UI in [RuleFormPage.tsx](/home/spencer/workspace/oc-proxy/src/renderer/pages/RuleFormPage/RuleFormPage.tsx)
- renderer type definitions in [proxy.ts](/home/spencer/workspace/oc-proxy/src/renderer/types/proxy.ts)
- Rust/shared schema in `src-tauri/src/domain/entities.rs`
- config normalization and validation in `src-tauri/src/config/`
- i18n strings in `src/renderer/i18n/en-US.ts` and `src/renderer/i18n/zh-CN.ts`

The implementation should keep the template catalog isolated from the component so data and search rules are easy to test.

## Testing Strategy

### Unit Tests

Add unit coverage for:

- vendor/model search
- alias search
- `full` template application
- `partial` template application preserving untouched existing values
- Anthropic cache field mapping
- `modifiedAfterApply` state transitions after manual edits
- compatibility when `cost.template` is absent

### Component Tests

Add UI tests for:

- billing section shows `Select Template` only when billing is enabled
- modal opens and renders vendor list
- vendor selection filters model list
- search narrows results correctly
- detail panel shows source URL and verification date
- applying a template updates the visible form fields
- editing after apply changes the summary to `modified after apply`

### Regression Tests

Protect current billing behavior:

- Provider save still works without templates
- old Providers without template metadata still load and save correctly
- runtime cost calculation still depends only on numeric fields
- logs and stats behavior remain unchanged

## Delivery Scope

Initial release scope:

- bundled billing template catalog
- template picker modal inside the existing billing section
- vendor browsing plus search
- template attribution persistence
- `full` and `partial` apply rules
- tests for catalog, UI flow, and config compatibility

Deferred scope:

- remote pricing sync
- automatic update prompts when vendor pricing changes
- expression-based billing formulas
- templated billing variables
- cross-vendor currency conversion

## Official Source Set Used For Design

The design direction above is based on official pricing or model documentation reviewed on `2026-03-29`:

- OpenAI:
  - https://developers.openai.com/api/docs/models/gpt-5.4
  - https://developers.openai.com/api/docs/models/gpt-5-mini
  - https://developers.openai.com/api/docs/models/gpt-5.4-nano
  - https://developers.openai.com/api/docs/models/gpt-4.1-mini
  - https://developers.openai.com/api/docs/models/gpt-4o
  - https://developers.openai.com/api/docs/models/gpt-4o-mini
- Anthropic:
  - https://platform.claude.com/docs/zh-CN/about-claude/pricing
- MiniMax:
  - https://platform.minimax.io/docs/guides/pricing-paygo
- Zhipu:
  - https://docs.bigmodel.cn/cn/guide/start/model-overview
  - https://docs.bigmodel.cn/cn/guide/models/text/glm-5
  - https://docs.bigmodel.cn/cn/guide/models/text/glm-5-turbo
  - https://docs.bigmodel.cn/cn/guide/models/text/glm-4
  - https://docs.bigmodel.cn/cn/guide/models/text/glm-4-long
  - https://docs.bigmodel.cn/cn/guide/models/text/glm-z1

These sources are sufficient to define the product shape, vendor coverage policy, and template completeness rules. During implementation, exact seeded prices must still be copied only from officially verifiable vendor pages.
