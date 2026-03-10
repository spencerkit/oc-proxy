# 全局 Providers 改造规划（新增 Providers Tab）

**记录日期**: 2026-03-10  
**状态**: 待实现，已完成交互与原型规划

## 需求拆解

你提出的目标可以拆成 3 件事：

1. Provider 从“每个 Group 内独立维护”改为“全局统一维护”。
2. 顶部一级 Tab（服务 / 设置 / Agent / 日志）新增 `Providers`，用于全局 Provider 管理。
3. Group 不再“新增 Provider”，而是“从已存在的全局 Providers 中选择并建立关联”。

## 目标与边界（v1）

### 目标

- 任何 Provider 只维护一份配置（Token、API 地址、协议、默认模型、额度与费用配置）。
- Group 只保存“关联关系 + 生效 Provider（activeProviderId）”。
- Providers 页面展示卡片信息与当前 Group 下的 Provider 卡片保持一致（字段与样式都一致）。

### 硬约束（你新增的要求）

- **保留现有 Provider 卡片 UI 样式不变**：布局、视觉、按钮风格与当前 `RuleList` 卡片一致。
- **保留现有 Provider 创建/编辑页样式不变**：继续复用 `RuleFormPage` 的现有 UI 与样式。
- 本次改造只改“数据归属（全局）+ 入口与交互（关联）”，不做视觉重设计。

### 非目标（v1 不做）

- 不做 Provider 的版本历史。
- 不做按 Group 覆盖 Provider 字段（即 Provider 仍是全局单实例）。
- 不改动日志统计口径（先兼容现有 groupId + providerId 维度）。

## 信息架构与路由

### 顶部导航

新增 `Providers` Tab：

- 服务 `(/)`
- 设置 `(/settings)`
- Agent `(/agents)`
- 日志 `(/logs)`
- Providers `(/providers)`

### 页面路由建议

- `GET /providers`：全局 Provider 列表页（新增）
- `GET /providers/new`：新建 Provider（新增）
- `GET /providers/:providerId/edit`：编辑 Provider（新增）
- `GET /groups/:groupId/providers/new`：保留一版兼容路由，跳转到 `/providers/new?bindGroup={groupId}`
- `GET /groups/:groupId/providers/:providerId/edit`：保留一版兼容路由，跳转到 `/providers/:providerId/edit?fromGroup={groupId}`

## 完整交互设计

## 1) Providers 管理页

### 页面结构

- 顶部：`Providers` 标题、总数、`添加 Provider` 按钮。
- 搜索区：按 `name / id / apiAddress` 过滤。
- 列表区：Provider 卡片列表（卡片字段对齐现有 Group 卡片）。

### Provider 卡片（全局）

字段与 Group 卡片一致：

- Provider 名称
- 协议
- API 地址
- Quota 状态
- 请求/Token/缓存/费用趋势

附加字段：

- 已关联 Group 数（例如：`已关联 3 个分组`）

操作：

- `编辑`：进入全局编辑。
- `删除`：全局删除（会解除所有 Group 关联）。
- `模型测试`：保留。

样式约束：

- 直接复用现有服务页 Provider 卡片样式，不新增视觉变体。

### 空态

- 无 Provider 时显示：`暂无 Provider，先添加一个全局 Provider`。
- 提供主按钮：`添加 Provider`。

## 2) 添加 Provider

入口：

- Providers 页主按钮。

交互：

1. 打开 Provider 表单（沿用现有 `RuleFormPage` 字段与样式）。
2. 校验通过后保存到全局 providers。
3. 保存成功后回到 Providers 页并高亮新卡片。
4. 可选 CTA：`关联到分组`（打开 Group 选择弹窗）。

## 3) 编辑 Provider

入口：

- Providers 卡片点击编辑。
- Group 页面卡片点击编辑（跳全局编辑页）。

提示规则：

- 编辑页顶部提示：`这是全局 Provider，修改会影响所有关联分组。`
- 页面结构与样式继续沿用当前 Provider 编辑页，不新增新皮肤。

## 4) 删除 Provider（全局）

入口：

- Providers 卡片删除按钮。

交互：

1. 弹确认框，展示影响范围：
   - 关联 Group 数
   - 将受影响的 Group 名称列表
2. 确认后执行：
   - 从全局 providers 删除
   - 从所有 group.providerIds 移除
   - 若某 Group 的 activeProviderId 被删除，自动切到该 Group 的第一个关联 Provider；若无则置空
3. Toast：`Provider 已删除，已同步解除 N 个分组关联`

## 5) Group 页面改造：从“新增”改为“关联”

入口变更：

- 现有 `添加 Provider` 按钮改为 `关联 Provider`。

交互：

1. 点击 `关联 Provider` -> 打开弹窗。
2. 弹窗中展示“尚未关联到当前 Group 的全局 Providers”。
3. 支持搜索、单选/多选（建议 v1 支持多选）。
4. 确认后写入当前 group.providerIds。
5. 若当前 Group 无 activeProviderId，则自动设为本次新增关联的第一项。

Group 卡片上的删除行为改名：

- 从 `删除 Provider` 改为 `移除关联`（仅解除当前 Group 关系，不删除全局 Provider）。

## 6) Group 与 Providers 的联动导航

- 在 Group 页面点击 `关联 Provider` 弹窗为空时，提供 `去 Providers 新建` 快捷入口。
- 在 Providers 页面卡片可查看关联 Group 列表并跳转目标 Group。

## 原型说明

已提供低保真可交互原型：

- `docs/features_next/prototypes/global-providers-prototype.html`

原型覆盖：

1. 顶部新增 `Providers` Tab 并切换视图。
2. Providers 列表与新增/删除 Provider。
3. Service 页面按 Group 展示已关联 Provider。
4. Group 的 `关联 Provider` 弹窗与多选关联。
5. Group 卡片 `移除关联`。
6. 全局删除 Provider 后自动解除所有 Group 关联并修正 activeProviderId。

说明：

- 该原型主要用于演示流程；实际落地时视觉必须复用现有 Provider 卡片与 `RuleFormPage` 样式。

## 数据模型改造（核心）

当前（v3）：

- `groups[].providers: Provider[]`

目标（v4）：

- `providers: Provider[]`（全局）
- `groups[].providerIds: string[]`（关联）
- `groups[].activeProviderId: string | null`（保持）

建议结构：

```ts
interface ProxyConfigV4 {
  configVersion: 4
  providers: Provider[]
  groups: Array<{
    id: string
    name: string
    models: string[]
    providerIds: string[]
    activeProviderId: string | null
  }>
}
```

## 配置迁移方案（v3 -> v4）

1. 扫描所有 `groups[].providers`。
2. 构建全局 providers 字典：
   - 优先使用原 provider.id
   - 如果不同 Group 出现相同 id 但内容冲突，为后者重分配新 id（UUID）
3. 每个 Group 写入 `providerIds`。
4. `activeProviderId` 按新旧 id 映射修正。
5. 删除旧 `groups[].providers` 字段。
6. `configVersion` 升级到 4。

## 存储层（SQLite）改造建议

当前表：

- `group_records(provider_ids_json)`
- `provider_records(group_id, provider_id, provider_json)`

目标：

- `group_records` 保持 `provider_ids_json`
- `provider_records` 改为全局主键：`provider_id PRIMARY KEY, provider_json, updated_at`

迁移策略：

- 初始化时检测旧表结构，若存在 `group_id` 维度记录，执行一次聚合迁移。
- 迁移后统一按新结构读写。

## 前端改造点

- 新页面：`src/renderer/pages/ProvidersPage/*`
- 头部导航：`src/renderer/components/layout/Header.tsx`
- 路由：`src/renderer/App.tsx`
- Service 页按钮与行为：`src/renderer/pages/ServicePage/RuleList.tsx`、`ServicePage.tsx`
- Provider 卡片复用：抽出或复用现有 `RuleList` 卡片渲染片段，保证样式不变
- 表单复用：`RuleFormPage` 支持全局 provider 模式（不依赖 groupId），保持现有样式
- i18n：`src/renderer/i18n/zh-CN.ts`、`src/renderer/i18n/en-US.ts`

## 后端改造点

- 实体：`src-tauri/src/domain/entities.rs`（新增 `ProxyConfig.providers`、`Group.provider_ids`）
- 迁移：`src-tauri/src/config/migrator.rs`（`CURRENT_CONFIG_VERSION` -> 4）
- 规范化：`src-tauri/src/config/schema.rs`
- 校验：`src-tauri/src/config/validator.rs`
- 持久化：`src-tauri/src/config_store.rs`
- 服务逻辑：`provider_service` / `quota_service` / `routing` 从 group-provider 关联解析 provider
- 备份与导入：`backup.rs`、`group_backup_service.rs`、`config_service.rs`
- 远端同步：`remote_sync.rs`（导入导出格式兼容）

## 验收标准（关键用例）

1. 新增 Provider 后，在 Providers 页立即可见，且可被任意 Group 关联。
2. Group 页面无法直接新建 Provider，只能关联已有 Provider。
3. Group 页面移除关联不会删除全局 Provider。
4. Providers 页删除 Provider 会解除所有 Group 关联并修复 activeProviderId。
5. 编辑 Provider 后，所有关联 Group 生效同一份配置。
6. 配置从 v3 自动迁移到 v4，旧数据不丢失。
7. Provider 卡片 UI 与当前线上样式一致，无新增视觉改动。
8. Provider 创建/编辑页 UI 与当前线上样式一致，无新增视觉改动。

## 实施分期建议

### Phase 1（数据与兼容）

- 完成 v4 schema + migrator + validator + config_store 改造。
- 保持旧 UI 可运行（中间兼容层）。

### Phase 2（UI 与交互）

- 新增 Providers 页面和顶部 Tab。
- 完成 Group “关联 Provider”改造。

### Phase 3（回归与收口）

- 备份/导入/远程同步回归。
- 日志统计与额度查询联调。
- 文案与引导优化。

## 风险与应对

1. **同 ID Provider 冲突**：迁移阶段必须重分配冲突 ID 并记录映射。
2. **activeProvider 失效**：任何删除/去关联后统一走 active 修复逻辑。
3. **旧备份兼容**：导入逻辑同时支持 v3（groups 内 providers）与 v4（global providers）。
