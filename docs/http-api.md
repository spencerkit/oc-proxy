# HTTP Management API

本项目在现有代理服务端口上新增了管理 API 与管理页面路由：

- 管理页面：`/management`
- 管理 API：`/api/*`

## 基础信息

- Base URL：`http://<host>:<port>`
- Content-Type：`application/json`
- 错误格式：
  ```json
  { "error": { "code": "validation_error", "message": "..." } }
  ```

## 管理页面

- `GET /management`
- `GET /management/*`（SPA fallback 到 `index.html`）

Release 模式下管理页面静态资源打包进二进制。
Debug 模式默认读取 `out/renderer`，可通过环境变量 `AOR_MANAGEMENT_ROOT` 指定静态资源目录。

## 应用与运行状态

- `GET /api/health`
- `GET /api/app/info`
- `GET /api/app/status`
- `POST /api/app/server/start`
- `POST /api/app/server/stop`
- `POST /api/app/renderer-ready`
- `POST /api/app/renderer-error`  
  Body: `{ kind, message, stack?, source? }`
- `GET /api/app/clipboard-text`

## 配置与备份

- `GET /api/config`
- `PUT /api/config`  
  Body: `{ nextConfig: <ProxyConfig> }`
- `POST /api/config/groups/export`
- `POST /api/config/groups/export-folder`
- `POST /api/config/groups/export-clipboard`
- `GET /api/config/groups/export-json`  
  返回 `{ text, fileName, groupCount, charCount }`，用于浏览器下载/剪贴板场景。
- `POST /api/config/groups/import`
- `POST /api/config/groups/import-json`  
  Body: `{ jsonText: string }`

## 远端规则同步

说明：

- 在 headless 模式中，`/api/app/server/start` 与 `/api/app/server/stop` 会返回 400（不允许通过管理页面起停服务）。
- 在 headless 模式中，不允许通过 `PUT /api/config` 修改 `server.port`（应在进程启动前指定端口）。

- `POST /api/config/remote-rules/upload`  
  Body: `{ force?: boolean }`
- `POST /api/config/remote-rules/pull`  
  Body: `{ force?: boolean }`

## 日志与统计

- `GET /api/logs?max=100`
- `DELETE /api/logs`
- `GET /api/logs/stats/summary`  
  Query: `hours?`, `ruleKeys?`, `ruleKey?`, `dimension?`, `enableComparison?`
- `GET /api/logs/stats/rule-cards`  
  Query: `groupId`, `hours?`
- `DELETE /api/logs/stats`  
  Query: `beforeEpochMs?`

## 配额

- `GET /api/quota/rule?groupId=...&ruleId=...`
- `GET /api/quota/group?groupId=...`
- `POST /api/quota/test-draft`  
  Body: `{ groupId, ruleName, ruleToken, ruleApiAddress, ruleDefaultModel, quota }`

## Provider

- `POST /api/provider/test-model`  
  Body: `{ groupId, providerId }`

## 集成目标

- `GET /api/integration/targets`
- `POST /api/integration/pick-directory`  
  Body: `{ initialDir?, kind? }`
- `POST /api/integration/targets`  
  Body: `{ kind, configDir }`
- `PUT /api/integration/targets`  
  Body: `{ targetId, configDir }`
- `DELETE /api/integration/targets?targetId=...`
- `POST /api/integration/write-group-entry`  
  Body: `{ groupId, targetIds }`
- `GET /api/integration/agent-config?targetId=...`
- `PUT /api/integration/agent-config`  
  Body: `{ targetId, config }`
- `PUT /api/integration/agent-config/source`  
  Body: `{ targetId, content, sourceId? }`
