# 会话问题汇总（2026-03-09）

## 文档说明
- 目的: 汇总本次会话中 Windows 写入 WSL 配置目录相关的问题、影响与修复状态。
- 与 `docs/problems.md` 的关系: 本文只记录本次会话排查与修复结果，不写入常驻问题库。
- 关联日志:
  - Windows 调试日志: `%LOCALAPPDATA%\art.shier.aiopenrouter\wsl-debug.log`
  - 工作区: `/home/spencer/workspace/oc-proxy`

## 状态定义
- 已修复: 已有代码修复，并有测试、日志或复现实验支持。
- 部分修复: 已缓解，但仍需后续验证或仍有边界场景。
- 未修复: 已定位原因，暂无代码修复落地。

## 问题总览
| ID | 问题 | 阶段 | 影响 | 状态 |
|---|---|---|---|---|
| W01 | Windows 侧直接对 WSL UNC 路径做元数据访问 | 目录校验/选择目录 | 可能 crash 或误报目录不存在 | 已修复 |
| W02 | `\\wsl$\\distro\\...` 被错误转换成 `/distro/...` | 路径转换 | 读写失败或命中错误 distro | 已修复 |
| W03 | WSL 读失败被当成空配置 | 读取配置 | 可能覆盖原有配置 | 已修复 |
| W04 | Codex 在空 `config.toml` 场景下 panic | TOML 写入 | 进程 crash | 已修复 |
| W05 | `sh -c` 写入参数传递不稳定 | WSL 写入 | `cannot create : Directory nonexistent` | 已修复 |
| W06 | `wsl.exe` 拉起时闪一下终端窗口 | UI 体验 | 每次探测/读写都闪控制台 | 已修复 |
| W07 | 旧构建残留导致日志与源码不一致 | 验证阶段 | 容易误判修复未生效 | 部分修复 |

---

## W01: Windows 直接访问 WSL UNC 元数据
### 原因
- 旧实现会对 `\\wsl$...` 或 `\\wsl.localhost\...` 路径调用 `exists()`、`is_dir()`、`canonicalize()`。
- 这些调用在 Windows 进程里访问 WSL UNC 路径时不稳定，既可能误判，也可能触发 crash。

### 影响
- 选择配置目录、校验 target、写入前检查都可能失败。
- 失败时表现不一致，可能是 toast，也可能是直接 crash。

### 修复情况
- 新增统一 WSL helper: [wsl.rs](/home/spencer/workspace/oc-proxy/src-tauri/src/wsl.rs)
- 对 WSL 路径不再使用 Windows 侧 `std::fs` 元数据接口。
- 校验和读写统一改为通过 `wsl.exe` 在对应 distro 内执行。

---

## W02: WSL 路径转换错误
### 原因
- 旧实现把 `\\wsl$\\Ubuntu\\home\\spencer\\.codex\\config.toml` 转成 `/Ubuntu/home/spencer/.codex/config.toml`。
- 同时没有显式指定 `-d Ubuntu`，会依赖默认 distro。

### 影响
- 读写命中错误路径。
- 在多 distro 环境下会读写错位置。

### 修复情况
- 新逻辑先解析 `distro + linux_path`，例如:
  - `\\wsl$\\Ubuntu\\home\\spencer\\.codex\\config.toml`
  - -> `distro=Ubuntu`
  - -> `linux_path=/home/spencer/.codex/config.toml`
- 后续统一通过 `wsl -d <distro> -- ...` 执行。

---

## W03: 读失败被吞成空配置
### 原因
- 旧实现只要 `wsl cat` 失败就返回空字符串。
- 上层把空内容当空 JSON/TOML 文档继续写回。

### 影响
- 路径错误、权限失败、distro 不匹配时，可能把已有配置覆盖掉。

### 修复情况
- 读取前先明确判断文件是否存在。
- 只有“文件确实不存在”时才返回 `None` 并走空配置初始化。
- 其他失败直接返回错误，不再吞掉。

---

## W04: Codex 空配置文件场景 panic
### 原因
- `config.toml` 不存在时会得到空 `DocumentMut`。
- 旧代码使用嵌套索引:
  - `doc["model_providers"]["aor_shared"]["base_url"] = ...`
- 在中间层不存在时会触发 `index not found` panic。

### 影响
- Codex 首次写入配置时直接 crash。

### 修复情况
- 改为显式确保 TOML table 存在，再逐层写入。
- 相关实现:
  - [integration_service.rs](/home/spencer/workspace/oc-proxy/src-tauri/src/services/integration_service.rs)
- 已补回归测试:
  - `codex_config_shape_can_be_created_from_empty_document`

---

## W05: `sh -c` 参数传递不稳定
### 原因
- 旧实现通过 `wsl ... sh -c 'mkdir -p ... && cat > "$1"' sh /path` 写文件。
- 实际 Windows/WSL 运行时出现参数未正确映射，shell 里重定向目标为空。

### 影响
- 日志表现为:
  - `sh: 1: cannot create : Directory nonexistent`
- 即使 WSL 路径解析正确，写入仍失败。

### 修复情况
- 去掉 `sh -c`。
- 改为直接执行:
  - `test`
  - `cat`
  - `mkdir -p`
  - `tee`
- 写入通过 `tee -- <path>` + stdin 完成，减少 shell quoting 和参数展开风险。

---

## W06: `wsl.exe` 闪终端窗口
### 原因
- GUI 进程直接拉起 `wsl.exe`，未设置 Windows 后台启动标志。

### 影响
- 选择目录、读配置、写配置时都会闪一下控制台窗口。

### 修复情况
- 在 Windows 下为 `wsl.exe` 子进程增加 `CREATE_NO_WINDOW`。
- 相关实现:
  - [wsl.rs](/home/spencer/workspace/oc-proxy/src-tauri/src/wsl.rs)

---

## W07: 旧构建残留导致误判
### 原因
- 项目通过 `.cargo/config.toml` 将 target 目录配置为 `dist/target`。
- 若只重启前端或未清理旧二进制，Windows 端可能继续运行旧 backend。

### 影响
- 日志仍显示旧实现（例如 `sh: 1:`），容易误以为新修复无效。

### 当前状态
- 已在排查过程中确认这一点，并通过新日志区分“旧实现”和“新实现”。
- 仍需在后续类似问题排查时优先确认当前运行二进制是否为最新构建，因此记为部分修复。

### 经验
- 出现“日志内容与当前源码明显对不上”时，应优先确认是否仍在运行旧构建。
- 本项目构建产物目录:
  - [dist/target](/home/spencer/workspace/oc-proxy/dist/target)

---

## 本次落地改动
- 新增 WSL 统一 helper: [wsl.rs](/home/spencer/workspace/oc-proxy/src-tauri/src/wsl.rs)
- 集成写入逻辑调整: [integration_service.rs](/home/spencer/workspace/oc-proxy/src-tauri/src/services/integration_service.rs)
- target 目录校验调整: [integration_store.rs](/home/spencer/workspace/oc-proxy/src-tauri/src/integration_store.rs)
- 目录选择器初始路径适配: [integration.rs](/home/spencer/workspace/oc-proxy/src-tauri/src/commands/integration.rs)

## 验证记录
- `cargo test wsl::tests -- --nocapture`
- `cargo test codex_config_shape_can_be_created_from_empty_document -- --nocapture`
- `cargo check`
