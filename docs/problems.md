# Problems

## 2026-03-06: `GLM /v1/chat/completions` 链路中 `/responses -> /chat/completions` 返回上游 400（角色信息不正确）

- 现象:
  - 代理返回:
    - `{"error":{"code":"upstream_error","message":"Upstream returned HTTP 400","upstreamStatus":400}}`
  - 上游返回:
    - `reason=InvalidRequestBody`
    - `metadata.reason=角色信息不正确`
- 典型 traceId:
  - `94c2c1e7-49ed-4da4-b889-b4ee7fff4e91`
- 调试日志位置:
  - `app_data_dir/proxy-dev-logs.jsonl`
  - 本机示例: `/home/spencer/.local/share/art.shier.aiopenrouter/proxy-dev-logs.jsonl`

### 根因

- 请求链路是 `entry=/responses`，目标上游是 `openai_completion`（`/v1/chat/completions`）。
- 本次问题发生在 **GLM 的 chat-completions 链路**（`forwardingAddress` 示例: `https://api.edgefn.net/v1/chat/completions`）。
- 转换后 `forwardRequestBody.messages` 中包含 `role=developer`。
- 部分上游（本次 GLM）不接受 `developer` 角色，只接受 `system/user/assistant/tool`，因此返回 400。

### 影响

- 含 `developer` 消息的请求在这类上游会稳定失败。
- 不含 `developer` 的同链路请求通常可成功。

### 处理建议

- 兼容映射策略:
  - 首选: `developer -> system`（若目标上游支持 `system`）。
  - 兜底: `developer -> user`（在目标上游不支持 `developer/system` 时）。
- 建议通过配置控制（按 provider/模型维度），避免全局强制降级导致行为偏移。

## 2026-03-06: `extract_token_usage` 字段冲突导致 `output/input` 统计为 0（Bytecat chat-completions）

- 现象:
  - 日志中 `responseBody.usage` 已有正常值（例如 `input_tokens=3, output_tokens=633`），但 `tokenUsage` 展示为:
    - `inputTokens=0`
    - `outputTokens=0`
    - `cacheReadTokens>0`
- 典型 traceId:
  - 非流式: `5f699654-1dea-463d-93a3-ead0ca663aa3`
  - 流式: `e77e93ba-1ca2-4231-9312-aad6de7c7580`
- 调试日志位置:
  - `app_data_dir/proxy-dev-logs.jsonl`
  - 本机示例: `/home/spencer/.local/share/art.shier.aiopenrouter/proxy-dev-logs.jsonl`

### 根因

- `extract_token_usage` 会从同一语义的多个别名字段取值（如 `output_tokens` / `completion_tokens`）。
- 旧实现（`first_u64`）是“先命中先返回”，即使命中值是 `0` 也会直接返回，不会继续看后续别名。
- 在 Bytecat 的 usage 里，常见模式是:
  - `output_tokens=0`，同时 `completion_tokens>0`
  - `input_tokens=0`，同时 `prompt_tokens>0`
  - `prompt_tokens_details.cached_tokens>0`
- 因此被提取成:
  - `output_tokens=0`（未继续读取 `completion_tokens`）
  - `input_tokens=0`（未继续读取 `prompt_tokens`）
  - `cache_read_tokens>0`（来自 `cached_tokens`）

### 影响范围

- 日志样本中复现集中在 `entry=anthropic -> downstream=openai_completion` 且上游为 `https://bytecat.lamclod.cn/v1/chat/completions`。
- 非流式与流式都可能命中：
  - 非流式: 即使转换后的响应 usage 正常，也可能因为上游 usage 先命中而被记录为 `0/0 + cache`。
  - 流式: usage 通常只来自上游 SSE 事件，更容易直接出现 `0/0 + cache`。

### 取值规则建议（兼容冲突字段）

- 对同义字段按“**优先非 0**，0 仅兜底”的规则读取:
  - `output`: `output_tokens` 与 `completion_tokens` 同时存在时，优先取非 0 值。
  - `input`: `input_tokens` 与 `prompt_tokens` 同时存在时，优先取非 0 值。
- 若两者都非 0 且不一致:
  - 优先规范字段（`output_tokens` / `input_tokens`），并记录冲突调试信息。
- cache 维度保持独立:
  - `cache_read_tokens` 继续来自 `cache_read_* / cached_tokens` 等字段，不参与覆盖 `output/input`。

### 建议动作

- 在 `extract_token_usage` 中将字段读取从“first-hit”调整为“prefer-nonzero”。
- 补充单测覆盖以下 case:
  - `output_tokens=0` + `completion_tokens>0` -> 取 `completion_tokens`
  - `input_tokens=0` + `prompt_tokens>0` -> 取 `prompt_tokens`
  - `output_tokens>0` + `completion_tokens>0` 且不一致 -> 使用规范字段并记录冲突

### 修复状态

- 2026-03-06 已在本仓库实现兼容修复（未提交）：
  - 字段读取改为“优先非 0，0 仅兜底”。
  - 已补对应回归测试。

## 2026-03-06: `anthropic /v1/messages -> openai /v1/responses` 中 `AskUserQuestion` 被文本化（非 `tool_use`）

- 现象:
  - Claude 侧看到的是文本（例如 `"[Tool Call: AskUserQuestion(...)]"`），而不是可执行的 `tool_use` block。
  - 表现上可能是“有响应但没有预期工具动作/无有效输出”。
- 典型 traceId:
  - `0e99deb1-76ae-42b8-b296-bc9027b091fd`
- 调试日志位置:
  - `app_data_dir/proxy-dev-logs.jsonl`
  - 本机示例: `/home/spencer/.local/share/art.shier.aiopenrouter/proxy-dev-logs.jsonl`

### 根因

- 该链路下，代理只会把上游 **结构化** `function_call` 映射成 Claude `tool_use`。
- 本 case 中，上游返回的是普通文本/`output_text`（内容像 `[Tool Call: ...]` 的占位文本），不是结构化 `function_call` 事件。
- 因此代理按协议透传为文本，不会生成 `tool_use`。


### 强制文本转 `tool_use` 的潜在问题（详细）

- 协议语义漂移:
  - 上游若给的是 `output_text`，语义上是“模型文本输出”，代理强转为 `tool_use` 会改变原始语义。
  - 这会让下游误以为“模型明确要求执行工具”，而实际上可能只是解释、示例或计划文本。
- 误判/误触发:
  - 文本里出现类似 `[Tool Call: xxx(...)]` 不一定代表真实工具调用，可能是模型复述历史、解释策略、或输出模板。
  - 正则/宽松解析容易把普通文本误识别为工具调用，导致错误执行。
- 参数完整性与类型安全:
  - 文本中的参数可能是非严格 JSON（单引号、尾逗号、转义不完整、被截断）。
  - 即使可解析，也可能缺少必填字段、字段类型错误、枚举值非法，强转后进入执行层会报错或产生不可预期行为。
- 安全与权限风险:
  - 强转意味着“把文本变成可执行动作”，可能放大 prompt injection 或越权操作风险。
  - 若工具具备外部副作用（文件写入、网络请求、系统命令），误触发成本很高。
- 幂等与重试风险:
  - 流式场景里，片段文本可能重复、乱序或中断重放；解析器若不严谨，可能多次生成同一 `tool_use`。
  - 下游自动重试时，可能把一次文本误判变成多次真实执行。
- 多工具/并发场景歧义:
  - 文本中可能包含多个“疑似调用”，难以可靠判定顺序、边界和归属。
  - 一旦链路有并发工具，错误配对 `tool_use_id` 会造成 tool_result 关联错位。
- 可观测性与排障复杂度上升:
  - 日志中会出现“代理生成的工具调用”，与“模型原生 function_call”混杂，定位问题更困难。
  - 需要额外记录“来源=强制转/原生”与“解析置信度”，否则很难追责和回放。
- 与业务策略冲突:
  - 业务侧可能依赖“只有原生 `function_call` 才执行工具”的治理规则；强转会绕过这类保护。
  - 一些模型评估/风控逻辑会把文本输出与工具调用分开统计，强转会污染指标。
- 向后兼容风险:
  - 同一提示在不同 provider/model 上返回格式不同，启用强转后行为分叉会变大，影响可预期性。
  - 后续模型升级后文本模板微调，解析规则可能失效，导致隐性回归。

### 如果必须做强制转，建议的约束

- 默认开启，并支持显式配置关闭。
- 仅对白名单工具名生效（例如 `AskUserQuestion`）。
- 仅接受严格格式（固定前缀 + 严格 JSON + schema 校验全部通过）。
- 增加“来源标记”和“降级原因”日志字段，便于审计。
- 任一校验失败立即降级为普通文本，绝不执行。

### 本仓库兜底实现方案（最小安全门槛）

- 开关:
  - `compat.textToolCallFallbackEnabled`（默认 `true`）。
- 白名单来源:
  - 仅允许本次请求声明过的 `tools[].name` 被反解析。
  - 未声明工具名即使文本匹配也不转换。
- 严格匹配规则:
  - 允许从文本中提取片段格式: `[Tool Call: <tool_name>(<json_object>)]`。
  - 参数必须是可解析的 **JSON object**；非 JSON、半截、或非对象类型一律降级文本。
- 流式链路策略:
  - 在 `response.output_text.delta` 阶段先缓冲文本；
  - 到 `response.output_item.done(message)` 时再做一次严格判定；
  - 判定成功才输出 `tool_use`，失败按原文输出 `text`。
- 非流式链路策略:
  - 在 `output.message.content.output_text` 上按同样规则做严格判定；
  - 仅成功时转 `tool_use`，否则保持文本。
- 失败处理:
  - 任一校验失败都“保留原文本”，不触发工具执行。
- 与 ccnexus 关系:
  - ccnexus 默认不做该反解析兜底；
  - 本实现为可选增强，默认开启；如需严格对齐可显式关闭开关。

### 风险与决策

- 当前策略: 默认开启受限兜底（白名单工具 + 严格 JSON 校验），解析失败降级为普通文本并可打调试日志。
- 可选策略: 若需要与 ccnexus 完全同行为，可关闭该开关。

## 2026-03-07: Claude Code `startsWith` 运行时异常（先记录，复现后继续排查）

- 现象:
  - 客户端报错: `undefined is not an object (evaluating 'H.startsWith')`
  - 同类报错: `Cannot read properties of undefined (reading 'startsWith')`
- 参考 issue:
  - https://github.com/anthropics/claude-code/issues/11113

### 已知背景（来自 issue）

- 场景集中在 Claude Code `PreToolUse` hook 可修改 `updatedInput` 的链路。
- 当 hook 只返回“部分字段”的 `updatedInput` 时，后续执行链路可能把它当成完整替换，导致某些必需字符串字段缺失。
- 后续内部逻辑调用 `startsWith` 时读到 `undefined`，触发上述异常。

### 当前仓库侧观察结论

- 代理日志中未发现该异常字符串由 proxy 直接返回；更像客户端（Claude Code）侧运行时错误。
- 同时存在“工具调用正常执行”的相邻日志，说明问题具有会话/环境相关性，不是单一转换必现。

### 暂定处理策略（本次不改代码）

- 先记录 case，不在 proxy 侧做额外兼容改动。
- 后续若再次稳定复现，再继续排查并补充证据链。

### 复现后优先采集的信息

- 报错发生时的完整会话上下文（工具名、工具输入、是否有 hooks）。
- 对应 `traceId` 与 `proxy-dev-logs.jsonl` 终态日志（`ok/error`）。
- 若存在 `PreToolUse` hook，采集其返回体（特别是 `updatedInput` 是否只包含部分字段）。

## 2026-03-07: 上游返回 SSE 但 `Content-Type=text/plain` 导致 502

- 现象:
  - 下游收到 `502 upstream_error`，错误信息类似:
    - `Upstream returned non-JSON response: event: response.created ...`
  - 日志中可见:
    - `upstreamStatus=200`
    - `upstreamResponseHeaders.content-type=text/plain; charset=utf-8`
    - `upstream_raw` 实际是 SSE 帧（`event:/data:`）。
- 典型 traceId:
  - `ebfbf798-ee4e-48f1-a038-a348c9be7bf3`
  - `76ae2776-4d15-40b4-acc6-44d4e826519a`

### 根因

- 旧逻辑仅按响应头判定是否流式:
  - 只有 `content-type` 包含 `text/event-stream` 才走 SSE 分支。
- 当上游头部标错为 `text/plain` 时，会误入非流式 JSON 解析分支，进而报非 JSON。

### 本仓库兼容处理（已实现）

- 保留原有 `text/event-stream` 判定。
- 增加 `stream=true` 场景下的 `text/plain` SSE 嗅探兜底:
  - 当请求本身是流式，且响应头不是 `event-stream/json` 但为 `text/plain`（或空）时，进入流式分支并对首块进行 SSE 前缀探测。
  - 若首块以 `event:` / `data:`（或注释行 `:`）开头，则按 SSE 正常处理。
  - 若首块不符合 SSE 前缀，则快速失败，避免把非 SSE 文本误当流。

## 2026-03-07: `to=update_plan` / `to=shell` 指令文本泄漏（未转为可执行 tool）

- 现象:
  - 响应里出现类似:
    - `to=update_plan ... {"plan":[...]}`
    - `to=shell ... {"command":["apply_patch","*** Begin Patch ..."]}`
  - 这类内容展示为普通文本，随后 `stop_reason=end_turn`，流程暂停，不会继续执行对应工具。
- 典型 traceId:
  - `26034077-5bb0-4ff3-acce-8c78b67ba33c`
  - `fa6ef9ba-a9e7-4cee-8450-07f28bb36c0b`
  - `a38dbe79-b5f8-4c69-bc4a-e23ab67d2f80`
- 调试日志位置:
  - `app_data_dir/proxy-dev-logs.jsonl`
  - 本机示例: `/home/spencer/.local/share/art.shier.aiopenrouter/proxy-dev-logs.jsonl`

### 根因

- 该类内容来自上游 `output_text`，不是结构化 `function_call` / `tool_use`。
- 当前文本兜底只支持两类:
  - `[Tool Call: <tool_name>(<json_object>)]`
  - `{"command":["bash","-lc","..."], ...}`（仅归一到 `Bash`）
- `to=update_plan` 与 `to=shell + apply_patch` 不在现有解析规则内，因此不会转为工具调用。
- 同时受白名单约束:
  - 仅本次请求 `tools` 中声明的工具名允许被反解析。
  - 该批日志里 `tools` 有 `TaskUpdate/Bash` 等，但没有 `update_plan`、`apply_patch`。

### 影响

- 模型“有执行意图”的文本无法落地执行，表现为“说要做但没做”。
- 伪指令文本会进入会话历史，增加后续继续漂移为伪调用的概率。

### 兼容建议（先记录，后续按需实现）

- 对 `to=update_plan`:
  - 不建议直接执行为同名 tool（当前工具集中无该名）。
  - 可在安全前提下映射到 `TaskUpdate` 体系；若缺任务 `id`，则仅记录日志并丢弃该段文本。
- 对 `to=shell` + `apply_patch`:
  - 若未声明 `apply_patch`，保持 fail-closed。
  - 可选兜底: 当 `Bash` 已声明时，将 `apply_patch` 补丁文本安全改写为 `Bash` heredoc 调用（需做 patch 边界与大小校验）。
- 可观测性:
  - 增加日志字段:
    - `tool_intent_detected`
    - `tool_intent_name`
    - `tool_intent_decision`（`execute` / `rewrite` / `drop`）
    - `tool_intent_reason`
