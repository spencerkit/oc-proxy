# Agent管理功能设计文档

**日期**: 2026-03-08
**功能**: Agent配置管理（Claude/Codex/OpenCode）

---

## 1. 功能概述

在ServicePage中添加Agent管理面板，支持：
- 管理多个Agent类型（Claude/Codex/OpenCode）
- 每个Agent支持多个配置目录
- 表单编辑和源文件编辑两种模式
- 基础连接配置 + Claude行为选项

---

## 2. 支持的配置项

### 2.1 基础连接配置（所有Agent）

| Agent | URL | API Key/Token | 默认模型 | 超时时间 |
|-------|-----|---------------|----------|----------|
| Claude Code | ANTHROPIC_BASE_URL | ANTHROPIC_AUTH_TOKEN | ANTHROPIC_MODEL | API_TIMEOUT_MS |
| OpenCode | baseURL | (via provider) | model | timeout |
| Codex | base_url | api_key | model | - |

### 2.2 Claude行为选项

- `alwaysThinkingEnabled` - 启用思考模式
- `includeCoAuthoredBy` - 提交时包含Co-Authored-By
- `skipDangerousModePermissionPrompt` - 跳过危险模式提示

---

## 3. UI/UX设计

### 3.1 入口

在ServicePage顶部导航栏添加"Agent管理"按钮，点击后展开Agent管理面板。

### 3.2 页面结构

**第一步：选择Agent类型**
- 显示3个Agent卡片（Claude/Codex/OpenCode）
- 每个卡片显示配置数量状态

**第二步：配置目录管理**
- 显示已添加的配置目录列表
- 每个目录显示：路径、当前URL配置状态
- 支持：编辑、删除、写入配置

**第三步：新增配置目录**
- 目录选择器（支持预设目录）
- 支持手动输入路径

**第四步：配置编辑**
- Tab切换：表单编辑 / 源文件编辑
- 保存按钮：保存到应用存储
- 取消按钮：放弃修改
- 写入配置按钮：写入到agent配置文件

---

## 4. 数据模型

### IntegrationTarget (现有)

```typescript
interface IntegrationTarget {
  id: string
  kind: "claude" | "codex" | "opencode"
  configDir: string
  // 新增：存储配置项
  config?: AgentConfig
}

interface AgentConfig {
  url?: string
  apiToken?: string
  model?: string
  timeout?: number
  // Claude行为选项
  alwaysThinkingEnabled?: boolean
  includeCoAuthoredBy?: boolean
  skipDangerousModePermissionPrompt?: boolean
}
```

---

## 5. 技术实现要点

### 5.1 后端 (Rust)

- 扩展 `integration_service.rs` 支持读取/写入更多配置项
- 新增读取配置文件的API
- 支持JSON和TOML格式解析

### 5.2 前端 (React)

- 在ServicePage中添加Agent管理面板组件
- 使用现有Modal或创建新组件
- 表单验证和状态管理

### 5.3 文件操作

| Agent | 配置文件 | 格式 |
|-------|----------|------|
| Claude | settings.json | JSON |
| OpenCode | opencode.json(c) | JSON/JSONC |
| Codex | config.toml | TOML |

---

## 6. 按钮功能说明

| 按钮 | 功能 |
|------|------|
| 保存 | 将配置保存到应用内部存储（integration_store） |
| 写入配置 | 将当前group的入口URL写入到agent配置文件中 |
| 取消 | 放弃当前修改 |
| 编辑 | 进入配置编辑界面 |
| 删除 | 删除配置目录记录 |

---

## 7. 现有功能整合

- 现有的"写入配置"功能保持不变
- Agent管理面板作为补充，提供更完整的配置管理
- 两者可独立使用
