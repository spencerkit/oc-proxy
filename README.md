# AI Open Router

Desktop proxy service for bidirectional routing between OpenAI-compatible APIs and Anthropic APIs.

中文文档: [docs/zh/README.md](docs/zh/README.md)
发布流程: [docs/release-process.md](docs/release-process.md)

## Overview

AI Open Router is a Tauri desktop app with a local proxy runtime.
It routes requests by group, forwards with the group active rule, and translates payloads between OpenAI-compatible and Anthropic protocols.

## Core Capabilities (Detailed)

1. Group-based gateway management
   - Create multiple groups, each with its own path (`/oc/:groupId`) and independent rule set.
   - Configure group display name and model list.
   - Keep one active rule per group through `activeRuleId`.
2. Rule-based upstream control
   - Add multiple rules under a group and switch active rule instantly.
   - Per rule settings include: protocol (`openai` or `anthropic`), token, upstream API base URL, default model, and model mappings.
   - Model mappings support exact/prefix/wildcard model keys via group model matching logic.
3. Unified local entry for mixed clients
   - Use one local service to accept OpenAI-compatible and Anthropic-style requests.
   - Entry supports `chat/completions`, `responses`, and `messages`; `/oc/:groupId` falls back to chat-completions.
   - Runtime status exposes both localhost and LAN address when applicable.
4. Backup and restore for groups/rules
   - Export backup as JSON to folder or clipboard.
   - Import backup from file or clipboard JSON.
   - Import supports multiple JSON payload shapes and replaces all current groups/rules.
5. Remote Git synchronization
   - Sync groups/rules backup JSON with a remote Git repository branch.
   - Upload local state or pull remote state from `groups-rules-backup.json`.
   - Built-in conflict confirmation when local/remote timestamps indicate newer data on one side.
6. Logging and statistics
   - Real-time request logs with status, group/rule context, protocol direction, upstream target, and token usage.
   - Detailed log view for request/response headers and payloads (when capture is enabled).
   - Aggregated stats by time range and rule filter; persisted locally with retention policy.
7. Rule quota visibility
   - Each rule can configure a dedicated quota endpoint and mapping fields for remaining quota.
   - Remaining quota status is shown directly on the rule card (`ok`, `low`, `empty`, etc.).
   - Supports heterogeneous provider payloads via JSON path and expression mapping.
8. Desktop runtime behavior
   - Launch on startup and close-to-tray behavior toggles.
   - Theme and language switching.
   - About panel showing app name/version.

## Current Features

### Proxy and Routing

- Group-based entry path routing: `/oc/:groupId/...`
- Active rule forwarding per group via `activeRuleId`
- Supported entry endpoints:
  - `POST /oc/:groupId/chat/completions`
  - `POST /oc/:groupId/responses`
  - `POST /oc/:groupId/messages`
- `POST /oc/:groupId` defaults to chat-completions behavior
- Health and runtime metrics endpoints:
  - `GET /healthz`
  - `GET /metrics-lite`

### Protocol Translation

- OpenAI-compatible -> Anthropic request/response mapping
- Anthropic -> OpenAI-compatible request/response mapping
- Basic tool-call field mapping
- Streaming behavior:
  - Same-protocol streaming uses SSE passthrough
  - OpenAI chat-completions -> Anthropic messages supports SSE event bridge conversion
  - Other cross-protocol streaming currently forwards upstream SSE bytes directly
  - Anthropic-entry requests default to `stream=true` when `stream` is omitted

### Model Routing

- Group model list controls which incoming models are accepted/matched for mapping
- Matching supports:
  - Exact match (`a1`)
  - Prefix-style match (`a1` matches `a1-mini`)
  - Wildcard suffix (`a1*`)
- Rule-level `modelMappings` are applied on matched models
- Falls back to rule `defaultModel` when no mapping/match applies

### Rule Quota Query

- Per-rule quota config fields:
  - `enabled`, `provider`, `endpoint`, `method`
  - `useRuleToken` / `customToken`
  - `authHeader`, `authScheme`, `customHeaders`
  - `response.remaining`, `response.unit`, `response.total`, `response.resetAt`
  - `lowThresholdPercent`
- Rule cards show current remaining quota and status badge.
- Quota can be refreshed per rule from the Service page.

Mapping examples:

```json
{
  "response": {
    "remaining": "$.data.remaining_balance",
    "unit": "$.data.currency",
    "total": "$.data.total_balance",
    "resetAt": "$.data.reset_at"
  }
}
```

```json
{
  "response": {
    "remaining": "$.data.remaining_balance/$.data.remaining_total",
    "unit": "$.data.unit"
  }
}
```

Expression support and safety:
- Allowed: numeric literals, `+ - * /`, parentheses, and `path('$.x.y')`
- Inline path expressions like `$.a/$.b` are supported
- No script execution (`eval`, JS runtime, external process) is used

### Desktop UI

- Service page:
  - Create/delete groups
  - Create/edit/delete rules
  - Activate one rule per group
  - Copy group entry URLs
- Server status and group entry cards both show reachable addresses:
  - localhost address
  - LAN IP address (when bound to `0.0.0.0`/`::`)
- Group editor:
  - Edit group name
  - Edit group model list
- Rule editor/creator:
  - Protocol, token, API address, default model
  - Per-model mapping
  - Quota endpoint + response mapping for remaining quota
  - Token visibility toggle
- Logs:
  - Request list, status filter, refresh, clear
  - Rule filter + time window filter
  - Stats summary (requests/errors/success rate + token metrics)
  - Log detail page (headers, bodies, errors, token usage)

### Settings

- Port configuration (saving port pins host to `0.0.0.0`)
- Strict mode toggle (`compat.strictMode`)
- Detailed body logging toggle (`logging.captureBody`)
- Launch on startup toggle
- Close-to-tray toggle
- Theme and language switch
- Group/rule backup export and import:
  - Export to folder
  - Export to clipboard JSON
  - Import from file
  - Import from clipboard JSON
- Remote Git sync for groups/rules:
  - Configure repo URL / token / branch
  - Upload local backup JSON to remote
  - Pull remote backup JSON into local config
  - Conflict confirmation when local/remote timestamps differ
- About dialog with app name/version

## Use Cases

- Call Anthropic models from OpenAI-compatible clients
- Call OpenAI-compatible models from Claude-style clients
- Expose one stable local endpoint for mixed toolchains
- Keep multi-provider tokens/rules isolated by group

## Screenshots

### Service

![Service Page](docs/assets/screenshots/service-page.png)

### Settings

![Settings Page](docs/assets/screenshots/settings-page.png)

### Logs

![Logs Page](docs/assets/screenshots/logs-page.png)

## Quick Start

### Prerequisites

- Node.js `>=20`
- npm `>=10`
- Rust toolchain (`cargo`)
- Tauri CLI (`cargo tauri`)

### Install and Run

```bash
npm install
npm start
```

`npm start` runs `cargo tauri dev`.

Default server bind is `0.0.0.0:8899`, so you will usually see both:
- `http://localhost:8899`
- `http://<your-lan-ip>:8899` (for example `http://172.25.224.1:8899`)

## Request Entry Examples

- `http://localhost:8899/oc/claude/chat/completions`
- `http://localhost:8899/oc/claude/responses`
- `http://localhost:8899/oc/claude/messages`

Local auth is optional. If enabled, send:

```http
Authorization: Bearer <server.localBearerToken>
```

## Runtime Rule Resolution

For each request:
1. Match `:groupId` from path
2. Find that group
3. Read group `activeRuleId`
4. Forward with that rule only
5. Translate request/response based on entry protocol and rule protocol

## Development Commands

```bash
npm run check
npm run test
npm run ci
```

## Debug Guide

```bash
# run desktop app in dev mode
npm start

# run frontend only
npm run dev

# check local proxy health
curl http://localhost:8899/healthz
curl http://localhost:8899/metrics-lite

# verify version consistency before release
npm run version:check

# dry-run release planning (no file changes)
npm run release:plan
```

Debug checklist:
- If proxy requests fail, check app Logs page first, then `GET /healthz` and `GET /metrics-lite`.
- If tests fail, run `npm run test:rust` to verify backend unit tests directly.
- If CI fails only on release logic, run `npm run release:plan -- --from-tag <tag>` locally.
- If release notes are empty, ensure `CHANGELOG.md` contains a `## vX.Y.Z - YYYY-MM-DD` section.

## Release Quick Start

```bash
# 1) preview next version and changelog section
npm run release:plan -- --from-tag v0.2.1

# 2) generate version bump + changelog
npm run release:prepare -- --from-tag v0.2.1

# 3) commit release changes and open PR
git checkout -b release/vX.Y.Z
git add package.json package-lock.json src-tauri/Cargo.toml src-tauri/tauri.conf.json CHANGELOG.md
git commit -m "chore(release): vX.Y.Z"

# 4) merge PR to main (CI will auto-create tag vX.Y.Z and trigger Release Build)
```

For complete release flow, see [docs/release-process.md](docs/release-process.md).

## Build Commands

```bash
npm run tauri:build
npm run tauri:build:win
npm run tauri:collect
```

Notes:
- `npm run tauri:build` sets `CARGO_TARGET_DIR=dist/target`, runs `cargo tauri build`, then collects artifacts to `dist/`.
- `npm run tauri:build:win` builds Windows target `x86_64-pc-windows-gnu` and collects artifacts.
- If you run `cargo tauri build` manually, run `npm run tauri:collect` afterward.

## Build Outputs

- `out/renderer`: frontend bundle
- `dist/target`: rust/tauri target dir used by npm build scripts
- `dist`: collected installer/binary artifacts (when available)

## Configuration and Persistence

On first launch, config is created in app data directory as `config.json`.

Core config sections:
- `server`: host, port, optional local bearer auth
- `compat`: strict mode
- `logging`: capture body and redact rules
- `ui`: theme, locale, startup/tray behavior
- `remoteGit`: remote sync settings
- `groups[]`: id/name/models/rules/activeRuleId

Data behavior:
- Request log list is in-memory (default max 100 entries)
- Aggregated stats are persisted in app data directory (`request-stats.json`)
- Stats retention window is 90 days
- Importing groups backup replaces all current groups/rules

## Security Notes

- Rule tokens and remote Git token are stored in local config as plain text.
- Use minimum-scope credentials for production-like environments.
- Prefer running on trusted local networks when exposing LAN address.
