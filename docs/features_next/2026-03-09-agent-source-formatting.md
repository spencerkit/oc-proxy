# Agent 源文件编辑器格式化功能

**记录日期**: 2026-03-09  
**状态**: 待规划，暂不实现

## 背景

当前 Agent 编辑页的源文件模式已经支持：

- 读取真实配置文件内容
- 在编辑器中修改原始内容
- 保存时直接写回目标配置文件
- 保存前做 JSON / JSONC / TOML 基础解析校验

但当前缺少“格式化”能力。用户在源文件模式手动编辑后，无法一键整理缩进、空行和结构顺序。

## 目标

在 Agent 编辑页的源文件模式中增加“格式化”功能，提升手工编辑体验，但不改变现有保存语义。

## 推荐方案

推荐做法：

- 仅在源文件模式下显示“格式化”按钮
- 点击“格式化”后，只更新编辑器中的文本内容
- “格式化”不自动保存到磁盘
- 用户仍需手动点击“保存”
- 格式化逻辑统一放在 Tauri 后端
- 不引入 Node 工具链，不依赖 `prettier` / `biome`

推荐支持范围：

- `Claude settings.json`：支持
- `Codex config.toml`：支持
- `OpenCode opencode.json`：支持
- `OpenCode opencode.jsonc`：`v1` 暂不支持

## 功能点

### 1. 前端交互

- 在源文件模式操作区新增 `格式化` 按钮
- 按钮点击后调用新的 IPC 接口获取格式化后的文本
- 成功后直接覆盖当前编辑器内容
- 若格式化失败，显示错误提示，不修改当前编辑器内容
- 若当前内容为空，允许保持空文本，不强制注入模板

### 2. 保存语义

- “格式化”不等于“保存”
- 格式化后应保留未保存状态，继续显示 dirty 状态
- 用户点击“保存”后，才真正写入目标文件

### 3. 各配置格式策略

- `settings.json`
  - 使用现有 JSON 解析
  - 使用 pretty print 输出
- `config.toml`
  - 使用现有 TOML 解析
  - 使用 `toml_edit` 重新输出
- `opencode.json`
  - 与普通 JSON 相同
- `opencode.jsonc`
  - `v1` 建议不提供格式化按钮，或点击后明确提示“当前格式暂不支持安全格式化”

## 不建议在 v1 做的内容

- 自动保存
- 失焦自动格式化
- 格式化后自动改动字段顺序的高级规则
- 保留 `jsonc` 注释并同时做稳定格式化
- 引入外部 CLI 格式化器

## 技术复杂度评估

| 范围 | 复杂度 | 说明 |
|------|--------|------|
| 仅 `settings.json` | 低 | 现有 `serde_json::to_string_pretty` 即可 |
| `settings.json + opencode.json` | 低 | 仍然只依赖现有 JSON 能力 |
| 再加 `config.toml` | 低到中 | `toml_edit` 可用，但格式化效果不一定像专用 formatter 那样强 |
| 支持 `opencode.jsonc` 且允许注释丢失 | 中 | 可以解析后重写，但行为侵入性较强 |
| 支持 `opencode.jsonc` 且要求保留注释 | 中高 | 需要额外引入 JSONC 专用 formatter 或复杂 AST 处理 |
| 引入 `prettier` / `biome` / 外部格式化 CLI | 中高 | 工具链更重，桌面环境依赖更复杂 |

## 推荐复杂度结论

推荐版本的总体复杂度为：**低到中**

原因：

- 前端只新增一个按钮和一次 IPC 调用
- 后端已有按类型解析文件内容的能力
- JSON pretty print 已经存在
- TOML 也已有现成解析和输出能力
- 真正高风险的是 `jsonc` 注释保留，这部分在推荐方案中明确延后

## 影响面

### 前端

- `src/renderer/pages/AgentEditPage/AgentEditPage.tsx`
- `src/renderer/pages/AgentEditPage/AgentEditPage.module.css`
- `src/renderer/utils/ipc.ts`
- `src/renderer/i18n/zh-CN.ts`
- `src/renderer/i18n/en-US.ts`

### Tauri / 后端

- `src-tauri/src/commands/integration.rs`
- `src-tauri/src/commands/mod.rs`
- `src-tauri/src/main.rs`
- `src-tauri/src/services/integration_service.rs`

## 可复用的现有能力

当前已有的可复用基础：

- 读取原始文件内容
- JSON / JSONC 解析
- TOML 解析
- 源文件写回
- JSON pretty print

这意味着新增“格式化”功能时，不需要从零搭建文件处理链路。

## 主要风险

### 1. JSONC 注释丢失

`jsonc` 当前只具备“解析为数据结构”的能力，若重新序列化，大概率会丢失：

- 注释
- 尾逗号风格
- 原始排版

因此该格式不建议在 `v1` 中启用安全格式化。

### 2. TOML 输出风格不完全可控

`toml_edit` 更偏向结构编辑，不是强语义 formatter。它能输出合法 TOML，但格式是否完全符合预期，需要单独验收。

### 3. Dirty 状态一致性

格式化后文本变化但尚未保存，因此前端需要明确保持：

- “有未保存改动”
- 保存按钮可点击
- 切换模式后不丢失内容

## 实现建议顺序

### Phase 1

- 增加后端格式化接口
- 支持 JSON 和 TOML
- 前端增加格式化按钮
- 格式化后只回填编辑器，不自动保存

### Phase 2

- 评估 `jsonc` 是否允许注释丢失
- 若不允许，继续调研 JSONC 专用 formatter

## 最终建议

若未来要实现，建议直接按以下边界执行：

- 做源文件模式的显式“格式化”按钮
- 不自动保存
- 先支持 `settings.json`、`config.toml`、`opencode.json`
- 暂不支持 `opencode.jsonc` 的安全格式化

这套方案依赖最轻、行为最稳、风险最可控。
