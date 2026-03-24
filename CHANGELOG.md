# Changelog

All notable changes to this project will be documented in this file.

## v0.2.14 - 2026-03-23
### Features
- feat: redesign provider cards and harden remote admin (b513d17)
### Fixes
- fix: apply rustfmt for tauri backend (b26a09f)

## v0.2.12 - 2026-03-22
### Features
- feat: add remote auth and openclaw integration (94ad54d)
- feat: add remote management password flow (edfa18c)
### Fixes
- fix: support opencode tokens and oc auth compatibility (ed07a3a)
### Maintenance
- ci: build renderer assets in linux smoke (0c10b35)
- refactor: simplify access control settings (ee3b8d6)
- docs: record desktop webview stability issue (7175043)
- ci: pin linux release to ubuntu 22.04 (7b85475)
- build: add linux release packaging (46d228a)

## v0.2.11 - 2026-03-13
### Fixes
- fix(release): build and sign updater artifacts explicitly (7482d5e)
- fix(release): only synthesize updater manifest from signed assets (da2b56e)
- fix: harden provider cleanup and updater release flow (b0fbdc5)
- fix(ci): correct headless packaging condition (bf34883)
- fix(ci): configure npm registry auth (47a2c31)
- fix(release): publish updater and headless assets (59dc5d5)
### Maintenance
- docs: add macOS damaged app workaround (d4796d0)
- chore: clarify updater manifest error (1325f3c)

## v0.2.10 - 2026-03-12
### Features
- feat(headless): default integration targets (abdddfd)
- feat(headless): http ui, cli, and release publish (2154c6f)
- feat: add auto updates and refresh docs (39d89d2)
### Fixes
- fix(logs): split provider/group filters (c47f86e)
### Maintenance
- chore(ci): format wdio config (17a6f46)
- chore: ignore tmp e2e data (546c52f)
- test(e2e): add tauri main flow (1efb5a4)
- docs: update quick start flow (6c30fc1)
- refactor: migrate relax state 0.0.9 (36a9f14)

## v0.2.9 - 2026-03-10
### Breaking Changes
- feat: add AgentConfig DTO for agent management (4fe7ba4)
### Features
- feat: apply latest providers and ui refinements (7c7c4e3)
- feat: migrate providers to global management flow (ca34f1b)
- feat: update app icons and header brand logo (4328627)
- feat: optimize group integration snapshot layout (38cb33d)
- feat: refine codex config handling and compact agent/group UI (aea0552)
- feat(transformer): add openai2 adapters and stream observability (0fac59c)
- feat: wire agents pages into app navigation (f66c0c6)
- feat: redesign agents management pages (4f68f5b)
- feat: integrate AgentManagementPanel in ServicePage (d05a01b)
- feat: add AgentManagementPanel component (ed5ce36)
- feat: add i18n for agent management (7c60437)
- feat: add AgentConfig types for frontend (6fb7522)
- feat: add API to write agent config files (1a3dae4)
- feat: add API to read agent config files (072a6c9)
- feat: add one-click client config write flow (056205c)
- feat: improve provider testing and proxy/service compatibility (6136b87)
- feat(transformer): complete Responses ↔ Chat Completions conversion (b8c2d95)
- feat(transformer): add streaming support and debug documentation (491ba23)
- feat(transformer): implement streaming conversion for OpenAI Responses API (749ab5f)
- feat(transformer): add complete protocol conversion support (1246e0d)
- feat(transformer): add streaming and Gemini support (213bebb)
### Fixes
- fix(transformer): align responses<->claude role and tool sequencing (7e7a063)
- fix(ci): apply rustfmt for stream converter (3b4fa39)
- fix(ci): resolve lint issues and stabilize transformer stream tests (dd3476d)
- fix: preserve upstream response events in claude stream (fc4fb2f)
- fix(proxy): keep draining upstream stream after client close (fa577fe)
- fix: add WSL path handling for read_agent_config (f09a41d)
- fix: harden responses stream handling and tool-call fallback (f075e30)
- fix(proxy): improve token usage extraction compatibility (01a709f)
- fix: align responses->chat path and improve debug logging (cece4ba)
- fix(transformer): map developer role to user for API compatibility (30ed8be)
- fix(transformer): map developer role to user for Claude compatibility (0267d73)
- fix(transformer): correct OpenAI Responses <-> Claude conversion (fa596ba)
- fix(pipeline): implement protocol conversion in request/response pipeline (c06c788)
### Maintenance
- chore: rustfmt (1988bd6)
- chore: format provider pages and styles (63a9593)
- refactor: split provider lists and optimize providers rendering (964be8c)
- chore(release): bump version to v0.2.8 (0227b29)
- style(integration): align conflict resolution with beta branch (62fd56f)
- docs: reorganize planning documents (ef27148)
- style: format code with cargo fmt (930550f)
- refactor: remove dead code in config parsing (788da33)
- other: Fix WSL config writes on Windows (6323c88)
- style(rust): format code with cargo fmt (4b6848e)
- chore(release): bump version to v0.2.7 (8a0225f)
- chore: add debug dump artifacts (7ec3e27)
- test(transformer): add unit tests and remove old mapper tests (ab4365a)
- refactor(transformer): implement ccNexus architecture with Claude-centric conversion (d2b05d5)

## v0.2.8 - 2026-03-09
### Breaking Changes
- feat: add AgentConfig DTO for agent management (4fe7ba4)
### Features
- feat: update app icons and header brand logo (4328627)
- feat: optimize group integration snapshot layout (38cb33d)
- feat: refine codex config handling and compact agent/group UI (aea0552)
- feat(transformer): add openai2 adapters and stream observability (0fac59c)
- feat: wire agents pages into app navigation (f66c0c6)
- feat: redesign agents management pages (4f68f5b)
- feat: integrate AgentManagementPanel in ServicePage (d05a01b)
- feat: add AgentManagementPanel component (ed5ce36)
- feat: add i18n for agent management (7c60437)
- feat: add AgentConfig types for frontend (6fb7522)
- feat: add API to write agent config files (1a3dae4)
- feat: add API to read agent config files (072a6c9)
- feat: add one-click client config write flow (056205c)
- feat: improve provider testing and proxy/service compatibility (6136b87)
- feat(transformer): complete Responses ↔ Chat Completions conversion (b8c2d95)
- feat(transformer): add streaming support and debug documentation (491ba23)
- feat(transformer): implement streaming conversion for OpenAI Responses API (749ab5f)
- feat(transformer): add complete protocol conversion support (1246e0d)
- feat(transformer): add streaming and Gemini support (213bebb)
### Fixes
- fix: preserve upstream response events in claude stream (fc4fb2f)
- fix(proxy): keep draining upstream stream after client close (fa577fe)
- fix: add WSL path handling for read_agent_config (f09a41d)
- fix: harden responses stream handling and tool-call fallback (f075e30)
- fix(proxy): improve token usage extraction compatibility (01a709f)
- fix: align responses->chat path and improve debug logging (cece4ba)
- fix(transformer): map developer role to user for API compatibility (30ed8be)
- fix(transformer): map developer role to user for Claude compatibility (0267d73)
- fix(transformer): correct OpenAI Responses <-> Claude conversion (fa596ba)
- fix(pipeline): implement protocol conversion in request/response pipeline (c06c788)
### Maintenance
- style(integration): align conflict resolution with beta branch (62fd56f)
- docs: reorganize planning documents (ef27148)
- style: format code with cargo fmt (930550f)
- refactor: remove dead code in config parsing (788da33)
- other: Fix WSL config writes on Windows (6323c88)
- style(rust): format code with cargo fmt (4b6848e)
- chore(release): bump version to v0.2.7 (8a0225f)
- chore: add debug dump artifacts (7ec3e27)
- test(transformer): add unit tests and remove old mapper tests (ab4365a)
- refactor(transformer): implement ccNexus architecture with Claude-centric conversion (d2b05d5)

## v0.2.7 - 2026-03-08
### Features
- feat: add one-click client config write flow (056205c)
- feat: improve provider testing and proxy/service compatibility (6136b87)
- feat(transformer): complete Responses ↔ Chat Completions conversion (b8c2d95)
- feat(transformer): add streaming support and debug documentation (491ba23)
- feat(transformer): implement streaming conversion for OpenAI Responses API (749ab5f)
- feat(transformer): add complete protocol conversion support (1246e0d)
- feat(transformer): add streaming and Gemini support (213bebb)
### Fixes
- fix(ci): fix artifact collection paths for NSIS and exe bundles (f0509fa)
- fix: harden responses stream handling and tool-call fallback (f075e30)
- fix(proxy): improve token usage extraction compatibility (01a709f)
- fix: align responses->chat path and improve debug logging (cece4ba)
- fix(transformer): map developer role to user for API compatibility (30ed8be)
- fix(transformer): map developer role to user for Claude compatibility (0267d73)
- fix(transformer): correct OpenAI Responses <-> Claude conversion (fa596ba)
- fix(pipeline): implement protocol conversion in request/response pipeline (c06c788)
### Maintenance
- chore: add debug dump artifacts (7ec3e27)
- test(transformer): add unit tests and remove old mapper tests (ab4365a)
- refactor(transformer): implement ccNexus architecture with Claude-centric conversion (d2b05d5)

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
