# Headless CLI

Headless 模式提供一个独立二进制 `ai-open-router`，以及 npm CLI `@spencer-kit/aor`。

## 构建

```bash
npm run build
cargo build --release --bin ai-open-router --manifest-path src-tauri/Cargo.toml
```

## 运行（二进制）

```bash
./target/release/ai-open-router
```

启动后访问：

```
http://127.0.0.1:8899/management
```

## 直接下载二进制

Release 会提供以下产物：

- `ai-open-router-<platform>-<arch>.tar.gz`
- `ai-open-router-<platform>-<arch>.zip`
- `ai-open-router-<platform>-<arch>`（或 `.exe`）

解压后直接运行即可。

## npm CLI

```bash
npm i -g @spencer-kit/aor
```

```bash
aor start --port 8899
aor status
aor stop
aor restart --port 8899
```

CLI 会在后台启动 `ai-open-router` 进程，并将管理页面暴露在：

```
http://127.0.0.1:<port>/management
```

## E2E 测试

```bash
npm run e2e:headless
```

## 环境变量

- `AOR_APP_DATA_DIR`：覆盖配置与数据目录
