//! Transformer registry - simplified
//! ccNexus uses direct function calls, not registry pattern

pub struct TransformerRegistry;

impl TransformerRegistry {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TransformerRegistry {
    fn default() -> Self {
        Self::new()
    }
}
