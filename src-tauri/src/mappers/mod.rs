//! Module Overview
//! Mapper module exports.
//! Exposes request/response mapping entrypoints and canonical surface enums.

mod adapters;
mod canonical;
mod engine;
pub(crate) mod helpers;
mod normalize;
mod policy;
mod request;
mod response;
mod response_engine;

#[allow(unused_imports)]
pub use canonical::MapperSurface;
pub use normalize::normalize_openai_request;
#[allow(unused_imports)]
pub use request::{
    map_anthropic_to_openai_completions_request, map_anthropic_to_openai_request,
    map_anthropic_to_openai_responses_request, map_openai_to_anthropic_request,
    map_request_by_surface,
};
#[allow(unused_imports)]
pub use response::{
    map_anthropic_to_openai_response, map_openai_chat_to_responses,
    map_openai_to_anthropic_response, map_response_by_surface,
};
pub(crate) use adapters::anthropic_messages::OpenaiChatToAnthropicStreamMapper;
pub(crate) use adapters::openai_chat_completions::OpenaiResponsesToChatStreamMapper;
pub(crate) use adapters::openai_responses::OpenaiChatToResponsesStreamMapper;

#[cfg(test)]
mod tests;
