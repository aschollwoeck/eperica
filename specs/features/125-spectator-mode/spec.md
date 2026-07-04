# Feature 125 — spectator mode: the omniscient read-only world view

**Status:** Verified (reviewer APPROVE at fd11234; both SHOULD-FIX findings closed in-slice)
**Depends on:** 016/018 read paths (rankings, reports), 009/003/007 (movements, build/training
queues), 034/045 (worlds, per-world context), 118 (bearer-key auth pattern), roles.md.
**Origin:** operator request — watch a world (e.g. an AI-fleet world) in full detail without
joining it: every village of every player, what's building/training, incoming and outgoing
attacks, via API and a first dashboard.

## Goal

A **Spectator** is a trusted, admin-granted observer. On any world — without holding a player
there — a spectator sees **everything, and can do nothing**: a live activity picture of the whole
world plus full drill-down into any village (resources, queues, garrison, movements with
compositions). The same omniscient picture is available as a read-only JSON API under dedicated
**spectator keys** for external tools.

## Concepts

- **A new account role: Spectator** (additive, like Moderator/Administrator; stored as
  `users.is_spectator`, granted/revoked in the admin console). Spectating requires the role —
  there is no per-world flag and no self-exclusion rule: **the grant is the trust decision**.
  ⚠ Documented caveat (roles.md): a spectator who also plays sees through fog everywhere,
  including worlds they play on. Operators grant this role to observers, casters, and themselves —
  not to active competitors.
- **Full omniscience, zero agency.** Spectators read the true server state: per-village resources
  (computed on read, P1), build queue, training batches, garrison, reinforcements, loyalty,
  research, and **movements in flight with their compositions** (attacks, raids, reinforcements,
  returns, settlers, merchants — both directions). No mutating route exists on the surface —
  read-only by construction, not by permission check (P4).
- **The disguise holds.** Spectators see the *game-facing* world: on `disguised` worlds AI players
  look human; on `labeled` worlds they carry the NPC tag. Operator truth stays in `/admin`.
- **Spectator keys** — `spk_<id>_<secret>`, admin-minted, SHA-256 at rest, shown once, revocable
  (the 118 pattern; separate `spectator_keys` table and prefix so agent and spectator credentials
  can never be confused). A key authenticates only while its account holds the Spectator role —
  revoking the role dead-ends the keys instantly.
- **The feed is a bounded snapshot, not a new event store (P1/P11).** "Ongoing activity" is
  assembled per request from what already exists: movements in flight, active build orders,
  active training batches, and the world's most recent battle reports — each capped (top-N per
  category, nearest-deadline first). No new persisted events, no polling loops server-side.

## Surfaces

**Dashboard (session-authenticated, role-gated):**
- `/spectate` — world picker (all worlds, running or frozen).
- `/spectate/{world}` — the **live feed**: movements in flight (kind, from → to, composition,
  arrival countdown), builds and trainings completing soonest, recent battles; each row links
  into the drill-down. Meta-refresh/auto-poll is fine for the prototype.
- `/spectate/{world}/players` → every player (population, villages, alliance) →
  `/spectate/{world}/village/{id}` — the full village internals.

**API (spectator-key bearer auth, JSON, mirrors the dashboard):**
- `GET /spectator/me` — key introspection.
- `GET /spectator/w/{world}/feed` — the capped activity snapshot.
- `GET /spectator/w/{world}/players?page=` — paged player index.
- `GET /spectator/w/{world}/village/{id}` — full village detail.
- Same JSON error contract as the Agent API (401/403/404/429 + `{error, reason}`); requests count
  against the same per-key rate budget class as agent keys.

## Acceptance criteria

- **AC1 — Role & grant.** `is_spectator` exists; the admin console grants/revokes it like the
  other roles; roles.md gains the Spectator row (with the fog caveat). Non-spectators get 403
  from every `/spectate` page; spectators without a player on the world get the full view.
- **AC2 — Keys.** Admin mints/revokes spectator keys (`spk_`, hash-at-rest, shown once). A key
  authenticates only while the account holds the role (role revoked ⇒ 401). Agent keys are
  refused on the spectator surface and vice versa.
- **AC3 — Omniscient village detail.** For a foreign village the spectator (web + API) sees
  resources (computed on read), build queue with deadlines, training batches, garrison,
  stationed reinforcements, loyalty — values equal to what the owner sees.
- **AC4 — Movements with compositions, both directions.** In-flight attacks/raids/reinforcements/
  returns/settlers/merchant shipments of **any** player are listed with kind, endpoints, arrival
  time and composition — including hostile movements that the defender's own view would only show
  as an arrival-only warning.
- **AC5 — The feed is bounded.** Each category is capped (N ≤ 50) and ordered by nearest
  deadline/most recent; the endpoint cost is a fixed set of indexed, world-scoped queries (P11 —
  no whole-world scan per refresh).
- **AC6 — Read-only by construction.** The spectator router registers no mutating route; no
  handler on the surface calls a mutating use-case. A POST to any spectator path is 404/405.
  Spectating never touches game state (no activity/presence side effects on the watched world).
- **AC7 — Disguise & visibility.** On `labeled` worlds spectator surfaces show NPC tags; on
  `disguised` worlds they don't reveal `is_ai` anywhere.
- **AC8 — Rate-limited.** Spectator-key requests share the agent budget class (429 +
  `retry_after_secs` beyond it).

## Roles & permissions

Per [roles.md](../../roles.md) — this slice **adds a role**:
- **Spectator (new):** everything above; no game actions anywhere (the role is orthogonal to
  Player — an account can hold both, see the caveat).
- **Player:** unchanged; cannot access `/spectate` (403) without the role.
- **Moderator:** unchanged (no implicit spectator powers).
- **Administrator:** grants/revokes the role, mints/revokes keys; the admin console lists key
  state per account.
- **Agent (AI):** agent keys do not open the spectator surface (AC2).

## Out of scope

- A persisted world event log / websocket push feed (the polled snapshot is the prototype).
- Replays, historical timelines, charts beyond what stat pages already give.
- Per-world spectate flags or player-exclusion rules (the admin grant is the only gate).
- Spectator visibility into `/admin`/`/mod` surfaces or operator truth (disguises stay).
