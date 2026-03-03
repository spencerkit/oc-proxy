//! Module Overview
//! Canonical protocol-neutral data model used by all adapters.
//! Acts as the stable intermediate representation between protocol-specific request/response formats.

use serde_json::Value;

/// Supported protocol surfaces for mapping adapters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapperSurface {
    AnthropicMessages,
    OpenaiChatCompletions,
    OpenaiResponses,
}

/// Runtime options passed through mapping pipeline.
#[derive(Clone, Debug)]
pub struct MapOptions {
    pub strict_mode: bool,
    pub target_model: String,
}

impl MapOptions {
    /// Build mapping options with strict-mode switch and resolved target model.
    pub fn new(strict_mode: bool, target_model: &str) -> Self {
        Self {
            strict_mode,
            target_model: target_model.to_string(),
        }
    }
}

/// Canonical role enum shared across protocol adapters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CanonicalRole {
    System,
    User,
    Assistant,
    Tool,
    Other(String),
}

impl CanonicalRole {
    /// Parse role string into canonical enum (unknown roles preserved in `Other`).
    pub fn from_str(role: &str) -> Self {
        match role {
            "system" => Self::System,
            "user" => Self::User,
            "assistant" => Self::Assistant,
            "tool" => Self::Tool,
            other => Self::Other(other.to_string()),
        }
    }

    /// Convert canonical role enum back to role string.
    pub fn as_str(&self) -> &str {
        match self {
            Self::System => "system",
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::Tool => "tool",
            Self::Other(role) => role.as_str(),
        }
    }
}

/// Canonical content blocks used to represent text/tool interactions.
#[derive(Clone, Debug)]
pub enum CanonicalBlock {
    Text(String),
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
    },
}

/// Canonical message made of role plus structured content blocks.
#[derive(Clone, Debug)]
pub struct CanonicalMessage {
    pub role: CanonicalRole,
    pub blocks: Vec<CanonicalBlock>,
}

/// Canonical tool declaration used by adapters.
#[derive(Clone, Debug)]
pub struct CanonicalTool {
    pub name: String,
    pub description: Option<Value>,
    pub input_schema: Value,
}

/// Canonical tool choice abstraction.
#[derive(Clone, Debug)]
pub struct CanonicalToolChoice {
    pub kind: String,
    pub name: Option<String>,
}

/// Canonical request model shared by all request adapters.
#[derive(Clone, Debug)]
pub struct CanonicalRequest {
    pub model: String,
    pub messages: Vec<CanonicalMessage>,
    pub max_tokens: Option<Value>,
    pub temperature: Option<Value>,
    pub top_p: Option<Value>,
    pub stream: bool,
    pub system: Option<Value>,
    pub tools: Option<Vec<CanonicalTool>>,
    pub tool_choice: Option<CanonicalToolChoice>,
    pub stop: Option<Value>,
    pub thinking: Option<Value>,
    pub context_management: Option<Value>,
}

/// Canonical tool call emitted by assistant response.
#[derive(Clone, Debug)]
pub struct CanonicalToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

/// Canonical finish reason abstraction for protocol differences.
#[derive(Clone, Debug)]
pub enum CanonicalFinishReason {
    Stop,
    ToolUse,
    MaxTokens,
    Other(String),
}

/// Canonical token usage structure for responses.
#[derive(Clone, Debug)]
pub struct CanonicalUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: Option<u64>,
}

/// Canonical response model shared by all response adapters.
#[derive(Clone, Debug)]
pub struct CanonicalResponse {
    pub id: String,
    pub created: i64,
    pub model: String,
    pub text: String,
    pub tool_calls: Vec<CanonicalToolCall>,
    pub finish_reason: CanonicalFinishReason,
    pub usage: CanonicalUsage,
}
