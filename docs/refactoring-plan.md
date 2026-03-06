# Proxy Refactoring Plan: ccNexus-Inspired Architecture

## Executive Summary

Refactor oc-proxy's protocol conversion layer using architectural patterns from ccNexus to improve:
- **Maintainability**: Clear separation of concerns with registry pattern
- **Extensibility**: Easy addition of new protocols (Gemini, etc.)
- **Testability**: Isolated transformer units with stateful streaming context
- **Performance**: Optimized streaming with proper state management

## Current Architecture Analysis

### oc-proxy (Rust)
```
Request Flow:
1. pipeline.rs → Route resolution & auth
2. mappers/engine.rs → Canonical model conversion
3. mappers/adapters/* → Protocol-specific encode/decode
4. stream_bridge/* → SSE event transformation
5. Response back through bridge

Strengths:
✓ Canonical model approach (decode → canonical → encode)
✓ Clean separation: request mapping vs response mapping
✓ Comprehensive streaming tests
✓ Type-safe with Rust

Weaknesses:
✗ Tightly coupled adapter dispatch in engine.rs
✗ No registry pattern - adding protocols requires code changes
✗ Stream state scattered across adapters
✗ Limited tool chain support
```

### ccNexus (Go)
```
Transformer Flow:
1. Registry pattern - transformers register themselves
2. Transformer interface: TransformRequest/TransformResponse
3. StreamContext - stateful streaming across events
4. Tool chain handler - recursive API calls
5. Protocol matrix: Claude↔OpenAI, OpenAI↔Gemini, etc.

Strengths:
✓ Registry pattern - dynamic transformer discovery
✓ Clean interface - easy to add new protocols
✓ StreamContext - proper state management
✓ Tool chain support built-in
✓ Comprehensive protocol coverage

Weaknesses:
✗ Go's type system less strict than Rust
✗ Manual JSON marshaling overhead
```

## Proposed Architecture

### Core Concepts

1. **Transformer Registry** (inspired by ccNexus)
   - Dynamic registration of protocol transformers
   - Runtime discovery by protocol pair
   - Decoupled from routing logic

2. **Unified Transformer Interface**
   ```rust
   trait Transformer {
       fn name(&self) -> &str;
       fn transform_request(&self, body: &Value) -> Result<Value, String>;
       fn transform_response(&self, body: &Value, streaming: bool) -> Result<Value, String>;
       fn transform_response_with_context(
           &self,
           body: &Value,
           streaming: bool,
           ctx: &mut StreamContext
       ) -> Result<Value, String>;
   }
   ```

3. **StreamContext** (port from ccNexus)
   - Centralized streaming state
   - Per-request context isolation
   - Handles: message_start, content blocks, tool calls, finish reasons

4. **Tool Chain Handler**
   - Recursive API call support
   - Tool use → tool result → continuation
   - Max depth protection


## Architecture Comparison

### Current (oc-proxy)
```
┌─────────────┐
│  Pipeline   │
└──────┬──────┘
       │
┌──────▼──────┐
│   Engine    │ (dispatch by MapperSurface enum)
└──────┬──────┘
       │
┌──────▼──────────────────┐
│  Adapters (hardcoded)   │
│  - anthropic_messages   │
│  - openai_chat         │
│  - openai_responses    │
└──────┬──────────────────┘
       │
┌──────▼──────┐
│StreamBridge │
└─────────────┘
```

### Proposed (ccNexus-inspired)
```
┌─────────────┐
│  Pipeline   │
└──────┬──────┘
       │
┌──────▼──────────┐
│    Registry     │ (dynamic lookup)
└──────┬──────────┘
       │
┌──────▼──────────────────┐
│  Transformer Trait      │
│  - ClaudeToOpenAI       │
│  - OpenAIToClaude       │
│  - OpenAIToGemini       │
│  - GeminiToOpenAI       │
│  - ... (extensible)     │
└──────┬──────────────────┘
       │
┌──────▼──────────────┐
│  StreamContext      │
│  + ToolChainHandler │
└─────────────────────┘
```

## Implementation Phases

### Phase 1: Foundation (Week 1)
**Goal**: Establish core infrastructure without breaking existing functionality

Tasks:
1. Create transformer module structure
2. Define Transformer trait
3. Port StreamContext from ccNexus
4. Create Registry with thread-safe storage

**Deliverables**:
- Trait definition with comprehensive docs
- Registry implementation with tests
- StreamContext struct
- Zero breaking changes to existing code


### Phase 2: Migrate Existing Adapters (Week 2)
**Goal**: Convert current adapters to Transformer trait implementations

Tasks:
1. Create transformer implementations
   - `AnthropicToOpenAITransformer`
   - `OpenAIToAnthropicTransformer`
   - `OpenAIResponsesToChatTransformer`
   - `ChatToOpenAIResponsesTransformer`

2. Wrap existing adapter logic in Transformer trait
3. Register transformers at startup
4. Add parallel path in pipeline (old + new)
5. Feature flag to switch between implementations

**Deliverables**:
- 4 transformer implementations
- Registration in main.rs
- Integration tests comparing old vs new output
- Feature flag: `use_transformer_registry`

### Phase 3: Enhanced Streaming (Week 3)
**Goal**: Improve streaming with proper state management

Tasks:
1. Integrate StreamContext into transformers
2. Replace stream_bridge with transformer-based streaming
3. Add tool call buffering and reconstruction
4. Implement thinking block handling
5. Add comprehensive streaming tests

**Deliverables**:
- StreamContext fully integrated
- Tool call streaming works correctly
- Thinking blocks properly handled
- 90%+ test coverage on streaming

### Phase 4: Tool Chain Support (Week 4)
**Goal**: Add recursive API call support for tool use

Tasks:
1. Port ToolChainHandler from ccNexus
2. Detect tool_use in responses
3. Execute tool calls (user-provided executor)
4. Construct tool_result messages
5. Recursive API call with depth limit

**Deliverables**:
- ToolChainHandler implementation
- Tool execution interface
- Max depth protection (default: 5)
- End-to-end tool chain tests


### Phase 5: Protocol Expansion (Week 5)
**Goal**: Add new protocol support (Gemini, etc.)

Tasks:
1. Define Gemini protocol types
2. Implement GeminiToOpenAITransformer
3. Implement OpenAIToGeminiTransformer
4. Add Gemini-specific features (thought blocks, function calls)
5. Update routing to support Gemini endpoints

**Deliverables**:
- Gemini protocol support
- 2 new transformers registered
- Gemini streaming tests
- Documentation for adding new protocols

### Phase 6: Cleanup & Migration (Week 6)
**Goal**: Remove old code and finalize migration

Tasks:
1. Remove feature flag
2. Delete old mappers/adapters code
3. Delete old stream_bridge code
4. Update all tests to use new architecture
5. Performance benchmarking (old vs new)
6. Documentation update

**Deliverables**:
- Old code removed
- All tests passing
- Performance report
- Migration guide for contributors

## Detailed Design

### 1. Transformer Trait

```rust
// src-tauri/src/transformer/mod.rs

pub trait Transformer: Send + Sync {
    /// Unique identifier for this transformer
    fn name(&self) -> &str;
    
    /// Source protocol this transformer accepts
    fn source_protocol(&self) -> Protocol;
    
    /// Target protocol this transformer produces
    fn target_protocol(&self) -> Protocol;
    
    /// Transform request from source to target protocol
    fn transform_request(&self, body: &Value) -> Result<Value, String>;
    
    /// Transform non-streaming response
    fn transform_response(&self, body: &Value) -> Result<Value, String>;
    
    /// Transform streaming chunk with context
    fn transform_stream_chunk(
        &self,
        chunk: &[u8],
        ctx: &mut StreamContext
    ) -> Vec<Bytes>;
    
    /// Finalize streaming (flush buffers, emit final events)
    fn finalize_stream(&self, ctx: &mut StreamContext) -> Vec<Bytes>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Protocol {
    AnthropicMessages,
    OpenAIChatCompletions,
    OpenAIResponses,
    Gemini,
}
```


### 2. Registry Implementation

```rust
// src-tauri/src/transformer/registry.rs

use super::{Protocol, Transformer};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

pub struct TransformerRegistry {
    transformers: RwLock<HashMap<(Protocol, Protocol), Arc<dyn Transformer>>>,
}

impl TransformerRegistry {
    pub fn new() -> Self {
        Self {
            transformers: RwLock::new(HashMap::new()),
        }
    }
    
    pub fn register(&self, transformer: Arc<dyn Transformer>) {
        let key = (transformer.source_protocol(), transformer.target_protocol());
        self.transformers.write().unwrap().insert(key, transformer);
    }
    
    pub fn get(&self, source: Protocol, target: Protocol) -> Option<Arc<dyn Transformer>> {
        self.transformers.read().unwrap().get(&(source, target)).cloned()
    }
    
    pub fn is_registered(&self, source: Protocol, target: Protocol) -> bool {
        self.transformers.read().unwrap().contains_key(&(source, target))
    }
}
```


### 3. StreamContext (Ported from ccNexus)

```rust
// src-tauri/src/transformer/context.rs

pub struct StreamContext {
    // Message state
    pub message_start_sent: bool,
    pub message_id: String,
    pub model_name: String,
    
    // Content tracking
    pub content_block_started: bool,
    pub content_index: usize,
    
    // Tool call state
    pub tool_block_started: bool,
    pub tool_block_pending: bool,
    pub current_tool_call: Option<ToolCall>,
    pub tool_call_buffer: String,
    pub tool_index: usize,
    
    // Thinking block state
    pub thinking_block_started: bool,
    pub thinking_index: usize,
    pub in_thinking_tag: bool,
    pub thinking_buffer: String,
    
    // Token usage
    pub input_tokens: usize,
    pub output_tokens: usize,
    
    // Finish state
    pub finish_reason_sent: bool,
}

impl StreamContext {
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
            thinking_block_started: false,
            thinking_index: 0,
            in_thinking_tag: false,
            thinking_buffer: String::new(),
            input_tokens: 0,
            output_tokens: 0,
            finish_reason_sent: false,
        }
    }
}
```


### 4. Example Transformer Implementation

```rust
// src-tauri/src/transformer/adapters/openai_to_anthropic.rs

use super::super::{Protocol, StreamContext, Transformer};
use axum::body::Bytes;
use serde_json::Value;

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
        // Extract OpenAI request fields
        let messages = body["messages"].as_array()
            .ok_or("missing messages")?;
        let model = body["model"].as_str()
            .ok_or("missing model")?;
        
        // Convert to Anthropic format
        let mut anthropic_messages = Vec::new();
        let mut system_prompt = None;
        
        for msg in messages {
            let role = msg["role"].as_str().unwrap_or("");
            let content = &msg["content"];
            
            if role == "system" {
                system_prompt = Some(content.clone());
            } else {
                anthropic_messages.push(json!({
                    "role": role,
                    "content": content
                }));
            }
        }
        
        let mut result = json!({
            "model": model,
            "messages": anthropic_messages,
            "max_tokens": body.get("max_tokens").unwrap_or(&json!(4096))
        });
        
        if let Some(system) = system_prompt {
            result["system"] = system;
        }
        
        Ok(result)
    }
    
    fn transform_response(&self, body: &Value) -> Result<Value, String> {
        // Non-streaming response transformation
        // Convert Anthropic message response to OpenAI chat completion
        Ok(json!({
            "id": body["id"],
            "object": "chat.completion",
            "model": body["model"],
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": body["content"][0]["text"]
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": body["usage"]["input_tokens"],
                "completion_tokens": body["usage"]["output_tokens"]
            }
        }))
    }
    
    fn transform_stream_chunk(&self, chunk: &[u8], ctx: &mut StreamContext) -> Vec<Bytes> {
        // Parse SSE chunk and emit OpenAI-compatible events
        // Implementation details...
        vec![]
    }
    
    fn finalize_stream(&self, ctx: &mut StreamContext) -> Vec<Bytes> {
        vec![Bytes::from("data: [DONE]\n\n")]
    }
}
```


## Migration Strategy

### Backward Compatibility
- Keep existing mappers/adapters during transition
- Feature flag: `TRANSFORMER_REGISTRY_ENABLED`
- Parallel execution: run both old and new, compare outputs
- Gradual rollout: enable per-protocol basis

### Testing Strategy
1. **Unit Tests**: Each transformer in isolation
2. **Integration Tests**: Full request/response cycles
3. **Comparison Tests**: Old vs new output validation
4. **Streaming Tests**: SSE event sequence verification
5. **Performance Tests**: Latency and throughput benchmarks

### Rollback Plan
- Keep old code until Phase 6
- Feature flag allows instant rollback
- Monitoring alerts on error rate changes
- Database schema unchanged (no migration needed)

## Key Benefits

### Maintainability
- Clear separation: each transformer is self-contained
- Easy to understand: one file per protocol pair
- Reduced coupling: registry decouples routing from transformation

### Extensibility
- Add new protocol: implement trait + register
- No changes to core pipeline
- Protocol matrix grows without code sprawl

### Testability
- Mock transformers for testing
- Isolated unit tests per transformer
- StreamContext makes state explicit

### Performance
- Registry lookup: O(1) hash map
- Streaming: proper buffering with context
- Tool chains: controlled recursion depth


## Protocol Conversion Matrix

### Supported Conversions (Current)
| Source | Target | Status | Notes |
|--------|--------|--------|-------|
| OpenAI Chat | Anthropic Messages | ✓ | Full support with streaming |
| Anthropic Messages | OpenAI Chat | ✓ | Full support with streaming |
| OpenAI Chat | OpenAI Responses | ✓ | Full support with streaming |
| OpenAI Responses | OpenAI Chat | ✓ | Full support with streaming |
| OpenAI Responses | Anthropic Messages | ✓ | Full support with streaming |

### Planned Conversions (Phase 5)
| Source | Target | Priority | Complexity |
|--------|--------|----------|------------|
| OpenAI Chat | Gemini | High | Medium |
| Gemini | OpenAI Chat | High | Medium |
| Anthropic Messages | Gemini | Medium | Medium |
| Gemini | Anthropic Messages | Medium | Medium |
| OpenAI Responses | Gemini | Low | Low |
| Gemini | OpenAI Responses | Low | Low |

## Risk Assessment

### High Risk
- **Breaking Changes**: Streaming event format changes
  - Mitigation: Comprehensive integration tests
  - Rollback: Feature flag instant disable

- **Performance Regression**: Registry lookup overhead
  - Mitigation: Benchmark before/after
  - Target: <1ms overhead per request

### Medium Risk
- **Tool Chain Bugs**: Recursive call handling
  - Mitigation: Max depth limits, extensive testing
  - Monitoring: Track tool chain depth metrics

- **State Management**: StreamContext lifecycle
  - Mitigation: Clear ownership rules, tests
  - Documentation: Context lifecycle diagram

### Low Risk
- **Memory Usage**: Context per request
  - Mitigation: Context is small (~1KB)
  - Monitoring: Track memory per request


## Success Metrics

### Code Quality
- [ ] Test coverage: >85% for transformer module
- [ ] Zero clippy warnings in new code
- [ ] All existing tests pass
- [ ] Documentation coverage: 100% public APIs

### Performance
- [ ] Request latency: <5ms overhead vs current
- [ ] Streaming latency: <10ms first byte
- [ ] Memory: <2KB per request context
- [ ] Throughput: Match or exceed current

### Functionality
- [ ] All current protocol pairs work
- [ ] Streaming events match exactly
- [ ] Tool calls properly buffered
- [ ] Error handling comprehensive

## Next Steps

1. **Review & Approval** (Week 0)
   - Team review of this plan
   - Identify concerns and risks
   - Adjust timeline if needed

2. **Spike Phase** (Week 0.5)
   - Prototype transformer trait
   - Test registry pattern
   - Validate StreamContext design
   - Confirm feasibility

3. **Execute Phases 1-6** (Weeks 1-6)
   - Follow implementation plan
   - Weekly progress reviews
   - Adjust as needed

4. **Production Rollout** (Week 7)
   - Enable feature flag gradually
   - Monitor metrics closely
   - Full rollout if stable

## Questions for Discussion

1. Should we support bidirectional transformers (single impl for both directions)?
2. Do we need versioning for transformers (v1, v2)?
3. Should tool chain execution be synchronous or async?
4. What's the priority order for Gemini support?
5. Should we expose transformer registry via API for debugging?


## Appendix A: Code Examples from ccNexus

### Registry Pattern (Go)
```go
// ccNexus: internal/transformer/registry.go
var (
    registry = make(map[string]Transformer)
    mu       sync.RWMutex
)

func Register(t Transformer) {
    mu.Lock()
    defer mu.Unlock()
    registry[t.Name()] = t
}

func Get(name string) (Transformer, error) {
    mu.RLock()
    defer mu.RUnlock()
    t, ok := registry[name]
    if !ok {
        return nil, fmt.Errorf("transformer not found: %s", name)
    }
    return t, nil
}
```

### Transformer Interface (Go)
```go
// ccNexus: internal/transformer/transformer.go
type Transformer interface {
    TransformRequest(claudeReq []byte) (targetReq []byte, err error)
    TransformResponse(targetResp []byte, isStreaming bool) (claudeResp []byte, err error)
    TransformResponseWithContext(targetResp []byte, isStreaming bool, ctx *StreamContext) (claudeResp []byte, err error)
    Name() string
}
```

### StreamContext (Go)
```go
// ccNexus: internal/transformer/types.go (simplified)
type StreamContext struct {
    MessageStartSent     bool
    ContentBlockStarted  bool
    ThinkingBlockStarted bool
    ToolBlockStarted     bool
    MessageID            string
    ModelName            string
    InputTokens          int
    OutputTokens         int
    ContentIndex         int
    FinishReasonSent     bool
    CurrentToolCall      *OpenAIToolCall
    ToolCallBuffer       string
}
```


## Appendix B: File Structure

### New Directory Layout
```
src-tauri/src/
├── transformer/
│   ├── mod.rs              # Trait definition, Protocol enum
│   ├── registry.rs         # TransformerRegistry implementation
│   ├── context.rs          # StreamContext struct
│   ├── tool_chain.rs       # ToolChainHandler (Phase 4)
│   └── adapters/
│       ├── mod.rs
│       ├── openai_to_anthropic.rs
│       ├── anthropic_to_openai.rs
│       ├── chat_to_responses.rs
│       ├── responses_to_chat.rs
│       ├── responses_to_anthropic.rs
│       └── gemini/         # Phase 5
│           ├── mod.rs
│           ├── openai_to_gemini.rs
│           └── gemini_to_openai.rs
├── mappers/                # Keep during migration
│   └── ...                 # Delete in Phase 6
└── proxy/
    ├── pipeline.rs         # Update to use registry
    └── ...
```

## Appendix C: References

### ccNexus Repository
- Transformer pattern: `internal/transformer/`
- Protocol conversions: `internal/transformer/convert/`
- Streaming: `internal/proxy/streaming.go`
- Tool chain: `internal/transformer/tool_chain.go`

### oc-proxy Current Code
- Mappers: `src-tauri/src/mappers/`
- Stream bridge: `src-tauri/src/proxy/stream_bridge/`
- Pipeline: `src-tauri/src/proxy/pipeline.rs`

---

**Document Version**: 1.0  
**Created**: 2026-03-05  
**Author**: AI Assistant (Claude)  
**Status**: Draft for Review

