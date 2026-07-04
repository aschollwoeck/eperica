# AI players & the Agent API — bots as true clients

**Status:** Accepted — **program complete** (all five slices Verified/merged) · **Date:** 2026-07-03,
completed 2026-07-04 · **Slices:** 118 (Agent API core) → 119 (Agent API actions complete) → 120
(AI players & seeding) → 121 (bot runner: rule-based reflexes) → 122 (LLM strategist).
**Depends on:** ADR 0034 (multi-world, `GameContext`, per-world players), ADR 0035 (per-world config),
022 (fair play: rate limiting + detection signals), 019 (protection & lifecycle), 020 (the Natar synthetic-
account precedent).

## Context

Every world today is populated only by human players (plus the Natar NPC villages, which never *act* —
they only defend). We want **AI players**: accounts that genuinely play the game — build, raid, defend,
settle, message — for two purposes at once:

1. **Living opponents.** Solo/small worlds feel alive: active neighbours, raid pressure, competition on
   the boards, targets that fight back.
2. **The LLM-plays-the-game experiment.** An LLM agent (e.g. Claude) connects and plays a world
   autonomously — a first-class project goal, not a hack.

Decisions taken (2026-07-03, aligned with the operator):

| Question | Decision |
|---|---|
| Purpose | Both: living opponents **and** the LLM experiment |
| Brain | **Hybrid** — rule-based reflexes + a periodic LLM strategist |
| Interface | **External JSON API** — bots are true clients |
| In-game visibility | **Operator-configurable per world** (labeled "NPC" vs disguised) |
| API keys | **AI accounts only** (admin-issued); humans keep the browser |
| Rankings/medals/stats | AI players are **full participants**; the label is cosmetic |

## Decision

### Bots are clients, not server code

The AI acts exclusively through a new **Agent API**: JSON endpoints, authenticated by API key, that
mirror the existing player surface 1:1. Nothing about the sim changes. This buys:

- **Honesty by construction (P4).** A bot *cannot* see or do more than a human: the API is built from
  the same application read models and use-cases as the HTML handlers. Fog of war holds — e.g. incoming
  attacks stay arrival-only (§7.3).
- **One API, both goals.** The same endpoints serve the `eperica-bots` runner (dozens of cheap
  rule-based opponents) and an LLM agent (the experiment). The bot runner is reference client #1.
- **The sim stays pure.** No AI decision logic enters `domain`/`application`. Bot *strategy* is player
  behaviour, not game rules — it lives in the runner crate (P3 unaffected).

### The three pieces

**1. Agent API (server; slices 118–119).**
- **Auth:** `Authorization: Bearer <key>` → resolves to an AI account exactly like a session cookie
  resolves a user; after resolution the request flows through the same `GameContext` world scoping.
  Keys are admin-issued, hashed at rest (like passwords), revocable, bound to one AI account.
- **Perception:** `GET /api/w/{world}/state` — one compact, LLM-token-friendly **state digest**: own
  villages (resources + rates + capacities, queues, garrison, fields/buildings), incoming attacks
  (arrival-only), unread report summaries, culture/expansion status, and a local map viewport per
  village. Assembled from existing read models only. Plus targeted reads (full report, map window).
- **Actions:** JSON POSTs that are thin adapters onto the **existing use-cases** — build/train (118);
  attack/raid/scout/reinforce, market, settle, messages (119). Zero new game logic; every rule is the
  use-case's, unchanged. Responses return the same denial reasons players see.
- **Rate limits:** the 022 DB-backed limiter covers agent keys with their own per-account budget —
  a runaway agent gets 429s, never a melted server (P11).

**2. AI-player support (server; slice 120).**
- `users.is_ai` (Natar-pattern synthetic accounts): no email/login path, admin-seeded into a world via
  the normal join flow (plausible names, tribe choice), one API key each.
- **Per-world visibility config** (ADR 0035 pattern): `labeled` (an "NPC" tag on profile/boards/map,
  like the Natars) vs `disguised` (indistinguishable). Cosmetic only — either way AI players are full
  participants in rankings, medals, alliances and stats.
- **Explicit carve-outs:** exempt from 022 *detection signals* (they would trivially trip
  inhuman-action-rate; moderators see `is_ai` instead) but **subject to rate limits**; **not** swept by
  the 019 inactivity lifecycle while enabled (a paused runner must not grey the world's population);
  beginner protection applies normally (faithful; also prevents day-one bot farming).
- **Admin controls:** seed N bots into world X, list/revoke keys, disable a bot (which re-enables the
  019 sweep for it so an abandoned bot decays like a quit player).

**3. The bot runner (`eperica-bots`, new workspace binary; slices 121–122).**
- A separate process: server URL + key file in, N bot identities out, each on a **humanized cadence**
  (activity windows, jitter, reaction delays — the difficulty knob). No shared state with the server.
- **Reflex layer (121):** pure, deterministic policy functions `digest → intents` (build-order
  priorities, raid loops, dodge/defend reactions, settling), unit-tested in the runner crate.
- **Strategist layer (122):** every few hours per bot, digest + goals → the Anthropic API → updated
  goals ("push crop, settle SE, farm inactives") and optional diplomacy/messages. Reflexes keep bots
  alive and cheap between strategist calls; the runner works with the LLM disabled.

## Consequences

- Two processes in production (server + runner) — acceptable for self-hosted; the runner is optional
  (a world without bots needs nothing).
- The Agent API becomes a **public contract** (versioned under `/api/`); changes must stay
  backward-compatible or version.
- Per-decision LLM cost is confined to the strategist cadence and can be turned off per runner config.
- Rankings include bots by design; a "humans only" board view is possible later but out of scope.
- Own-account keys for humans ("Claude plays my account") were considered and **rejected for now** —
  fair-play policy for automating human accounts is its own discussion; nothing in the key model
  precludes adding it later.

## Slices

| Slice | Delivers | Acceptance sketch |
|---|---|---|
| **118 agent-api-core** | Key auth + state digest + economy actions (build/train) | A scripted client plays the opening loop (read state → build fields → train troops) end-to-end against a live world |
| **119 agent-api-actions** | Military (attack/raid/scout/reinforce), market, settle, messages; agent rate budget | The client can raid a farm, dodge, trade and found a village; over-budget calls 429 |
| **120 ai-players** | `is_ai`, admin seeding + keys, per-world labeled/disguised, carve-outs | Admin seeds 20 bots; they appear (labeled or not) on map/boards; detection ignores them; sweep skips them |
| **121 bot-runner** | `eperica-bots` reflex policies + cadence | A seeded world visibly *lives*: bots build up, raid inactives, dodge, settle — for days, unattended |
| **122 llm-strategist** | Anthropic-API strategy layer + diplomacy | Bot goals shift plausibly over days; bots send coherent messages; runner still works with LLM off |
