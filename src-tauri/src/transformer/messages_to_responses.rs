//! Messages to Responses transformer
//! Anthropic Messages API -> OpenAI Responses API

use super::convert::{claude_openai, claude_openai_stream, openai_claude};
use super::{StreamContext, Transformer};

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

    fn transform_response(
        &self,
        target_resp: &[u8],
        is_streaming: bool,
    ) -> Result<Vec<u8>, String> {
        if is_streaming {
            Ok(target_resp.to_vec())
        } else {
            openai_claude::openai_resp_to_claude(target_resp)
        }
    }

    fn transform_response_with_context(
        &self,
        target_resp: &[u8],
        is_streaming: bool,
        ctx: &mut StreamContext,
    ) -> Result<Vec<u8>, String> {
        if is_streaming {
            claude_openai_stream::claude_stream_to_openai(target_resp, ctx)
        } else {
            openai_claude::openai_resp_to_claude(target_resp)
        }
    }

    fn name(&self) -> &str {
        "messages_to_responses"
    }
}
