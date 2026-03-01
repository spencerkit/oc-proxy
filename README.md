# OA Proxy

Desktop proxy service for bidirectional protocol forwarding between OpenAI-compatible APIs and Anthropic APIs.

中文文档: [docs/zh/README.md](docs/zh/README.md)

## Overview

OA Proxy provides:
- Group-based routing (`/oc/:groupId/...`)
- Rule-based upstream selection (`activeRuleId`)
- Bidirectional protocol translation:
  - OpenAI-compatible -> Anthropic
  - Anthropic -> OpenAI-compatible
- Streaming bridge (SSE) and basic tool call mapping
- Local request chain logs with redaction support
- Group/rule backup and restore (JSON file and clipboard)

## Use Cases

- Use OpenAI-compatible APIs from Claude-style clients:
  Configure a group's active rule with downstream protocol `openai`, then call `POST /oc/:groupId/messages` from Anthropic/Claude-style clients.
- Use Anthropic models from OpenAI-compatible clients:
  Configure downstream protocol `anthropic`, then call `POST /oc/:groupId/chat/completions` or `POST /oc/:groupId/responses`.
- Unify mixed client protocols behind one local endpoint:
  Route by group ID and keep each group's model/token/upstream config isolated.
- Local team/dev gateway:
  Keep upstream tokens in local config while exposing one stable local API surface to tools and scripts.

## Screenshots

### Service

![Service Page](docs/assets/screenshots/service-page.png)

### Settings

![Settings Page](docs/assets/screenshots/settings-page.png)

### Logs

![Logs Page](docs/assets/screenshots/logs-page.png)

## Supported Entry Endpoints

The server listens on `0.0.0.0:8899` by default.

For each group:
- `POST /oc/:groupId/chat/completions`
- `POST /oc/:groupId/responses`
- `POST /oc/:groupId/messages`

If no suffix is provided, `/oc/:groupId` defaults to chat-completions behavior.

Example:
- `http://localhost:8899/oc/claude/chat/completions`
- `http://localhost:8899/oc/claude/responses`

## Rule Resolution

For each request:
1. Match `:groupId` from path
2. Load that group's `activeRuleId`
3. Use only the active rule for forwarding
4. Translate request/response based on entry protocol + downstream rule protocol

## Start

```bash
npm install
npm start
```

Notes:
- `npm start` runs the local desktop app using built output in `out/`.
- If `out/` is stale or missing, run `npm run build` first.

## Development & Debugging

Install dependencies:

```bash
npm install
```

Start renderer dev server (Terminal 1):

```bash
npm run dev
```

Start desktop app (Terminal 2):

```bash
npm start
```

Debug tips:
- Main-process logs are printed in the terminal running `npm start`.
- Renderer logs are visible in the app DevTools (opened automatically in current setup).

## Engineering Workflow

Local quality gates:

```bash
npm run check
```

Full CI-equivalent local run:

```bash
npm run ci
```

GitHub workflows:
- `CI` (`.github/workflows/ci.yml`): runs on push/PR to `main`, executes install + `npm run ci`.
- `Release Build` (`.github/workflows/release.yml`): runs on tags like `v1.2.3` (or manual dispatch), packages Windows/macOS installers and uploads artifacts.
- Dependabot (`.github/dependabot.yml`): weekly updates for npm dependencies and GitHub Actions.

## Test

```bash
npm test
```

## Build

```bash
npm run build
```

Build output:
- `out/renderer`: frontend assets
- `out/main`: main process bundle
- `out/preload`: preload bundle
- `out/proxy`: synced proxy runtime modules

## Configuration

On first launch, config is created under the app user-data directory as `config.json`.

Core sections:
- `server`: host/port/auth
- `compat`: strict mode
- `ui`: theme/locale/startup behavior
- `logging`: body capture + redaction rules
- `groups[]`:
  - `id`, `name`, `models[]`
  - `rules[]` (`protocol`, `token`, `apiAddress`, `defaultModel`, `modelMappings`)
  - `activeRuleId`

Notes:
- No groups are created by default.
- Logs are kept in-memory with a default limit of 100 entries.
- Importing backup JSON replaces all current groups and nested rules.

## Backup & Restore

Backup entry is in Settings:
- Export:
  - Export to folder (auto file name)
  - Copy JSON to clipboard
- Import (with confirmation):
  - Import from JSON file
  - Paste/import from clipboard JSON

Accepted import JSON formats:
- `groups` array directly
- `{ "groups": [...] }`
- `{ "config": { "groups": [...] } }`

## Security Notes

- Upstream tokens are currently stored in local config (plain text).
- Use minimum-scope upstream credentials in production-like environments.
