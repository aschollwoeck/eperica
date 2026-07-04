# Plan — 125 spectator mode

**Status:** Draft (spec approved)

## Constitution check

- **P1:** the feed is assembled on read from existing due-stamped state (movements, orders,
  batches, reports) — no new event storage, no ticking.
- **P3:** no new game rules; pure read aggregation. Any new pure helpers (feed capping/ordering)
  live in `application`.
- **P4:** the surface is read-only **by construction** (no mutating routes registered); keys are
  hash-at-rest with the 118 verify path; role checked server-side on every request.
- **P11:** every endpoint is a fixed set of world-scoped indexed queries with hard caps (≤ 50 per
  category; players page ≤ 50/page).

## Migration

`0052_spectator.sql`: `users.is_spectator boolean NOT NULL DEFAULT false` +
`spectator_keys` (clone of the `agent_keys` shape: text id PK, user_id FK, secret_hash,
created_at, revoked_at; index on user_id). Token prefix `spk_` (the 118 `apikey` module is
prefix-parameterised or gains a sibling constructor).

## Module changes

| Layer | Change |
|---|---|
| `application/ports.rs` | `set_spectator(user, bool)`, `find_spectator_key`, `insert_spectator_key`, `revoke_spectator_keys(user)`; read ports it can reuse: `villages_of_world` (paged), `movements_in_world`, `active_build_orders_in_world`, `active_training_in_world`, `recent_reports_in_world` — new world-scoped, capped queries in `infrastructure/repo.rs` (each backed by existing indexes; add any missing `(world_id, arrives_at)`-style index in the migration) |
| `application/spectate.rs` (new) | the aggregation use-cases: `world_feed(world) -> Feed` (four capped lists), `player_index(world, page)`, `village_detail(world, village)` — all pure assembly over ports; village detail reuses the exact owner-view read paths (economy compute-on-read) so AC3's "equal to owner view" holds by construction |
| `web/spectator_api.rs` (new) | bearer extractor `SpectatorAuth` (spk parse → `find_spectator_key` → verify → **role check `is_spectator`** → rate-budget class shared with agents); JSON handlers for `/spectator/me`, `/spectator/w/{world}/feed`, `…/players`, `…/village/{id}`; same `ApiError` JSON contract |
| `web/handlers.rs` | session-gated dashboard handlers (`spectate_worlds`, `spectate_feed`, `spectate_players`, `spectate_village`) — `require_spectator` guard (403 template like `admin_forbidden`); admin console: Spectator role toggle rides the existing `POST /admin/role`; key mint/revoke forms (`POST /admin/spectator-key`, `POST /admin/spectator-key/revoke`) |
| `web/templates` | `spectate_worlds.html`, `spectate_feed.html` (four sections + countdowns via the existing countdown JS), `spectate_players.html`, `spectate_village.html`; admin.html gains the spectator column/panel; `<meta http-equiv="refresh" content="15">` on the feed for the prototype |
| `web/lib.rs` | route registration: the spectator API router (GET-only) + dashboard routes; the agent rate-guard extended to also namespace `spk` tokens (`spectator:<id>`) |

## Key decisions

- **Separate `spectator_keys` table + `spk_` prefix** (not a kind column on `agent_keys`):
  credentials for different trust domains never share a lookup path — an agent key can never be
  presented on the spectator surface even by bug, because the surfaces query different tables (AC2
  by construction).
- **Role re-checked at auth time** (not only at mint): revoking the role dead-ends existing keys
  without hunting them down.
- **Owner-view reuse for village detail**: the spectator village page/JSON calls the same
  read-model the owner's village page uses (with the owner's id substituted), so values can never
  drift from the owner's truth.
- **No activity side effects**: spectator routes are exempt from the presence-touch middleware
  (they are not player actions on any world).

## Test strategy

- Repo: `#[sqlx::test]` for each new world-scoped query (caps, ordering, world isolation).
- Integration (`crates/web/tests/integration.rs`): AC1 (403 without role, 200 with; admin toggle
  round-trip); AC2 (mint/verify/revoke; role-revoke ⇒ 401; agent key on spectator surface ⇒ 401
  and vice versa); AC3 (owner-view equality on a seeded foreign village); AC4 (a launched attack
  appears with composition for the spectator while the defender's own page stays arrival-only);
  AC5 (seed > cap, assert cap + order); AC6 (POST to spectator paths ⇒ 404/405; `last_activity`
  of the spectator's account unchanged by spectating — and no touch on any watched player);
  AC7 (labeled vs disguised worlds); AC8 (budget exhaustion ⇒ 429 with retry_after_secs).

## Risks

- **Payload size on big worlds** — mitigated by hard caps + paging; the village detail is single-
  village; the feed is O(caps).
- **Fog leak by accident on non-spectator surfaces** — no shared templates with player pages
  except read-model structs; review checks no player-facing route gained new fields.
- **Rate-guard confusion between key kinds** — distinct namespaces in the budget keying
  (`agent:` / `spectator:`), tested.
