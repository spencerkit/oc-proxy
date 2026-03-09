use crate::transformer::convert::common::override_request_model;
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
    fn transform_request(&self, claude_req: &[u8]) -> Result<Vec<u8>, String> {
        override_request_model(claude_req, &self.model)
    }

    fn transform_response(
        &self,
        target_resp: &[u8],
        _is_streaming: bool,
    ) -> Result<Vec<u8>, String> {
        Ok(target_resp.to_vec())
    }

    fn transform_response_with_context(
        &self,
        target_resp: &[u8],
        _is_streaming: bool,
        _ctx: &mut StreamContext,
    ) -> Result<Vec<u8>, String> {
        Ok(target_resp.to_vec())
    }

    fn name(&self) -> &str {
        "cc_claude"
    }
}
