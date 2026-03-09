use crate::transformer::convert::claude_openai;
use crate::transformer::{StreamContext, Transformer};

pub struct OpenAITransformer {
    model: String,
}

impl OpenAITransformer {
    pub fn new(model: String) -> Self {
        Self { model }
    }
}

impl Transformer for OpenAITransformer {
    fn transform_request(&self, claude_req: &[u8]) -> Result<Vec<u8>, String> {
        claude_openai::claude_req_to_openai(claude_req, &self.model)
    }

    fn transform_response(
        &self,
        target_resp: &[u8],
        _is_streaming: bool,
    ) -> Result<Vec<u8>, String> {
        claude_openai::openai_resp_to_claude(target_resp)
    }

    fn transform_response_with_context(
        &self,
        target_resp: &[u8],
        _is_streaming: bool,
        ctx: &mut StreamContext,
    ) -> Result<Vec<u8>, String> {
        claude_openai::openai_stream_to_claude(target_resp, ctx)
    }

    fn name(&self) -> &str {
        "cc_openai"
    }
}
