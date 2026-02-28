# OA Proxy (Electron)

一个 Electron 桌面中转服务，支持按分组路径做 OpenAI 与 Claude 协议双向转发。

## 交互模型
- 顶部状态栏：显示服务运行状态 + 服务开关 + 添加分组 + 设置 + 日志页面切换
- 分组 Tab：每个分组对应一个 path（如 `claude`、`codex`）
- 分组规则：每个分组可配置多条规则，但同一时刻只能有一条生效
- 规则字段：`模型名称`、`token`、`api地址`、`方向（OpenAI -> Anthropic / Anthropic -> OpenAI）`
- 设置页：可修改服务 host/port（保存后若服务在运行中会自动重启，并提示“重启完成”）
- 日志页：单独页面展示完整请求链路（请求地址、转发地址、请求报文、转发报文、响应报文、错误信息）

## 转发入口
服务默认监听 `0.0.0.0:8899`，按分组 path 转发：
- `POST /oc/:groupPath`：统一入口，具体转发方向由该分组当前生效规则决定

例如分组 path 为 `claude`：
- `http://localhost:8899/oc/claude`

## 规则生效逻辑
1. 通过 URL path 命中分组
2. 读取该分组 `activeRuleId`
3. 仅使用该生效规则进行转发
4. 按生效规则的 `direction` 自动决定协议转换方向

## 启动
```bash
npm install
npm start
```

## 测试
```bash
npm test
```

## 配置文件
首次启动会在 Electron `userData/config.json` 生成配置。

核心结构：
- `server`: host/port/auth
- `compat`: strictMode
- `logging`: 是否记录 body + 脱敏规则
- `groups[]`:
  - `name`, `path`
  - `rules[]` (model/token/apiAddress/direction)
  - `activeRuleId`

默认不会创建任何分组，需要在界面里手动添加分组和规则。

日志默认仅保留最近 `100` 条请求链路记录。

## 说明
- 当前版本已支持流式桥接（SSE）与工具调用基础映射。
- token 当前按本地配置文件保存（明文），适合开发与内网场景。
