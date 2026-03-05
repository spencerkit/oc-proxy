//! Messages to Responses transformer
//! Anthropic Messages API -> OpenAI Responses API

use super::{Transformer, StreamContext};
use super::convert::{claude_openai, openai_claude};

pub struct MessagesToResponsesTransformer {
    model: String,
}

impl MessagesToResponsesTransformer {
    pub fn new(model: String) -> Self {
        Self { model }
    }
}

impl Transformer for MessagesToResponsesTransformer {
    fn transform_request(&self, claude_req: &[u8]) -> Result<Vec<u8>, String> {
        claude_openai::claude_req_to_openai(claude_req, &self.model)
    }

    fn transform_response(&self, target_resp: &[u8], _is_streaming: bool) -> Result<Vec<u8>, String> {
        openai_claude::openai_resp_to_claude(target_resp)
    }

    fn transform_response_with_context(
        &self,
        target_resp: &[u8],
        _is_streaming: bool,
        _ctx: &mut StreamContext,
    ) -> Result<Vec<u8>, String> {
        openai_claude::openai_resp_to_claude(target_resp)
    }

    fn name(&self) -> &str {
        "messages_to_responses"
    }
}
