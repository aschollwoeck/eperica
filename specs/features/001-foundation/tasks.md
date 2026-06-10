# Feature 001 — Foundation & Skeleton — Tasks

**Plan:** ./plan.md · **Spec:** ./spec.md

Ordered for dependency and for testability (pure domain first). Each task is small enough to finish
and verify in one sitting. AC references point back to `spec.md`.

## Scaffolding

- [x] **T1 — Cargo workspace.** Create the workspace + four crates (`domain`, `application`,
  `infrastructure`, `web`) with the dependency direction from the plan; confirm `domain` cannot
  reference `infrastructure`/`web` (P3).
- [x] **T2 — Tooling/CI.** `cargo fmt`, `clippy` (deny warnings), `cargo test` wired into CI
  (`.github/workflows/ci.yml`); `.gitattributes` normalizes line endings; basic `tracing` subscriber
  set up (P11 observability).
- [x] **T3 — Config & Postgres.** `AppConfig::from_env` loads `WorldConfig { speed, radius }` + DB URL
  (operator-set, P7; `.env` support); SQLx `PgPool` + embedded `MIGRATOR`/`run_migrations`; dev
  Postgres via Docker; connectivity test (skips without `DATABASE_URL`).

## Domain (pure, test-first)

- [x] **T4 — Value objects.** `GameSpeed` (scale duration/rate), `Coordinate::in_bounds`,
  `WorldConfig`, hand-rolled `DomainError` (zero deps); unit tests for proportional scaling (**AC5**)
  and bounds (**AC3**).
- [x] **T5 — Balance data.** `specs/balance/starting-village.toml` (4-4-4-6 layout + core buildings);
  infrastructure `balance::starting_village()` embeds and parses it into the domain `StartingVillage`
  (serde DTOs keep the domain serialization-free). Test verifies the layout (**AC4**).
- [x] **T6 — Village construction.** `PlayerId`/`VillageId`/`Tribe`, `ResourceField`/`BuildingSlot`,
  validated `StartingVillage`, and `Village::found` (18 fields + Main Building + Rally Point); unit
  tests (**AC4**).
- [x] **T7 — Event types.** `Timestamp` (Unix-ms UTC), `ScheduledEvent` + `EventKind::Heartbeat` (the
  trivial event for **AC6**).

## Persistence (`infrastructure` + `migrations/`)

- [x] **T8 — Migrations.** `0001_initial_schema.sql`: `worlds`, `users`, `villages`
  (UNIQUE `(world_id,x,y)` — **AC3**; owner index), `village_fields`, `village_buildings`,
  `scheduled_events` (`seq` + `(status,due_at,seq)` index, P11); `timestamptz` throughout. Sessions
  table is created by the tower-sessions store at startup (T14). Verified applied against live DB.
- [x] **T9 — Repository adapters.** `Argon2Hasher` (PasswordHasher), `PgAccountRepository`
  (AccountRepository: transactional `create_account` = user + village with savepoint-based placement;
  `find_user_by_*`, `villages_of`), and `ensure_world`. Integration test against live Postgres covers
  AC1/AC3/AC4. (EventStore arrives with the scheduler, T12.)

## Application (use-cases)

- [ ] **T10 — Register.** Validate + uniqueness; argon2 hash; **single transaction** creating the user
  and their starting village server-side at a unique in-bounds coordinate; honor `email_confirmed`
  policy (**AC1, AC3, AC7**).
- [ ] **T11 — Login / logout.** Verify credentials; establish/clear session (**AC2**).
- [x] **T12 — Scheduler.** `EventStore` port + `PgEventStore` (schedule; atomic `claim_due`
  pending→processing with `FOR UPDATE SKIP LOCKED`, `(due_at,seq)` order — P11; `mark_done`).
  `process_due` use-case processes claimed events exactly once. `Scheduler::run` background poll loop.
  Integration test covers **AC6** (once-only) + persistence. *Plan deviation (P8): short-poll loop now;
  sleep-until-due + LISTEN/NOTIFY is a later refinement.*
- [x] **T13 — Authorization.** `AuthUser` extractor enforces Player-only access server-side (Visitors
  redirected to `/login`); world config is Administrator-set via env, with no Player-reachable endpoint
  (**AC7**, roles.md).

## Web (`web` — Axum + Askama)

- [x] **T14 — App skeleton.** Axum router (`lib::router`) + `AppState` (DI via `Arc`), TraceLayer
  (per-request spans), graceful shutdown, scheduler spawned. *Sessions use encrypted cookies
  (`PrivateCookieJar`) — stateless (P5), no session store; plan deviation from tower-sessions (P8).*
- [x] **T15 — Auth extractor + guard.** `AuthUser` reads the encrypted cookie → `PlayerId`; missing/
  invalid → redirect to `/login`. Login/register set the cookie; logout clears it (**AC7**).
- [x] **T16 — Routes + templates + CSS foundation.** `GET /`, `GET/POST /register`, `GET/POST /login`,
  `POST /logout`, `GET /village`, `GET /static/app.css`; Askama `base/index/register/login/village`
  templates (**AC1, AC2, AC3** view). Front-end foundation per `specs/ui-style-guide.md`: token
  stylesheet, base, app shell, button/field/alert/resource components.

## Verification

- [x] **T17 — Test harness.** `spawn()` boots the real `router` on an ephemeral port over the live DB;
  cookie-aware `reqwest` client; tests skip without `DATABASE_URL`. (Spawned-app + reqwest rather than
  `sqlx::test`.)
- [x] **T18 — Integration tests.** End-to-end: register→village (AC1/AC3/AC4), login + bad password
  (AC2), `/village` unauthenticated → `/login` (AC7), duplicate rejected (AC1), and persist → fresh
  instance → same account & village (**AC8**). AC6 covered by the scheduler test (T12).
- [x] **T19 — P11 smoke test.** Asserts the read path `GET /village` completes **< 50 ms** (auth POSTs
  exempt — argon2 by design).

## Documentation & acceptance

- [x] **T20 — Technical docs.** rustdoc on public items; `CLAUDE.md` updated with working
  build/test/run/db commands + the cargo/toolchain and migration gotchas; `docs/architecture/` notes
  for workspace/layering, the event scheduler, and auth/sessions.
- [x] **T21 — End-user docs.** `docs/manual/` index + `getting-started.md` (register, confirm if
  enabled, log in, view your starting village).
- [ ] **T22 — Review & accept.** Run the `eperica-reviewer` agent on `git diff main...HEAD`; address
  every MUST-FIX; re-review until verdict = **APPROVE**.

## Done when

Per the [Definition of Done](../../implementation-workflow.md#definition-of-done-checklist--applies-to-every-slice):
acceptance criteria **AC1–AC8** pass with tests, the **P11 < 50 ms** budget holds, all gates are green,
**T1–T22** are checked, technical + end-user docs are written, the reviewer verdict is **APPROVE**, the
**PR is merged**, and `spec.md`/`plan.md` are set to **Verified** with slice 001 marked done in the roadmap.
