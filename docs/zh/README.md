# AI Open Router

一个桌面端本地 AI 网关：**统一入口、协议切换、Token/费率统计、云备份**。

English documentation: [../../README.md](../../README.md)

开发文档（数据流/逻辑流）：[development-flow.md](./development-flow.md)

## 为什么用它

- 多个 AI 提供商协议不一致，客户端接入成本高。
- 团队常常需要“同一个入口”切换不同上游规则。
- 需要把 Token 消耗、请求成功率、费率剩余看清楚。
- 配置容易丢失，需要可回滚、可同步的云备份能力。

## 核心功能（你关心的 4 点）

| 功能 | 解决什么问题 | 在哪里使用 |
| --- | --- | --- |
| 协议切换 | OpenAI 兼容客户端与 Anthropic 客户端可共用一个本地网关 | 服务页（分组 + 规则） |
| 费率统计 | 每条规则展示剩余额度与状态（`ok`/`low`/`empty`） | 服务页规则卡片 |
| Token 统计 | 按时间窗口/规则查看请求量、成功率、Token 用量 | 日志页（统计区） |
| 云备份（Git） | 分组与规则可上传/拉取，支持冲突检测 | 设置页（Remote Git） |

## 3 分钟上手（新用户必看）

### 1）运行应用

如果你是开发者从源码启动：

```bash
npm install
npm start
```

默认监听 `0.0.0.0:8899`，可通过以下地址访问：
- `http://localhost:8899`
- `http://<本机局域网IP>:8899`

### 2）创建第一个路由分组

1. 进入“服务页”创建分组（例如 `claude`）。
2. 在分组中设置模型列表（可先填 `claude-3-5-sonnet`）。
3. 记住入口前缀：`/oc/<groupId>`。

### 3）添加规则并完成协议切换

1. 在该分组下新增规则，选择协议（`openai` 或 `anthropic`）。
2. 填写上游 API 地址、Token、默认模型。
3. 将该规则设为“生效规则”。

### 4）发起一次请求验证

```bash
curl http://localhost:8899/oc/claude/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-3-5-sonnet",
    "messages": [{"role":"user","content":"hi"}]
  }'
```

如果开启本地鉴权，请额外加：

```http
Authorization: Bearer <server.localBearerToken>
```

### 5）确认“已经配置成功”

完成以下 3 项即为成功：
- 日志页出现一条新请求，状态为成功。
- 日志详情可看到请求/响应与 Token 用量。
- 统计区的请求数与 Token 指标有更新。

## 功能详解

### 1）协议切换（OpenAI 兼容 ↔ Anthropic）

你可以让不同协议客户端都走同一本地入口，减少客户端改造成本。

- 分组路径：`/oc/:groupId/...`
- 支持入口：
  - `POST /oc/:groupId/chat/completions`
  - `POST /oc/:groupId/responses`
  - `POST /oc/:groupId/messages`
- `POST /oc/:groupId` 默认按 `chat/completions` 处理
- 每次请求只使用分组的 `activeRuleId` 对应规则转发

处理流程：
1. 从路径解析 `groupId`
2. 找到分组和 `activeRuleId`
3. 按规则转发到上游
4. 按入口协议与规则协议进行请求/响应转换

### 2）费率统计（规则额度可视化）

每条规则可以配置独立额度查询接口，并在规则卡片直接展示剩余状态。

可配置字段：
- `endpoint`、`method`、`authHeader`、`authScheme`
- `useRuleToken` / `customToken`
- `response.remaining`、`response.total`、`response.unit`、`response.resetAt`
- `lowThresholdPercent`

示例映射：

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

表达式仅支持数字、`+ - * /`、括号和 JSONPath，不执行脚本。

### 3）Token 统计（请求视角 + 聚合视角）

- 实时日志：查看单次请求的状态、协议方向、上游目标、Token 用量。
- 聚合统计：按时间窗口 + 规则筛选查看请求数、错误数、成功率与 Token。
- 明细排查：在日志详情查看请求头/响应头/请求体/响应体（开启 `logging.captureBody` 时）。

### 4）云备份（Remote Git）

在设置页配置 `repo URL + token + branch` 后可执行：
- 上传本地分组/规则备份到远端 `groups-rules-backup.json`
- 从远端拉取备份覆盖本地
- 本地和远端时间冲突时二次确认

适用场景：
- 换机器快速恢复配置
- 团队共享同一套路由规则模板
- 回滚错误配置

## 界面预览

### 协议切换与规则管理（服务页）

![Service Page](../assets/screenshots/service-page.png)

### 分组模型配置

![Group Edit Page](../assets/screenshots/group-edit-page.png)

### 费率统计配置（规则编辑）

![Rule Edit Page](../assets/screenshots/rule-edit-page.png)

### 云备份与全局设置

![Settings Page](../assets/screenshots/settings-page.png)

### Token 统计与日志排查

![Logs Page](../assets/screenshots/logs-page.png)

### 请求明细排查

![Log Detail Page](../assets/screenshots/log-detail-page.png)

## 常见问题（FAQ）

### 需要改造现有客户端吗？

通常不需要。大多数情况下只需把请求地址改为本地入口 `http://localhost:8899/oc/:groupId/...`。

### 一个分组能挂多条规则吗？

可以。一个分组下可维护多条规则，但同一时间只会有一条生效（`activeRuleId`）。

### 云备份拉取会覆盖本地吗？

会。导入或远端拉取都会覆盖当前分组与规则，建议先导出本地备份。

### Token 和 Git Token 安全性如何？

当前会保存在本地配置中（明文），请使用最小权限凭证，并仅在可信环境使用。

## 用户文档与开发文档分层

面向使用者：
- 本文档（上手与功能说明）

面向开发者：
- `docs/dev-database.md`（数据与持久化）
- `docs/release-process.md`（发布流程）
- `docs/tauri-architecture.md`（架构说明）

## 开发命令（开发者）

```bash
npm run check
npm run test
npm run ci
```

## 使用 Playwright 重新生成截图

```bash
npm run screenshots:mock
```

默认会生成：
- `docs/assets/screenshots/service-page.png`
- `docs/assets/screenshots/group-edit-page.png`
- `docs/assets/screenshots/rule-edit-page.png`
- `docs/assets/screenshots/settings-page.png`
- `docs/assets/screenshots/logs-page.png`
- `docs/assets/screenshots/log-detail-page.png`
