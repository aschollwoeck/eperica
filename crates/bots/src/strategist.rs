//! LLM backend seam for the strategist.
//!
//! `StrategistBackend` is dyn-compatible (via `async_trait`); the runner stores
//! `Option<Arc<dyn StrategistBackend>>`.  Two implementations provided:
//!
//! - [`AnthropicBackend`] — real Anthropic Messages API via reqwest (HTTPS).
//!   The API key is **never** logged, even in error paths.
//! - [`ScriptedBackend`] — deterministic fake for tests; never calls the network.
//!
//! Also contains [`LlmBudget`]: the fleet-wide rolling-hour call counter.

use std::collections::VecDeque;
use std::sync::Mutex;

use async_trait::async_trait;

// ---------------------------------------------------------------------------
// StrategistBackend trait
// ---------------------------------------------------------------------------

/// Dyn-compatible seam for the LLM backend.
///
/// The runner stores `Option<Arc<dyn StrategistBackend>>`.
/// Each call to [`advise`] passes the assembled prompt and receives the model's
/// raw text reply, or an error string (never contains the API key).
#[async_trait]
pub trait StrategistBackend: Send + Sync {
    /// Ask the backend to produce a strategy reply for `prompt`.
    ///
    /// Returns the raw text of the model's first content block.
    /// Errors are descriptive but must NEVER include the API key.
    async fn advise(&self, prompt: &str) -> Result<String, String>;
}

// ---------------------------------------------------------------------------
// AnthropicBackend — pure helpers (testable without HTTP)
// ---------------------------------------------------------------------------

/// Build the JSON request body for an Anthropic Messages API call.
///
/// Pure function — testable without any HTTP interaction.
pub fn request_body(model: &str, prompt: &str) -> serde_json::Value {
    serde_json::json!({
        "model": model,
        "max_tokens": 1024,
        "messages": [{"role": "user", "content": prompt}]
    })
}

/// Extract `content[0].text` from a successful Anthropic Messages API response.
///
/// Pure function — testable without any HTTP interaction.
/// Returns `Err` when the path `content[0].text` is missing or not a string.
/// Never panics.
pub fn extract_text(v: &serde_json::Value) -> Result<String, String> {
    v.get("content")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|block| block.get("text"))
        .and_then(|t| t.as_str())
        .map(str::to_owned)
        .ok_or_else(|| "missing or non-string content[0].text in Anthropic API response".to_owned())
}

// ---------------------------------------------------------------------------
// AnthropicBackend
// ---------------------------------------------------------------------------

/// Anthropic Messages-API backend.
///
/// POSTs to `https://api.anthropic.com/v1/messages` using reqwest.
/// The `key` field is **never** logged or included in error strings.
pub struct AnthropicBackend {
    key: String,
    model: String,
    http: reqwest::Client,
}

impl AnthropicBackend {
    /// Create a new backend.  `model` is the model ID (e.g. `"claude-haiku-4-5-20251001"`).
    pub fn new(key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            model: model.into(),
            // 126: explicit timeouts here too — this call runs INSIDE a bot's tick while the
            // fleet semaphore permit is held, so a hung Anthropic request would freeze the
            // fleet exactly like a hung game-API request. Total is generous (LLM responses
            // are slow); a timeout surfaces as an advise Err → prior strategy persists.
            http: reqwest::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(5))
                .timeout(std::time::Duration::from_secs(120))
                .build()
                .expect("HTTP client construction must succeed at startup"),
        }
    }
}

#[async_trait]
impl StrategistBackend for AnthropicBackend {
    async fn advise(&self, prompt: &str) -> Result<String, String> {
        let body = request_body(&self.model, prompt);
        // The bots crate uses reqwest without the `json` feature; bodies go
        // through serde_json manually (see Cargo.toml note on NO json feature).
        let body_str = serde_json::to_string(&body)
            .map_err(|e| format!("failed to serialise Anthropic request body: {e}"))?;

        let resp = self
            .http
            .post("https://api.anthropic.com/v1/messages")
            // Key header — value is never logged even in error paths.
            .header("x-api-key", &self.key)
            .header("anthropic-version", "2023-06-01")
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body_str)
            .send()
            .await
            .map_err(|e| format!("HTTP transport error contacting Anthropic API: {e}"))?;

        let status = resp.status().as_u16();
        let text = resp
            .text()
            .await
            .map_err(|e| format!("failed to read Anthropic API response body: {e}"))?;

        if !(200..300).contains(&status) {
            // Include status and a body snippet for diagnostics; NEVER include the key.
            let snippet: String = text.chars().take(200).collect();
            return Err(format!("Anthropic API returned HTTP {status}: {snippet}"));
        }

        let json: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| format!("Anthropic API response is not valid JSON: {e}"))?;

        extract_text(&json)
    }
}

// ---------------------------------------------------------------------------
// ScriptedBackend
// ---------------------------------------------------------------------------

/// Deterministic fake backend for tests.
///
/// Returns pre-scripted replies in order (pop from front); when the queue is
/// exhausted, every subsequent call returns `Err("scripted backend exhausted")`.
/// Never touches the network.
pub struct ScriptedBackend {
    replies: Mutex<VecDeque<Result<String, String>>>,
}

impl ScriptedBackend {
    /// Create a new scripted backend with the given reply queue.
    pub fn new(replies: Vec<Result<String, String>>) -> Self {
        Self {
            replies: Mutex::new(replies.into_iter().collect()),
        }
    }
}

#[async_trait]
impl StrategistBackend for ScriptedBackend {
    async fn advise(&self, _prompt: &str) -> Result<String, String> {
        self.replies
            .lock()
            .expect("ScriptedBackend mutex poisoned")
            .pop_front()
            .unwrap_or_else(|| Err("scripted backend exhausted".to_owned()))
    }
}

// ---------------------------------------------------------------------------
// LlmBudget
// ---------------------------------------------------------------------------

/// Fleet-wide rolling-hour call counter for the LLM strategist.
///
/// Shared across all bot tick tasks as `Arc<Mutex<LlmBudget>>`.
/// All methods operate on Unix milliseconds (`now_ms`).
///
/// `try_take` is the sole write operation: it prunes events older than one
/// rolling hour, records the current call, and returns whether it was allowed.
pub struct LlmBudget {
    /// Timestamps of calls within the rolling 1-hour window (oldest first).
    events: VecDeque<i64>,
}

impl LlmBudget {
    /// Create an empty budget counter.
    pub fn new() -> Self {
        Self {
            events: VecDeque::new(),
        }
    }

    /// Attempt to record a call at `now_ms` against a `per_hour` limit.
    ///
    /// Returns `true` and records the call when under the limit.
    /// Returns `false` (and does NOT record) when at or over the limit.
    ///
    /// Events older than one rolling hour (now_ms − 3 600 000 ms) are pruned
    /// before the check so the limit is truly rolling.
    pub fn try_take(&mut self, now_ms: i64, per_hour: u32) -> bool {
        const ONE_HOUR_MS: i64 = 3_600_000;
        let cutoff = now_ms - ONE_HOUR_MS;
        // Prune stale events (the deque is ordered oldest-first).
        while self
            .events
            .front()
            .copied()
            .map(|t| t <= cutoff)
            .unwrap_or(false)
        {
            self.events.pop_front();
        }
        if self.events.len() < per_hour as usize {
            self.events.push_back(now_ms);
            true
        } else {
            false
        }
    }
}

impl Default for LlmBudget {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // ScriptedBackend — sequencing
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn scripted_backend_returns_replies_in_order() {
        let b = ScriptedBackend::new(vec![
            Ok("first".to_owned()),
            Err("transient error".to_owned()),
            Ok("third".to_owned()),
        ]);
        assert_eq!(b.advise("p").await, Ok("first".to_owned()));
        assert_eq!(b.advise("p").await, Err("transient error".to_owned()));
        assert_eq!(b.advise("p").await, Ok("third".to_owned()));
    }

    #[tokio::test]
    async fn scripted_backend_exhausted_returns_err() {
        let b = ScriptedBackend::new(vec![Ok("only".to_owned())]);
        b.advise("p").await.unwrap();
        let r = b.advise("p").await;
        assert_eq!(r, Err("scripted backend exhausted".to_owned()));
    }

    #[tokio::test]
    async fn scripted_backend_empty_is_immediately_exhausted() {
        let b = ScriptedBackend::new(vec![]);
        assert_eq!(
            b.advise("p").await,
            Err("scripted backend exhausted".to_owned())
        );
    }

    // -----------------------------------------------------------------------
    // AnthropicBackend pure helpers — request_body shape
    // -----------------------------------------------------------------------

    #[test]
    fn request_body_has_correct_model_and_max_tokens() {
        let v = request_body("claude-haiku-4-5-20251001", "hello");
        assert_eq!(v["model"], "claude-haiku-4-5-20251001");
        assert_eq!(v["max_tokens"], 1024);
    }

    #[test]
    fn request_body_has_single_user_message() {
        let v = request_body("some-model", "what do I do?");
        let msgs = v["messages"].as_array().expect("messages must be an array");
        assert_eq!(msgs.len(), 1, "exactly one message");
        assert_eq!(msgs[0]["role"], "user");
        assert_eq!(msgs[0]["content"], "what do I do?");
    }

    #[test]
    fn request_body_prompt_is_embedded_verbatim() {
        let prompt = "attack now or wait?";
        let v = request_body("m", prompt);
        assert_eq!(v["messages"][0]["content"], prompt);
    }

    // -----------------------------------------------------------------------
    // AnthropicBackend pure helpers — extract_text valid/missing/empty
    // -----------------------------------------------------------------------

    #[test]
    fn extract_text_valid_response() {
        let v = serde_json::json!({
            "content": [{"type": "text", "text": "the reply"}]
        });
        assert_eq!(extract_text(&v), Ok("the reply".to_owned()));
    }

    #[test]
    fn extract_text_missing_content_key_is_err() {
        let v = serde_json::json!({"id": "msg_123", "role": "assistant"});
        assert!(extract_text(&v).is_err(), "missing 'content' must be Err");
    }

    #[test]
    fn extract_text_empty_content_array_is_err() {
        let v = serde_json::json!({"content": []});
        assert!(extract_text(&v).is_err(), "empty content array must be Err");
    }

    #[test]
    fn extract_text_content_block_missing_text_field_is_err() {
        // Tool-use blocks have type/id but no text.
        let v = serde_json::json!({
            "content": [{"type": "tool_use", "id": "tu_xyz", "name": "some_tool"}]
        });
        assert!(
            extract_text(&v).is_err(),
            "content block without 'text' field must be Err"
        );
    }

    #[test]
    fn extract_text_text_is_not_string_is_err() {
        let v = serde_json::json!({
            "content": [{"type": "text", "text": 42}]
        });
        assert!(extract_text(&v).is_err(), "non-string 'text' must be Err");
    }

    // -----------------------------------------------------------------------
    // LlmBudget — rolling-hour counter
    // -----------------------------------------------------------------------

    #[test]
    fn budget_allows_calls_under_limit() {
        let mut b = LlmBudget::new();
        let now = 1_700_000_000_000i64;
        assert!(b.try_take(now, 3), "1st of 3 allowed");
        assert!(b.try_take(now, 3), "2nd of 3 allowed");
        assert!(b.try_take(now, 3), "3rd of 3 allowed");
    }

    #[test]
    fn budget_blocks_at_limit() {
        let mut b = LlmBudget::new();
        let now = 1_700_000_000_000i64;
        b.try_take(now, 2);
        b.try_take(now, 2);
        assert!(
            !b.try_take(now, 2),
            "3rd call should be blocked when limit=2"
        );
    }

    #[test]
    fn budget_blocks_at_limit_does_not_record() {
        // After a blocked call, the event count must not change.
        let mut b = LlmBudget::new();
        let now = 1_700_000_000_000i64;
        b.try_take(now, 1); // fills budget
        b.try_take(now, 1); // blocked
        // One hour later: exactly 1 event should be in the window.
        let later = now + 1_000;
        // Budget still full (event is within the window).
        assert!(
            !b.try_take(later, 1),
            "should still be blocked before the hour expires"
        );
    }

    #[test]
    fn budget_prunes_events_older_than_one_hour() {
        let mut b = LlmBudget::new();
        let t0 = 1_700_000_000_000i64;
        b.try_take(t0, 2);
        b.try_take(t0, 2); // now full

        // More than 1 hour later: both events fall outside the rolling window.
        let t1 = t0 + 3_600_001;
        assert!(
            b.try_take(t1, 2),
            "after >1h, stale events should be pruned and call allowed"
        );
    }

    #[test]
    fn budget_event_at_exact_one_hour_boundary_is_pruned() {
        // Cutoff = now - 3_600_000. An event at exactly (now - 3_600_000) satisfies
        // `event <= cutoff` and must be pruned.
        let mut b = LlmBudget::new();
        let t0 = 1_700_000_000_000i64;
        b.try_take(t0, 1); // fills budget (1/1)

        let t1 = t0 + 3_600_000; // exactly 1h later; cutoff = t0
        // t0 <= t0 → pruned → budget has room
        assert!(
            b.try_take(t1, 1),
            "event at the exact 1-hour boundary must be pruned and allow a new call"
        );
    }

    #[test]
    fn budget_per_hour_zero_always_blocks() {
        let mut b = LlmBudget::new();
        let now = 1_700_000_000_000i64;
        // per_hour = 0 → events.len() (0) < 0 is false → always blocked.
        assert!(!b.try_take(now, 0), "per_hour=0 must always block");
    }
}
