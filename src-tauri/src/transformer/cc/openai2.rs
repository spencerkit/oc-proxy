use crate::transformer::convert::claude_openai_responses::{self, ResponsesToClaudeOptions};
use crate::transformer::convert::claude_openai_responses_stream;
use crate::transformer::{StreamContext, Transformer};

pub struct OpenAI2Transformer {
    model: String,
    response_options: ResponsesToClaudeOptions,
}

impl OpenAI2Transformer {
    pub fn new(model: String, response_options: ResponsesToClaudeOptions) -> Self {
        Self {
            model,
            response_options,
        }
    }
}

impl Transformer for OpenAI2Transformer {
    fn transform_request(&self, claude_req: &[u8]) -> Result<Vec<u8>, String> {
        claude_openai_responses::claude_req_to_openai_responses(claude_req, &self.model)
    }

    fn transform_response(
        &self,
        target_resp: &[u8],
        _is_streaming: bool,
    ) -> Result<Vec<u8>, String> {
        claude_openai_responses::openai_responses_resp_to_claude_with_options(
            target_resp,
            &self.response_options,
        )
    }

    fn transform_response_with_context(
        &self,
        target_resp: &[u8],
        _is_streaming: bool,
        ctx: &mut StreamContext,
    ) -> Result<Vec<u8>, String> {
        claude_openai_responses_stream::openai_responses_stream_to_claude(target_resp, ctx)
    }

    fn name(&self) -> &str {
        "cc_openai2"
    }
}
