//! Transformer trait and core abstractions.
//!
//! Reference: ccNexus/internal/transformer/transformer.go

pub mod cc;
pub mod convert;
pub mod cx;
pub mod messages_to_responses;
pub mod registry;
pub mod types;

#[cfg(test)]
mod tests;

pub use messages_to_responses::MessagesToResponsesTransformer;
pub use types::StreamContext;

/// Transformer defines the interface for API format transformation
/// Reference: ccNexus Transformer interface
pub trait Transformer: Send + Sync {
    /// Transform Claude format request to target API format
    fn transform_request(&self, claude_req: &[u8]) -> Result<Vec<u8>, String>;

    /// Transform target API format response to Claude format
    fn transform_response(&self, target_resp: &[u8], is_streaming: bool)
        -> Result<Vec<u8>, String>;

    /// Transform target API format response to Claude format with streaming context
    /// This method is used for streaming responses that require context management
    fn transform_response_with_context(
        &self,
        target_resp: &[u8],
        is_streaming: bool,
        ctx: &mut StreamContext,
    ) -> Result<Vec<u8>, String>;

    /// Returns the transformer name
    fn name(&self) -> &str;
}
