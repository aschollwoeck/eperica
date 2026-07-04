# The Spectator API (v0.1 — slice 125)

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

Statuses: `401` auth (bad/expired/wrong-surface key), `403` scope (e.g. role lost mid-session),
`404` unknown world/village ids, `429` rate-limited (adds `retry_after_secs`). A `POST` (or any
other mutating method) to any path under this surface returns `404`/`405` — there is nothing to
route it to.

## Rate budget

Spectator-key traffic shares the **same rate-budget class as the Agent API**
(`agent_limit_per_window`, `specs/balance/fairplay.toml` — 120 requests/minute per key at time of
writing). Over budget → `429` with `retry_after_secs`. Poll the feed at a sane interval; there is
no server-side push.

## Endpoints

### `GET /spectator/me`

Key introspection — confirms the key is live and which account it belongs to.

> v0.1 — subject to the implementation; see integration tests for the exact shape. Expect
> something in the spirit of `{ account, username }`.

### `GET /spectator/w/{world}/feed`

The capped activity snapshot for the world — the same data the `/spectate/{world}` dashboard page
renders. A **bounded snapshot assembled per request from existing state** (P1/P11): no new event
store, no server-side polling loop. Each category is capped (N ≤ 50) and ordered by nearest
deadline / most recent:

- **Movements in flight**, both directions — attacks, raids, reinforcements, returns, settlers,
  merchant shipments — with kind, origin/destination, arrival time, and full **composition**.
  Unlike a defender's own view (which only ever shows an arrival-only warning for an incoming
  hostile movement), the spectator feed shows what's actually coming.
- **Build orders** completing soonest, world-wide.
- **Training batches** completing soonest, world-wide.
- **Recent battle reports** for the world.

> v0.1 — subject to the implementation; see integration tests for the exact JSON shape of each
> category.

### `GET /spectator/w/{world}/players?page=`

A paged index of every player in the world, ordered by population, **50 per page**. Expect
population, village count, and alliance per row.

> v0.1 — subject to the implementation; see integration tests for the exact JSON shape.

### `GET /spectator/w/{world}/village/{id}`

Full village internals — **equal to what the village's own owner sees**: resources (computed on
read, P1), build queue with deadlines, training batches, garrison, stationed reinforcements,
loyalty, research. No fog of war and no redaction; a spectator's view of a foreign village is the
owner's view.

> v0.1 — subject to the implementation; see integration tests for the exact JSON shape.

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
