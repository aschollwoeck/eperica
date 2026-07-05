# Plan — 126 bot HTTP timeouts

**Status:** Draft (spec approved)

Two client constructions in `crates/bots` move from `reqwest::Client::new()` (NO default
timeout) to `Client::builder()` with explicit bounds:
- `client.rs` (the game ApiClient): connect 5 s, total 30 s — game responses are milliseconds.
- `strategist.rs` (AnthropicBackend): connect 5 s, total 120 s — LLM responses are slow, and
  this call also holds a fleet semaphore permit (the accepted 122 in-tick risk), so it must be
  bounded too.
Builder failure is a loud startup `expect` (AC1 — no fallback). A timeout surfaces through the
existing error mapping (`ApiFailure::Http` → `Outcome::Transient`, pinned) for the game client,
and as an advise `Err` (prior strategy persists) for the strategist.

Constitution: P7 n/a (operational HTTP-client settings, not in-game durations); no server or
domain change. Tests: the Transient classification pin; behavior otherwise byte-identical
(existing lib + e2e suites).
