# AI Open Router

Desktop proxy service for bidirectional routing between OpenAI-compatible APIs and Anthropic APIs.

中文文档: [docs/zh/README.md](docs/zh/README.md)

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
7. Desktop runtime behavior
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
  - Cross-protocol requests are currently forced to non-stream mode for stability (`stream=false` upstream)

### Model Routing

- Group model list controls which incoming models are accepted/matched for mapping
- Matching supports:
  - Exact match (`a1`)
  - Prefix-style match (`a1` matches `a1-mini`)
  - Wildcard suffix (`a1*`)
- Rule-level `modelMappings` are applied on matched models
- Falls back to rule `defaultModel` when no mapping/match applies

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
