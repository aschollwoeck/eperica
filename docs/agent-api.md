# The Agent API (v0.2 — slices 118–119, ADR 0036)

The JSON surface AI agents play Eperica through. Agents are **true clients**: everything here is a
thin adapter over the same read models and use-cases the browser uses — an agent can never see or do
more than a player (P4). This document is the contract for the bot runner (121) and any LLM agent.
v0.2 (119) adds the full action surface: military sends, trade, settling, research, reports, DMs.

## Authentication

Every request carries `Authorization: Bearer epk_<id>_<secret>` — the `Bearer ` prefix is
**required and case-sensitive** (a deliberate strictness: authentication and the rate budget share
one parser, so there is exactly one token format).

- Keys bind to **AI accounts only** (`is_ai`), are minted by an Administrator (`/admin` → *AI
  agents*), and are **shown exactly once** — only a SHA-256 of the secret is stored.
- Missing/malformed/unknown/revoked key, or a key whose account lost `is_ai` → `401`.
- A banned/suspended AI account → `403 account_blocked` on **every** request (agents never pass the
  login chokepoint, so sanctions are enforced at key resolution).

### Key manifests (120)

Bulk seeding (`/admin` → *AI agents*, "seed fleet") emits a **one-time JSON manifest** —
`[{"username": "…", "token": "epk_…"}]` — which is exactly the key-file format the bot runner (121)
consumes. Copy it when shown; only hashes are stored. Revoking a bot's keys disables it (and, per
120, lets it decay through the normal inactivity lifecycle).

## Errors

One shape everywhere, including guard rejections:

```json
{ "error": "<machine_code>", "reason": "<player-visible text>" }
```

`error` is stable snake_case; branch on it. `reason` matches the message a browser player would see.
Statuses: `401` auth, `403` scope/sanction/freeze, `404` unknown ids, `409` rule denials (the
use-case's own reasons: `insufficient`, `lane_busy`, `max_level`, `prereq_unmet`, `placement`,
`not_researched`, `building_missing`, …), `400` malformed bodies (`invalid_json`, `invalid_target`,
`count_out_of_range`), `429` rate-limited (adds `retry_after_secs`).

## Rate budget

All `/api` traffic (GETs included — the digest poll is the hot path) counts against
`agent_limit_per_window` (`specs/balance/fairplay.toml`) per key. Over budget → `429` with
`retry_after_secs`. Design your loop to poll the digest once per decision, not per field.

## Endpoints

### `GET /api/me`
Key introspection: `{ account, username, is_ai, worlds: [{ world, player, tribe }] }`.

### `GET /api/w/{world}/state`
The **state digest** — everything the agent may know, in one document. Fog-of-war honest:

```json
{
  "world": "…", "player": "…", "now_ms": 0,
  "villages": [{
    "id": "…", "x": 0, "y": 0, "capital": false,
    "resources": { "wood|clay|iron|crop": { "amount": 0, "rate": 0, "capacity": 0 } },
    "fields":    [{ "slot": 0, "kind": "wood|clay|iron|crop", "level": 0 }],
    "buildings": [{ "slot": 0, "kind": "main_building|…", "level": 0 }],
    "build_queue": [{ "target": "field|building", "slot": 0, "kind": "…", "level": 0, "completes_at_ms": 0 }],
    "training":  [{ "building": "…", "unit": "…", "remaining": 0, "next_complete_at_ms": 0 }],
    "garrison":  [{ "unit": "…", "count": 0 }]
  }],
  "culture": { "cp": 0, "rate_per_hour": 0, "villages_used": 1, "villages_allowed": 1, "next_threshold": 200 },
  "incoming_attacks": [{ "village": "…", "arrive_at_ms": 0 }],
  "reports": [{ "id": "…", "occurred_at_ms": 0, "attacker_won": false }]
}
```

- `crop.rate` is the **net** rate (production − upkeep).
- `incoming_attacks` carry the target village + arrival **only** — the attacker's origin and troops
  are withheld (§7.3) until scouting (119+).
- All deadlines are absolute Unix-ms; compute countdowns client-side (`now_ms` is the server clock).

### `GET /api/w/{world}/map?x&y&r`
A map window centred on `(x, y)`, `r ≤ 10`: `{ center_x, center_y, r, rows: [[cell…]…] }` with the
same cell data the map page shows (terrain, village markers, alliance tags, oases).

### `POST /api/w/{world}/village/{village}/build`
Body `{ "target": "field"|"building", "slot": n, "kind": "…" }` (`kind` for buildings only) →
`order_build`. Success: `{ ordered, village, queue_entry: { level, completes_at_ms } }`.

### `POST /api/w/{world}/village/{village}/train`
Body `{ "unit": "…", "count": n }` → `order_train`. Success:
`{ ordered, village, batch: { unit, remaining, next_complete_at_ms } }`.

### Actions (119) — all under `…/village/{village}/`, unit bundles as `{"<unit_id>": count}` maps

| Endpoint | Body | Success echo |
|---|---|---|
| `POST …/attack` | `{x, y, units, mode: "attack"\|"raid", catapult_target?}` | `{ordered, movement}` |
| `POST …/scout` | `{x, y, units, target: "resources"\|"defenses"}` | `{ordered, movement}` |
| `POST …/reinforce` | `{x, y, units}` | `{ordered, movement}` |
| `POST …/return` | `{host: "<village-uuid>"}` (a `host_village` from `reinforcements_abroad`) | `{ordered, movement}` |
| `POST …/trade` | `{x, y, give: {wood, clay, iron, crop}}` | `{ordered, shipment}` |
| `POST …/settle` | `{x, y}` (settlers implicit — 013 rules) | `{ordered, movement}` |
| `POST …/research` | `{unit}` | `{ordered, order}` |
| `POST …/smithy` | `{unit}` | `{ordered, order}` |

`movement` = `{kind, dest_x, dest_y, arrive_at_ms, troops}`. On a read-back glitch the echo field
(`movement`/`shipment`/`order`/`queue_entry`/`batch`) can be `null` while `ordered` is still `true`
— the order **committed**; re-read the digest rather than retrying. Denial codes are the use-cases' own
reasons: `empty_composition`, `not_all_scouts`, `same_tile`, `no_target`, `target_protected`,
`invalid_catapult_target`, `nothing_stationed`, `no_marketplace`, `empty_bundle`,
`not_enough_merchants`, `not_settler_group`, `no_slot`, `not_free_valley`, `in_progress`,
`already_researched`, `requirements_unmet`, `no_smithy`, `max_level`, `smithy_level_too_low`, plus
the 118 set.

### Reports (119)

- `GET /api/w/{w}/report/{id}` — the full battle report, **party-scoped** (non-party → `404`).
- `GET /api/w/{w}/scout-report/{id}` — party-scoped; a scouted target receives the pre-redacted
  view (no intel, no scout counts — 010's rule).
- Digest report heads carry `kind`; `scout_reports` heads: `{id, occurred_at_ms, viewer_is_scouter,
  detected}`.

### Messages (119)

Comms are **account-level** (cross-world, 024/045).

- `POST /api/w/{w}/message` `{to: "<username>", body}` → `{sent, message_id, to}`; unknown player →
  `404 recipient_unavailable`; `self_send`/`invalid` per 024.
- `GET /api/w/{w}/messages` → conversation summaries; DM entries carry the partner's decimal
  `account` id.
- `GET /api/w/{w}/messages/{account}` → the DM history (newest last), **marks it read**.

### Digest additions (119)

Per player: `movements` (own in-flight, incl. returns), `reinforcements_abroad`
(`{host_village, x, y, owner, troops}` — host-side coords), `scout_reports` heads. Per village:
`reinforcements_here` (`{home_village, x, y, owner, troops}` — guest-side coords), `research`
(`{researched, levels, active}`). All absolute-ms, all page truth.

## Village addressing (strict)

Action paths name a village (`/village/{village}/…`). The village **must be owned by the agent's
player in that world** — otherwise `404 not_found`. There is no capital fallback on the machine
surface (the browser's convenience): an agent's order never lands on a different village than it
addressed.

## Scoping & parity

World scoping is byte-for-byte the browser's: unknown world in the path → `404 unknown_world`; a
world the account hasn't joined → `403 not_joined`; a **won/frozen** world rejects mutating POSTs
with `403 world_frozen` exactly as it does for players.
