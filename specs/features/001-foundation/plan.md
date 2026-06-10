# Feature 001 — Foundation & Skeleton — Technical Plan

**Status:** Verified
**Spec:** ./spec.md

## Stack decision (settled here — this is the first slice that writes code)

| Concern | Choice | Why (tied to principles) |
|--------|--------|--------------------------|
| Language | **Rust** | No GC pauses, predictable sub-ms latency (P11); strong type system for a correct domain (P3). |
| Web framework | **Axum** (on Tokio) | High-performance async, ergonomic, the modern Rust default. |
| Templating | **Askama** | Jinja-like `.html`, **compiled & type-checked** → no runtime parse, fastest SSR (P11). |
| Persistence access | **SQLx** (async; runtime-checked queries in v1, offline-cached compile-time checking optional later) | Fast, no heavy ORM; builds require no live DB. |
| Database | **PostgreSQL** | Concurrency + scale, `timestamptz` µs precision (P11), pairs with a Redis hot-cache later. |
| Auth | **argon2** password hashing + **encrypted-cookie sessions** (`axum-extra` `PrivateCookieJar`) | Stateless: the player id lives in an encrypted cookie, so any instance serves any request (P5). No session store. *(Chosen over tower-sessions; recorded per P8.)* |
| Observability | **tracing** | Latency is measurable from day one (P11). |

**Frontend interactivity** (htmx + a small JS countdown helper) is **deferred to 002/003**; slice 001
uses plain server-rendered pages with form posts only.

### Workspace layout (enforces P3 by dependency direction)

```
eperica/
├── Cargo.toml                 # workspace
├── crates/
│   ├── domain/                # PURE: entities, value objects, rules. NO I/O, no sqlx, no axum.
│   ├── application/           # use-cases/commands + PORTS (traits) the domain needs. Depends on domain.
│   ├── infrastructure/        # SQLx adapters implementing the ports. Depends on application + domain.
│   └── web/                   # Axum handlers + Askama templates + auth. Depends on application + infrastructure.
├── migrations/                # SQLx migrations
└── templates/                 # Askama .html templates
```

`domain` has **no** dependency on `infrastructure`/`web`; the compiler enforces the pure core (P3).
`application` defines traits (ports) like `VillageRepository`; `infrastructure` implements them with
SQLx (hexagonal style → DB is swappable, domain stays testable).

## Constitution check

- **P3 (pure domain):** game rules live in `domain`; the workspace forbids it from importing I/O crates.
- **P4 (server authority):** every mutation runs in `application` services behind server-side
  authorization (roles); the client never supplies ownership or coordinates (AC7).
- **P5 (stateless tier / DB truth):** Axum handlers hold no game state; the session is an encrypted
  cookie (no server-side session state), so any instance serves any request and a restart loses no
  account/village data (AC8).
- **P7 (configurable speed):** `WorldConfig { speed, radius }` is loaded at startup (operator-set) and
  passed into the domain; a `GameSpeed` value object provides `scale(base_duration) = base / speed`.
  No wall-clock constant is hardcoded (AC5).
- **P1 (lazy, event-driven):** a `scheduled_events` table + a Tokio scheduler that **sleeps until the
  next due event** (no global tick). Slice 001 proves it with one trivial event kind (AC6).
- **P2 (reproducible / restart):** all state (accounts, villages, pending events) is in Postgres; the
  scheduler re-reads pending events on startup (AC6, AC8).
- **P11 (performance & timing):** Rust/Axum low-latency path; `timestamptz` at µs resolution; scheduler
  fires with minimal jitter (sleep-until-due + `LISTEN/NOTIFY` to wake early when a sooner event is
  inserted); deterministic ordering by `(due_at, seq)`. **Latency budget for 001:** read/interactive
  handlers (e.g. `GET /village`) complete **< 50 ms** under dev load; a smoke test asserts it. Auth
  POSTs (`register`/`login`) are intentionally dominated by argon2 hashing and are exempt (security,
  not a hot game path). (Real load testing → slice 023.)
- **P6 (seeded randomness):** not exercised in 001; coordinate assignment uses a deterministic
  placeholder strategy (sequential/spiral), with seeded map generation arriving in 006.

## Domain model (`domain` crate)

- `WorldConfig { speed: GameSpeed, radius: u32 }` — value object.
- `GameSpeed(f64)` — with `fn scale(&self, base: Duration) -> Duration` and rate equivalents (AC5).
- `Coordinate { x: i32, y: i32 }` — with `fn in_bounds(radius) -> bool` (AC3).
- `Player { id, ... }` (account identity; credentials handled in infra).
- `Village { id, owner: PlayerId, coord: Coordinate, tribe: Option<Tribe> /* None in 001 */,
  fields: [ResourceFieldSlot; 18], buildings: Vec<BuildingSlot> }`.
- `fn create_starting_village(owner, coord, config) -> Result<Village>` — builds the baseline:
  18 field slots + Main Building + Rally Point at starting levels from balance data (AC4).
- `ScheduledEvent { id, kind, due_at, seq }` + `enum EventKind { Heartbeat /* trivial, 001 */ }`.

Pure unit tests target: `GameSpeed::scale` (AC5), `Coordinate::in_bounds` (AC3),
`create_starting_village` shape (AC4).

## Persistence (`infrastructure` crate + `migrations/`)

Tables (all timestamps `timestamptz`):
- `worlds (id, speed, radius, created_at)`
- `users (id, username, email, password_hash, email_confirmed bool, created_at)`
- `villages (id, world_id, owner_id, x, y, tribe nullable, created_at, UNIQUE(world_id, x, y))` — the
  unique constraint guarantees AC3's "no two villages share a coordinate."
- `village_fields (village_id, slot smallint, resource_type, level)` — 18 rows per village (AC4).
- `village_buildings (village_id, slot, building_type, level)` — Main Building + Rally Point (AC4).
- `scheduled_events (id, kind, payload jsonb, due_at, seq bigserial, status, created_at)` — index on
  `(status, due_at, seq)` for the scheduler; `seq` gives deterministic same-instant ordering (P11).
- *(No session table — sessions are encrypted cookies, P5.)*

Balance values (starting levels, the 18-field default layout) live in `specs/balance/` and are loaded,
not hardcoded (AC4).

## Application / services (`application` crate)

- Ports (traits): `UserRepository`, `VillageRepository`, `EventStore`, `WorldConfigProvider`.
- `register(cmd) -> Result<...>`: validate + uniqueness; hash password (argon2); in **one
  transaction** create the user and, server-side, their starting village at a unique in-bounds
  coordinate (AC1, AC3, AC7). Honors `email_confirmed` policy (AC1 / Decisions).
- `login` / `logout`: verify credentials; establish/clear session (AC2).
- `Scheduler`: on startup load `status='pending'` events; loop = pick next by `(due_at, seq)`, sleep
  until `due_at`, process exactly once (transactional status flip → idempotent), `LISTEN/NOTIFY` to
  wake on newly-inserted earlier events (AC6, P11).
- Authorization helpers mapping the spec's role table to checks (Player vs Administrator vs Visitor).

## Interface (`web` crate — Axum + Askama)

Routes:
- `GET /` — public landing (Visitor).
- `GET/POST /register`, `GET/POST /login`, `POST /logout` (Visitor → becomes Player).
- `GET /village` — the player's starting village; **auth required** (Player only).

Cross-cutting:
- Auth extractor resolves the session → current `Player`; a role guard rejects unauthorized access
  (Visitor → village = redirect/401; any world-config endpoint = Administrator only) — AC7, roles table.
- `WorldConfig` is operator-set via config/env in 001 (no admin UI yet); a Player has no path to change
  speed (AC5 negative).
- Templates: `base.html`, `index.html`, `register.html`, `login.html`, `village.html` (Askama).
- **Conforms to [ui-style-guide.md](../../ui-style-guide.md).** This slice **establishes the front-end
  foundation**: the token stylesheet, base/reset, the app-shell skeleton, and the button/field
  components needed for register/login/village views.

## Test strategy

| AC | Test |
|----|------|
| AC1 | Integration: valid register creates user; invalid/duplicate rejected, no row created. Email-confirm flag honored. |
| AC2 | Integration: correct creds log in; wrong creds rejected; logout clears session. |
| AC3 | Integration: after register, exactly one village owned by the user, in-bounds; UNIQUE blocks duplicate coords. |
| AC4 | Domain unit: `create_starting_village` yields 18 fields + Main Building + Rally Point at balance levels. |
| AC5 | Domain unit: `GameSpeed::scale(D)` == `D/S`; varying `S` scales proportionally; grep/test guard that no hardcoded duration constant exists on hot paths. |
| AC6 | Integration: schedule a near-future Heartbeat → fires once at/after due; re-create scheduler (simulated restart) with a pending event → still fires (persisted). |
| AC7 | Integration: crafted request cannot create/own/misplace a village; non-owner/visitor blocked server-side. |
| AC8 | Integration: persist user+village, restart app/process, log in → same account & village. |
| P11 | Smoke: register/login/view handlers complete < 50 ms server-side under dev load (tracing spans). |

Test infra: `sqlx::test` (or testcontainers) for an ephemeral Postgres; domain tests need no DB.

## Notes / follow-ups

- This plan realizes the README's placeholder `src/` as a **Cargo workspace under `crates/`**; the
  README's process tree should be updated to match (small standing-doc edit).
- `specs/balance/` is introduced by this slice (starting levels + default 18-field layout).
- htmx + client-side countdown helper deferred to 002/003 (first slice with live timers).
