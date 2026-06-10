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

- [ ] **T8 — Migrations.** `worlds`, `users`, `villages` (UNIQUE `(world_id,x,y)` — **AC3**),
  `village_fields`, `village_buildings`, `scheduled_events` (`seq` + `(status,due_at,seq)` index),
  `sessions`; all timestamps `timestamptz` (P11).
- [ ] **T9 — Repository adapters.** Implement the `application` ports (`UserRepository`,
  `VillageRepository`, `EventStore`, `WorldConfigProvider`) with SQLx.

## Application (use-cases)

- [ ] **T10 — Register.** Validate + uniqueness; argon2 hash; **single transaction** creating the user
  and their starting village server-side at a unique in-bounds coordinate; honor `email_confirmed`
  policy (**AC1, AC3, AC7**).
- [ ] **T11 — Login / logout.** Verify credentials; establish/clear session (**AC2**).
- [ ] **T12 — Scheduler.** Load pending events on startup; sleep-until-due loop; process exactly once
  (transactional status flip, idempotent); `LISTEN/NOTIFY` to wake early; deterministic `(due_at,seq)`
  ordering (**AC6**, P11).
- [ ] **T13 — Authorization.** Role helpers mapping the spec's role table (Visitor/Player/Administrator)
  to server-side checks (**AC7**).

## Web (`web` — Axum + Askama)

- [ ] **T14 — App skeleton.** Axum router, `tower-sessions` with the Postgres store (stateless tier,
  P5), tracing middleware (per-request latency spans).
- [ ] **T15 — Auth extractor + guard.** Resolve session → current `Player`; reject unauthorized access
  (Visitor→`/village` blocked; world-config endpoints Administrator-only) (**AC7**).
- [ ] **T16 — Routes + templates + CSS foundation.** `GET /`, `GET/POST /register`, `GET/POST /login`,
  `POST /logout`, `GET /village`; Askama `base/index/register/login/village` templates (**AC1, AC2,
  AC3** view). Establish the front-end foundation per `specs/ui-style-guide.md`: token stylesheet,
  base/reset, app-shell skeleton, and button/field components.

## Verification

- [ ] **T17 — Test harness.** Ephemeral Postgres for integration tests (`sqlx::test` or testcontainers).
- [ ] **T18 — Integration tests.** AC1 (register + rejections), AC2 (login/logout), AC3 (one village,
  in-bounds, unique), AC6 (event fires once + survives a simulated scheduler restart), AC7 (authz
  negatives), **AC8** (persist → restart → same account & village).
- [ ] **T19 — P11 smoke test.** Assert register/login/view handlers complete **< 50 ms** server-side
  under dev load (tracing spans).

## Documentation & acceptance

- [ ] **T20 — Technical docs.** rustdoc on public `domain`/`application` items; fill in `CLAUDE.md`
  build/test/run/migrate commands now that they work; add `docs/architecture/` notes for the Cargo
  workspace, the due-event scheduler, and the auth/session design.
- [ ] **T21 — End-user docs.** `docs/manual/getting-started.md` — how to register, confirm (if
  enabled), log in, and view your starting village.
- [ ] **T22 — Review & accept.** Run the `eperica-reviewer` agent on `git diff main...HEAD`; address
  every MUST-FIX; re-review until verdict = **APPROVE**.

## Done when

Per the [Definition of Done](../../implementation-workflow.md#definition-of-done-checklist--applies-to-every-slice):
acceptance criteria **AC1–AC8** pass with tests, the **P11 < 50 ms** budget holds, all gates are green,
**T1–T22** are checked, technical + end-user docs are written, the reviewer verdict is **APPROVE**, the
**PR is merged**, and `spec.md`/`plan.md` are set to **Verified** with slice 001 marked done in the roadmap.
