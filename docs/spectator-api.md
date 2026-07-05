# The Spectator API (v1.0 — slice 125)

The read-only JSON surface behind the `/spectate` dashboard. It mirrors the dashboard exactly:
everything a spectator can see in the browser, an external tool (an overlay, a caster's bot, a
research script) can pull as JSON — and nothing more. There is no write surface: **read-only by
construction, not by permission check** (P4) — no mutating route is registered on this router at
all.

## Authentication

Every request carries `Authorization: Bearer spk_<id>_<secret>`.

- Keys are admin-minted (`/admin` → *Spectator keys*), shown **exactly once** — only a SHA-256 of
  the secret is stored (the same pattern as `epk_` agent keys, but a separate table and prefix;
  the two credential types can never be confused or cross-authenticate).
- A key authenticates **only while its account holds the Spectator role at the moment of the
  request** — auth re-checks the role on every call, not just at mint time. An administrator
  revoking the role dead-ends every key that account holds, instantly, with nothing further to
  revoke.
- An `epk_` (Agent API) key is refused on every spectator path, and an `spk_` key is refused on
  every Agent API path — wrong-prefix credentials fail like any other unknown key.
- Missing/malformed/unknown/revoked key, or a key whose account lost the Spectator role → `401`.

## Errors

Same shape as the Agent API, everywhere on this surface:

```json
{ "error": "<machine_code>", "reason": "<human-readable text>" }
```

Statuses: `401` auth — missing/malformed/unknown/revoked key, **or** a key whose account lost the
Spectator role (the role is re-checked on every request, not just at mint, so a role revoke
dead-ends a key exactly like an unknown one — no separate "role lost" code); `403
account_blocked` — a suspended/banned account, on **every** request (spectators never pass the
login chokepoint); `404` unknown world/village ids, or an unknown `/spectator` path;
`429` rate-limited (adds `retry_after_secs`). A `POST` (or any other mutating method) to a path this
surface **does** recognise (e.g. `/spectator/w/{world}/feed`) is `405` — axum refuses it before any
handler runs, since only `GET` is registered; a `POST` to a path it does **not** recognise is `404`
via the same JSON fallback as an unknown `GET` path.

## Rate budget

Spectator-key traffic shares the **same rate-budget class as the Agent API**
(`agent_limit_per_window`, `specs/balance/fairplay.toml` — 120 requests/minute per key at time of
writing). Over budget → `429` with `retry_after_secs`. Poll the feed at a sane interval; there is
no server-side push.

## Endpoints

### `GET /spectator/me`

Key introspection — confirms the key is live and which account it belongs to:

```json
{ "account": "…", "username": "…", "is_spectator": true }
```

### `GET /spectator/w/{world}/feed`

The capped activity snapshot for the world — the same data the `/spectate/{world}` dashboard page
renders. A **bounded snapshot assembled per request from existing state** (P1/P11): no new event
store, no server-side polling loop. Each category is capped (N ≤ 50) and ordered by nearest
deadline / most recent. All deadlines are absolute Unix-ms (agent-digest convention) — compute
countdowns client-side.

- **Movements in flight**, both directions — attacks, raids, reinforcements, returns, scouts,
  settlers, oasis attacks/reinforcements — with kind, origin/destination, arrival time, and full
  **composition**. Unlike a defender's own view (which only ever shows an arrival-only warning for
  an incoming hostile movement), the spectator feed shows what's actually coming.
- **Merchant shipments**, either leg (deliver/return), with the carried bundle and merchant count.
- **Build orders** completing soonest, world-wide.
- **Training batches** completing soonest, world-wide.
- **Recent battle/scout reports** for the world, each with a precomputed one-line `outcome`.

```json
{
  "world": "…", "now_ms": 0,
  "movements": [{
    "id": "…", "kind": "attack|raid|reinforce|return|scout|settle|oasis_attack|oasis_reinforce",
    "origin": { "village": "…", "x": 0, "y": 0, "owner": "…" },
    "destination": { "village": "…"|null, "x": 0, "y": 0, "owner": "…"|null },
    "arrive_at_ms": 0,
    "troops": { "<unit_id>": 0 }
  }],
  "shipments": [{
    "id": "…", "kind": "deliver|return",
    "origin": { "village": "…", "x": 0, "y": 0, "owner": "…" },
    "destination": { "village": "…", "x": 0, "y": 0, "owner": "…" },
    "arrive_at_ms": 0,
    "give": { "wood": 0, "clay": 0, "iron": 0, "crop": 0 },
    "merchants": 0
  }],
  "builds": [{
    "village": "…", "x": 0, "y": 0, "owner": "…",
    "target": "field|building", "slot": 0, "kind": "…"|null,
    "target_level": 0, "completes_at_ms": 0
  }],
  "trainings": [{
    "village": "…", "x": 0, "y": 0, "owner": "…",
    "unit": "…", "remaining": 0, "next_complete_at_ms": 0
  }],
  "reports": [{
    "id": "…", "occurred_at_ms": 0, "kind": "attack|raid|scout",
    "attacker": { "name": "…", "x": 0, "y": 0 },
    "defender": { "name": "…", "x": 0, "y": 0 },
    "outcome": "…"
  }]
}
```

### `GET /spectator/w/{world}/players?page=`

A paged index of every player in the world, ordered by population descending, **50 per page**.
`npc` is derived server-side as `is_ai && world.ai_labeled` — the raw `is_ai` truth is never itself
serialized, on either a labeled or a disguised world (AC7). Each row also carries its `villages` —
the players → village drill-down: every village that player owns, capital first then coordinate,
each linking to `GET /spectator/w/{world}/village/{id}` below.

```json
{
  "world": "…", "page": 1, "has_next": false,
  "players": [{
    "player": "…", "username": "…", "tribe": "romans|teutons|gauls"|null,
    "population": 0, "village_count": 0,
    "villages": [{ "id": "…", "x": 0, "y": 0, "capital": false }],
    "alliance_tag": "…"|null, "npc": false
  }]
}
```

### `GET /spectator/w/{world}/village/{id}`

Full village internals — **equal to what the village's own owner sees**: resources (computed on
read, P1), fields/buildings, build queue with deadlines, training batches, garrison, stationed
reinforcements, loyalty, research. No fog of war and no redaction; a spectator's view of a foreign
village is the owner's view — the handler reuses the exact owner-view read-model with the village's
true owner substituted for the caller, so these numbers can never drift from the owner's own page.

```json
{
  "world": "…", "village": "…", "owner": "…",
  "x": 0, "y": 0, "capital": false, "tribe": "romans|teutons|gauls"|null,
  "resources": { "wood|clay|iron|crop": { "amount": 0, "rate": 0, "capacity": 0 } },
  "fields":    [{ "slot": 0, "kind": "wood|clay|iron|crop", "level": 0 }],
  "buildings": [{ "slot": 0, "kind": "main_building|…", "level": 0 }],
  "build_queue": [{ "target": "field|building", "slot": 0, "kind": "…"|null, "level": 0, "completes_at_ms": 0 }],
  "training":  [{ "unit": "…", "remaining": 0, "next_complete_at_ms": 0 }],
  "garrison":  [{ "unit": "…", "count": 0 }],
  "reinforcements": [{ "home_village": "…", "x": 0, "y": 0, "owner": "…", "troops": { "<unit_id>": 0 } }],
  "loyalty": 100,
  "researched": ["…"]
}
```

- `crop.rate` is the **net** rate (production − upkeep), same as the Agent API digest.

## Disguise rule (AC7)

The spectator surface shows the **game-facing** world, not the operator's truth:

- On a `labeled` world, NPC (AI) players carry the same visible tag a spectator would see in the
  dashboard or on any player-facing surface.
- On a `disguised` world, nothing on this surface reveals `is_ai` — AI players look exactly like
  humans, the same as they do to a player. Operator truth (`is_ai`, the true account state) only
  ever lives behind `/admin`.

## Scope

This is the same read-only picture the `/spectate` dashboard shows — see
[docs/manual/spectating.md](manual/spectating.md) for the player-facing explanation of what a
spectator is and what they can see. For the mutating, action-taking surface AI agents use to play
the game, see [docs/agent-api.md](agent-api.md) — the two surfaces are unrelated and their keys
never cross-authenticate.
