use crate::transformer::convert::{claude_openai, claude_openai_stream};
use crate::transformer::{StreamContext, Transformer};

pub struct ClaudeTransformer {
    model: String,
}

impl ClaudeTransformer {
    pub fn new(model: String) -> Self {
        Self { model }
    }
}

impl Transformer for ClaudeTransformer {
    fn transform_request(&self, openai_req: &[u8]) -> Result<Vec<u8>, String> {
        claude_openai::openai_req_to_claude(openai_req, &self.model)
    }

    fn transform_response(
        &self,
        target_resp: &[u8],
        _is_streaming: bool,
    ) -> Result<Vec<u8>, String> {
        claude_openai::claude_resp_to_openai(target_resp, &self.model)
    }

    fn transform_response_with_context(
        &self,
        target_resp: &[u8],
        _is_streaming: bool,
        ctx: &mut StreamContext,
    ) -> Result<Vec<u8>, String> {
        claude_openai_stream::claude_stream_to_openai(target_resp, ctx)
    }

    fn name(&self) -> &str {
        "cx_chat_claude"
    }
}
