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
