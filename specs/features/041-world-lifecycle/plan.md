# Feature 041 — World lifecycle admin (create worlds live) — Plan

**Spec:** ./spec.md · **Program design:** [ADR 0034](../../../docs/architecture/0034-multi-world-and-administration.md)

## Approach

Turn 040's startup-only registry into a **live** one and wire a create-world admin action through it. The
spawn logic moves from `main.rs`'s inline loop into a `WorldRegistry::start_world(id)` used by **both**
startup (every world) and the create handler (the new world) — one path. No domain change (P3); gameplay
unchanged.

## Layers

- **Infra (`world.rs`).** `create_world(pool, config, offsets) -> World` (always inserts a fresh row, the
  non-idempotent sibling of `ensure_world`); `world_by_id(pool, id)` (the registry loads the new world).
  Re-export `PgPool` from the infra crate so the web registry can name the pool without a `sqlx` dep.
- **Ports.** `AdminRepository::create_world(speed, radius, artifact_offset, wonder_offset) -> WorldId` +
  `list_worlds() -> Vec<AdminWorld>`; new `AdminWorld` view.
- **Use-cases (`application::admin`).** `create_world(accounts, admin, actor, speed, radius)` — admin-gated,
  validates via `GameSpeed::new` + `MAX_WORLD_RADIUS`, returns the new `WorldId`. `list_worlds`.
- **Web registry (`registry.rs`).** `WorldRegistry` holds the pool + the shared scheduler-rule `Arc`s +
  the shutdown receiver + a `Mutex<HashMap<WorldId, JoinHandle>>`. `start_world(id)` builds the world's
  scoped `WorldMap`/repo/event-store and spawns its `Scheduler` (idempotent); `join_all()` awaits on
  shutdown. `main.rs` builds it once and `start_world`s every world; `AppState` holds `Arc<WorldRegistry>`.
- **Web handler + UI.** `POST /admin/world` → `create_world` use-case → `registry.start_world(new_id)` →
  flash. `/admin` lists all worlds (`list_worlds`) + a create form.

## Why this shape

- **One spawn path.** The registry replaces 040's inline `main.rs` loop, so startup and live-create share
  exactly the same construction — no drift.
- **`main.rs` is untested**, so the live path is validated by an integration test (the test harness builds a
  real `WorldRegistry`, so `POST /admin/world` actually spawns a scheduler in-process) **and** a boot smoke
  test (admin creates a world → "registry started scheduler for world" with no restart; DB grows by one).

## Out of scope

- Archive/freeze/stop a world (a sibling lifecycle action); players joining a created world (042).
