# Feature 119 — Agent API actions complete (military, market, settle, research, messages)

**Status:** Draft
**Depends on:** 118 (Agent API core — auth, digest, error contract, rate budget, strict village
addressing), ADR 0036 (program), and the use-cases this exposes: 007 (movement), 009 (combat),
010 (scouting), 008 (trade), 013 (settling), 005/011 (research & smithy), 024 (messaging).
**Roadmap:** slice 2 of the AI-players program (118–122).

## Goal

An agent can play the **whole game loop** over JSON: raid and attack, scout, reinforce and recall,
trade, research and upgrade units, found villages, read its battle reports, and message other
players — every action the same use-case a browser player triggers (P4), every response in the 118
error/success contract. After 119, the bot runner (121) and an LLM agent need nothing further from
the server to play competitively.

## Concepts

- **Same contract as 118.** All endpoints live under `/api/w/{world}/…`, authenticate via the 118
  bearer key, share the 118 error shape (`{error, reason}` + 4xx/5xxs), sit under the same agent
  rate budget, and use **strict village addressing** (the path village must be owned — no fallback).
- **Military sends carry a unit bundle** `{ "<unit_id>": count, … }` — validated entirely by the
  use-cases (garrison coverage, roster/tribe scoping, ≥1 unit, target rules). Send endpoints mirror
  the rally-point forms 1:1: attack, raid, scout, reinforce; return/recall mirrors the stationed
  panel.
- **Digest grows the missing loop-closure reads** (same read models as the pages, 118 AC3 rule):
  own **movements in progress** (outgoing/returning, kind + destination + arrival — own troops are
  never fog-limited), **reinforcements** stationed here and abroad, and **research/smithy state**
  (researched units, current levels, active orders). Fog rules unchanged: nothing about foreign
  villages beyond what the map/scout reports reveal.
- **Reports become readable**: the digest's report heads (118) gain a detail read returning the same
  per-party view the report page renders — a defender sees what the 016 faithful report shows a
  defender, never the attacker's hidden remainder.
- **Messages** are the 024 DMs: open/continue a conversation with a player by name; list unread.
  (Chat channels and the alliance forum stay browser-only for now — out of scope.)

## Endpoints (this slice)

| Method & path | → use-case |
|---|---|
| `POST /api/w/{w}/village/{v}/attack` | `order_attack` (`{x, y, units, mode: "attack"\|"raid", catapult_target?}`) |
| `POST /api/w/{w}/village/{v}/scout` | `order_scout` (`{x, y, count, target: "resources"\|"defences"}`) |
| `POST /api/w/{w}/village/{v}/reinforce` | `order_reinforcement` (`{x, y, units}`) |
| `POST /api/w/{w}/village/{v}/return` | `order_return` (`{station}` — a stationed-group id from the digest) |
| `POST /api/w/{w}/village/{v}/trade` | `order_trade` (`{x, y, give: {wood,clay,iron,crop}}`) |
| `POST /api/w/{w}/village/{v}/settle` | `order_settle` (`{x, y}`) |
| `POST /api/w/{w}/village/{v}/research` | `order_research` (`{unit}`) |
| `POST /api/w/{w}/village/{v}/smithy` | `order_smithy_upgrade` (`{unit}`) |
| `GET  /api/w/{w}/report/{id}` | the 016 per-party report view |
| `POST /api/w/{w}/message` | `open_dm`/`send_dm` (`{to: "<username>", body}`) |
| `GET  /api/w/{w}/messages?since_ms=` | unread/new DMs for the agent (page read models) |

Success responses return the created movement/order with its absolute-ms arrival/completion (the
118 pattern: read back through the page's own read models). Digest additions: `movements`,
`reinforcements_here`, `reinforcements_abroad`, `research` (per village: researched units, smithy
levels, active research/smithy orders).

## Acceptance criteria

- **AC1 — Sends are the use-cases.** attack/raid/scout/reinforce succeed and fail under exactly
  `order_attack`/`order_scout`/`order_reinforcement` rules (garrison coverage, roster/tribe scope,
  empty-bundle, self-target, protection (019) and Natar/oasis rules). Success returns the movement
  with arrival; denials use the 118 error contract with the player-visible reason.

- **AC2 — Recall.** A stationed group listed in the digest can be recalled by id via
  `order_return`; a group not owned by the agent (or an unknown id) → `404 not_found` (P4).

- **AC3 — Trade & settle.** `trade` and `settle` mirror `order_trade`/`order_settle` (merchant
  capacity/counts, target rules, settler/CP/slot gates). Success returns the shipment/settling
  movement with arrival.

- **AC4 — Research & smithy.** `research`/`smithy` mirror `order_research`/`order_smithy_upgrade`
  (academy/smithy presence + levels, prerequisites, costs, one-at-a-time lanes). The digest's new
  `research` block reflects the results (page truth).

- **AC5 — Reports.** `GET /api/w/{w}/report/{id}` returns the report **only to a party** (P4;
  anyone else → `404`), with the same per-party fields the report page shows that party (a defender
  never sees the attacker's surviving composition beyond the faithful 016 view).

- **AC6 — Messages.** The agent can DM an existing player by username (open-or-continue semantics,
  024 validation) and read new DMs since a timestamp. Unknown recipient → `404`; body rules are
  024's (length caps). No new visibility: only conversations the agent is a party to.

- **AC7 — Digest closure (page truth).** The digest additions (movements, reinforcements
  here/abroad, research state) equal the corresponding page read models at the same instant, carry
  absolute-ms times, and leak nothing about foreign villages (own-troops data only).

- **AC8 — Loop test.** A scripted two-agent test: agent A raids agent B (units leave A's digest,
  the movement appears with arrival, B's digest shows the incoming attack arrival-only); after the
  due combat processes, both agents read their own report views; A reinforces B, B's digest shows
  the stationed group, A recalls it. All over JSON, no HTML.

- **AC9 — Same budget & guards.** Every new endpoint sits under the 118 agent rate budget, the
  freeze guard, and strict village addressing — no new gate surface.

## Roles & permissions

Per [roles.md](../../roles.md) — unchanged from 118: the agent acts as the AI account's Player in
the selected world; everything above is Player-permitted action surface, denied cross-account by
the use-cases (P4). Visitor/human-Player/Moderator/Administrator exactly as 118.

## Out of scope

- Alliance actions (found/join/diplomacy), chat channels, the alliance forum — later, if the
  strategist (122) needs them.
- Demolition, oasis raids from the rally page's oasis flow, wonder building — later slices of the
  program if needed; not required for the 121 runner's opening strategies.
- Any digest field about foreign villages beyond existing map/report/scout truth.

## Open questions (for plan.md)

- Whether `catapult_target` (009 siege) ships now or with a later "siege polish" — leaning now,
  since `order_attack` already takes it.
- The `messages` read shape: reuse `conversation_list` + per-conversation reads vs a flat
  "new messages since" view — leaning flat (simplest for a bot loop).
- Whether scout results land in `reports` (they do on the page — scout reports are reports) — the
  digest heads may need a `kind` field.
