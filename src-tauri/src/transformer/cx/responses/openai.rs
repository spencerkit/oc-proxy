use crate::transformer::convert::{openai_chat_responses, openai_chat_responses_stream};
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
    fn transform_request(&self, openai_req: &[u8]) -> Result<Vec<u8>, String> {
        openai_chat_responses::openai_responses_req_to_chat(openai_req, &self.model)
    }

    fn transform_response(
        &self,
        target_resp: &[u8],
        _is_streaming: bool,
    ) -> Result<Vec<u8>, String> {
        openai_chat_responses::openai_chat_resp_to_responses(target_resp)
    }

    fn transform_response_with_context(
        &self,
        target_resp: &[u8],
        _is_streaming: bool,
        ctx: &mut StreamContext,
    ) -> Result<Vec<u8>, String> {
        openai_chat_responses_stream::openai_chat_stream_to_responses(target_resp, ctx)
    }

    fn name(&self) -> &str {
        "cx_resp_openai"
    }
}
