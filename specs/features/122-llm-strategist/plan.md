# Plan — 122 the LLM strategist

**Status:** Verified (built as planned; reviewer APPROVE)

## Constitution check

- **P3 (spirit):** the decision layer stays pure — `Strategy` is data; the biases live inside
  `plan_tick` as unit-tested rules. All I/O (the Anthropic call, message sending) stays in the
  runner/backend modules.
- **P4:** untouched; AC8 pins zero server change. The strategist can only act through the bot's
  agent key; the fog boundary is the digest/map the bot already has.
- **P11 (client-side):** one LLM call per bot per interval under a fleet-wide hourly budget; the
  prompt is bounded; strategist failures never block the reflex tick.
- **No-surprising-fallback rule (121 operator directive):** invalid strategist output is rejected
  loudly and the previous strategy persists — never clamped, never guessed, never defaulted.

## Module changes (`crates/bots`)

| Module | Change |
|---|---|
| `strategy.rs` (new) | `Strategy { focus: Focus, aggression: Option<u8>, raid_quadrant: Option<Quadrant>, settle_quadrant: Option<Quadrant>, motto: String }`, `Focus { Economy, Military, Expansion, Balanced }` (`Balanced` = default, biases nothing); `parse_reply(&str) -> Result<StrategistReply, String>` — strict serde (`deny_unknown_fields`), explicit range validation (aggression ≤ 3), `StrategistReply { strategy: Strategy, message: Option<OutboundMessage> }`; prompt assembly `build_prompt(&Digest, Option<&MapWindow>, &Persona, &Strategy) -> String` (pure, bounded: capped lists — 5 report heads, 10 map rows summarised) |
| `policy.rs` | `plan_tick(…, strategy: &Strategy, …)` — biases: `Military` ⇒ training floor ×2, raid party max +4; `Economy` ⇒ raids require garrison ≥ 2×floor; `Expansion` ⇒ the settler chain outranks the core-doctrine remainder once Residence exists; `aggression` override replaces the persona value everywhere it's read; `raid_quadrant`/`settle_quadrant` re-order candidate sorting (quadrant-match first, then distance). `Strategy::default()` must be a provable no-op (AC1 test: identical intents with/without) |
| `strategist.rs` (new) | `trait StrategistBackend { async fn advise(&self, prompt: &str) -> Result<String, String> }`; `AnthropicBackend { key, model, http }` → POST `https://api.anthropic.com/v1/messages` (`max_tokens` 1024, one user message, `anthropic-version: 2023-06-01`), extracting the first text block; `ScriptedBackend(Vec<String>)` for tests. The JSON-only instruction lives in the prompt's system section |
| `runner.rs` | per-bot `strategy: Strategy` + `next_strategist_at_ms`; the tick checks due-ness (interval default 4h, ±persona jitter, in-window only) AND a fleet `LlmBudget` (pure rolling-hour counter, default 12/h) before calling the backend off the tick's critical path (the reflex tick proceeds regardless); apply-or-reject per AC3; send the optional DM via the executor's message call (one per cycle); `--dry-run` logs instead of applying/sending |
| `main.rs` | key detection (`ANTHROPIC_API_KEY` / `EPB_ANTHROPIC_KEY`), `EPB_LLM_MODEL` (default `claude-haiku-4-5-20251001`), `--no-llm`, `--llm-budget N` |

## Prompt contract (fixed template)

System: "You are the strategist for {name}, a {tribe} chieftain in a Travian-like war game… Reply
with ONLY a JSON object: {focus, aggression?, raid_quadrant?, settle_quadrant?, motto, message?}."
User: compact situation — resources/rates, field/building levels (summarised), garrison, incoming
count, villages n/allowed, last ≤5 report heads (own, party-scoped), ≤10 nearby map entries
(label + distance), current strategy, persona aggression. Bounded by construction.

## Test strategy

- **Unit:** `Strategy::default()` no-op (AC1); each bias rule positive+negative (AC2); reply
  parsing per failure class incl. prose-wrapped and out-of-range (AC3); prompt determinism + caps +
  fog check on a fixture (AC4); budget/due pure logic incl. window gating (AC5).
- **E2E (`tests/e2e.rs`, ScriptedBackend, no network):** military strategy changes the training
  order vs baseline; invalid reply leaves baseline; scripted message lands in the recipient's
  conversations (via a second seeded player + the messages endpoint) and only once (AC6/AC7).
- **AC8:** diff-stat check in review.

## Key risks

- **LLM latency inside a tick:** the strategist call runs in the bot's tick task (simplest);
  worst-case it delays that bot's own next reflex tick only — the semaphore cap bounds fleet
  impact; the interval makes it rare. Recorded as accepted for this slice.
- **Prompt drift vs digest DTOs:** the prompt builder consumes the typed DTOs, so server additions
  don't change it silently; caps keep growth bounded.
- **Cost control:** budget default errs low (12/h fleet-wide ≈ a few cents/day on the cheap tier);
  restart resets the window (errs cheaper).
