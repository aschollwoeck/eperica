# Plan — 118 Agent API core

**Status:** Draft (spec approved — ADR 0036)

## Constitution check

- **P1 (lazy time):** nothing scheduled; the digest is compute-on-read via the same settled read models
  the pages use (`load_economy`, `load_culture`, …). Timestamps in responses are absolute ms.
- **P3 (pure domain):** no domain change at all. The API is adapters: routes/extractor/DTOs in `web`,
  reusing `application` read models + use-cases. No AI logic anywhere server-side (ADR 0036).
- **P4 (server-authoritative):** actions call the **same use-cases** as the form handlers
  (`order_build`, `order_train`); the bearer path resolves an account and then reuses the exact
  world-scope resolution (`player_in_world`, registry) `GameContext` performs. No agent-only bypass.
- **P7 (configurable speed):** nothing new is time-scaled; responses carry absolute deadlines computed
  by the existing speed-aware code.
- **P11 (performance):** one digest request = the same bounded reads as one village-page render, summed
  over owned villages; keys verify by **SHA-256 lookup** (not argon2 — see Decisions) so auth adds ~0 to
  the hot path; all agent traffic sits behind a dedicated 022 rate budget.

## Decisions (resolving the spec's open questions)

1. **Auth plumbing.** A new `AgentContext` extractor (in a new `crates/web/src/api.rs`):
   `Authorization: Bearer epk_…` → key lookup → bound AI account → then the **same steps** as
   `GameContext` (world from path, `player_in_world`, `world_registry.context_for`) — but every failure
   returns **JSON** (`401`/`403`/`404`) instead of a redirect. To avoid drift, the world-resolution core
   is shared with `GameContext` (extract a helper in `auth.rs`; `GameContext` keeps redirects, the agent
   path maps the same failures to JSON).
2. **Key format & storage.** `epk_<id>_<secret>` — `id` = 8-byte hex (indexed lookup), `secret` =
   32 random bytes base64url. Stored: `sha256(secret)` hex, compared constant-time. *Deviation from the
   spec's "hashed like passwords" wording:* argon2 exists to stretch low-entropy passwords; a 256-bit
   random secret gains nothing from stretching and argon2 (~50–100 ms) would tax every agent request
   (P11). SHA-256-at-rest preserves the property that a DB leak reveals no usable keys. Keys are
   revocable (`revoked_at`); plaintext is returned exactly once at creation.
3. **AI account bootstrap.** Keys bind only to `users.is_ai` accounts (AC-level rule). 120 owns the full
   seeding UI; 118 ships the minimum: `POST /admin/agent { username, world, tribe }` (admin console,
   admin-gated like `POST /admin/world`) → creates the `is_ai` account, joins it to the world via the
   existing join use-case, issues a key, and shows the key once. Enough to run AC6 and the future runner.
4. **Digest assembly.** In `web` (`api.rs`), composing the **five existing read points** per the village
   page: `load_economy`, `active_builds`, `active_training`, `load_culture`,
   `incoming_against` — plus the map-window cells via the existing map read models. No new repository
   methods, no raw SQL; DTOs are `serde::Serialize` structs in `api.rs`. (Considered an `application`
   assembler; rejected — it would add serde to `application` for zero logic gain, and AC3 pins
   digest-equals-page at the integration level regardless.)
5. **Rate limiting.** A dedicated `agent_rate_guard` middleware on the `/api` router covering **all**
   methods (the global guard skips GETs, but digest polling is the agent hot path): subject = the
   **unverified key-id** (`agent:<id>`, parsed by the same strict `bearer_token` helper the extractor
   uses — the budget and authentication can never disagree on what counts as a token). Keying on the
   unverified id is a recorded trade: no DB round-trip or hash on the guard path (P11), failed-auth
   floods are still counted, and the id is only ever exposed inside the one-time plaintext token, so a
   third party cannot burn a victim's budget without already holding the key. Action `"agent"`, limit
   from a new `agent_limit_per_window` in `fairplay.toml` (process-global, 048 precedent). Over budget
   → `429` JSON with `retry_after_secs`.
6. **Errors.** One JSON error shape everywhere: `{ "error": "<machine_code>", "reason": "<text>" }` —
   `error` is a stable snake_case code (`unauthorized`, `not_joined`, `rate_limited`, `insufficient`,
   `lane_busy`, `max_level`, …) mapped from the use-case error enums; `reason` is the player-visible
   message. Status: 401 auth, 403 scope, 404 unknown ids, 409 rule denials, 429 rate.

## Persistence

Migration `0050_agent_keys.sql`:

```sql
ALTER TABLE users ADD COLUMN is_ai boolean NOT NULL DEFAULT false;   -- formalized further in 120
CREATE TABLE agent_keys (
    id          text PRIMARY KEY,          -- the public key-id half (hex)
    user_id     uuid NOT NULL REFERENCES users(id),
    secret_hash text NOT NULL,             -- sha256(secret), hex
    created_at  timestamptz NOT NULL DEFAULT now(),
    revoked_at  timestamptz
);
CREATE INDEX agent_keys_user ON agent_keys (user_id);
```

Ports (`application/ports.rs`, implemented in `infrastructure/repo.rs`): `create_agent_key`,
`find_agent_key(id) -> Option<(user_id, secret_hash, revoked)>`, `revoke_agent_key`, and `set_is_ai`.
(No separate `create_ai_account`: the bootstrap **composes** the existing `register` use-case — with a
synthetic random password + `@ai.invalid` email, pre-confirmed — then `set_is_ai`, so account/village
placement logic is never duplicated.)

## Interface

| Method & path | Auth | Notes |
|---|---|---|
| `GET  /api/me` | key | account, `is_ai`, worlds joined (id, name, player, tribe) |
| `GET  /api/w/{world}/state` | key + joined | the digest (spec) |
| `GET  /api/w/{world}/map?x&y&r` | key + joined | map window, `r` clamped to 10 |
| `POST /api/w/{world}/village/{village}/build` | key + joined | `{target, slot, kind?}` → `order_build` |
| `POST /api/w/{world}/village/{village}/train` | key + joined | `{unit, count}` → `order_train` |
| `POST /admin/agent` | admin session | bootstrap: AI account + world join + key (shown once) |

Request bodies are `axum::Json` (first use in the codebase — the form handlers stay `Form`).

## Test strategy

- **Unit (web):** key parse/format round-trip; constant-time verify; error-shape mapping from each
  use-case error variant to (status, code).
- **Integration (`crates/web/tests/integration.rs`, `#[sqlx::test]`):**
  - AC1: bad/missing/revoked key → 401 JSON (no redirect); valid key → `/api/me` reflects the account.
  - AC2: key for an account not joined to `{world}` → 403 JSON; blocked account denied.
  - AC3: digest fields equal the page-truth read models for a seeded village (assert against the same
    application calls, not HTML scraping); incoming-attack entries contain **only** village + arrival.
  - AC4: build/train success + each denial class (insufficient, lane busy, max level, foreign village)
    with correct status/code; effects visible in the next digest.
  - AC5: exceed `agent_limit_per_window` → 429 with `retry_after_secs`.
  - AC6: the scripted opening loop from the spec, end-to-end over HTTP against the spawned app.
- **P11:** digest handler stays within the existing page budget (it performs the same reads).

## Tasks (serial, gated per task)

See [tasks.md](tasks.md). Gates every task: `cargo fmt --all -- --check`,
`clippy --all-targets -- -D warnings`, `cargo test --workspace`, P11 budget.

## Key risks

- **Extractor drift.** Two context paths (cookie vs bearer) could diverge on a future gate (e.g. a new
  freeze check added only to `GameContext`). Mitigated by sharing the world-resolution helper and a
  test asserting the agent path hits the same denial on a frozen world.
- **First `axum::Json` bodies.** Malformed-JSON rejections must come back as our JSON error shape, not
  axum's default plain text — needs a rejection mapper on the router.
- **Key-in-logs.** The plaintext key must never be logged; only the `id` half may appear in traces.
- **Digest cost on many-village accounts.** One `load_economy(selected = v)` per owned village — and
  each call re-hydrates the owner's village list inside `select_village`, so the digest is O(V²) in
  owned villages today. Deliberately accepted: page-truth parity outweighs the cost at bot scale
  (agents hold a handful of villages), and the reads are bounded per call. Revisit with a shared
  village list (the pure `pick_village`) if 119+ agents grow large empires.
