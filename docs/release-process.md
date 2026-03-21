# 版本管理与发布流程

本文档定义本仓库的版本升级、发布和 Changelog 生成流程。

## 1. 版本策略

- 采用 `SemVer`：`MAJOR.MINOR.PATCH`
- 版本号必须在以下文件保持一致：
- `package.json`
- `package-lock.json`
- `src-tauri/Cargo.toml`
- `src-tauri/tauri.conf.json`
- `npm run version:check` 用于校验一致性（CI 必跑）

## 2. 提交规范与自动升级规则

提交信息使用 Conventional Commits（仓库已有 `commit-msg` hook）：

- `feat:` -> `minor`
- `fix:` -> `patch`
- 其他类型（`docs/chore/refactor/build/ci/test/style/perf`）默认 -> `patch`
- 带 `!` 或包含 `BREAKING CHANGE:` -> `major`

自动升级规则在 `scripts/release.js` 中实现（`--bump auto`）。

Changelog 分组规则：

- Breaking：提交头带 `!` 或正文包含 `BREAKING CHANGE:`
- Features：`feat:`
- Fixes：`fix:`
- Maintenance：其他类型

## 3. 日常开发阶段

在 PR 中确保以下检查通过：

```bash
npm run ci
npm run test:rust
```

CI 会额外执行：

- `npm run version:check`
- `cargo check --manifest-path src-tauri/Cargo.toml`
- `cargo test --manifest-path src-tauri/Cargo.toml`

## 4. 准备发布版本（生成版本号 + changelog）

### 方式 A：本地命令（推荐）

1. 预览自动升级结果（不改文件）

```bash
npm run release:plan
```

2. 生成发布内容

```bash
npm run release:prepare
```

可选：手工指定版本

```bash
npm run release:prepare -- --version 0.3.0
```

可选：手工指定基线 tag（当历史 tag 与当前版本文件不一致时）

```bash
npm run release:plan -- --from-tag v0.2.1
npm run release:prepare -- --from-tag v0.2.1
```

命令会自动更新：

- `package.json` / `package-lock.json`
- `src-tauri/Cargo.toml`
- `src-tauri/tauri.conf.json`
- `CHANGELOG.md`（新增 `## vX.Y.Z - YYYY-MM-DD`）

3. 提交 release PR（例如）

```bash
git checkout -b release/vX.Y.Z
git add package.json package-lock.json src-tauri/Cargo.toml src-tauri/tauri.conf.json CHANGELOG.md
git commit -m "chore(release): vX.Y.Z"
```

### 方式 B：GitHub 手工触发

- 使用工作流 `Release Prepare`（`workflow_dispatch`）
- 输入 `bump` 或 `version`
- 工作流会自动创建 release PR

## 5. 正式发布

1. 合并 release PR 到 `main`
2. `CI` 工作流自动读取 `package.json` 版本并创建 `vX.Y.Z` tag（若远端已存在同名 tag 会自动跳过）
3. `Release Build` 工作流自动执行：
- 多平台打包（Linux + macOS + Windows）
- Linux 产物包含 `deb` 与 `AppImage`
- 上传产物 artifact
- 从 `CHANGELOG.md` 提取当前版本说明
- 自动创建 GitHub Release 并附带产物
 - 生成并上传更新清单 `latest.json` 与签名文件（用于自动更新）

## 8. Headless CLI 产物

为了支持 `npm i -g @spencer-kit/aor`，需要额外打包 headless 二进制：

- 二进制名称：`ai-open-router`
- 产物命名：`ai-open-router-<platform>-<arch>.tar.gz`
  - 示例：`ai-open-router-darwin-arm64.tar.gz`
- 同时输出原始二进制与 zip 包：
  - `ai-open-router-<platform>-<arch>`（或 `.exe`）
  - `ai-open-router-<platform>-<arch>.zip`

打包脚本：

```bash
bash ./scripts/package-headless-artifacts.sh
```

上传到 GitHub Release 后，CLI 安装脚本会自动下载对应版本的产物。

发布前检查清单：

- `npm run ci` 通过
- `npm run test:rust` 通过
- `npm run version:check` 通过
- `CHANGELOG.md` 含目标版本段落（`## vX.Y.Z - YYYY-MM-DD`）
- `release:plan` 与预期 bump 一致
- GitHub Actions secrets 已配置：
  - `TAURI_SIGNING_PRIVATE_KEY`
  - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`（如私钥加密）
  - `NPM_TOKEN`（发布 `@spencer-kit/aor`）

## 6. 回滚与修复发布

- 若发现发布问题，创建修复提交并走 `patch` 发布流程：
- 提交 `fix:` 类型 commit
- 重新执行 `release:prepare`
- 生成 `vX.Y.(Z+1)` 并发布

## 7. 发布调试与排障

### 7.1 `release:plan` 输出版本不符合预期

- 症状：自动算出的版本过高或过低
- 处理：
- 显式指定基线 tag：`npm run release:plan -- --from-tag v0.2.1`
- 检查最近提交信息是否符合 Conventional Commits
- 如需强制版本，使用 `--version`：`npm run release:prepare -- --version 0.3.0`

### 7.2 `version:check` 失败

- 症状：提示多文件版本不一致
- 处理：
- 检查 `package.json`、`package-lock.json`、`src-tauri/Cargo.toml`、`src-tauri/tauri.conf.json`
- 使用 `npm run release:prepare` 重新统一版本

### 7.3 `Release Build` 无法生成 Release Notes

- 症状：`extract-changelog.js` 报错找不到版本段落
- 处理：
- 确认 `CHANGELOG.md` 有 `## vX.Y.Z - YYYY-MM-DD`
- 确认 tag 为 `vX.Y.Z` 且与 changelog 标题版本一致

### 7.4 合并后未自动发布

- 症状：release PR 合并后没有触发 `Release Build`
- 处理：
- 检查 `CI` 工作流中的 `Auto Tag Release Version` 是否成功
- 检查仓库权限是否允许 GitHub Actions 推送 tag（`contents: write`）
- 确认 `package.json` 版本对应的 tag 在远端不存在（若已存在会跳过）

### 7.5 产物上传失败（if-no-files-found）

- 症状：artifact path 找不到文件
- 处理：
- 本地先执行 `npm run tauri:build` + `npm run tauri:collect`
- 检查 `dist/target` 与 `dist/` 下是否有目标平台产物
- 根据产物实际路径调整 `release.yml` 的 `path` 配置

### 7.6 快速回滚方案

- 已打 tag 但发布内容错误：
- 新建修复版本（`patch`）并发布 `vX.Y.(Z+1)`，不建议改写已有 tag
- 仅 GitHub Release 说明有误：
- 在 GitHub Release 页面编辑说明，源码与安装包不变
