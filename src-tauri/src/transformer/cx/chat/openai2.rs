use crate::transformer::convert::{openai_chat_responses, openai_chat_responses_stream};
use crate::transformer::{StreamContext, Transformer};

pub struct OpenAI2Transformer {
    model: String,
}

impl OpenAI2Transformer {
    pub fn new(model: String) -> Self {
        Self { model }
    }
}

impl Transformer for OpenAI2Transformer {
    fn transform_request(&self, openai_req: &[u8]) -> Result<Vec<u8>, String> {
        openai_chat_responses::openai_chat_to_responses(openai_req, &self.model)
    }

    fn transform_response(
        &self,
        target_resp: &[u8],
        _is_streaming: bool,
    ) -> Result<Vec<u8>, String> {
        openai_chat_responses::openai_responses_to_chat(target_resp)
    }

    fn transform_response_with_context(
        &self,
        target_resp: &[u8],
        _is_streaming: bool,
        ctx: &mut StreamContext,
    ) -> Result<Vec<u8>, String> {
        openai_chat_responses_stream::openai_responses_stream_to_chat(target_resp, ctx)
    }

    fn name(&self) -> &str {
        "cx_chat_openai2"
    }
}
