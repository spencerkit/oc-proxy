//! Claude to Gemini conversion (minimal)

use serde_json::{json, Value};

pub fn claude_req_to_gemini(claude_req: &[u8], model: &str) -> Result<Vec<u8>, String> {
    let req: Value = serde_json::from_slice(claude_req)
        .map_err(|e| format!("parse: {}", e))?;

    let gemini_req = json!({
        "model": model,
        "contents": req.get("messages").unwrap_or(&json!([]))
    });

    serde_json::to_vec(&gemini_req).map_err(|e| format!("serialize: {}", e))
}

pub fn gemini_resp_to_claude(gemini_resp: &[u8]) -> Result<Vec<u8>, String> {
    let resp: Value = serde_json::from_slice(gemini_resp)
        .map_err(|e| format!("parse: {}", e))?;

    let claude_resp = json!({
        "id": "gemini-resp",
        "type": "message",
        "role": "assistant",
        "content": [{"type": "text", "text": ""}],
        "stop_reason": "end_turn"
    });

    serde_json::to_vec(&claude_resp).map_err(|e| format!("serialize: {}", e))
}
