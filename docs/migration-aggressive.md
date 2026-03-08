# 破坏性迁移方案：Transformer 架构重构

## 迁移原则

1. **直接删除旧代码** - 不保留并行路径
2. **严格遵循 ccNexus** - 转换逻辑完全对齐
3. **完善文档** - 每个转换都有详细注释和数据流图
4. **一次性完成** - 避免中间状态

## 迁移步骤

### Step 1: 创建 Transformer 基础架构 (Day 1)

#### 1.1 创建模块结构
```bash
mkdir -p src-tauri/src/transformer/adapters
touch src-tauri/src/transformer/mod.rs
touch src-tauri/src/transformer/registry.rs
touch src-tauri/src/transformer/context.rs
touch src-tauri/src/transformer/types.rs
```

#### 1.2 定义核心类型
文件：`src-tauri/src/transformer/types.rs`

```rust
//! Protocol type definitions and conversion structures.
//!
//! This module defines all protocol-specific types used in transformations.
//! Structures mirror ccNexus types to ensure conversion compatibility.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Protocol identifier enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Protocol {
    /// Anthropic Messages API (/v1/messages)
    AnthropicMessages,
    /// OpenAI Chat Completions API (/v1/chat/completions)
    OpenAIChatCompletions,
    /// OpenAI Responses API (/v1/responses)
    OpenAIResponses,
}

// ============================================================================
// OpenAI Chat Completions Types (aligned with ccNexus)
// ============================================================================

/// OpenAI tool call structure
/// Reference: ccNexus/internal/transformer/types.go:OpenAIToolCall
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIToolCall {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<i32>,
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: String, // "function"
    pub function: OpenAIFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIFunction {
    pub name: String,
    pub arguments: String, // JSON string
}

/// OpenAI message structure
/// Reference: ccNexus/internal/transformer/types.go:OpenAIMessage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIMessage {
    pub role: String, // "system", "user", "assistant", "tool"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Value>, // string or array
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<OpenAIToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

/// OpenAI request structure
/// Reference: ccNexus/internal/transformer/types.go:OpenAIRequest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIRequest {
    pub model: String,
    pub messages: Vec<OpenAIMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_completion_tokens: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<Value>,
}

// ============================================================================
// Anthropic Messages Types (aligned with ccNexus)
// ============================================================================

/// Anthropic message structure
/// Reference: ccNexus/internal/transformer/types.go:ClaudeMessage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicMessage {
    pub role: String, // "user", "assistant"
    pub content: Value, // string or array of content blocks
}

/// Anthropic request structure
/// Reference: ccNexus/internal/transformer/types.go:ClaudeRequest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicRequest {
    pub model: String,
    pub messages: Vec<AnthropicMessage>,
    pub max_tokens: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<Value>, // string or array
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<Value>,
}

// ============================================================================
// OpenAI Responses API Types (aligned with ccNexus)
// ============================================================================

/// OpenAI Responses input item
/// Reference: ccNexus/internal/transformer/types.go:OpenAI2InputItem
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAI2InputItem {
    #[serde(rename = "type")]
    pub item_type: String, // "message"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Vec<Value>>,
}

/// OpenAI Responses request structure
/// Reference: ccNexus/internal/transformer/types.go:OpenAI2Request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAI2Request {
    pub model: String,
    pub input: Value, // string or array of OpenAI2InputItem
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
}
```


#### 1.3 StreamContext 定义
文件：`src-tauri/src/transformer/context.rs`

```rust
//! Streaming context for stateful SSE event transformation.
//!
//! Reference: ccNexus/internal/transformer/types.go:StreamContext
//! 
//! This context tracks state across multiple SSE chunks for a single request.
//! Each request gets its own isolated context to prevent state leakage.

use super::types::OpenAIToolCall;

/// Streaming state context (aligned with ccNexus StreamContext)
#[derive(Debug, Clone)]
pub struct StreamContext {
    // Message lifecycle state
    pub message_start_sent: bool,
    pub message_id: String,
    pub model_name: String,
    
    // Content block tracking
    pub content_block_started: bool,
    pub content_index: usize,
    
    // Tool call state (ccNexus: ToolBlockStarted, ToolBlockPending, etc.)
    pub tool_block_started: bool,
    pub tool_block_pending: bool,
    pub current_tool_call: Option<OpenAIToolCall>,
    pub tool_call_buffer: String,
    pub tool_index: usize,
    pub last_tool_index: usize,
    
    // Thinking block state (ccNexus: ThinkingBlockStarted, InThinkingTag, etc.)
    pub thinking_block_started: bool,
    pub thinking_index: usize,
    pub in_thinking_tag: bool,
    pub thinking_buffer: String,
    pub pending_thinking_text: String,
    
    // Token usage tracking
    pub input_tokens: usize,
    pub output_tokens: usize,
    
    // Finish state
    pub finish_reason_sent: bool,
    pub enable_thinking: bool,
}

impl StreamContext {
    /// Create new context with default values
    /// Reference: ccNexus/internal/transformer/types.go:NewStreamContext
    pub fn new() -> Self {
        Self {
            message_start_sent: false,
            message_id: String::new(),
            model_name: String::new(),
            content_block_started: false,
            content_index: 0,
            tool_block_started: false,
            tool_block_pending: false,
            current_tool_call: None,
            tool_call_buffer: String::new(),
            tool_index: 0,
            last_tool_index: 0,
            thinking_block_started: false,
            thinking_index: 0,
            in_thinking_tag: false,
            thinking_buffer: String::new(),
            pending_thinking_text: String::new(),
            input_tokens: 0,
            output_tokens: 0,
            finish_reason_sent: false,
            enable_thinking: false,
        }
    }
}

impl Default for StreamContext {
    fn default() -> Self {
        Self::new()
    }
}
```


#### 1.4 Transformer Trait 定义
文件：`src-tauri/src/transformer/mod.rs`

```rust
//! Transformer trait and core abstractions.
//!
//! This module defines the Transformer trait that all protocol converters must implement.
//! Design is aligned with ccNexus transformer interface.

use axum::body::Bytes;
use serde_json::Value;

pub mod context;
pub mod registry;
pub mod types;

pub use context::StreamContext;
pub use types::Protocol;

/// Transformer trait for protocol conversion
/// 
/// Reference: ccNexus/internal/transformer/transformer.go:Transformer
/// 
/// Each transformer handles bidirectional conversion between two protocols.
/// Implementations must be thread-safe (Send + Sync).
pub trait Transformer: Send + Sync {
    /// Unique transformer name (e.g., "openai_chat_to_anthropic_messages")
    fn name(&self) -> &str;
    
    /// Source protocol this transformer accepts
    fn source_protocol(&self) -> Protocol;
    
    /// Target protocol this transformer produces
    fn target_protocol(&self) -> Protocol;
    
    /// Transform request body from source to target protocol
    /// 
    /// # Arguments
    /// * `body` - Source protocol request JSON
    /// 
    /// # Returns
    /// * `Ok(Value)` - Transformed target protocol request
    /// * `Err(String)` - Transformation error message
    /// 
    /// # Reference
    /// ccNexus: TransformRequest(claudeReq []byte) (targetReq []byte, err error)
    fn transform_request(&self, body: &Value) -> Result<Value, String>;
    
    /// Transform non-streaming response from target to source protocol
    /// 
    /// # Arguments
    /// * `body` - Target protocol response JSON
    /// 
    /// # Returns
    /// * `Ok(Value)` - Transformed source protocol response
    /// * `Err(String)` - Transformation error message
    /// 
    /// # Reference
    /// ccNexus: TransformResponse(targetResp []byte, isStreaming bool) (claudeResp []byte, err error)
    fn transform_response(&self, body: &Value) -> Result<Value, String>;
    
    /// Transform streaming chunk with context
    /// 
    /// # Arguments
    /// * `chunk` - Raw SSE chunk bytes
    /// * `ctx` - Mutable streaming context for state tracking
    /// 
    /// # Returns
    /// * `Vec<Bytes>` - Zero or more transformed SSE frames
    /// 
    /// # Reference
    /// ccNexus: TransformResponseWithContext(targetResp []byte, isStreaming bool, ctx *StreamContext)
    fn transform_stream_chunk(&self, chunk: &[u8], ctx: &mut StreamContext) -> Vec<Bytes>;
    
    /// Finalize streaming (flush buffers, emit final events)
    /// 
    /// Called when upstream closes the stream.
    /// Must emit any buffered content and final events (e.g., message_stop, [DONE]).
    fn finalize_stream(&self, ctx: &mut StreamContext) -> Vec<Bytes>;
}
```


#### 1.5 Registry 实现
文件：`src-tauri/src/transformer/registry.rs`

```rust
//! Transformer registry for dynamic protocol converter lookup.
//!
//! Reference: ccNexus/internal/transformer/registry.go

use super::{Protocol, Transformer};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Thread-safe transformer registry
pub struct TransformerRegistry {
    transformers: RwLock<HashMap<(Protocol, Protocol), Arc<dyn Transformer>>>,
}

impl TransformerRegistry {
    /// Create new empty registry
    pub fn new() -> Self {
        Self {
            transformers: RwLock::new(HashMap::new()),
        }
    }
    
    /// Register a transformer
    /// 
    /// Reference: ccNexus Register(t Transformer)
    pub fn register(&self, transformer: Arc<dyn Transformer>) {
        let key = (transformer.source_protocol(), transformer.target_protocol());
        self.transformers
            .write()
            .unwrap()
            .insert(key, transformer);
    }
    
    /// Get transformer by protocol pair
    /// 
    /// Reference: ccNexus Get(name string) (Transformer, error)
    pub fn get(&self, source: Protocol, target: Protocol) -> Option<Arc<dyn Transformer>> {
        self.transformers
            .read()
            .unwrap()
            .get(&(source, target))
            .cloned()
    }
    
    /// Check if transformer is registered
    /// 
    /// Reference: ccNexus IsRegistered(name string) bool
    pub fn is_registered(&self, source: Protocol, target: Protocol) -> bool {
        self.transformers
            .read()
            .unwrap()
            .contains_key(&(source, target))
    }
    
    /// List all registered protocol pairs
    pub fn list(&self) -> Vec<(Protocol, Protocol)> {
        self.transformers
            .read()
            .unwrap()
            .keys()
            .copied()
            .collect()
    }
}

impl Default for TransformerRegistry {
    fn default() -> Self {
        Self::new()
    }
}
```


### Step 2: 实现核心转换器 (Day 2-3)

#### 2.1 OpenAI Chat → Anthropic Messages 转换器
文件：`src-tauri/src/transformer/adapters/openai_to_anthropic.rs`

```rust
//! OpenAI Chat Completions to Anthropic Messages transformer.
//!
//! Data Flow:
//! ```
//! OpenAI Request                    Anthropic Request
//! ┌─────────────────┐              ┌─────────────────┐
//! │ model           │──────────────>│ model           │
//! │ messages[]      │              │ messages[]      │
//! │   - role        │──────────────>│   - role        │
//! │   - content     │──────────────>│   - content     │
//! │   - tool_calls  │──────────────>│   (tool_use)    │
//! │ max_tokens      │──────────────>│ max_tokens      │
//! │ temperature     │──────────────>│ temperature     │
//! │ tools[]         │──────────────>│ tools[]         │
//! └─────────────────┘              └─────────────────┘
//! 
//! System message extraction:
//!   messages[0].role=="system" → system field
//! ```
//!
//! Reference: ccNexus/internal/transformer/convert/claude_openai.go:ClaudeReqToOpenAI

use super::super::{Protocol, StreamContext, Transformer};
use axum::body::Bytes;
use serde_json::{json, Value};

pub struct OpenAIToAnthropicTransformer;

impl Transformer for OpenAIToAnthropicTransformer {
    fn name(&self) -> &str {
        "openai_chat_to_anthropic_messages"
    }
    
    fn source_protocol(&self) -> Protocol {
        Protocol::OpenAIChatCompletions
    }
    
    fn target_protocol(&self) -> Protocol {
        Protocol::AnthropicMessages
    }
    
    fn transform_request(&self, body: &Value) -> Result<Value, String> {
        // Extract required fields
        let messages = body["messages"]
            .as_array()
            .ok_or("missing messages array")?;
        let model = body["model"]
            .as_str()
            .ok_or("missing model field")?;
        
        // Separate system messages from conversation messages
        // Reference: ccNexus extractSystemText() and message conversion loop
        let mut system_parts = Vec::new();
        let mut anthropic_messages = Vec::new();
        
        for msg in messages {
            let role = msg["role"].as_str().unwrap_or("");
            
            if role == "system" {
                // Extract system prompt
                // ccNexus: if req.System != nil { systemText := extractSystemText(req.System) }
                if let Some(content) = msg["content"].as_str() {
                    system_parts.push(content.to_string());
                }
            } else {
                // Convert user/assistant/tool messages
                anthropic_messages.push(self.convert_message(msg)?);
            }
        }
        
        // Build Anthropic request
        let mut result = json!({
            "model": model,
            "messages": anthropic_messages,
            "max_tokens": body.get("max_tokens")
                .or(body.get("max_completion_tokens"))
                .and_then(|v| v.as_i64())
                .unwrap_or(4096)
        });
        
        // Add system prompt if present
        if !system_parts.is_empty() {
            result["system"] = json!(system_parts.join("\n"));
        }
        
        // Copy optional fields
        if let Some(temp) = body.get("temperature") {
            result["temperature"] = temp.clone();
        }
        if let Some(stream) = body.get("stream") {
            result["stream"] = stream.clone();
        }
        if let Some(tools) = body.get("tools") {
            result["tools"] = self.convert_tools(tools)?;
        }
        if let Some(tool_choice) = body.get("tool_choice") {
            result["tool_choice"] = tool_choice.clone();
        }
        
        Ok(result)
    }
    
    fn transform_response(&self, body: &Value) -> Result<Value, String> {
        // Non-streaming response transformation
        // Anthropic response → OpenAI chat completion
        
        let content = body["content"]
            .as_array()
            .ok_or("missing content array")?;
        
        // Extract text and tool_use blocks
        // Reference: ccNexus response conversion logic
        let mut text_parts = Vec::new();
        let mut tool_calls = Vec::new();
        
        for (idx, block) in content.iter().enumerate() {
            match block["type"].as_str() {
                Some("text") => {
                    if let Some(text) = block["text"].as_str() {
                        text_parts.push(text);
                    }
                }
                Some("tool_use") => {
                    // Convert Anthropic tool_use to OpenAI tool_call
                    tool_calls.push(json!({
                        "id": block["id"],
                        "type": "function",
                        "index": idx,
                        "function": {
                            "name": block["name"],
                            "arguments": serde_json::to_string(&block["input"])
                                .unwrap_or_default()
                        }
                    }));
                }
                _ => {}
            }
        }
        
        // Build OpenAI response
        let mut message = json!({
            "role": "assistant"
        });
        
        if !text_parts.is_empty() {
            message["content"] = json!(text_parts.join(""));
        }
        if !tool_calls.is_empty() {
            message["tool_calls"] = json!(tool_calls);
        }
        
        let finish_reason = match body["stop_reason"].as_str() {
            Some("end_turn") => "stop",
            Some("tool_use") => "tool_calls",
            Some("max_tokens") => "length",
            _ => "stop"
        };
        
        Ok(json!({
            "id": body["id"],
            "object": "chat.completion",
            "created": chrono::Utc::now().timestamp(),
            "model": body["model"],
            "choices": [{
                "index": 0,
                "message": message,
                "finish_reason": finish_reason
            }],
            "usage": {
                "prompt_tokens": body["usage"]["input_tokens"],
                "completion_tokens": body["usage"]["output_tokens"],
                "total_tokens": body["usage"]["input_tokens"].as_i64().unwrap_or(0)
                    + body["usage"]["output_tokens"].as_i64().unwrap_or(0)
            }
        }))
    }
    
    fn transform_stream_chunk(&self, chunk: &[u8], ctx: &mut StreamContext) -> Vec<Bytes> {
        // Streaming transformation implementation
        // Will be detailed in streaming section
        vec![]
    }
    
    fn finalize_stream(&self, ctx: &mut StreamContext) -> Vec<Bytes> {
        vec![Bytes::from("data: [DONE]\n\n")]
    }
}

impl OpenAIToAnthropicTransformer {
    /// Convert OpenAI message to Anthropic message format
    /// Reference: ccNexus message conversion in ClaudeReqToOpenAI
    fn convert_message(&self, msg: &Value) -> Result<Value, String> {
        let role = msg["role"].as_str().unwrap_or("");
        let content = &msg["content"];
        
        match role {
            "user" | "assistant" => {
                Ok(json!({
                    "role": role,
                    "content": content
                }))
            }
            "tool" => {
                // Convert tool message to tool_result block
                // Reference: ccNexus tool_result handling
                Ok(json!({
                    "role": "user",
                    "content": [{
                        "type": "tool_result",
                        "tool_use_id": msg["tool_call_id"],
                        "content": content
                    }]
                }))
            }
            _ => Err(format!("unsupported role: {}", role))
        }
    }
    
    /// Convert OpenAI tools to Anthropic tools format
    fn convert_tools(&self, tools: &Value) -> Result<Value, String> {
        let tools_array = tools.as_array().ok_or("tools must be array")?;
        let mut anthropic_tools = Vec::new();
        
        for tool in tools_array {
            if tool["type"] == "function" {
                anthropic_tools.push(json!({
                    "name": tool["function"]["name"],
                    "description": tool["function"]["description"],
                    "input_schema": tool["function"]["parameters"]
                }));
            }
        }
        
        Ok(json!(anthropic_tools))
    }
}
```


#### 2.2 Anthropic Messages → OpenAI Chat 转换器
文件：`src-tauri/src/transformer/adapters/anthropic_to_openai.rs`

```rust
//! Anthropic Messages to OpenAI Chat Completions transformer.
//!
//! Data Flow:
//! ```
//! Anthropic Request                 OpenAI Request
//! ┌─────────────────┐              ┌─────────────────┐
//! │ model           │──────────────>│ model           │
//! │ system          │──────────────>│ messages[0]     │
//! │                 │              │   role: system  │
//! │ messages[]      │──────────────>│ messages[1..]   │
//! │   - content[]   │              │   - content     │
//! │     - text      │──────────────>│     (string)    │
//! │     - tool_use  │──────────────>│   - tool_calls  │
//! │     - tool_result│─────────────>│   role: tool    │
//! │ max_tokens      │──────────────>│ max_tokens      │
//! └─────────────────┘              └─────────────────┘
//! ```
//!
//! Reference: ccNexus/internal/transformer/convert/claude_openai.go:OpenAIRespToClaude

use super::super::{Protocol, StreamContext, Transformer};
use axum::body::Bytes;
use serde_json::{json, Value};

pub struct AnthropicToOpenAITransformer;

impl Transformer for AnthropicToOpenAITransformer {
    fn name(&self) -> &str {
        "anthropic_messages_to_openai_chat"
    }
    
    fn source_protocol(&self) -> Protocol {
        Protocol::AnthropicMessages
    }
    
    fn target_protocol(&self) -> Protocol {
        Protocol::OpenAIChatCompletions
    }
    
    fn transform_request(&self, body: &Value) -> Result<Value, String> {
        let model = body["model"].as_str().ok_or("missing model")?;
        let messages = body["messages"].as_array().ok_or("missing messages")?;
        
        let mut openai_messages = Vec::new();
        
        // Add system message if present
        // Reference: ccNexus system prompt handling
        if let Some(system) = body.get("system") {
            let system_text = match system {
                Value::String(s) => s.clone(),
                Value::Array(arr) => {
                    arr.iter()
                        .filter_map(|v| v["text"].as_str())
                        .collect::<Vec<_>>()
                        .join("\n")
                }
                _ => String::new()
            };
            
            if !system_text.is_empty() {
                openai_messages.push(json!({
                    "role": "system",
                    "content": system_text
                }));
            }
        }
        
        // Convert Anthropic messages to OpenAI format
        for msg in messages {
            openai_messages.push(self.convert_message(msg)?);
        }
        
        let mut result = json!({
            "model": model,
            "messages": openai_messages
        });
        
        // Copy optional fields
        if let Some(max_tokens) = body.get("max_tokens") {
            result["max_tokens"] = max_tokens.clone();
        }
        if let Some(temp) = body.get("temperature") {
            result["temperature"] = temp.clone();
        }
        if let Some(stream) = body.get("stream") {
            result["stream"] = stream.clone();
        }
        if let Some(tools) = body.get("tools") {
            result["tools"] = self.convert_tools(tools)?;
        }
        
        Ok(result)
    }
    
    fn transform_response(&self, body: &Value) -> Result<Value, String> {
        // OpenAI response → Anthropic message
        let choices = body["choices"].as_array().ok_or("missing choices")?;
        let choice = choices.get(0).ok_or("empty choices")?;
        let message = &choice["message"];
        
        let mut content_blocks = Vec::new();
        
        // Add text content
        if let Some(text) = message["content"].as_str() {
            if !text.is_empty() {
                content_blocks.push(json!({
                    "type": "text",
                    "text": text
                }));
            }
        }
        
        // Convert tool_calls to tool_use blocks
        if let Some(tool_calls) = message["tool_calls"].as_array() {
            for call in tool_calls {
                let args_str = call["function"]["arguments"].as_str().unwrap_or("{}");
                let args: Value = serde_json::from_str(args_str).unwrap_or(json!({}));
                
                content_blocks.push(json!({
                    "type": "tool_use",
                    "id": call["id"],
                    "name": call["function"]["name"],
                    "input": args
                }));
            }
        }
        
        let stop_reason = match choice["finish_reason"].as_str() {
            Some("stop") => "end_turn",
            Some("tool_calls") => "tool_use",
            Some("length") => "max_tokens",
            _ => "end_turn"
        };
        
        Ok(json!({
            "id": body["id"],
            "type": "message",
            "role": "assistant",
            "content": content_blocks,
            "model": body["model"],
            "stop_reason": stop_reason,
            "usage": {
                "input_tokens": body["usage"]["prompt_tokens"],
                "output_tokens": body["usage"]["completion_tokens"]
            }
        }))
    }
    
    fn transform_stream_chunk(&self, chunk: &[u8], ctx: &mut StreamContext) -> Vec<Bytes> {
        vec![]
    }
    
    fn finalize_stream(&self, ctx: &mut StreamContext) -> Vec<Bytes> {
        let mut output = Vec::new();
        
        // Emit message_stop event
        output.push(Bytes::from("event: message_stop\ndata: {}\n\n"));
        
        output
    }
}

impl AnthropicToOpenAITransformer {
    fn convert_message(&self, msg: &Value) -> Result<Value, String> {
        let role = msg["role"].as_str().unwrap_or("");
        let content = &msg["content"];
        
        // Handle different content formats
        match content {
            Value::String(s) => {
                Ok(json!({
                    "role": role,
                    "content": s
                }))
            }
            Value::Array(blocks) => {
                self.convert_content_blocks(role, blocks)
            }
            _ => Err("invalid content format".to_string())
        }
    }
    
    fn convert_content_blocks(&self, role: &str, blocks: &[Value]) -> Result<Value, String> {
        let mut text_parts = Vec::new();
        let mut tool_calls = Vec::new();
        let mut tool_results = Vec::new();
        
        for block in blocks {
            match block["type"].as_str() {
                Some("text") => {
                    if let Some(text) = block["text"].as_str() {
                        text_parts.push(text);
                    }
                }
                Some("tool_use") => {
                    tool_calls.push(json!({
                        "id": block["id"],
                        "type": "function",
                        "function": {
                            "name": block["name"],
                            "arguments": serde_json::to_string(&block["input"]).unwrap_or_default()
                        }
                    }));
                }
                Some("tool_result") => {
                    tool_results.push(json!({
                        "role": "tool",
                        "tool_call_id": block["tool_use_id"],
                        "content": block["content"]
                    }));
                }
                _ => {}
            }
        }
        
        // Build message based on content type
        if !tool_results.is_empty() {
            // Return tool results as separate messages
            return Ok(json!(tool_results));
        }
        
        let mut msg = json!({ "role": role });
        
        if !text_parts.is_empty() {
            msg["content"] = json!(text_parts.join(""));
        }
        if !tool_calls.is_empty() {
            msg["tool_calls"] = json!(tool_calls);
        }
        
        Ok(msg)
    }
    
    fn convert_tools(&self, tools: &Value) -> Result<Value, String> {
        let tools_array = tools.as_array().ok_or("tools must be array")?;
        let mut openai_tools = Vec::new();
        
        for tool in tools_array {
            openai_tools.push(json!({
                "type": "function",
                "function": {
                    "name": tool["name"],
                    "description": tool["description"],
                    "parameters": tool["input_schema"]
                }
            }));
        }
        
        Ok(json!(openai_tools))
    }
}
```


### Step 3: 删除旧代码 (Day 4)

#### 3.1 删除旧的 mappers 模块

```bash
# 删除旧的 mapper 实现
rm -rf src-tauri/src/mappers/adapters/
rm -f src-tauri/src/mappers/engine.rs
rm -f src-tauri/src/mappers/request.rs
rm -f src-tauri/src/mappers/response.rs
rm -f src-tauri/src/mappers/response_engine.rs
rm -f src-tauri/src/mappers/policy.rs
rm -f src-tauri/src/mappers/normalize.rs

# 保留 canonical.rs 和 helpers.rs（可能被其他模块使用）
# 如果确认无依赖，也可删除
```

#### 3.2 删除旧的 stream_bridge 模块

```bash
# 删除旧的流式转换实现
rm -rf src-tauri/src/proxy/stream_bridge/
```

#### 3.3 更新 mod.rs 导出

文件：`src-tauri/src/mappers/mod.rs`

```rust
//! Mappers module - DEPRECATED, use transformer module instead
//! 
//! This module is kept for backward compatibility during migration.
//! All new code should use src/transformer instead.

// 删除所有旧的导出
// pub use request::*;
// pub use response::*;
// pub use engine::*;

// 只保留必要的类型定义
pub use canonical::MapperSurface;
```


### Step 4: 集成到 Pipeline (Day 5)

#### 4.1 更新 pipeline.rs

文件：`src-tauri/src/proxy/pipeline.rs`

```rust
//! Request processing pipeline - Updated to use Transformer registry
//!
//! Changes:
//! - Removed dependency on mappers/engine
//! - Use TransformerRegistry for protocol conversion
//! - Simplified streaming logic with StreamContext

use crate::transformer::{Protocol, StreamContext, TransformerRegistry};
use std::sync::Arc;

// 在 ServiceState 中添加 registry
pub struct ServiceState {
    pub db: Arc<Database>,
    pub metrics: Arc<Metrics>,
    pub transformer_registry: Arc<TransformerRegistry>, // 新增
}

// 修改 handle_proxy_request 函数
pub(super) async fn handle_proxy_request(
    state: ServiceState,
    method: Method,
    headers: HeaderMap,
    body: Body,
    parsed_path: ParsedPath,
) -> Response {
    // ... 前面的认证和路由逻辑保持不变 ...
    
    // 解析请求体
    let body_bytes = match to_bytes(body, MAX_REQUEST_BODY_BYTES).await {
        Ok(b) => b,
        Err(e) => return error_response(&state, trace_id, format!("body too large: {}", e)),
    };
    
    let request_body: Value = match serde_json::from_slice(&body_bytes) {
        Ok(v) => v,
        Err(e) => return error_response(&state, trace_id, format!("invalid json: {}", e)),
    };
    
    // 确定入口协议和目标协议
    let entry_protocol = detect_entry_protocol(&parsed_path.suffix);
    let target_protocol = rule_protocol_to_transformer_protocol(&route.rule.protocol);
    
    // 使用 Transformer 转换请求
    let upstream_body = if entry_protocol != target_protocol {
        let transformer = state.transformer_registry
            .get(entry_protocol, target_protocol)
            .ok_or_else(|| format!("no transformer for {:?} -> {:?}", entry_protocol, target_protocol))?;
        
        match transformer.transform_request(&request_body) {
            Ok(v) => v,
            Err(e) => return error_response(&state, trace_id, format!("transform error: {}", e)),
        }
    } else {
        request_body.clone()
    };
    
    // 发送上游请求
    let is_streaming = upstream_body["stream"].as_bool().unwrap_or(false);
    
    if is_streaming {
        handle_streaming_request(state, trace_id, route, upstream_body, entry_protocol, target_protocol).await
    } else {
        handle_non_streaming_request(state, trace_id, route, upstream_body, entry_protocol, target_protocol).await
    }
}

// 流式请求处理
async fn handle_streaming_request(
    state: ServiceState,
    trace_id: String,
    route: RouteResolution,
    upstream_body: Value,
    entry_protocol: Protocol,
    target_protocol: Protocol,
) -> Response {
    // 发送上游请求
    let upstream_resp = send_upstream_request(&route, &upstream_body).await?;
    
    // 如果需要协议转换
    if entry_protocol != target_protocol {
        let transformer = state.transformer_registry
            .get(target_protocol, entry_protocol)
            .expect("transformer must exist");
        
        // 创建流式上下文
        let mut ctx = StreamContext::new();
        ctx.model_name = upstream_body["model"].as_str().unwrap_or("").to_string();
        
        // 创建转换流
        let (tx, rx) = mpsc::channel(32);
        
        tokio::spawn(async move {
            let mut stream = upstream_resp.bytes_stream();
            
            while let Some(chunk_result) = stream.try_next().await.transpose() {
                match chunk_result {
                    Ok(chunk) => {
                        // 使用 transformer 转换 chunk
                        let transformed = transformer.transform_stream_chunk(&chunk, &mut ctx);
                        for frame in transformed {
                            let _ = tx.send(Ok(frame)).await;
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e)).await;
                        break;
                    }
                }
            }
            
            // 完成流式处理
            let final_frames = transformer.finalize_stream(&mut ctx);
            for frame in final_frames {
                let _ = tx.send(Ok(frame)).await;
            }
        });
        
        // 返回转换后的流
        let body = Body::from_stream(ReceiverStream::new(rx));
        let mut resp = Response::new(body);
        resp.headers_mut().insert("content-type", "text/event-stream".parse().unwrap());
        return resp;
    }
    
    // 无需转换，直接透传
    Response::new(Body::from_stream(upstream_resp.bytes_stream()))
}

// 非流式请求处理
async fn handle_non_streaming_request(
    state: ServiceState,
    trace_id: String,
    route: RouteResolution,
    upstream_body: Value,
    entry_protocol: Protocol,
    target_protocol: Protocol,
) -> Response {
    // 发送上游请求
    let upstream_resp = send_upstream_request(&route, &upstream_body).await?;
    let upstream_body_bytes = to_bytes(upstream_resp.into_body(), MAX_RESPONSE_BODY_BYTES).await?;
    let upstream_response: Value = serde_json::from_slice(&upstream_body_bytes)?;
    
    // 如果需要协议转换
    let downstream_body = if entry_protocol != target_protocol {
        let transformer = state.transformer_registry
            .get(target_protocol, entry_protocol)
            .expect("transformer must exist");
        
        transformer.transform_response(&upstream_response)?
    } else {
        upstream_response
    };
    
    // 返回响应
    (StatusCode::OK, Json(downstream_body)).into_response()
}

// 协议映射辅助函数
fn detect_entry_protocol(suffix: &str) -> Protocol {
    if suffix.contains("/messages") {
        Protocol::AnthropicMessages
    } else if suffix.contains("/responses") {
        Protocol::OpenAIResponses
    } else {
        Protocol::OpenAIChatCompletions
    }
}

fn rule_protocol_to_transformer_protocol(rule_protocol: &RuleProtocol) -> Protocol {
    match rule_protocol {
        RuleProtocol::Anthropic => Protocol::AnthropicMessages,
        RuleProtocol::OpenAI => Protocol::OpenAIChatCompletions,
        RuleProtocol::OpenAIResponses => Protocol::OpenAIResponses,
    }
}
```


#### 4.2 注册 Transformers

文件：`src-tauri/src/main.rs` 或 `src-tauri/src/lib.rs`

```rust
//! Application initialization with transformer registration

use crate::transformer::{TransformerRegistry, adapters::*};
use std::sync::Arc;

/// Initialize transformer registry with all protocol converters
pub fn init_transformer_registry() -> Arc<TransformerRegistry> {
    let registry = Arc::new(TransformerRegistry::new());
    
    // Register OpenAI <-> Anthropic transformers
    registry.register(Arc::new(OpenAIToAnthropicTransformer));
    registry.register(Arc::new(AnthropicToOpenAITransformer));
    
    // Register OpenAI Chat <-> Responses transformers
    registry.register(Arc::new(ChatToResponsesTransformer));
    registry.register(Arc::new(ResponsesToChatTransformer));
    
    // Register Responses <-> Anthropic transformers
    registry.register(Arc::new(ResponsesToAnthropicTransformer));
    
    // Log registered transformers
    let pairs = registry.list();
    println!("Registered {} transformer pairs:", pairs.len());
    for (source, target) in pairs {
        println!("  {:?} -> {:?}", source, target);
    }
    
    registry
}

// 在应用启动时调用
#[tokio::main]
async fn main() {
    // ... 其他初始化 ...
    
    let transformer_registry = init_transformer_registry();
    
    let state = ServiceState {
        db: Arc::new(db),
        metrics: Arc::new(metrics),
        transformer_registry,
    };
    
    // ... 启动服务器 ...
}
```


## 数据流转详细文档

### 请求转换流程图

```
客户端请求 (OpenAI Chat)
│
├─ POST /oc/group1/chat/completions
│  {
│    "model": "gpt-4",
│    "messages": [
│      {"role": "system", "content": "You are helpful"},
│      {"role": "user", "content": "Hello"}
│    ],
│    "max_tokens": 100
│  }
│
▼
Pipeline: detect_entry_protocol()
│  → Protocol::OpenAIChatCompletions
│
▼
Routing: 查找 group1 的 activeRule
│  → rule.protocol = "anthropic"
│  → target_protocol = Protocol::AnthropicMessages
│
▼
Registry: get(OpenAIChatCompletions, AnthropicMessages)
│  → OpenAIToAnthropicTransformer
│
▼
Transformer: transform_request()
│
├─ 提取 system message
│  messages[0].role == "system"
│  → system: "You are helpful"
│
├─ 转换 user message
│  messages[1] → messages[0]
│  {
│    "role": "user",
│    "content": "Hello"
│  }
│
└─ 构建 Anthropic 请求
   {
     "model": "gpt-4",
     "system": "You are helpful",
     "messages": [
       {"role": "user", "content": "Hello"}
     ],
     "max_tokens": 100
   }
│
▼
发送到上游 Anthropic API
│
▼
接收上游响应
{
  "id": "msg_123",
  "type": "message",
  "role": "assistant",
  "content": [
    {"type": "text", "text": "Hi there!"}
  ],
  "stop_reason": "end_turn",
  "usage": {"input_tokens": 20, "output_tokens": 5}
}
│
▼
Transformer: transform_response()
│
├─ 提取 content blocks
│  content[0].type == "text"
│  → text: "Hi there!"
│
├─ 映射 stop_reason
│  "end_turn" → "stop"
│
└─ 构建 OpenAI 响应
   {
     "id": "msg_123",
     "object": "chat.completion",
     "model": "gpt-4",
     "choices": [{
       "index": 0,
       "message": {
         "role": "assistant",
         "content": "Hi there!"
       },
       "finish_reason": "stop"
     }],
     "usage": {
       "prompt_tokens": 20,
       "completion_tokens": 5,
       "total_tokens": 25
     }
   }
│
▼
返回给客户端
```


### 流式响应转换流程图

```
上游 Anthropic SSE 流
│
├─ event: message_start
│  data: {"type":"message_start","message":{"id":"msg_123","model":"claude-3"}}
│
▼
StreamContext 初始化
│  message_start_sent = false
│  message_id = ""
│  content_block_started = false
│
▼
Transformer: transform_stream_chunk()
│
├─ 解析 SSE 事件
│  event = "message_start"
│  payload.message.id = "msg_123"
│
├─ 更新 context
│  ctx.message_id = "msg_123"
│  ctx.model_name = "claude-3"
│  ctx.message_start_sent = true
│
└─ 生成 OpenAI chunk
   data: {"id":"msg_123","object":"chat.completion.chunk","model":"claude-3","choices":[{"index":0,"delta":{"role":"assistant"},"finish_reason":null}]}
│
├─ event: content_block_start
│  data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}
│
▼
Transformer: transform_stream_chunk()
│
├─ 更新 context
│  ctx.content_block_started = true
│  ctx.content_index = 0
│
└─ 不输出（等待实际内容）
│
├─ event: content_block_delta
│  data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}
│
▼
Transformer: transform_stream_chunk()
│
└─ 生成 OpenAI chunk
   data: {"id":"msg_123","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}
│
├─ event: content_block_delta
│  data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" world"}}
│
▼
Transformer: transform_stream_chunk()
│
└─ 生成 OpenAI chunk
   data: {"id":"msg_123","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"content":" world"},"finish_reason":null}]}
│
├─ event: content_block_stop
│  data: {"type":"content_block_stop","index":0}
│
▼
Transformer: transform_stream_chunk()
│
└─ 不输出（等待 message_delta）
│
├─ event: message_delta
│  data: {"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":5}}
│
▼
Transformer: transform_stream_chunk()
│
├─ 更新 context
│  ctx.output_tokens = 5
│  ctx.finish_reason_sent = true
│
└─ 生成 OpenAI chunk
   data: {"id":"msg_123","object":"chat.completion.chunk","choices":[{"index":0,"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":20,"completion_tokens":5}}
│
├─ event: message_stop
│  data: {"type":"message_stop"}
│
▼
Transformer: finalize_stream()
│
└─ 生成终止标记
   data: [DONE]
│
▼
返回给客户端
```


### 工具调用转换流程

```
OpenAI 请求 (带工具调用)
│
{
  "model": "gpt-4",
  "messages": [
    {"role": "user", "content": "What's the weather in SF?"}
  ],
  "tools": [{
    "type": "function",
    "function": {
      "name": "get_weather",
      "description": "Get weather",
      "parameters": {"type": "object", "properties": {"city": {"type": "string"}}}
    }
  }]
}
│
▼
Transformer: transform_request()
│
├─ 转换 tools 数组
│  OpenAI format → Anthropic format
│  {
│    "type": "function",
│    "function": {...}
│  }
│  ↓
│  {
│    "name": "get_weather",
│    "description": "Get weather",
│    "input_schema": {"type": "object", ...}
│  }
│
└─ 生成 Anthropic 请求
   {
     "model": "gpt-4",
     "messages": [...],
     "tools": [{
       "name": "get_weather",
       "description": "Get weather",
       "input_schema": {...}
     }]
   }
│
▼
上游 Anthropic 响应
{
  "content": [
    {"type": "tool_use", "id": "toolu_123", "name": "get_weather", "input": {"city": "SF"}}
  ],
  "stop_reason": "tool_use"
}
│
▼
Transformer: transform_response()
│
├─ 提取 tool_use blocks
│  content[0].type == "tool_use"
│  → id: "toolu_123"
│  → name: "get_weather"
│  → input: {"city": "SF"}
│
├─ 转换为 OpenAI tool_calls
│  {
│    "id": "toolu_123",
│    "type": "function",
│    "function": {
│      "name": "get_weather",
│      "arguments": "{\"city\":\"SF\"}"  // JSON 字符串
│    }
│  }
│
├─ 映射 stop_reason
│  "tool_use" → "tool_calls"
│
└─ 生成 OpenAI 响应
   {
     "choices": [{
       "message": {
         "role": "assistant",
         "tool_calls": [{
           "id": "toolu_123",
           "type": "function",
           "function": {
             "name": "get_weather",
             "arguments": "{\"city\":\"SF\"}"
           }
         }]
       },
       "finish_reason": "tool_calls"
     }]
   }
│
▼
客户端执行工具并发送结果
{
  "messages": [
    {"role": "user", "content": "What's the weather in SF?"},
    {"role": "assistant", "tool_calls": [...]},
    {"role": "tool", "tool_call_id": "toolu_123", "content": "Sunny, 72°F"}
  ]
}
│
▼
Transformer: transform_request()
│
├─ 识别 tool message
│  role == "tool"
│  → tool_call_id: "toolu_123"
│  → content: "Sunny, 72°F"
│
├─ 转换为 Anthropic tool_result
│  {
│    "role": "user",
│    "content": [{
│      "type": "tool_result",
│      "tool_use_id": "toolu_123",
│      "content": "Sunny, 72°F"
│    }]
│  }
│
└─ 继续对话...
```


## 测试验证步骤

### Step 5: 单元测试 (Day 6)

#### 5.1 Transformer 单元测试

文件：`src-tauri/src/transformer/adapters/tests.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    
    #[test]
    fn test_openai_to_anthropic_request_basic() {
        let transformer = OpenAIToAnthropicTransformer;
        
        let input = json!({
            "model": "gpt-4",
            "messages": [
                {"role": "system", "content": "You are helpful"},
                {"role": "user", "content": "Hello"}
            ],
            "max_tokens": 100
        });
        
        let result = transformer.transform_request(&input).unwrap();
        
        assert_eq!(result["model"], "gpt-4");
        assert_eq!(result["system"], "You are helpful");
        assert_eq!(result["messages"][0]["role"], "user");
        assert_eq!(result["messages"][0]["content"], "Hello");
        assert_eq!(result["max_tokens"], 100);
    }
    
    #[test]
    fn test_openai_to_anthropic_tool_calls() {
        let transformer = OpenAIToAnthropicTransformer;
        
        let input = json!({
            "model": "gpt-4",
            "messages": [
                {"role": "user", "content": "What's the weather?"}
            ],
            "tools": [{
                "type": "function",
                "function": {
                    "name": "get_weather",
                    "description": "Get weather",
                    "parameters": {"type": "object"}
                }
            }]
        });
        
        let result = transformer.transform_request(&input).unwrap();
        
        assert_eq!(result["tools"][0]["name"], "get_weather");
        assert_eq!(result["tools"][0]["input_schema"]["type"], "object");
    }
    
    #[test]
    fn test_anthropic_to_openai_response_with_tool_use() {
        let transformer = OpenAIToAnthropicTransformer;
        
        let input = json!({
            "id": "msg_123",
            "type": "message",
            "role": "assistant",
            "content": [{
                "type": "tool_use",
                "id": "toolu_1",
                "name": "get_weather",
                "input": {"city": "SF"}
            }],
            "model": "claude-3",
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 10, "output_tokens": 5}
        });
        
        let result = transformer.transform_response(&input).unwrap();
        
        assert_eq!(result["choices"][0]["finish_reason"], "tool_calls");
        assert_eq!(result["choices"][0]["message"]["tool_calls"][0]["id"], "toolu_1");
        assert_eq!(result["choices"][0]["message"]["tool_calls"][0]["function"]["name"], "get_weather");
    }
}
```


#### 5.2 集成测试

文件：`src-tauri/tests/integration_transformer.rs`

```rust
//! Integration tests for transformer-based protocol conversion

#[tokio::test]
async fn test_end_to_end_openai_to_anthropic() {
    // 启动测试服务器
    let app = create_test_app().await;
    
    // 发送 OpenAI 格式请求
    let response = app
        .post("/oc/test-group/chat/completions")
        .json(&json!({
            "model": "gpt-4",
            "messages": [{"role": "user", "content": "Hello"}]
        }))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.status(), 200);
    
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["object"], "chat.completion");
    assert!(body["choices"][0]["message"]["content"].is_string());
}

#[tokio::test]
async fn test_streaming_conversion() {
    let app = create_test_app().await;
    
    let response = app
        .post("/oc/test-group/chat/completions")
        .json(&json!({
            "model": "gpt-4",
            "messages": [{"role": "user", "content": "Count to 3"}],
            "stream": true
        }))
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.headers()["content-type"], "text/event-stream");
    
    let mut stream = response.bytes_stream();
    let mut chunks = Vec::new();
    
    while let Some(chunk) = stream.next().await {
        chunks.push(chunk.unwrap());
    }
    
    let combined = String::from_utf8(chunks.concat()).unwrap();
    
    // 验证 SSE 格式
    assert!(combined.contains("data: "));
    assert!(combined.contains("chat.completion.chunk"));
    assert!(combined.contains("data: [DONE]"));
}
```


## 迁移检查清单

### Day 1: 基础架构
- [ ] 创建 `src-tauri/src/transformer/` 目录结构
- [ ] 实现 `types.rs` - 所有协议类型定义
- [ ] 实现 `context.rs` - StreamContext
- [ ] 实现 `mod.rs` - Transformer trait
- [ ] 实现 `registry.rs` - TransformerRegistry
- [ ] 编译通过，无警告

### Day 2-3: 核心转换器
- [ ] 实现 `OpenAIToAnthropicTransformer`
  - [ ] transform_request() - 请求转换
  - [ ] transform_response() - 响应转换
  - [ ] convert_message() - 消息转换
  - [ ] convert_tools() - 工具转换
- [ ] 实现 `AnthropicToOpenAITransformer`
  - [ ] transform_request() - 请求转换
  - [ ] transform_response() - 响应转换
  - [ ] convert_content_blocks() - 内容块转换
- [ ] 单元测试覆盖率 >80%

### Day 4: 删除旧代码
- [ ] 删除 `src-tauri/src/mappers/adapters/`
- [ ] 删除 `src-tauri/src/mappers/engine.rs`
- [ ] 删除 `src-tauri/src/mappers/request.rs`
- [ ] 删除 `src-tauri/src/mappers/response.rs`
- [ ] 删除 `src-tauri/src/proxy/stream_bridge/`
- [ ] 更新 `mappers/mod.rs` 导出
- [ ] 编译通过

### Day 5: Pipeline 集成
- [ ] 更新 `ServiceState` 添加 `transformer_registry`
- [ ] 修改 `handle_proxy_request()` 使用 registry
- [ ] 实现 `handle_streaming_request()` 新逻辑
- [ ] 实现 `handle_non_streaming_request()` 新逻辑
- [ ] 在 `main.rs` 注册所有 transformers
- [ ] 编译通过，无警告

### Day 6: 测试验证
- [ ] 运行所有单元测试 - 全部通过
- [ ] 运行集成测试 - 全部通过
- [ ] 手动测试：OpenAI → Anthropic
- [ ] 手动测试：Anthropic → OpenAI
- [ ] 手动测试：流式响应
- [ ] 手动测试：工具调用
- [ ] 性能测试：延迟 <5ms
- [ ] 内存测试：无泄漏

### Day 7: 文档和发布
- [ ] 更新 README.md
- [ ] 更新 API 文档
- [ ] 创建迁移说明文档
- [ ] 代码审查
- [ ] 合并到主分支
- [ ] 发布新版本


## 关键注意事项

### 1. 严格遵循 ccNexus 转换逻辑

**必须对齐的转换规则**：

#### System Prompt 处理
```rust
// ccNexus: extractSystemText()
// OpenAI system message → Anthropic system field
messages[0].role == "system" → system: "content"

// 多个 system messages 合并
system_parts.join("\n")
```

#### Tool Calls 转换
```rust
// OpenAI → Anthropic
{
  "id": "call_123",
  "type": "function",
  "function": {
    "name": "get_weather",
    "arguments": "{\"city\":\"SF\"}"  // JSON 字符串
  }
}
↓
{
  "type": "tool_use",
  "id": "call_123",
  "name": "get_weather",
  "input": {"city": "SF"}  // JSON 对象
}

// 关键：arguments 是字符串，input 是对象
// 必须 JSON.parse(arguments) → input
```

#### Stop Reason 映射
```rust
// ccNexus 映射规则
Anthropic → OpenAI:
  "end_turn" → "stop"
  "tool_use" → "tool_calls"
  "max_tokens" → "length"

OpenAI → Anthropic:
  "stop" → "end_turn"
  "tool_calls" → "tool_use"
  "length" → "max_tokens"
```

#### Thinking Blocks 处理
```rust
// ccNexus: 跳过 thinking blocks
// Reference: claude_openai.go:53-56
case "thinking":
    // Skip thinking blocks - they are Claude's internal reasoning
    // and should not be forwarded to other APIs
    hasThinking = true
    continue
```

### 2. StreamContext 状态管理

**关键状态转换**：

```
初始状态
  ↓
message_start 事件
  → message_start_sent = true
  → message_id = "..."
  ↓
content_block_start 事件
  → content_block_started = true
  → content_index++
  ↓
content_block_delta 事件（多次）
  → 累积内容
  ↓
content_block_stop 事件
  → content_block_started = false
  ↓
message_delta 事件
  → finish_reason_sent = true
  → 累积 token usage
  ↓
message_stop 事件
  → 流结束
```

**并发安全**：
- 每个请求独立的 StreamContext
- 不共享状态
- 使用 `&mut StreamContext` 确保独占访问

### 3. 工具调用缓冲

**ccNexus 的缓冲策略**：

```rust
// 工具调用参数可能分多个 chunk 发送
// 必须缓冲完整后再发送

// Chunk 1:
delta.tool_calls[0].function.arguments = "{\"city\":"

// Chunk 2:
delta.tool_calls[0].function.arguments = "\"SF\"}"

// 缓冲逻辑：
if tool_call.index is Some {
    // 新的 tool call
    if let Some(prev) = ctx.current_tool_call {
        // 发送完整的前一个 tool call
        emit_tool_use_block(prev, &ctx.tool_call_buffer);
    }
    ctx.current_tool_call = Some(tool_call);
    ctx.tool_call_buffer.clear();
}

ctx.tool_call_buffer.push_str(arguments);

// 在 finalize_stream() 中发送最后一个
```

### 4. 错误处理

**必须处理的边缘情况**：

```rust
// 1. 空 messages 数组
if messages.is_empty() {
    return Err("messages array is empty".to_string());
}

// 2. 缺少必需字段
let model = body["model"]
    .as_str()
    .ok_or("missing model field")?;

// 3. 无效的 JSON
let args: Value = serde_json::from_str(args_str)
    .unwrap_or(json!({}));  // 降级处理

// 4. 未知的 content block type
match block["type"].as_str() {
    Some("text") => { /* ... */ }
    Some("tool_use") => { /* ... */ }
    _ => {
        // 忽略未知类型，不报错
        continue;
    }
}
```


### 5. 性能优化要点

**Registry 查找优化**：
```rust
// 使用 HashMap，O(1) 查找
// 键是 (Protocol, Protocol) 元组
// 值是 Arc<dyn Transformer>，避免克隆

// 读多写少场景，使用 RwLock
let transformer = registry.transformers
    .read()  // 多个并发读
    .unwrap()
    .get(&(source, target))
    .cloned();  // 只克隆 Arc，不克隆 Transformer
```

**StreamContext 内存优化**：
```rust
// 预分配缓冲区容量
pub fn new() -> Self {
    Self {
        tool_call_buffer: String::with_capacity(1024),
        thinking_buffer: String::with_capacity(512),
        // ...
    }
}
```

**避免不必要的序列化**：
```rust
// 错误：多次序列化
let json_str = serde_json::to_string(&value)?;
let json_value: Value = serde_json::from_str(&json_str)?;

// 正确：直接使用 Value
let json_value = value.clone();
```

### 6. 调试和监控

**添加详细日志**：
```rust
use tracing::{debug, info, warn, error};

impl Transformer for OpenAIToAnthropicTransformer {
    fn transform_request(&self, body: &Value) -> Result<Value, String> {
        debug!("Transforming OpenAI request to Anthropic");
        debug!("Input: {}", serde_json::to_string_pretty(body).unwrap_or_default());
        
        let result = self.do_transform(body)?;
        
        debug!("Output: {}", serde_json::to_string_pretty(&result).unwrap_or_default());
        Ok(result)
    }
}
```

**性能指标**：
```rust
use std::time::Instant;

let start = Instant::now();
let result = transformer.transform_request(body)?;
let elapsed = start.elapsed();

if elapsed.as_millis() > 5 {
    warn!("Slow transformation: {}ms", elapsed.as_millis());
}
```


## 总结

### 迁移优势

1. **架构清晰**
   - Transformer trait 统一接口
   - Registry 动态查找
   - StreamContext 显式状态管理

2. **易于扩展**
   - 添加新协议只需实现 trait + 注册
   - 无需修改核心 pipeline
   - 协议矩阵自然扩展

3. **与 ccNexus 对齐**
   - 转换逻辑完全一致
   - 类型定义对应
   - 行为可预测

4. **可维护性**
   - 每个转换器独立文件
   - 清晰的数据流文档
   - 完善的测试覆盖

### 风险控制

虽然是破坏性迁移，但风险可控：

1. **编译时保证**
   - Rust 类型系统防止大部分错误
   - 删除旧代码后编译失败会立即发现

2. **测试覆盖**
   - 单元测试验证转换逻辑
   - 集成测试验证端到端流程
   - 手动测试覆盖关键场景

3. **快速回滚**
   - Git 回滚到迁移前
   - 重新编译即可恢复

### 时间估算

- **Day 1**: 基础架构 (4-6 小时)
- **Day 2-3**: 核心转换器 (8-12 小时)
- **Day 4**: 删除旧代码 (2-3 小时)
- **Day 5**: Pipeline 集成 (4-6 小时)
- **Day 6**: 测试验证 (4-6 小时)
- **Day 7**: 文档和发布 (2-4 小时)

**总计**: 24-37 小时 (3-5 个工作日)

### 成功标准

- ✅ 所有测试通过
- ✅ 编译无警告
- ✅ 性能无回退 (<5ms 开销)
- ✅ 功能完全对等
- ✅ 文档完整

## 参考资料

### ccNexus 源码
- `internal/transformer/types.go` - 类型定义
- `internal/transformer/registry.go` - 注册表
- `internal/transformer/convert/claude_openai.go` - OpenAI ↔ Claude 转换
- `internal/transformer/tool_chain.go` - 工具链处理

### oc-proxy 现有代码
- `src-tauri/src/mappers/` - 当前 mapper 实现
- `src-tauri/src/proxy/stream_bridge/` - 当前流式转换
- `src-tauri/src/proxy/pipeline.rs` - 请求处理流程

### 协议文档
- [Anthropic Messages API](https://docs.anthropic.com/claude/reference/messages_post)
- [OpenAI Chat Completions API](https://platform.openai.com/docs/api-reference/chat)
- [OpenAI Responses API](https://platform.openai.com/docs/api-reference/responses)

---

**文档版本**: 1.0  
**创建日期**: 2026-03-05  
**作者**: AI Assistant  
**状态**: 准备执行

