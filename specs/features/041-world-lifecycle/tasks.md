# Feature 041 — World lifecycle admin (create worlds live) — Tasks

**Plan:** ./plan.md · **Spec:** ./spec.md

Each task a commit; gates (`fmt` / `clippy -D warnings` / `test` + P11) pass before advancing. No
pure-domain task (identity/role + runtime composition).

## Persistence & ports

- [x] **T1 — Infra create/read + ports.** `world::create_world` + `world_by_id`; re-export `PgPool`.
  `AdminRepository::create_world` + `list_worlds` (+ `AdminWorld` view) impl'd on `PgAccountRepository`. (AC1/AC3)

## Use-cases

- [x] **T2 — `application::admin::create_world` + `list_worlds`.** Admin-gated; validate speed
  (`GameSpeed::new`) + radius (`MAX_WORLD_RADIUS`); return the new `WorldId`. **Unit test:**
  `create_world_is_gated_and_validated` (non-admin rejected; bad speed/radius rejected; valid creates). (AC1)

## Web — registry + handler + UI

- [x] **T3 — `WorldRegistry` + one spawn path.** `registry.rs`: holds the shared rules + shutdown +
  running map; `start_world(id)` builds the world-scoped runtime and spawns its scheduler (idempotent);
  `join_all()` on shutdown. `main.rs` uses it for every world; `AppState` holds `Arc<WorldRegistry>`. (AC2)
- [x] **T4 — Create-world handler + console UI.** `POST /admin/world` (gated) → `create_world` →
  `registry.start_world` → flash; `/admin` lists all worlds + the create form. (AC1/AC2/AC3)

## Acceptance

- [x] **T5 — Integration + boot smoke.** `admin_creates_world_live`: non-admin 403; invalid radius flashed
  (no row); valid create persists with the given speed/radius + is listed; the harness's real registry
  spawns its scheduler. Boot smoke: admin creates a world via `POST /admin/world` → "registry started
  scheduler for world" with **no restart**; the DB grows by one. (AC1/AC2/AC3)
- [x] **T6 — Regression.** Full workspace suite passes unchanged (AC4). Spec/plan/tasks + roadmap/ADR.
