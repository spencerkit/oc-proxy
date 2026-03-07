# Changelog

All notable changes to this project will be documented in this file.

## v0.2.6 - 2026-03-07
### Features
- feat(mapper): add OpenAI Responses to Anthropic Messages streaming conversion (f5342ba)
- feat: show request logs early and backfill responses (302ac8e)
- feat: harden webview boot failure fallback and diagnostics (15eb1e1)
### Fixes
- fix(mapper): emit content_block_delta events for tool arguments in responses->anthropic (92dc33d)
- fix(proxy): remove forced stream=false for messages->responses route (ec3fd43)
- fix(proxy): handle SSE responses regardless of stream parameter (86ea8d5)
- fix(proxy): force streaming mode for /realtime endpoints (46f4085)
- fix(mapper): add strip_schema_field to openai_chat_completions adapter (8eb37d5)
- fix(logger): normalize tool schemas before forwarding (6059bb4)
### Maintenance
- other: Revert "chore(release): bump version to v0.3.0" (a332e12)
- chore(release): bump version to v0.3.0 (1c45d73)
- ci(release): upload only installer assets to avoid duplicate names (a0572de)
- docs: add method-level comments and development flow documentation (731944c)
- refactor: split stream bridge modules and align responses-completions bridges (ea43115)
- refactor: move openai-chat to anthropic stream parsing into adapter (a2b5607)

## v0.2.4 - 2026-03-04
### Features
- feat(logs): add date-based stats reset modal and chart tweaks (761cb03)
- feat: add billing metrics UI and provider cost display (72562ac)
- feat: migrate provider config storage and refine stats/import behavior (e328fb0)
- feat(stats): switch rpm/tpm to active-time average (a748cb9)
- feat(stats): enhance logs analytics and polish rule card UX (210a7f0)
- feat(stats): support multi-rule analytics, rpm/tpm trends, and rule-card mini charts (0211213)
- feat(quota): query only active rule and make quota auto-refresh interval configurable (b37a3c5)
- feat(proxy): add cross-protocol SSE bridge for chat->messages (bafbd11)
- feat(proxy): integrate mapper engine for cross-protocol routing (04f2d15)
- feat(benchmark): add health grading and threshold config (23d5b24)
- feat: add proxy benchmark and local mock upstream scripts (5391905)
- feat(quota): add unit-type based quota display and threshold rules (ee7a1b8)
- feat(quota): add draft quota test panel and dev request logs (b7b386e)
- feat(quota): add per-rule remaining quota query with mapping adapter (d16d079)
- feat: improve logs dashboard and cloud sync settings UX (1ec1d98)
- feat(logs): capture stream payload and add collapsible json viewer (f72209b)
### Fixes
- fix(service): persist active group selection across page navigation (94c8ee3)
- fix(stats): align openai input tokens with cache-read semantics (5c4ec1b)
- fix(proxy): stabilize messages->responses defaults and mapper tool schema (85fbba9)
- fix(proxy): propagate stream errors and normalize responses tool calls (c36b8e5)
- fix(mapper): normalize anthropic system blocks to responses instructions string (33efd93)
- fix(proxy): restore downstream v1 compatibility and improve upstream routing/logging (2ed133f)
### Maintenance
- refactor(provider): unify provider naming and keep legacy rule compatibility (3d1f6e0)
- chore: update tauri identifier to art.shier.aiopenrouter (670821a)
- chore(screenshots): add playwright mock capture script and refresh docs images (073c7ba)
- refactor(proxy): reuse stream bridge for non-stream response mapping (45d5b07)
- refactor(proxy): make stream bridge pluggable (6949d29)
- refactor(mappers): share openai stream and usage semantics (bbee840)
- chore: commit remaining workspace changes (5859ec6)
- chore(proxy): remove unused legacy TS proxy modules (2e8693c)
- docs(rust): add module and core-flow comments across tauri backend (1517918)
- refactor(mappers): introduce canonical adapter-based mapping engine (60eb90f)
- chore(ci): enforce tauri boundaries and backend quality checks (720c837)
- refactor(config): add versioned config migration pipeline (cd2a232)
- refactor(services): introduce typed app errors across service boundary (b350039)
- test(contract): add fixture-based backend contract tests (bb97aed)
- refactor(tauri): decouple core modules and improve testability (03a68c0)
- chore(release): bump version to 0.2.4-beta (19d31c4)
- chore: commit all remaining changes (acef5a5)
- refactor(proxy): split openai protocols and remove unused RuleForm (ec5aad3)
- refactor(proxy): remove protocol bridging and follow entry protocol (cd50984)
- test(rust): migrate proxy unit tests from js to src-tauri (29d0e18)
- ci: automate tagging and formalize release workflow (dbb9eb3)
- ci: harden checks and fix streaming pipeline issues (9fe1187)

## Unreleased

- Use `npm run release:prepare` (or Release Prepare workflow) to generate the next version entry.
