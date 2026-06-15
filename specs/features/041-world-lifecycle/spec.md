# Feature 041 — World lifecycle admin (create worlds live)

**Status:** Draft
**Depends on:** 036 (admin console + `require_admin`), 040 (world registry runtime)
**Roadmap:** M9 multi-world & administration, slice 6 — see [ADR 0034](../../../docs/architecture/0034-multi-world-and-administration.md).
**Program note:** Turns the registry on for **operators**: an admin creates a new world from the `/admin`
web page and it **starts running immediately — no process restart, other worlds undisturbed** (the ADR's
"hot" requirement). Built on 040's per-world runtime.

## Problem

040 spawns a scheduler per world **at startup**. To run a fresh round an operator had to insert a `worlds`
row by hand and restart the process. Creation must be a first-class, server-authoritative admin action
(P4) that **registers the new world's runtime + scheduler live**.

## Goal

- **AC1 — Create a world (admin).** From `/admin`, an admin submits a new world's **speed** and **map
  radius** (with default end-game release offsets); the server validates them (`speed` finite > 0;
  `radius` in a sane range) and inserts a `worlds` row with a deterministic per-world seed. Gated on the
  real admin (`RealUser` + `require_admin`); a non-admin gets 403.
- **AC2 — Starts live.** Immediately after creation the new world's runtime (map/repo/event-store) +
  **scheduler** are registered and running — **no restart** — and the other worlds' schedulers are
  untouched. Idempotent: starting an already-running world is a no-op.
- **AC3 — Worlds list.** The `/admin` page lists every world (id, speed, radius, created, win state) — so
  the operator can see the worlds the registry is running.
- **AC4 — Behaviour preserved.** The home world + single-world gameplay are unchanged; the pure `domain`
  crate is untouched (P3).

## Design

- **`WorldRegistry`** (web) — the live runtime registry. Holds the pool, the shared scheduler rule `Arc`s,
  the shutdown receiver, and a tracked set of running worlds (`Mutex<HashSet<WorldId>>`). `start_world(id)`
  loads the `World`, builds its per-world `WorldMap`/`PgAccountRepository`/`PgEventStore` (038/039), spawns
  its `Scheduler`, and records it (idempotent). **`main.rs` builds the registry once and calls
  `start_world` for every world at startup** (replacing 040's inline loop — one spawn path). `AppState`
  holds `Arc<WorldRegistry>`.
- **Create-world use-case** (`application::admin::create_world`) — admin-gated; validates via
  `GameSpeed::new` + a radius bound; inserts the row through a new `AdminRepository::create_world(speed,
  radius, artifact_offset, wonder_offset) -> WorldId` (infra: a non-idempotent `world::create_world`, the
  sibling of `ensure_world`). Returns the new `WorldId`.
- **Web** — `POST /admin/world` (gated): `create_world(…)` → `registry.start_world(new_id)` → flash +
  redirect. `/admin` gains a worlds table (`AdminRepository::list_worlds`) + the create form.
- **No domain change (P3).** Runtime composition + an admin write/read; gameplay rules untouched.

## Out of scope (follow-up)

- **Archive / freeze / stop** a world (stop its scheduler + freeze via the 021 guard) — a sibling lifecycle
  action; this slice delivers **create + live start**.
- Players joining/selecting the new world (042) — a created world has no players until the player UX ships.
