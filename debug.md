# Debug Guide

## Debug Log File

All debug logs are written to:

```
/tmp/oc-proxy-debug.log
```

## Viewing Debug Logs

### Real-time monitoring
```bash
tail -f /tmp/oc-proxy-debug.log
```

### View recent logs
```bash
tail -100 /tmp/oc-proxy-debug.log
```

### Clear debug log
```bash
rm -f /tmp/oc-proxy-debug.log
```

## Debug Log Sections

The debug log contains the following sections:

### 1. OpenAI Responses Request
Records the original OpenAI Responses API request received from the client.

```
=== OpenAI Responses Request ===
{
  "model": "gpt-4",
  "input": [...],
  "tools": [...]
}
```

### 2. Converted Claude Request
Records the converted Claude Messages API request sent to upstream.

```
=== Converted Claude Request ===
{
  "model": "claude-3-opus",
  "messages": [...],
  "tools": [...]
}
```

### 3. Claude Response (Non-streaming)
Records the Claude API response (for non-streaming requests).

```
=== Claude Response ===
{
  "id": "msg_xxx",
  "content": [...],
  "usage": {...}
}
```

### 4. Converted OpenAI Responses Response
Records the converted OpenAI Responses API response sent back to client (non-streaming).

```
=== Converted OpenAI Responses Response ===
{
  "id": "resp_xxx",
  "output": [...],
  "usage": {...}
}
```

### 5. Claude Stream Event
Records each Claude SSE event during streaming conversion.

```
=== Claude Stream Event ===
Event type: message_start
Data: {"type":"message_start","message":{...}}
```

### 6. Converted OpenAI Responses Stream
Records each converted OpenAI Responses SSE event.

```
=== Converted OpenAI Responses Stream ===
data: {"type":"response.created","response":{...}}

data: {"type":"response.output_text.delta","delta":"..."}
```

## Debugging Common Issues

### Issue: Client receives empty response

**Symptoms:**
- `response.completed` has empty id and 0 tokens
- No content displayed to user

**Debug steps:**
1. Check if `message_start` event exists in logs
2. Check if `content_block_start` events exist
3. Verify token usage is being tracked

**Common causes:**
- Upstream API sends incomplete event sequence
- Missing `message_start` or `content_block_start` events
- StreamContext not properly initialized

### Issue: Tool call conversion errors

**Symptoms:**
- Error: "tool call result does not follow tool call"

**Debug steps:**
1. Check the order of `function_call` and `function_call_output` in request
2. Verify tool_result messages immediately follow tool_use messages

### Issue: Streaming disconnected

**Symptoms:**
- Error: "stream closed before response.completed"

**Debug steps:**
1. Check if all stream events are being converted
2. Verify `response.completed` event is sent before `[DONE]`
3. Check StreamContext token accumulation

## Debug Log Implementation

Debug logs are written using `std::fs::OpenOptions` with append mode:

```rust
if let Ok(mut file) = std::fs::OpenOptions::new()
    .create(true)
    .append(true)
    .open("/tmp/oc-proxy-debug.log")
{
    use std::io::Write;
    let _ = writeln!(file, "\n=== Section Name ===");
    let _ = writeln!(file, "{}", serde_json::to_string_pretty(&data).unwrap());
}
```

### Files with debug logging:

1. `src-tauri/src/transformer/convert/claude_openai_responses.rs`
   - Request conversion: `openai_responses_req_to_claude()`
   - Response conversion: `claude_resp_to_openai_responses()`, `openai_responses_resp_to_claude()`

2. `src-tauri/src/transformer/convert/claude_openai_responses_stream.rs`
   - Stream event conversion: `claude_stream_to_openai_responses()`

## Enabling/Disabling Debug Logs

Debug logs are always enabled in debug builds. To disable:

1. Comment out or remove the debug logging blocks in the source files
2. Rebuild the application

## Log File Rotation

The debug log file is not automatically rotated. For long-running sessions:

```bash
# Rotate log file manually
mv /tmp/oc-proxy-debug.log /tmp/oc-proxy-debug.log.$(date +%Y%m%d_%H%M%S)
touch /tmp/oc-proxy-debug.log
```

## Performance Impact

Debug logging has minimal performance impact:
- Uses buffered I/O via `std::fs::File`
- Append-only writes
- JSON serialization is the main overhead
- File is opened/closed per log entry (could be optimized with file handle caching)
