# Agent管理功能实现计划

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 在ServicePage中添加Agent管理面板，支持管理Claude/Codex/OpenCode的多个配置目录，实现表单编辑和源文件编辑两种模式。

**Architecture:**
- 后端：扩展现有的integration_service.rs，新增读取/写入配置文件的API
- 前端：在ServicePage中添加Agent管理面板组件，支持目录选择、配置编辑、写入配置
- 数据存储：扩展现有的integration_store，支持存储Agent配置项

**Tech Stack:** Rust (Tauri) + React + TypeScript

---

## 阶段一：后端数据结构扩展

### Task 1: 扩展后端DTO - 添加AgentConfig结构体

**Files:**
- Modify: `src-tauri/src/api/dto.rs:90-98`

**Step 1: 添加AgentConfig结构体定义**

```rust
// 在 IntegrationTarget 之前添加

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentConfig {
    pub url: Option<String>,
    pub api_token: Option<String>,
    pub model: Option<String>,
    pub timeout: Option<u64>,
    // Claude行为选项
    pub always_thinking_enabled: Option<bool>,
    pub include_coauthored_by: Option<bool>,
    pub skip_dangerous_mode_permission_prompt: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationTarget {
    pub id: String,
    pub kind: IntegrationClientKind,
    pub config_dir: String,
    pub config: Option<AgentConfig>,  // 新增字段
    pub created_at: String,
    pub updated_at: String,
}
```

**Step 2: 添加读取配置文件的DTO**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentConfigFile {
    pub target_id: String,
    pub kind: IntegrationClientKind,
    pub config_dir: String,
    pub file_path: String,
    pub content: String,  // 源文件内容
    pub parsed_config: Option<AgentConfig>,  // 解析后的配置
}
```

**Step 3: 添加写入配置的请求/响应DTO**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteAgentConfigRequest {
    pub target_id: String,
    pub config: AgentConfig,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteAgentConfigResult {
    pub ok: bool,
    pub target_id: String,
    pub file_path: String,
    pub message: Option<String>,
}
```

**Step 4: Commit**

```bash
git add src-tauri/src/api/dto.rs
git commit -m "feat: add AgentConfig DTO for agent management"
```

---

### Task 2: 扩展integration_store支持AgentConfig存储

**Files:**
- Modify: `src-tauri/src/integration_store.rs`

**Step 1: 更新存储结构**

```rust
// 找到 IntegrationTarget 存储结构，添加 config 字段
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrationTargetData {
    pub id: String,
    pub kind: IntegrationClientKind,
    pub config_dir: String,
    pub config: Option<AgentConfig>,  // 新增
    pub created_at: String,
    pub updated_at: String,
}
```

**Step 2: 更新list()方法返回完整数据**

**Step 3: Commit**

```bash
git add src-tauri/src/integration_store.rs
git commit -m "feat: extend integration_store to store AgentConfig"
```

---

## 阶段二：后端API扩展

### Task 3: 新增读取Agent配置文件API

**Files:**
- Modify: `src-tauri/src/services/integration_service.rs`
- Modify: `src-tauri/src/commands/integration.rs`

**Step 1: 在integration_service.rs中添加读取配置函数**

```rust
/// 读取Agent配置文件内容
pub fn read_agent_config(
    state: &SharedState,
    target_id: &str,
) -> AppResult<AgentConfigFile> {
    let targets = state.integration_store.list();
    let target = targets
        .into_iter()
        .find(|t| t.id == target_id)
        .ok_or_else(|| AppError::not_found(format!("target not found: {}", target_id)))?;

    let config_dir = PathBuf::from(&target.config_dir);
    let (file_path, content) = match target.kind {
        IntegrationClientKind::Claude => {
            let path = config_dir.join("settings.json");
            let content = if path.exists() {
                std::fs::read_to_string(&path).unwrap_or_default()
            } else {
                "{}".to_string()
            };
            (path, content)
        }
        IntegrationClientKind::Codex => {
            let path = config_dir.join("config.toml");
            let content = if path.exists() {
                std::fs::read_to_string(&path).unwrap_or_default()
            } else {
                "".to_string()
            };
            (path, content)
        }
        IntegrationClientKind::Opencode => {
            let path = resolve_opencode_config_path(&config_dir);
            let content = if path.exists() {
                std::fs::read_to_string(&path).unwrap_or_default()
            } else {
                "{}".to_string()
            };
            (path, content)
        }
    };

    let parsed_config = parse_agent_config(&target.kind, &content).ok();

    Ok(AgentConfigFile {
        target_id: target.id,
        kind: target.kind,
        config_dir: target.config_dir,
        file_path: file_path.to_string_lossy().to_string(),
        content,
        parsed_config,
    })
}

/// 解析配置文件为AgentConfig
fn parse_agent_config(kind: &IntegrationClientKind, content: &str) -> AppResult<AgentConfig> {
    match kind {
        IntegrationClientKind::Claude => parse_claude_config(content),
        IntegrationClientKind::Opencode => parse_opencode_config(content),
        IntegrationClientKind::Codex => parse_codex_config(content),
    }
}

fn parse_claude_config(content: &str) -> AppResult<AgentConfig> {
    // 解析 settings.json
    // 提取 env.ANTHROPIC_BASE_URL, env.ANTHROPIC_AUTH_TOKEN, env.ANTHROPIC_MODEL
    // 提取 alwaysThinkingEnabled, includeCoAuthoredBy 等
    // 返回 AgentConfig
    todo!()
}

fn parse_opencode_config(content: &str) -> AppResult<AgentConfig> {
    // 解析 opencode.json
    todo!()
}

fn parse_codex_config(content: &str) -> AppResult<AgentConfig> {
    // 解析 config.toml
    todo!()
}
```

**Step 2: 在integration.rs中添加command**

```rust
#[tauri::command]
pub async fn integration_read_agent_config(
    state: State<'_, SharedState>,
    target_id: String,
) -> Result<AgentConfigFile, String> {
    integration_service::read_agent_config(&state, &target_id).map_err(|e| e.to_string())
}
```

**Step 3: 注册command到main.rs**

**Step 4: 测试编译**

```bash
cd src-tauri && cargo check
```

**Step 5: Commit**

```bash
git add src-tauri/src/services/integration_service.rs src-tauri/src/commands/integration.rs
git commit -m "feat: add API to read agent config files"
```

---

### Task 4: 新增写入Agent配置API

**Files:**
- Modify: `src-tauri/src/services/integration_service.rs`
- Modify: `src-tauri/src/commands/integration.rs`

**Step 1: 添加写入配置函数**

```rust
/// 写入Agent配置
pub fn write_agent_config(
    state: &SharedState,
    target_id: &str,
    config: AgentConfig,
) -> AppResult<WriteAgentConfigResult> {
    let targets = state.integration_store.list();
    let mut target = targets
        .into_iter()
        .find(|t| t.id == target_id)
        .ok_or_else(|| AppError::not_found(format!("target not found: {}", target_id)))?;

    let config_dir = PathBuf::from(&target.config_dir);
    let file_path = match target.kind {
        IntegrationClientKind::Claude => {
            write_claude_full_config(&config_dir, &config)?
        }
        IntegrationClientKind::Opencode => {
            write_opencode_full_config(&config_dir, &config)?
        }
        IntegrationClientKind::Codex => {
            write_codex_full_config(&config_dir, &config)?
        }
    };

    // 更新存储的配置
    target.config = Some(config);
    state.integration_store.update_target(&target_id, target.config_dir, Some(config))
        .map_err(AppError::validation)?;

    Ok(WriteAgentConfigResult {
        ok: true,
        target_id: target_id.to_string(),
        file_path: file_path.to_string_lossy().to_string(),
        message: None,
    })
}

fn write_claude_full_config(config_dir: &Path, config: &AgentConfig) -> AppResult<PathBuf> {
    let file_path = config_dir.join("settings.json");
    let mut root = read_json_like_object(&file_path)?;

    // 写入 env
    let env = ensure_child_object(&mut root, "env");
    if let Some(url) = &config.url {
        env.insert("ANTHROPIC_BASE_URL".to_string(), Value::String(url.clone()));
    }
    if let Some(token) = &config.api_token {
        env.insert("ANTHROPIC_AUTH_TOKEN".to_string(), Value::String(token.clone()));
    }
    if let Some(model) = &config.model {
        env.insert("ANTHROPIC_MODEL".to_string(), Value::String(model.clone()));
    }
    if let Some(timeout) = config.timeout {
        env.insert("API_TIMEOUT_MS".to_string(), Value::String(timeout.to_string()));
    }

    // 写入行为选项
    if let Some(enabled) = config.always_thinking_enabled {
        root.insert("alwaysThinkingEnabled".to_string(), Value::Bool(enabled));
    }
    if let Some(enabled) = config.include_coauthored_by {
        root.insert("includeCoAuthoredBy".to_string(), Value::Bool(enabled));
    }
    if let Some(enabled) = config.skip_dangerous_mode_permission_prompt {
        root.insert("skipDangerousModePermissionPrompt".to_string(), Value::Bool(enabled));
    }

    write_json_object(&file_path, &root)?;
    Ok(file_path)
}

// 类似添加 write_opencode_full_config 和 write_codex_full_config
```

**Step 2: 添加command**

```rust
#[tauri::command]
pub async fn integration_write_agent_config(
    state: State<'_, SharedState>,
    target_id: String,
    config: AgentConfig,
) -> Result<WriteAgentConfigResult, String> {
    integration_service::write_agent_config(&state, &target_id, config).map_err(|e| e.to_string())
}
```

**Step 3: 测试编译**

```bash
cd src-tauri && cargo check
```

**Step 4: Commit**

```bash
git add src-tauri/src/services/integration_service.rs src-tauri/src/commands/integration.rs
git commit -m "feat: add API to write agent config files"
```

---

### Task 5: 新增更新target配置的API

**Files:**
- Modify: `src-tauri/src/services/integration_service.rs`
- Modify: `src-tauri/src/commands/integration.rs`

**Step 1: 更新update_target函数签名**

```rust
pub fn update_target(
    state: &SharedState,
    target_id: &str,
    config_dir: String,
    config: Option<AgentConfig>,  // 新增参数
) -> AppResult<IntegrationTarget> {
    // 更新存储
}
```

**Step 2: 更新command**

```rust
#[tauri::command]
pub async fn integration_update_target_config(
    state: State<'_, SharedState>,
    target_id: String,
    config_dir: String,
    config: Option<AgentConfig>,
) -> Result<IntegrationTarget, String> {
    integration_service::update_target(&state, &target_id, config_dir, config)
        .map_err(|e| e.to_string())
}
```

**Step 3: Commit**

```bash
git add src-tauri/src/
git commit -m "feat: add API to update target config"
```

---

## 阶段三：前端类型和i18n

### Task 6: 扩展前端类型定义

**Files:**
- Modify: `src/renderer/types/index.ts:233-239`

**Step 1: 添加AgentConfig类型**

```typescript
export interface AgentConfig {
  url?: string
  apiToken?: string
  model?: string
  timeout?: number
  alwaysThinkingEnabled?: boolean
  includeCoAuthoredBy?: boolean
  skipDangerousModePermissionPrompt?: boolean
}

export interface IntegrationTarget {
  id: string
  kind: IntegrationClientKind
  configDir: string
  config?: AgentConfig  // 新增
  createdAt: string
  updatedAt: string
}

export interface AgentConfigFile {
  targetId: string
  kind: IntegrationClientKind
  configDir: string
  filePath: string
  content: string
  parsedConfig?: AgentConfig
}

export interface WriteAgentConfigResult {
  ok: boolean
  targetId: string
  filePath: string
  message?: string
}
```

**Step 2: Commit**

```bash
git add src/renderer/types/index.ts
git commit -m "feat: add AgentConfig types for frontend"
```

---

### Task 7: 扩展i18n支持

**Files:**
- Modify: `src/renderer/i18n/en-US.ts`
- Modify: `src/renderer/i18n/zh-CN.ts`

**Step 1: 添加i18n keys**

```typescript
// en-US.ts
agentManagement: {
  title: "Agent Management",
  selectType: "Select Agent Type",
  configured: "configured",
  notConfigured: "not configured",
  configDir: "Configuration Directory",
  addConfigDir: "Add Configuration Directory",
  editConfig: "Edit Configuration",
  deleteConfig: "Delete Configuration",
  writeConfig: "Write Configuration",
  formEditor: "Form Editor",
  sourceEditor: "Source File",
  save: "Save",
  cancel: "Cancel",
  url: "URL Address",
  apiToken: "API Token",
  model: "Default Model",
  timeout: "Timeout (ms)",
  alwaysThinkingEnabled: "Enable Thinking Mode",
  includeCoAuthoredBy: "Include Co-Authored-By",
  skipDangerousModePermissionPrompt: "Skip Dangerous Mode Permission",
  changeDir: "Change Directory",
  presetDirs: "Preset Directories",
  nextConfig: "Next: Configure Agent",
},
```

**Step 2: Commit**

```bash
git add src/renderer/i18n/
git commit -m "feat: add i18n for agent management"
```

---

## 阶段四：前端UI组件

### Task 8: 创建Agent管理面板组件

**Files:**
- Create: `src/renderer/components/AgentManagementPanel/AgentManagementPanel.tsx`
- Create: `src/renderer/components/AgentManagementPanel/AgentManagementPanel.module.css`
- Create: `src/renderer/components/AgentManagementPanel/index.ts`

**Step 1: 创建组件结构**

```typescript
import React, { useState, useEffect } from "react"
import { shallow } from "zustand/shallow"
import { useTranslation } from "@/hooks"
import { useProxyStore } from "@/store"
import type { IntegrationClientKind, IntegrationTarget, AgentConfig, AgentConfigFile } from "@/types"
import { ipc } from "@/utils/ipc"
import styles from "./AgentManagementPanel.module.css"

type Step = "selectType" | "manageDirs" | "addDir" | "editConfig"

export const AgentManagementPanel: React.FC = () => {
  const { t } = useTranslation()
  const [step, setStep] = useState<Step>("selectType")
  const [targets, setTargets] = useState<IntegrationTarget[]>([])
  const [selectedKind, setSelectedKind] = useState<IntegrationClientKind | null>(null)
  const [selectedTarget, setSelectedTarget] = useState<IntegrationTarget | null>(null)
  const [configFile, setConfigFile] = useState<AgentConfigFile | null>(null)
  const [editMode, setEditMode] = useState<"form" | "source">("form")

  // 加载targets
  useEffect(() => {
    loadTargets()
  }, [])

  const loadTargets = async () => {
    const result = await ipc("integration-list-targets")
    setTargets(result)
  }

  // ... 更多状态和方法
}
```

**Step 2: 实现选择Agent类型界面**

```typescript
const renderSelectType = () => (
  <div className={styles.selectType}>
    <h3>{t("agentManagement.selectType")}</h3>
    <div className={styles.agentCards}>
      {agentTypes.map(kind => {
        const count = targets.filter(t => t.kind === kind).length
        return (
          <div key={kind} className={styles.agentCard} onClick={() => handleSelectKind(kind)}>
            <span className={styles.agentIcon}>{getAgentIcon(kind)}</span>
            <span className={styles.agentName}>{getAgentName(kind)}</span>
            <span className={styles.agentStatus}>
              {count > 0 ? `${count} ${t("agentManagement.configured")}` : t("agentManagement.notConfigured")}
            </span>
          </div>
        )
      })}
    </div>
  </div>
)
```

**Step 3: 实现配置目录管理界面**

```typescript
const renderManageDirs = () => {
  const kindTargets = targets.filter(t => t.kind === selectedKind)
  return (
    <div className={styles.manageDirs}>
      <div className={styles.dirList}>
        {kindTargets.map(target => (
          <div key={target.id} className={styles.dirItem}>
            <span className={styles.dirPath}>{target.configDir}</span>
            <div className={styles.dirActions}>
              <Button size="small" onClick={() => handleEdit(target)}>{t("agentManagement.editConfig")}</Button>
              <Button size="small" variant="danger" onClick={() => handleDelete(target.id)}>{t("common.delete")}</Button>
              <Button size="small" onClick={() => handleWrite(target.id)}>{t("agentManagement.writeConfig")}</Button>
            </div>
          </div>
        ))}
      </div>
      <Button onClick={() => setStep("addDir")}>{t("agentManagement.addConfigDir")}</Button>
    </div>
  )
}
```

**Step 4: 实现配置编辑界面（表单模式）**

```typescript
const renderFormEditor = () => {
  const [formData, setFormData] = useState<AgentConfig>({
    url: configFile?.parsedConfig?.url || "",
    apiToken: configFile?.parsedConfig?.apiToken || "",
    model: configFile?.parsedConfig?.model || "",
    timeout: configFile?.parsedConfig?.timeout || 300000,
    alwaysThinkingEnabled: configFile?.parsedConfig?.alwaysThinkingEnabled || false,
    includeCoAuthoredBy: configFile?.parsedConfig?.includeCoAuthoredBy || false,
    skipDangerousModePermissionPrompt: configFile?.parsedConfig?.skipDangerousModePermissionPrompt || false,
  })

  return (
    <div className={styles.formEditor}>
      <Input label={t("agentManagement.url")} value={formData.url} onChange={v => setFormData({...formData, url: v})} />
      <Input label={t("agentManagement.apiToken")} type="password" value={formData.apiToken} onChange={v => setFormData({...formData, apiToken: v})} />
      <Input label={t("agentManagement.model")} value={formData.model} onChange={v => setFormData({...formData, model: v})} />
      <Input label={t("agentManagement.timeout")} type="number" value={formData.timeout} onChange={v => setFormData({...formData, timeout: Number(v)})} />

      {selectedKind === "claude" && (
        <>
          <Checkbox label={t("agentManagement.alwaysThinkingEnabled")} checked={formData.alwaysThinkingEnabled} onChange={v => setFormData({...formData, alwaysThinkingEnabled: v})} />
          <Checkbox label={t("agentManagement.includeCoAuthoredBy")} checked={formData.includeCoAuthoredBy} onChange={v => setFormData({...formData, includeCoAuthoredBy: v})} />
          <Checkbox label={t("agentManagement.skipDangerousModePermissionPrompt")} checked={formData.skipDangerousModePermissionPrompt} onChange={v => setFormData({...formData, skipDangerousModePermissionPrompt: v})} />
        </>
      )}

      <div className={styles.actions}>
        <Button variant="primary" onClick={() => handleSave(formData)}>{t("agentManagement.save")}</Button>
        <Button onClick={() => setStep("manageDirs")}>{t("agentManagement.cancel")}</Button>
      </div>
    </div>
  )
}
```

**Step 5: 实现源文件编辑模式**

```typescript
const renderSourceEditor = () => (
  <div className={styles.sourceEditor}>
    <textarea
      value={configFile?.content || ""}
      onChange={e => setConfigFile(prev => prev ? {...prev, content: e.target.value} : null)}
      className={styles.sourceTextarea}
    />
    <div className={styles.actions}>
      <Button variant="primary" onClick={handleSaveSource}>{t("agentManagement.save")}</Button>
      <Button onClick={() => setStep("manageDirs")}>{t("agentManagement.cancel")}</Button>
    </div>
  </div>
)
```

**Step 6: Commit**

```bash
git add src/renderer/components/AgentManagementPanel/
git commit -m "feat: add AgentManagementPanel component"
```

---

### Task 9: 在ServicePage中集成Agent管理面板

**Files:**
- Modify: `src/renderer/pages/ServicePage/ServicePage.tsx`
- Modify: `src/renderer/pages/ServicePage/ServicePage.module.css`

**Step 1: 导入组件**

```typescript
import { AgentManagementPanel } from "@/components/AgentManagementPanel"
```

**Step 2: 添加状态**

```typescript
const [showAgentManagement, setShowAgentManagement] = useState(false)
```

**Step 3: 添加按钮到导航栏**

```typescript
<Button
  variant={showAgentManagement ? "primary" : "default"}
  onClick={() => setShowAgentManagement(!showAgentManagement)}
>
  {t("agentManagement.title")}
</Button>
```

**Step 4: 条件渲染面板**

```typescript
{showAgentManagement && (
  <AgentManagementPanel onClose={() => setShowAgentManagement(false)} />
)}
```

**Step 5: Commit**

```bash
git add src/renderer/pages/ServicePage/
git commit -m "feat: integrate AgentManagementPanel in ServicePage"
```

---

## 阶段五：IPC通道注册

### Task 10: 注册前端IPC通道

**Files:**
- Modify: `src/renderer/utils/ipc.ts`

**Step 1: 添加IPC调用**

```typescript
"integration-read-agent-config": (targetId: string) => ipcRenderer.invoke("integration-read-agent-config", targetId),
"integration-write-agent-config": (targetId: string, config: AgentConfig) => ipcRenderer.invoke("integration-write-agent-config", targetId, config),
"integration-update-target-config": (targetId: string, configDir: string, config?: AgentConfig) => ipcRenderer.invoke("integration-update-target-config", targetId, configDir, config),
```

**Step 2: Commit**

```bash
git add src/renderer/utils/ipc.ts
git commit -m "feat: add IPC channels for agent management"
```

---

## 测试计划

### Task 11: 端到端测试

**Step 1: 测试添加配置目录**
- 打开Agent管理面板
- 选择Agent类型
- 添加新的配置目录
- 验证目录出现在列表中

**Step 2: 测试表单编辑**
- 选择已配置的目录
- 修改配置项
- 保存配置
- 验证配置正确保存

**Step 3: 测试源文件编辑**
- 切换到源文件编辑模式
- 修改JSON/TOML内容
- 保存并验证

**Step 4: 测试写入配置**
- 点击"写入配置"按钮
- 验证agent配置文件中内容正确

---

## 实现顺序建议

1. Task 1-2: 后端数据结构
2. Task 3: 读取配置API
3. Task 4: 写入配置API
4. Task 5: 更新配置API
5. Task 6-7: 前端类型和i18n
6. Task 8: Agent管理面板组件
7. Task 9: 集成到ServicePage
8. Task 10: IPC通道
9. Task 11: 测试

---

**Plan saved to:** `docs/plans/2026-03-08-agent-management-implementation.md`
