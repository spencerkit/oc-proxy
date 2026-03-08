# 会话问题汇总（2026-03-07）

## 文档说明
- 目的: 汇总本次会话中遇到的核心问题，集中记录原因、影响、修复情况与剩余风险。
- 与 `docs/problems.md` 的关系: 本文为单独会话汇总，不与常驻问题库混写。
- 关联日志:
  - `/home/spencer/.local/share/art.shier.aiopenrouter/proxy-dev-logs.jsonl`
  - 调试导出: `/home/spencer/workspace/oc-proxy/debug-dumps/`

## 状态定义
- 已修复: 已有代码修复，并有测试或日志证据。
- 部分修复: 已有兜底/缓解，但仍存在边界场景。
- 未修复: 已定位原因，暂无代码修复落地。

## 问题总览
| ID | 问题 | 转发链路（from -> to） | 问题阶段 | 影响 | 状态 |
|---|---|---|---|---|---|
| S01 | `developer` 角色导致上游 400 | OpenAI `/responses` -> OpenAI `/chat/completions`（GLM provider） | 请求侧（请求转换） | 请求失败 | 已修复 |
| S02 | usage 映射把输出 token 记成 0（缓存 token 非 0） | OpenAI `/chat/completions`（含流式）-> 代理统一 usage | 响应侧（usage 提取/归一） | 额度统计错误 | 已修复 |
| S03 | `function_call_output` 被上游判定非法类型 | Anthropic `/v1/messages` -> OpenAI `/v1/responses` | 请求侧（请求转换） | 请求 400 | 已修复 |
| S04 | `stream=null` 进入非流式路径，SSE 被按 JSON 解析报错 | Anthropic `/v1/messages` -> OpenAI `/v1/responses` | 请求侧（stream 默认值） | 502/中断 | 已修复 |
| S05 | 上游返回 SSE 但 `Content-Type=text/plain` 导致误判 | Anthropic `/v1/messages` -> OpenAI `/v1/responses` | 响应侧（响应分流） | 502/中断 | 已修复 |
| S06 | `AskUserQuestion`/工具意图被文本化，不触发真实 tool_use | OpenAI `/v1/responses` -> Anthropic `/v1/messages` | 响应侧（响应转换） | 工具链暂停 | 部分修复 |
| S07 | `to=update_plan ...` 作为文本泄漏 | OpenAI `/v1/responses` -> Anthropic `/v1/messages` | 响应侧（文本兜底未覆盖） | 计划指令不执行，污染上下文 | 未修复 |
| S08 | `to=shell ... {"command":["apply_patch", ...]}` 文本泄漏 | OpenAI `/v1/responses` -> Anthropic `/v1/messages` | 响应侧（文本兜底未覆盖） | 补丁意图不执行 | 未修复 |
| S09 | `undefined ... startsWith`（Claude Code hooks 场景） | Claude Code 客户端内部（hook -> runtime） | 客户端侧（非代理链路） | 会话中断 | 未修复（外部问题） |

---

## S01: `developer` 角色导致上游 400
### 链路与阶段
- 转发链路: OpenAI `/responses` -> OpenAI `/chat/completions`（GLM provider）。
- 问题阶段: 请求侧（代理构造上游请求时）。

### 原因
- 部分上游（本次 GLM）不接受 `developer` 角色，仅接受 `system/user/assistant/tool`。
- 转换后消息仍带 `developer`，上游返回 400。

### 影响
- 含 `developer` 消息的请求在该 provider 上稳定失败。

### 修复情况
- 已落地角色降级兼容（`developer -> system/user` 按能力降级）。
- 相关提交:
  - `0267d73`
  - `30ed8be`

### 证据
- 问题记录见 `docs/problems.md` 对应 GLM 条目。

---

## S02: usage 映射把输出 token 记成 0
### 链路与阶段
- 转发链路: OpenAI `/chat/completions`（含 stream）-> 代理 usage 统一抽取与展示。
- 问题阶段: 响应侧（读取上游 usage 字段时）。

### 原因
- 旧逻辑采用“先命中先返回”，遇到 `output_tokens=0` 时不会继续读取 `completion_tokens`。
- 同类问题也会影响 `input_tokens` 与 `prompt_tokens`。

### 影响
- 展示层出现 `input/output=0`，仅缓存 token 非 0，额度统计失真。

### 修复情况
- 已改为“优先非 0，0 仅兜底”的兼容读取策略。
- 相关提交:
  - `01a709f`
- 已补测试覆盖冲突字段场景。

### 证据
- 代码与测试在 `src-tauri/src/transformer/` 下已更新。

---

## S03: `function_call_output` 被上游判定非法类型
### 链路与阶段
- 转发链路: Anthropic `/v1/messages` -> OpenAI `/v1/responses`。
- 问题阶段: 请求侧（messages 转 responses 的输入项构造）。

### 原因
- 目标 responses 接口对消息 `content.type` 有白名单限制，不接受该链路里的 `function_call_output` 直接透传。
- 直接透传会触发上游 400（Invalid value）。

### 影响
- 工具结果回传阶段请求失败，后续链路中断。

### 修复情况
- 已将对应内容转换为上游可接受的文本输入类型（如 `input_text`）。
- 相关实现/测试位置:
  - `src-tauri/src/transformer/convert/claude_openai_responses.rs`
  - `src-tauri/src/transformer/tests.rs`（含 `maps_tool_result_to_input_text`）

### 证据
- 关键搜索词: `function_call_output`, `input_text`。

---

## S04: `stream=null` 误入非流式路径
### 链路与阶段
- 转发链路: Anthropic `/v1/messages` -> OpenAI `/v1/responses`。
- 问题阶段: 请求侧（`stream` 默认值决策影响后续响应处理路径）。

### 原因
- 早期逻辑中 `stream` 缺失/为 `null` 时默认等价 `false`，导致走非流式 JSON 分支。
- 当上游实际返回 SSE 时，会被误判为非 JSON 响应。

### 影响
- 下游出现 `upstream_error` / 502，流式对话中断。

### 修复情况
- 已调整该链路默认行为: `stream` 缺失或 `null` 默认按 `true` 处理。
- 对应测试已补:
  - `test_claude_messages_to_responses_defaults_stream_to_true_when_missing_or_null`

### 证据
- 代码位置:
  - `src-tauri/src/transformer/convert/claude_openai_responses.rs`
  - `src-tauri/src/transformer/tests.rs`

---

## S05: SSE + `Content-Type=text/plain` 误判
### 链路与阶段
- 转发链路: Anthropic `/v1/messages` -> OpenAI `/v1/responses`。
- 问题阶段: 响应侧（按响应头分流时误判）。

### 原因
- 旧逻辑仅凭 `content-type=text/event-stream` 判断流式。
- 上游有头部标错为 `text/plain` 的情况，但 body 实际是 SSE。

### 影响
- 被按非流式 JSON 解析，触发 502。

### 修复情况
- 已增加 SSE 嗅探兜底:
  - `stream=true` 且响应头可疑时，检测首块是否为 `event:`/`data:` 前缀。
- 相关提交:
  - `f075e30`
- 关键实现位置:
  - `src-tauri/src/proxy/pipeline.rs`

### 证据
- 历史 trace 中可见 `upstreamStatus=200` 但代理报 502 的场景，修复后通过嗅探分流。

---

## S06: 工具意图被文本化（`AskUserQuestion` 等）
### 链路与阶段
- 转发链路: OpenAI `/v1/responses` -> Anthropic `/v1/messages`。
- 问题阶段: 响应侧（responses 输出项转 claude content block 时）。

### 原因
- 上游返回 `output_text`（文本形式的工具意图）而非结构化 `function_call/tool_use`。
- 代理默认只对结构化调用做工具转换。

### 影响
- 模型“说要调工具”但未真正触发，流程停在文本输出。

### 修复情况
- 已加文本兜底解析（受白名单约束），支持:
  - `[Tool Call: <name>({...})]`
  - `{"command":["bash","-lc","..."], ...}`（Bash 归一）
- 默认开启开关:
  - `compat.text_tool_call_fallback_enabled=true`
- 相关提交:
  - `f075e30`

### 当前剩余风险
- 兜底仍非全格式覆盖；超出已识别格式时仍会回落为文本。

---

## S07: `to=update_plan ...` 文本泄漏
### 链路与阶段
- 转发链路: OpenAI `/v1/responses` -> Anthropic `/v1/messages`。
- 问题阶段: 响应侧（文本工具兜底解析阶段）。

### 原因
- 该格式是内部计划指令风格，但在本链路里并非有效的结构化工具调用。
- 当前请求 `tools` 中也无 `update_plan` 工具名。
- 解析器不识别 `to=update_plan` 语法。

### 影响
- 指令不执行，返回 `end_turn`。
- 指令文本进入会话历史，增加后续漂移概率。

### 修复情况
- 未修复（仅记录问题与方案）。

### 首次污染记录（已固化）
- 首次出现在响应侧:
  - `traceId`: `26034077-5bb0-4ff3-acce-8c78b67ba33c`
  - `timestamp`: `2026-03-06T17:41:03.154663387+00:00`（北京时间 `2026-03-07 01:41:03`）
  - 证据: 首次 `responseBody.payload` 出现 `to=update_plan ...`，当轮 `forwardRequestBody` 尚不含该字符串。
- 对应导出:
  - `/home/spencer/workspace/oc-proxy/debug-dumps/26034077-5bb0-4ff3-acce-8c78b67ba33c.requestBody.json`
  - `/home/spencer/workspace/oc-proxy/debug-dumps/26034077-5bb0-4ff3-acce-8c78b67ba33c.forwardRequestBody.json`
  - `/home/spencer/workspace/oc-proxy/debug-dumps/26034077-5bb0-4ff3-acce-8c78b67ba33c.responseBody.json`

### 为什么没走 `TaskUpdate`（证据 + 推断）
- 已确认事实:
  - 当轮 `tools` 中存在 `TaskUpdate`。
  - 当轮 `tools` 中不存在 `update_plan`。
  - 模型输出为 `content_block_delta.text`（文本），不是结构化 `tool_use/function_call`。
- 可以直接解释的原因:
  - 文本不会触发工具执行，必须是结构化调用才会执行。
  - 当前兜底解析不识别 `to=update_plan` 语法，所以不会转成工具调用。
- 推断性原因（高概率）:
  - `TaskUpdate` 的 schema 要求 `taskId` 必填；该文本仅给了步骤列表，没有 task id，模型未走 `TaskList/TaskGet -> TaskUpdate` 规范路径。
  - 一旦首轮输出了 `to=update_plan` 文本，后续会话会把该文本带入上下文，形成格式漂移并持续复现。

### 建议方案
- 不直接执行同名工具。
- 可在严格条件下映射到 `TaskUpdate` 体系；缺 task id 则 fail-closed（丢弃或仅记录日志）。

### 证据
- 典型 trace:
  - `26034077-5bb0-4ff3-acce-8c78b67ba33c`
  - `fa6ef9ba-a9e7-4cee-8450-07f28bb36c0b`
  - `ad0c2b01-e712-4e02-8d08-992093e3182c`

---

## S08: `to=shell ... apply_patch` 文本泄漏
### 链路与阶段
- 转发链路: OpenAI `/v1/responses` -> Anthropic `/v1/messages`。
- 问题阶段: 响应侧（文本工具兜底解析阶段）。

### 原因
- 该格式不是当前兜底支持的严格模板。
- 命令数组分支仅支持 shell 程序（bash/sh/zsh）归一到 `Bash`。
- 本次请求工具白名单中无 `apply_patch`，即使识别到意图也不可执行。

### 影响
- 补丁执行意图无法落地，表现为“说要改但没执行”。

### 修复情况
- 未修复（仅记录问题与方案）。

### 首次污染记录（已固化）
- 首次出现在响应侧:
  - `traceId`: `a38dbe79-b5f8-4c69-bc4a-e23ab67d2f80`
  - `timestamp`: `2026-03-06T17:42:37.752754719+00:00`（北京时间 `2026-03-07 01:42:37`）
  - 证据: `responseBody.payload` 出现 `to=shell ... {"command":["apply_patch", ...]}`。
- 对应导出:
  - `/home/spencer/workspace/oc-proxy/debug-dumps/a38dbe79-b5f8-4c69-bc4a-e23ab67d2f80.requestBody.json`
  - `/home/spencer/workspace/oc-proxy/debug-dumps/a38dbe79-b5f8-4c69-bc4a-e23ab67d2f80.forwardRequestBody.json`
  - `/home/spencer/workspace/oc-proxy/debug-dumps/a38dbe79-b5f8-4c69-bc4a-e23ab67d2f80.responseBody.json`

### 建议方案
- 保持 fail-closed。
- 可选兼容: 在 `Bash` 已声明时，将 `apply_patch` 文本安全改写为 Bash heredoc 执行；需加 patch 边界与大小校验。

### 证据
- 典型 trace:
  - `a38dbe79-b5f8-4c69-bc4a-e23ab67d2f80`
- 关联导出:
  - `/home/spencer/workspace/oc-proxy/debug-dumps/ad0c2b01-e712-4e02-8d08-992093e3182c.responseBody.json`

---

## S09: `startsWith` 运行时异常（Claude Code hooks）
### 链路与阶段
- 转发链路: Claude Code 客户端内部 hook 处理链，不在 proxy 转发主链路内。
- 问题阶段: 客户端侧（hook 返回处理/字段读取）。

### 原因
- 属于客户端/hook 场景问题（外部 issue），典型触发与 hook 返回结构不完整相关。
- 当前证据不足以归因到 proxy 转换链路。

### 影响
- 会话/工具调用流程中断。

### 修复情况
- 未在本仓库修复（先记录，待可复现再深入）。
- 参考外部 issue:
  - `https://github.com/anthropics/claude-code/issues/11113`

---

## 本次会话产出与后续建议
### 已产出
- 新增本会话汇总文档（本文）。
- 保留了单条 trace 的请求/转换后请求/响应体导出:
  - `/home/spencer/workspace/oc-proxy/debug-dumps/ad0c2b01-e712-4e02-8d08-992093e3182c.requestBody.json`
  - `/home/spencer/workspace/oc-proxy/debug-dumps/ad0c2b01-e712-4e02-8d08-992093e3182c.forwardRequestBody.json`
  - `/home/spencer/workspace/oc-proxy/debug-dumps/ad0c2b01-e712-4e02-8d08-992093e3182c.responseBody.json`

### 建议优先级
1. 先处理 S07/S08（文本指令漂移）以降低“假工具调用”导致的流程暂停。
2. 为 S07/S08 增加可观测字段（意图识别、决策、原因），便于后续精确回放。
3. 回归验证 S04/S05 在 `stream=null`、`text/plain + SSE`、正常 JSON 三类路径下的稳定性。
