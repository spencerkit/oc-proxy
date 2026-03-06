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
