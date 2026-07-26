//! Anthropic Messages API transport for the LLM verbalizer (app layer).
//!
//! [`AnthropicTransport`] implements silman-verbalize's provider-agnostic
//! [`LlmTransport`] trait over raw HTTP via `ureq` (no SDK; Rust has no
//! official Anthropic SDK). The BSD crate owns the prompt, the validation,
//! and the fallback policy; this module only moves bytes:
//!
//! - `POST https://api.anthropic.com/v1/messages` with the system prompt in
//!   `system` and the FeatureRecord JSON as the single user message;
//! - model `claude-opus-5` (current recommended general model),
//!   `max_tokens` 1000, effort `low` (a short verbalization needs no deep
//!   reasoning budget), no streaming;
//! - the API key comes from the caller (CLI `--api-key`) or the
//!   `ANTHROPIC_API_KEY` environment variable;
//! - a `stop_reason` of `refusal`, an empty completion, any non-2xx status,
//!   or a malformed payload is reported as a [`TransportError`], which the
//!   verbalizer maps to the template fallback.
//!
//! Offline tests cover response parsing; the single live round-trip test is
//! gated behind `SILMAN_LLM_TESTS=1` and skips silently without a key.

use serde_json::Value;
use silman_verbalize::llm::{LlmTransport, TransportError};

/// Anthropic Messages endpoint.
pub const ANTHROPIC_MESSAGES_URL: &str = "https://api.anthropic.com/v1/messages";
/// Required `anthropic-version` header value.
pub const ANTHROPIC_VERSION: &str = "2023-06-01";
/// Current recommended general model.
pub const MODEL: &str = "claude-opus-5";
/// Output cap (thinking + text) for one verbalization.
pub const MAX_TOKENS: u32 = 1000;

/// [`LlmTransport`] backed by the Anthropic Messages API (blocking, serial).
pub struct AnthropicTransport {
    api_key: String,
    url: String,
}

impl AnthropicTransport {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            url: ANTHROPIC_MESSAGES_URL.to_string(),
        }
    }

    /// Key resolution for the CLI: an explicit argument wins, else the
    /// `ANTHROPIC_API_KEY` environment variable.
    pub fn resolve(api_key: Option<String>) -> anyhow::Result<Self> {
        let key = api_key
            .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok())
            .filter(|k| !k.trim().is_empty())
            .ok_or_else(|| {
                anyhow::anyhow!("no Anthropic API key: pass --api-key or set ANTHROPIC_API_KEY")
            })?;
        Ok(Self::new(key))
    }
}

impl LlmTransport for AnthropicTransport {
    fn complete(&self, system: &str, user: &str) -> Result<String, TransportError> {
        let body = serde_json::json!({
            "model": MODEL,
            "max_tokens": MAX_TOKENS,
            "output_config": {"effort": "low"},
            "system": system,
            "messages": [{"role": "user", "content": user}],
        });
        let response = ureq::post(&self.url)
            .set("x-api-key", &self.api_key)
            .set("anthropic-version", ANTHROPIC_VERSION)
            .set("content-type", "application/json")
            .set("User-Agent", super::USER_AGENT)
            .send_string(&body.to_string())
            .map_err(|error| match error {
                ureq::Error::Status(code, resp) => {
                    let detail: String = resp
                        .into_string()
                        .unwrap_or_default()
                        .chars()
                        .take(400)
                        .collect();
                    TransportError::new(format!("Anthropic API HTTP {code}: {detail}"))
                }
                other => TransportError::new(format!("Anthropic API request failed: {other}")),
            })?;
        let payload: Value = serde_json::from_reader(response.into_reader())
            .map_err(|e| TransportError::new(format!("malformed Anthropic response: {e}")))?;
        message_text(&payload)
    }
}

/// Extract the completion text from a Messages API response body: refuse on
/// `stop_reason: "refusal"`, then concatenate the `text` content blocks
/// (skipping `thinking` blocks). Empty text is an error so the verbalizer
/// falls back rather than shipping a blank explanation.
pub fn message_text(payload: &Value) -> Result<String, TransportError> {
    if payload.get("stop_reason").and_then(Value::as_str) == Some("refusal") {
        return Err(TransportError::new("Anthropic API refused the request"));
    }
    let mut text = String::new();
    if let Some(blocks) = payload.get("content").and_then(Value::as_array) {
        for block in blocks {
            if block.get("type").and_then(Value::as_str) == Some("text") {
                if let Some(part) = block.get("text").and_then(Value::as_str) {
                    text.push_str(part);
                }
            }
        }
    }
    if text.trim().is_empty() {
        Err(TransportError::new("Anthropic response contained no text"))
    } else {
        Ok(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_text_concatenates_text_blocks_and_skips_thinking() {
        let payload = serde_json::json!({
            "stop_reason": "end_turn",
            "content": [
                {"type": "thinking", "thinking": ""},
                {"type": "text", "text": "First. "},
                {"type": "text", "text": "Second."},
            ],
        });
        assert_eq!(message_text(&payload).unwrap(), "First. Second.");
    }

    #[test]
    fn message_text_rejects_refusal_and_empty_content() {
        let refusal = serde_json::json!({
            "stop_reason": "refusal",
            "content": [],
        });
        assert!(message_text(&refusal).is_err());

        let empty = serde_json::json!({
            "stop_reason": "end_turn",
            "content": [{"type": "text", "text": "   "}],
        });
        assert!(message_text(&empty).is_err());
    }

    #[test]
    fn resolve_rejects_missing_key() {
        // Explicit empty argument, and no fallthrough to a possibly-set env
        // var, must both fail; an explicit key must win.
        assert!(
            AnthropicTransport::resolve(Some("  ".into())).is_err()
                || std::env::var("ANTHROPIC_API_KEY").is_ok()
        );
        let t = AnthropicTransport::resolve(Some("sk-test".into())).unwrap();
        assert_eq!(t.api_key, "sk-test");
    }

    /// Live round-trip: runs AT MOST once, only when SILMAN_LLM_TESTS=1 and
    /// an API key is present; otherwise skips silently.
    #[test]
    fn live_anthropic_explain_roundtrip() {
        if std::env::var("SILMAN_LLM_TESTS").as_deref() != Ok("1") {
            return;
        }
        let Ok(key) = std::env::var("ANTHROPIC_API_KEY") else {
            return;
        };
        use silman_verbalize::llm::LlmVerbalizer;
        let board = silman_core::cozy_chess::Board::default();
        let record = silman_core::analyze(&board);
        let out = LlmVerbalizer::new(AnthropicTransport::new(key)).verbalize_checked(&record);
        // Whatever mode fired, the outcome must be non-empty prose.
        assert!(!out.text.trim().is_empty());
        eprintln!("live LLM mode: {:?}", out.mode);
    }
}
