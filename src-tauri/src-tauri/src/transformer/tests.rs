//! Basic tests for transformer infrastructure

#[cfg(test)]
mod tests {
    use crate::transformer::{registry::TransformerRegistry, Protocol, StreamContext, Transformer};
    use axum::body::Bytes;
    use serde_json::{json, Value};
    use std::sync::Arc;

    // Mock transformer for testing
    struct MockTransformer;

    impl Transformer for MockTransformer {
        fn name(&self) -> &str {
            "mock_transformer"
        }

        fn source_protocol(&self) -> Protocol {
            Protocol::OpenAIChatCompletions
        }

        fn target_protocol(&self) -> Protocol {
            Protocol::AnthropicMessages
        }

        fn transform_request(&self, body: &Value) -> Result<Value, String> {
            Ok(json!({"transformed": true, "original": body}))
        }

        fn transform_response(&self, body: &Value) -> Result<Value, String> {
            Ok(json!({"transformed": true, "original": body}))
        }

        fn transform_stream_chunk(&self, _chunk: &[u8], _ctx: &mut StreamContext) -> Vec<Bytes> {
            vec![]
        }

        fn finalize_stream(&self, _ctx: &mut StreamContext) -> Vec<Bytes> {
            vec![]
        }
    }

    #[test]
    fn test_registry_register_and_get() {
        let registry = TransformerRegistry::new();
        let transformer = Arc::new(MockTransformer);

        registry.register(transformer.clone());

        let retrieved = registry.get(Protocol::OpenAIChatCompletions, Protocol::AnthropicMessages);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name(), "mock_transformer");
    }

    #[test]
    fn test_registry_is_registered() {
        let registry = TransformerRegistry::new();
        registry.register(Arc::new(MockTransformer));

        assert!(registry.is_registered(Protocol::OpenAIChatCompletions, Protocol::AnthropicMessages));
        assert!(!registry.is_registered(Protocol::AnthropicMessages, Protocol::OpenAIChatCompletions));
    }

    #[test]
    fn test_stream_context_default() {
        let ctx = StreamContext::new();
        assert!(!ctx.message_start_sent);
        assert_eq!(ctx.content_index, 0);
        assert_eq!(ctx.input_tokens, 0);
    }
}
