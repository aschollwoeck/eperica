# Feature 118 — Agent API core (key auth + state digest + economy actions)

**Status:** Verified (built on feature/118-agent-api-core; reviewer APPROVE)
**Depends on:** ADR 0036 (program), ADR 0034 (`GameContext`/world scoping), 022 (rate limiting),
002/003/005 (the economy/build/train use-cases this exposes).
**Roadmap:** slice 1 of the AI-players program (118–122).

## Goal

A machine client holding an **API key** can *play the opening loop* of Eperica over JSON: read its full
(player-visible) state, order field/building upgrades, and train troops — with every rule enforced by the
**existing use-cases** (P4) and nothing visible beyond what the HTML pages show a human.

This slice delivers the API *foundation*: auth, the state digest, and the economy actions. Military,
market, settling and messages follow in 119; AI-account seeding/visibility in 120.

## Concepts

- **Agent key.** A bearer token bound to exactly one **AI account** (`users.is_ai`, formalized in 120 —
  this slice creates keys for accounts flagged as AI; a bootstrap admin path creates one such account for
  testing). Keys are generated server-side, shown once, stored **hashed** (like passwords), revocable.
  `Authorization: Bearer <key>` resolves the account exactly as a session cookie resolves a user; from
  there the request flows through the same effective-identity + `GameContext` path — world scoping,
  ownership checks and freeze/block gates are byte-for-byte the players'.
- **State digest.** One compact JSON document per world (`GET /api/w/{world}/state`) assembled **only
  from the read models the HTML handlers already use** — never from repositories directly. Fog-of-war
  honest by construction: incoming attacks are arrival-only (§7.3), no enemy garrison/production, map
  cells carry what the map page carries.
- **Action adapters.** JSON POSTs that deserialize into the **same use-case calls** the form handlers
  make (`order_build`, `order_train`). No new game logic, no new authorization logic. Use-case denials
  map to structured JSON errors with the same reasons a player would see.
- **Versioning.** Everything lives under `/api/` and constitutes a public contract (ADR 0036); breaking
  changes require a new version prefix.

## Endpoints (this slice)

| Method & path | Purpose |
|---|---|
| `GET  /api/w/{world}/state` | The state digest (below) |
| `GET  /api/w/{world}/map?x&y&r` | A bounded map window (same data as the map page; `r` clamped) |
| `POST /api/w/{world}/village/{village}/build` | `{ "target": "field"\|"building", "slot": n, "kind"?: "..." }` → `order_build` |
| `POST /api/w/{world}/village/{village}/train` | `{ "unit": "...", "count": n }` → `order_train` |
| `GET  /api/me` | Key introspection: account, worlds joined, per-world player + tribe |

**State digest contents:** per owned village — id, coordinate, capital flag, resources
(amount/rate/capacity per kind, crop net), fields (slot/kind/level), buildings (slot/kind/level), build
queue (target, level, completes-at), training queue (unit, remaining, next-complete-at), garrison;
per player — culture (cp, rate, next threshold, slots used/allowed), incoming attacks (village,
arrival-at — **nothing else**), and the latest report heads (id, occurred-at, outcome; battle reports
carry no read-state in the schema, so there is no unread count — the full report read arrives with
119). Timestamps are absolute ms
(P1/P7-safe: the client computes countdowns; nothing is wall-clock-dependent server-side).

## Acceptance criteria

- **AC1 — Key auth.** A request with a valid `Authorization: Bearer` key acts as the bound AI account;
  a missing/invalid/revoked key gets `401` JSON (never a login redirect). Keys are stored hashed;
  the plaintext is shown exactly once at creation. A key never grants another account's data (P4).

- **AC2 — Same gates as players.** Agent requests pass through the same world-scope resolution as
  session requests: an unjoined world → `403` JSON; a frozen (world-won) world or blocked account is
  denied exactly where a player would be. No agent-only bypass exists.

- **AC3 — Digest = page truth.** Every number in the state digest equals what the corresponding page
  renders for that player at the same instant (same read models). The digest contains **no**
  fog-of-war-violating field: incoming attacks carry arrival time only; map cells match the map page.

- **AC4 — Economy actions are the use-cases.** `build`/`train` succeed and fail under exactly the rules
  of `order_build`/`order_train` (affordability, queue lanes, prerequisites, caps, ownership). A denial
  returns structured JSON `{ "error": <machine code>, "reason": <the player-visible message> }` with
  the appropriate 4xx status. Success returns the created queue entry with its completes-at.

- **AC5 — Rate-limited (P11).** Agent endpoints are covered by the 022 DB-backed limiter under an
  agent-specific budget (config-backed); exceeding it returns `429` with a retry-after. The digest is
  cheap: one request performs a bounded number of queries independent of world size.

- **AC6 — Opening loop end-to-end.** A scripted client (test) holding a fresh key: reads `/api/me`,
  reads the digest, orders a field upgrade, is correctly denied a second same-lane order, trains units
  after affording them, and observes both queues progressing in subsequent digests — all against a live
  world, no HTML endpoints involved.

- **AC7 — No sim change.** No behaviour of the game changes for browser players; the API adds read/act
  adapters only. `domain` gains no I/O (P3).

## Roles & permissions

Per [roles.md](../../roles.md). The agent key introduces a **machine credential for an AI account** —
it is not a new role; the bound account is a Player in each world it joined.

| Role | Permitted | Denied (server-enforced) |
|------|-----------|--------------------------|
| **Agent (AI account via key)** | Read own digest/map; build/train in own villages (AC1–AC4). | Anything cross-account; unjoined worlds; all admin/moderation surfaces (an AI account is never moderator/admin). |
| **Visitor** | — | All `/api/` endpoints (`401`). |
| **Player (human)** | — (no keys in this program, ADR 0036). | Creating/holding agent keys. |
| **Administrator** | Create AI accounts (bootstrap), issue/revoke keys (full UI in 120). | — |
| **Moderator** | N/A (considered) — moderation of AI accounts arrives with 120's visibility work. | — |
| **System** | Key-hash verification at request time; nothing scheduled. | — |

## Out of scope (later slices)

- Military/market/settle/message actions — **119**.
- `is_ai` formalization, admin seeding UI, per-world labeled/disguised visibility, fair-play/lifecycle
  carve-outs — **120**.
- The bot runner and any decision logic — **121/122**. This slice ships no bot behaviour at all.
- Own-account keys for human players — rejected for this program (ADR 0036).
- OpenAPI/docs generation — nice-to-have, not required to accept.

## Open questions (to resolve in plan.md)

- Auth plumbing: a parallel `AgentContext` extractor vs. teaching the session extractor a bearer path.
  Leaning: a small bearer→account resolver that then reuses the existing `GameContext` internals.
- Key format: random 256-bit, `epk_`-prefixed, argon2-hashed like passwords (reuse the existing hasher).
- Where the digest assembler lives: `application` (a read model composing existing read models), so the
  web layer stays a serializer (P3-consistent).
