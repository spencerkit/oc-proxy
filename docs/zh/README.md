# OA Proxy

桌面代理服务，用于在 OpenAI 兼容协议与 Anthropic 协议之间做双向转发。

English documentation: [../../README.md](../../README.md)

## 概览

OA Proxy 提供：
- 按分组路径路由（`/oc/:groupId/...`）
- 按生效规则转发（`activeRuleId`）
- 双向协议转换：
  - OpenAI 兼容 -> Anthropic
  - Anthropic -> OpenAI 兼容
- 流式桥接（SSE）与基础工具调用映射
- 本地请求链路日志与脱敏能力
- 分组/规则的 JSON 备份与恢复（文件与剪贴板）

## 使用场景

- 在 Claude 协议客户端中使用 OpenAI 兼容 API：
  将分组生效规则的下游协议设为 `openai`，客户端通过 `POST /oc/:groupId/messages` 接入即可。
- 在 OpenAI 兼容客户端中使用 Anthropic 模型：
  将下游协议设为 `anthropic`，客户端通过 `POST /oc/:groupId/chat/completions` 或 `POST /oc/:groupId/responses` 调用。
- 为多种客户端协议提供统一本地入口：
  通过分组 ID 路由，并按分组隔离模型、Token 与上游地址配置。
- 本地开发/团队网关：
  上游凭证保存在本地配置中，对外只暴露统一且稳定的本地 API 入口。

## 支持的入口路径

服务默认监听 `0.0.0.0:8899`。

每个分组可使用：
- `POST /oc/:groupId/chat/completions`
- `POST /oc/:groupId/responses`
- `POST /oc/:groupId/messages`

若不带后缀，`/oc/:groupId` 默认按 chat-completions 处理。

示例：
- `http://localhost:8899/oc/claude/chat/completions`
- `http://localhost:8899/oc/claude/responses`

## 规则生效逻辑

每个请求会按以下顺序处理：
1. 从路径匹配 `:groupId`
2. 读取该分组的 `activeRuleId`
3. 仅使用这一条生效规则进行转发
4. 根据入口协议与规则下游协议做请求/响应转换

## 启动

```bash
npm install
npm start
```

说明：
- `npm start` 启动本地桌面应用，读取 `out/` 中的构建产物。
- 若 `out/` 不存在或内容过旧，请先执行 `npm run build`。

## 开发与调试

安装依赖：

```bash
npm install
```

启动前端开发服务（终端 1）：

```bash
npm run dev
```

启动桌面应用（终端 2）：

```bash
npm start
```

调试建议：
- 主进程日志输出在执行 `npm start` 的终端。
- 渲染进程日志在应用 DevTools 中查看（当前配置会自动打开）。

## 测试

```bash
npm test
```

## 构建

```bash
npm run build
```

构建产物目录：
- `out/renderer`：前端静态资源
- `out/main`：主进程产物
- `out/preload`：预加载脚本产物
- `out/proxy`：同步后的代理运行模块

## 配置说明

首次启动会在应用用户数据目录生成 `config.json` 配置文件。

核心配置结构：
- `server`: host/port/auth
- `compat`: 严格模式
- `ui`: 主题/语言/开机启动
- `logging`: 请求体记录与脱敏规则
- `groups[]`:
  - `id`, `name`, `models[]`
  - `rules[]`（`protocol`, `token`, `apiAddress`, `defaultModel`, `modelMappings`）
  - `activeRuleId`

补充说明：
- 默认不会自动创建分组。
- 日志在内存中保留，默认上限为 100 条。
- 导入备份 JSON 时会覆盖当前全部分组及其规则。

## 备份与恢复

在设置页可进行分组/规则备份：
- 导出：
  - 导出到文件夹（自动生成文件名）
  - 复制 JSON 到剪贴板
- 导入（含确认弹框）：
  - 从 JSON 文件导入
  - 从剪贴板 JSON 粘贴导入

支持导入的 JSON 结构：
- 直接传 `groups` 数组
- `{ "groups": [...] }`
- `{ "config": { "groups": [...] } }`

## 安全说明

- 上游 Token 当前以明文保存在本地配置中。
- 在生产或准生产环境中请使用最小权限凭证。
