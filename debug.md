# Debug Guide

This file documents the active debug artifacts and where to find them.

## 1. Debug File Paths

All runtime debug artifacts are under `app_data_dir` (resolved by Tauri at startup):

- `app_data_dir/proxy-dev-logs.jsonl`
  - Debug-only (`cfg(debug_assertions)`).
  - One JSON log entry per line.
  - Used for full-chain debugging.
- `app_data_dir/config.json`
  - Runtime config.
- `app_data_dir/request-stats.sqlite`
  - Request/token/cost statistics store.

Important:

- The old temp debug file `/tmp/oc-proxy-debug.log` is no longer used.

## 2. How To Locate `app_data_dir`

Use one of the following:

- Check startup stderr for:
  - `dev log persistence enabled: <absolute_path>/proxy-dev-logs.jsonl`
- Search manually:
  - `find "$HOME" -type f -name proxy-dev-logs.jsonl 2>/dev/null`

Code reference:

- `src-tauri/src/bootstrap.rs` (`app.path().app_data_dir()`)
- `src-tauri/src/log_store.rs` (`with_dev_log_file`)

## 3. What `proxy-dev-logs.jsonl` Records

Each line is a serialized `LogEntry` object. Core fields include:

- Request context:
  - `timestamp`, `traceId`, `status`, `method`, `requestPath`, `requestAddress`
  - `groupPath`, `groupName`, `ruleId`
  - `entryProtocol`, `downstreamProtocol`
  - `model`, `forwardedModel`, `forwardingAddress`
- Headers:
  - `requestHeaders`, `forwardRequestHeaders`
  - `upstreamResponseHeaders`, `responseHeaders`
- Bodies:
  - `requestBody`, `forwardRequestBody`, `responseBody`
- Metrics:
  - `tokenUsage`, `costSnapshot`, `httpStatus`, `upstreamStatus`, `durationMs`

Status lifecycle usually appears as:

- `processing` (request accepted and forwarded)
- `ok` or `error` (finalized result)

## 4. Body Capture Behavior

`proxy-dev-logs.jsonl` behavior:

- In debug builds, it always stores full request/forwarded-request/response payloads for debugging.
- This is independent of `logging.capture_body`.

Business/UI log behavior:

- `logging.capture_body` controls body fields in normal in-memory/business logs (`LogEntry` used by app UI and API).
- Default value is `false` in config schema.

## 5. Streaming Notes

For streaming requests (`text/event-stream`):

- `responseBody` is stored as:
  - `{"stream": true, "payload": "...", "truncated": ...}`
- Debug log file stores full transformed stream payload in debug mode.

## 6. Useful Commands

- Follow logs:
  - `tail -f <app_data_dir>/proxy-dev-logs.jsonl`
- View recent entries:
  - `tail -n 100 <app_data_dir>/proxy-dev-logs.jsonl`
- Pretty-print one line:
  - `tail -n 1 <app_data_dir>/proxy-dev-logs.jsonl | jq .`

## 7. Source References

- `src-tauri/src/bootstrap.rs`
- `src-tauri/src/log_store.rs`
- `src-tauri/src/proxy/observability.rs`
- `src-tauri/src/proxy/pipeline.rs`
- `src-tauri/src/domain/entities.rs`
