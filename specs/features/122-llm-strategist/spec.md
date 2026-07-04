# Feature 122 — the LLM strategist: long-horizon goals & diplomacy for the bot fleet

**Status:** Draft
**Depends on:** 121 (the runner: personas, pure reflex doctrine, executor, fleet loop), 119
(messages endpoint), ADR 0036.
**Roadmap:** slice 5 of 5 — the AI-players program capstone.

## Goal

Give a fleet **long-term cunning without losing its cheap heartbeat**: every few hours a bot's
situation is summarised and sent to an LLM (the Anthropic API), which returns an updated
**strategy** — a small, validated set of goal knobs that *bias* the 121 reflexes (aggression,
economic focus, raid preferences, expansion direction) — and, optionally, a short in-character
diplomatic message to another player. Between strategist calls the reflexes carry on exactly as in
121; **with no API key configured, the runner behaves byte-identically to 121**.

## Concepts

- **Strategy is a data overlay, not a second brain.** `Strategy` is a plain struct the pure
  doctrine consumes alongside the persona: focus (`economy` | `military` | `expansion`),
  aggression override (0–3), preferred raid quadrant, settle-direction preference, and a free-text
  `motto` (log colour only). `plan_tick(digest, map, persona, strategy, now, tribe)` stays pure —
  the LLM changes *inputs*, never the decision code.
- **Strict, validated output — no surprising fallbacks (the 121 rule).** The strategist must
  return a single JSON object matching the strategy schema. Anything else — malformed JSON, extra
  prose, out-of-range values, unknown focus — is an **error**: logged loudly, the previous strategy
  stays in force, and the failure is counted. Values are never silently clamped or guessed.
- **A backend seam for testability.** The LLM sits behind a `StrategistBackend` trait: the real
  Anthropic Messages-API implementation (reqwest, no SDK dependency; model + key from env) and a
  scripted fake for tests. **No test ever calls the network.**
- **Cadence & budget.** Per bot, the strategist runs every `strategist_interval` (default ~4h,
  persona-jittered), only inside the activity window, and never more than a fleet-wide
  `llm_budget_per_hour` (default 12 calls). The digest summary sent is compact (the strategist
  sees what the bot may know — the same fog-honest digest, condensed).
- **Diplomacy.** The strategy may include at most **one** outbound message per strategist cycle
  (`{to, body}`), sent via the 119 message endpoint under its rules (unknown recipient → the
  server's 404 is logged, not retried). Messages are in-character, bounded by 024's body rules.

## Operation

`ANTHROPIC_API_KEY` (or `EPB_ANTHROPIC_KEY`) enables the strategist; `EPB_LLM_MODEL` overrides the
default model; `--no-llm` forces it off. `--dry-run` also covers strategist *application* (the call
may run; strategy/messages are logged, not applied/sent). Structured logs per strategist cycle:
prompt size, outcome (applied / rejected / budget-skipped), motto.

## Acceptance criteria

- **AC1 — Off by default, off means identical.** Without a key (or with `--no-llm`), no strategist
  code path runs and fleet behaviour is byte-identical to 121 (`Strategy::default()` biases
  nothing — pinned by a unit test comparing `plan_tick` with and without the default overlay).

- **AC2 — Strategy biases the pure doctrine.** With a strategy overlay: `focus=military` raises
  the training floor and raid cadence; `focus=economy` suppresses raiding below the doubled
  garrison floor; `focus=expansion` prioritises the settler chain; the aggression override
  replaces the persona's; the preferred raid quadrant re-orders target selection; the settle
  preference re-orders valley choice. Each bias is a pure, unit-tested rule; `plan_tick` stays
  deterministic given (digest, persona, strategy, now).

- **AC3 — Strict output contract.** The backend's reply must parse as the exact strategy JSON
  schema. Malformed/out-of-range/unknown-value replies are rejected (error log + counter), the
  prior strategy remains, and the bot's reflexes continue unaffected. Unit-tested per failure
  class (prose-wrapped JSON, unknown focus, aggression 7, missing fields, valid-with-message).

- **AC4 — Prompt is fog-honest and compact.** The prompt is assembled from the digest + map window
  + persona + current strategy only (nothing the bot couldn't see; the same 119 fog rules), and
  its size is bounded (a fixed template + capped lists). Unit-tested against a digest fixture
  (deterministic prompt text, bound respected, no foreign-village data beyond map labels).

- **AC5 — Cadence & budget.** Strategist calls happen at most once per interval per bot, only
  in-window, and the fleet-wide hourly budget is never exceeded (over-budget cycles are skipped
  with a log and the reflexes proceed). Unit-tested via the scheduler's pure due/budget logic.

- **AC6 — Diplomacy.** A strategy carrying a message sends exactly one DM via the 119 endpoint;
  denials (unknown recipient, body rules) are logged outcomes, never retried in-cycle. E2E (fake
  backend): a scripted strategy with a message produces the DM in the recipient's conversation
  list; a second cycle without a message sends nothing.

- **AC7 — E2E with a scripted backend.** Against the in-process server: a fake backend returns a
  `focus=military, aggression=3` strategy → the next tick's intents differ from the no-strategy
  baseline exactly as AC2 predicts (bigger training order); a scripted **invalid** reply leaves
  behaviour at baseline. No network, no real key, deterministic.

- **AC8 — No server change.** Zero server-crate edits; the Anthropic dependency surface is
  reqwest-only (already a runner dependency).

## Roles & permissions

Per [roles.md](../../roles.md): unchanged — the strategist acts entirely through the bot's existing
agent key (Agent surface). The API key for the LLM is runner-side operator configuration, never
sent to or stored by the game server.

## Out of scope

- Alliance strategy, multi-bot coordination, war planning across the fleet (a future slice if
  ever — each bot's strategist sees only that bot's world).
- Streaming/tool-use/agentic LLM loops — one request, one JSON reply per cycle.
- Server-side anything; strategist state persistence across runner restarts (a fresh runner starts
  from `Strategy::default()` — restart-safe like everything else in 121).

## Open questions (for plan.md)

- Default model id (leaning `claude-haiku-4-5-20251001` — the cheap tier fits a 4-hourly goal
  refresh; env-overridable).
- Whether the strategist prompt includes recent battle-report heads (leaning yes — capped at 5,
  they are the bot's own party-scoped reports).
- Where the budget window lives (leaning a simple in-process rolling hour counter — the runner is
  a single process; restart resets it, which errs cheap).
