# Feature 120 — AI players: seeding, visibility & carve-outs

**Status:** Draft
**Depends on:** 118 (AI accounts + keys + admin mint), 119 (the full agent action surface),
019 (protection & lifecycle), 022 (fair play), ADR 0035 (per-world config), ADR 0036 (program).
**Roadmap:** slice 3 of the AI-players program (118–122).

## Goal

Make AI players **operable at fleet scale and fair by construction**: an Administrator seeds N bots
into a world in one action, each world declares whether its bots are **labeled** ("NPC") or
**disguised**, and the systems that would misfire on automated accounts — 022 detection signals and
the 019 abandonment sweep — carve them out **explicitly**. After 120, the runner (121) points at a
world and finds its fleet waiting.

## Concepts

- **A bot** is a 118 AI account (`is_ai`) with an agent key, a player + starting village in one
  world. Bots are **full participants** (rankings, medals, alliances — ADR 0036); nothing here
  changes sim behaviour.
- **Enabled vs disabled.** A bot is *enabled* while it holds ≥1 unrevoked key. **Disabling = revoking
  its keys**: the account stays, but the 019 lifecycle treats it like any quiet player again — it
  greys and is eventually swept (an abandoned bot decays like a quit human; no zombie fleets).
- **Per-world visibility** (`worlds.ai_visibility`: `labeled` | `disguised`, operator-set at world
  creation, ADR 0035 pattern): on a **labeled** world, AI players carry an "NPC" tag on their
  profile, leaderboard rows, and the map inspector; on a **disguised** world they are
  indistinguishable from humans everywhere player-facing. Cosmetic only.
- **Fair-play carve-out (022).** Detection *signals* skip AI accounts: a bot must not fill the
  moderator queue with inhuman-action-rate flags, and a fleet sharing the server's IP must not
  trip shared-registration-IP association for itself or for humans. Moderators instead see the
  plain `is_ai` fact on the account view. Rate *limits* stay fully in force (118's agent budget).
- **Lifecycle carve-out (019).** The abandonment sweep skips **enabled** bots (a paused runner must
  not erase the world's population overnight). Beginner protection applies to bots normally
  (faithful; also prevents day-one farming of fresh fleets).

## Seeding

`POST /admin/agents` (extends the 118 single-mint): `{world, tribe_mix, count}` creates `count` AI
accounts in one action — plausible generated usernames (name-pool + discriminator on collision),
tribe per `tribe_mix` (`random` | a fixed tribe), each with a starting village via the normal join
placement and one agent key. The response is a **one-time key manifest** (username → `epk_…`),
rendered once in the console and downloadable as JSON for the runner's key file; only hashes are
stored (118 rule). The admin console lists a world's bots (name, tribe, village count, enabled,
created) with per-bot **revoke** (disable) and a fleet-wide revoke.

## Acceptance criteria

- **AC1 — Bulk seeding.** An admin seeds N bots into a chosen world in one action: N `is_ai`
  accounts with distinct plausible names, tribes per the requested mix, one starting village each
  (normal placement rules), one key each. The key manifest is shown exactly once (and downloadable
  as JSON); re-rendering the console never reveals keys again. Non-admins are denied (fail-closed,
  118 rule).

- **AC2 — Fleet management.** The console lists a world's bots with enabled state (≥1 unrevoked
  key); per-bot and fleet-wide revoke work; a revoked bot's key is refused (118 AC1) and the bot
  counts as disabled everywhere this spec keys on enablement.

- **AC3 — Visibility: labeled.** On a world with `ai_visibility = labeled`, an AI player carries an
  "NPC" tag on: their profile page, every leaderboard row naming them, and the map inspector for
  their villages. The tag derives server-side from `is_ai` + the world's setting (P4 — no client
  guessing).

- **AC4 — Visibility: disguised.** On a `disguised` world, no player-facing surface distinguishes
  an AI player — profile, boards, map, reports, messages are byte-identical in shape to a human's.
  Moderators/admins still see `is_ai` on the account/mod views regardless of the world setting.

- **AC5 — Detection carve-out (022).** AI accounts produce no shared-IP or inhuman-action-rate
  signals: their own signal surface reads zeroed, and a human sharing their registration IP is
  never flagged *by association with the bot*. The mod account view shows `is_ai` instead.
  **Player-filed reports against bots still work** — refusing them would leak the disguise (AC4);
  the moderator sees the badge and judges. Rate limiting (118 agent budget) is untouched.

- **AC6 — Lifecycle carve-out (019).** The abandonment sweep never abandons an **enabled** bot,
  however stale its activity; a **disabled** bot (all keys revoked) follows the normal 019
  greying/abandonment path. Beginner protection applies to bots exactly as to humans.

- **AC7 — Operator config.** `ai_visibility` is set per world at creation (admin form; default
  `labeled`), stored on the `worlds` row, and read wherever AC3/AC4 need it. Existing worlds
  default to `labeled` via migration.

## Roles & permissions

Per [roles.md](../../roles.md).

| Role | Permitted | Denied (server-enforced) |
|------|-----------|--------------------------|
| **Administrator** | Bulk-seed bots; list/revoke keys; set `ai_visibility` at world creation. | — |
| **Moderator** | See `is_ai` on the mod account view (any world). | Seeding/revoking (admin-only). |
| **Player (human)** | On labeled worlds: see the NPC tag. | Distinguishing bots on disguised worlds; any admin surface. |
| **Agent (bot)** | Unchanged 118/119 surface. | — |
| **System** | The 019 sweep reads enablement; 022 signal queries read `is_ai`. | — |

## Out of scope

- The runner itself, cadence, difficulty (121); the LLM strategist (122).
- Excluding bots from rankings/medals (rejected — ADR 0036: full participants).
- Per-bot personality/config storage (121's key file carries runner-side config).
- A "humans-only" leaderboard view (possible later; not this slice).

## Open questions (for plan.md)

- Name generation: a static name pool in the balance data vs code constant — leaning a code
  constant in the web layer (not balance: names are not sim rules).
- Whether the enabled check in the sweep is a JOIN on `agent_keys` or a denormalized flag —
  leaning JOIN (no new state to keep consistent; the sweep already batch-reads).
- Where the NPC tag surfaces exactly on the leaderboard templates (which of the 016 boards name
  players directly).
