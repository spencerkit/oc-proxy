# 开发文档：数据流转与逻辑流转

本文面向开发者，描述 AI Open Router 在 **一次请求生命周期** 中的数据路径、关键模块职责，以及流式/非流式的差异化处理逻辑。

## 1. 总体分层

- UI 层（`src/renderer`）
  - 负责分组/规则配置编辑、运行状态展示、日志展示。
  - 通过 Tauri IPC 调用 Rust 命令，不直接参与协议转换。
- 桌面应用层（`src-tauri/src`）
  - 配置管理：加载、校验、迁移、导入导出、远程同步。
  - 代理运行时：接收请求、路由、协议映射、上游转发、响应回写。
  - 观测：日志、统计、额度计算。
- 映射层（`src-tauri/src/mappers`）
  - 统一 canonical 结构（请求/响应）作为中间语义模型。
  - 负责 OpenAI Chat / OpenAI Responses / Anthropic Messages 之间转换。
- 流式桥接层（`src-tauri/src/proxy/stream_bridge`）
  - 负责 SSE 帧解析与跨协议流式事件重组。

---

## 2. 请求数据流（主路径）

1. 客户端请求进入代理入口（如 `/oc/:groupId/chat/completions`）。
2. 路由模块解析 `groupId`、端点类型（chat/responses/messages）。
3. 从配置中解析当前分组与生效规则（目标协议、上游地址、默认模型、模型映射）。
4. 按入口 surface 与目标 surface 决定是否需要 `mappers` 做请求映射。
5. 构建上游请求并转发。
6. 获取上游响应后：
   - 非流式：优先尝试桥接非流式映射，再回退常规 `map_response_by_surface`。
   - 流式：按 surface 组合选择是否启用 `stream_bridge`。
7. 返回下游响应，记录日志/统计/额度信息。

---

## 3. 逻辑流（关键决策点）

### 3.1 surface 决策

- 入口 surface 由请求路径决定：
  - chat-completions -> `OpenaiChatCompletions`
  - responses -> `OpenaiResponses`
  - messages -> `AnthropicMessages`
- 目标 surface 由规则协议决定。
- `source == target`：尽量直通（仅做必要字段修正）。
- `source != target`：进入映射逻辑（非流式 mapper 或流式 bridge）。

### 3.2 模型决策

- 先按规则映射表匹配模型（精确/前缀/通配）。
- 未命中则回退规则默认模型。
- 某些映射方向会在请求层强制替换 `model`，确保上游一致性。

### 3.3 流式与非流式分叉

- 非流式：整包 JSON 映射，强调字段完整性与兼容性。
- 流式：按 SSE 帧逐步处理，强调事件顺序、增量内容拼接、终止帧（`[DONE]`）一致性。

---

## 4. mappers 的 canonical 中间层

canonical 的意义：

- 屏蔽不同协议字段差异。
- 在请求/响应方向都可以先归一再编码。
- 降低 adapter 之间 N*N 直接耦合复杂度。

典型流程：

- `decode_*`：协议 JSON -> canonical
- 中间规则处理（模型、工具、系统提示等）
- `encode_*`：canonical -> 目标协议 JSON

---

## 5. SSE 流式桥接流程

模块位于 `src-tauri/src/proxy/stream_bridge`：

- `parser.rs`
  - 把字节流切分为 SSE 帧（支持 `event:` + 多行 `data:` 聚合）。
  - 识别 JSON 帧与 `[DONE]`。
- `registry.rs`
  - 维护 `source -> target` 到具体 bridge adapter 的注册。
  - 同时处理非流式桥接入口。
- `emit.rs`
  - 统一 SSE 输出编码：`event+data`、`data`、`[DONE]`。
- `mod.rs`
  - 协调整体流程：`consume_chunk` + `finish`。

关键保证：

- 每个流生命周期仅发送一次 `[DONE]`。
- 非流式桥接无法提供可靠结果时回退常规 mapper。
- 对分片输入（半行、无末尾换行）具备容错。

---

## 6. OpenAI Chat <-> OpenAI Responses 流转重点

### Responses -> Chat（流式）

- 输入事件（如 `response.output_text.delta`、`response.function_call_arguments.delta`）被重组为 chat chunk。
- tool call 通过内部状态机聚合（id/index/arguments 增量）。
- 完成时输出带 `finish_reason` 的最终 chunk。

### Chat -> Responses（流式）

- chat `delta.content` 转成 `response.output_text.delta`。
- chat `tool_calls` 转成 `response.output_item.added` / `response.function_call_arguments.delta`。
- 结束时发 `response.completed`，并附 usage。

---

## 7. 配置与 UI 数据流

1. UI 修改配置（分组/规则/设置）。
2. Zustand store 组装 payload，经 IPC 调用 Rust 命令。
3. Rust 侧做 schema + 语义校验后落盘。
4. 运行时读取最新配置参与下一次请求路由。

导入导出场景：

- 导入时会做兼容字段解析（新旧字段别名）。
- 导出时统一以当前主字段结构输出。

---

## 8. Renderer 逻辑流（前端）

`src/renderer` 的主链路如下：

1. `main.tsx` 启动时决定主题、路由类型并挂载 React 根节点。
2. `App.tsx` 初始化 store 并注册页面路由。
3. 页面层（`pages/*`）只负责交互与展示，不直接调用后端命令细节。
4. 状态层（`store/proxyStore.ts`）聚合 UI 状态、请求动作、错误处理、持久化字段。
5. IPC 层（`utils/ipc.ts`）统一维护 `invoke` 命令映射，屏蔽 Tauri 细节。
6. 工具层（`utils/*`）处理地址、语言、token 展示、通用格式化。

设计约束：

- 页面尽量“薄”，业务逻辑下沉到 store / utils。
- IPC 命令名与参数集中在 `ipc.ts`，便于审计与重构。
- 所有跨页共享状态通过 store 管理，避免隐式耦合。

---

## 9. 开发建议

- 新增协议字段时，优先补 canonical 与 adapter 双向测试。
- 新增 bridge 时，必须同时覆盖：
  - 流式正常路径；
  - 分片输入路径；
  - `[DONE]` 单次输出；
  - 非流式回退/映射行为。
- 若需要跨层新增能力，优先保持 “UI -> IPC -> Service -> Mapper/Proxy” 单向依赖。

---

## 10. WebView 启动保护与失败兜底（2026-03）

为降低 release 场景下偶发 “页面加载失败，刷新后恢复” 的不可观测问题，当前加入了以下链路：

1. Renderer 启动上报
   - 前端 `main.tsx` 在 React 挂载后通过 IPC 调用 `app_renderer_ready`。
   - Rust 侧记录 ready 状态，供 watchdog 判定。
2. Renderer 异常上报
   - 前端全局错误入口（`window.onerror`、`onunhandledrejection`、`init().catch`）通过 `app_report_renderer_error` 上报。
   - Rust 侧落错误日志（包含 kind / source / stack 摘要）。
3. 启动超时 watchdog
   - Rust 在 release 模式下启动后开启 10 秒 watchdog（debug 不启用，避免开发阶段误判）。
   - 若未收到 ready，上报 `renderer_boot_timeout` 日志并切换到失败兜底页。
4. 失败兜底页（完整 HTML）
   - Rust 直接构造完整 HTML 字符串，编码为 `data:text/html` 并调用 `window.navigate(...)` 跳转。
   - 页面内提供“重新加载”按钮，会回到超时前记录的原始 URL 并重试加载。
   - 该兜底页自带右键/刷新快捷键限制，与 release 主页面策略保持一致。
5. Release WebView 交互收敛
   - 仅 release 模式下注入脚本，禁用右键菜单及常见刷新/开发者快捷键（F5、Ctrl/Cmd+R、F12、Ctrl/Cmd+Shift+I）。
   - debug 模式不受影响，保持开发调试体验。
6. Debug 强制演练开关
   - 仅 debug 下支持环境变量 `AOR_DEBUG_FORCE_LOAD_FAILED_PAGE=1`。
   - 启动后会主动跳转到失败兜底页，便于快速验收失败 UI 与重试逻辑。
