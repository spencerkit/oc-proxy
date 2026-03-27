# 开发文档：本地数据与数据库

本文档描述当前版本的本地持久化设计，便于开发排查与后续演进。

## 数据目录

应用数据目录由 Tauri `app_data_dir` 决定，常见路径示例：

- Linux: `~/.local/share/com.local.aiopenrouter/`
- macOS: `~/Library/Application Support/com.local.aiopenrouter/`
- Windows: `%APPDATA%\\com.local.aiopenrouter\\`

## 存储拆分

当前采用“配置 / Provider / 统计”分库设计，并配合少量 JSON 文件保存其他本地状态：

- `config.json`
  - 保存基础配置（`server`、`compat`、`logging`、`ui`、`remoteGit` 等）。
  - `groups` 不再作为主存储，写入时会被置为空数组。
- `providers.sqlite`
  - 保存分组与 Provider（原 rule）配置。
- `request-stats.sqlite`
  - 保存逐请求统计事件，用于日志页统计、趋势图、规则卡片轻量图。
- `client-integrations.json`
  - 保存已接入客户端目标与对应本地配置目录。
- `remote-admin-auth.json`
  - 保存远程管理密码哈希与认证状态快照。

## `providers.sqlite` 结构

### 表：`group_records`

- `group_id TEXT PRIMARY KEY`
- `group_name TEXT NOT NULL`
- `models_json TEXT NOT NULL`
- `active_provider_id TEXT`
- `provider_ids_json TEXT NOT NULL`
- `group_json TEXT`
- `updated_at INTEGER NOT NULL`

### 表：`provider_records`

- `provider_id TEXT PRIMARY KEY`
- `provider_json TEXT NOT NULL`
- `updated_at INTEGER NOT NULL`

说明：
- `group_records` 保存分组基础字段，以及用于兼容/迁移的 `group_json` 快照。
- `provider_records` 以 `provider_id` 为主键保存 Provider 详情，不按 `(group_id, provider_id)` 分表。
- 分组级 failover 配置包含在 `group_json` 中持久化，不单独拆成 `failover_json` 列。
- 运行时当前生效 provider / failover 激活状态不落盘，只存在内存运行态中，并通过状态接口返回给前端显示。

## `request-stats.sqlite` 结构

### 表：`app_meta`

- `key TEXT PRIMARY KEY`
- `value TEXT NOT NULL`
- `updated_at INTEGER NOT NULL`

当前写入：`stats_schema_version`。

### 表：`request_events`

- `id INTEGER PRIMARY KEY AUTOINCREMENT`
- `ts_epoch_ms INTEGER NOT NULL`
- `hour TEXT NOT NULL`
- `group_id TEXT`
- `group_name TEXT`
- `rule_id TEXT`
- `entry_protocol TEXT`
- `downstream_protocol TEXT`
- `http_status INTEGER`
- `errors INTEGER NOT NULL DEFAULT 0`
- `input_tokens INTEGER NOT NULL DEFAULT 0`
- `output_tokens INTEGER NOT NULL DEFAULT 0`
- `cache_read_tokens INTEGER NOT NULL DEFAULT 0`
- `cache_write_tokens INTEGER NOT NULL DEFAULT 0`
- `duration_ms INTEGER NOT NULL DEFAULT 0`
- `total_cost REAL`
- `currency TEXT`
- `input_price_snapshot REAL`
- `output_price_snapshot REAL`
- `cache_input_price_snapshot REAL`
- `cache_output_price_snapshot REAL`

### 索引

- `idx_request_events_ts (ts_epoch_ms)`
- `idx_request_events_provider_time (group_id, rule_id, ts_epoch_ms)`
- `idx_request_events_protocol_time (downstream_protocol, ts_epoch_ms)`
- `idx_request_events_status_time (http_status, ts_epoch_ms)`

### 保留策略

- 当前不做自动过期清理，统计事件持续保留，后续通过手动清理功能处理。

## 日志与统计关系

- 请求日志列表：内存环形队列（默认最多 100 条），用于界面实时展示。
- 开发模式下：额外写入 `proxy-dev-logs.jsonl`（便于调试）。
- 统计计算：来自 `request_events`（SQLite），不依赖内存日志容量。
