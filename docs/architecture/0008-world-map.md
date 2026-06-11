# The world map — seeded, generate-on-read terrain

**Status:** Current
**Date:** 2026-06-11 · **Slice:** 006

## Context
The world needed a map: every tile a valley (with a field layout), an oasis (with a bonus), or a
reserved Natar tile — plus the toroidal distance the movement engine (007) will ride on. The map
must be reproducible and auditable (P6) and cheap (a normal world is ~160k tiles).

## Design
- **Generate-on-read, not materialized.** The terrain is a pure function `tile(seed, x, y)`: there
  is **no tiles table**. `WorldMap::tile_at` mixes `(seed, x, y)` with SplitMix64 into a hash,
  buckets `hash % 1000` against the balance densities (oasis / Natar / valley), and a second
  independent draw picks the field distribution or oasis bonus by weight. Selection is
  **integer-only**, so the map is bit-reproducible across platforms (P6). Mutable state (villages
  now; oasis occupation later, 012) lives in its own tables and is layered on top — fitting the P1
  compute-on-read spirit and avoiding a six-figure-row materialization.
- **The seed is world config.** `worlds.seed` (migration 0009) is backfilled deterministically per
  world (`hashtextextended(id::text, 0)`) and generated the same way for new worlds — no RNG
  dependency, distinct per world. `ensure_world` returns `{ id, seed }`; the repo and the web
  `AppState` hold a `WorldMap` built from it plus the embedded `map.toml` balance.
- **Toroidal distance & wrap.** The map wraps (GDD §7.2): each axis takes the shorter of the direct
  or wrapped gap on a width-`2R+1` torus, combined as `√(dx²+dy²)`. `Coordinate::wrapped` brings any
  coordinate into bounds, used to render the viewport seamlessly across the edge.
- **Placement reads the map.** A new village is founded on the **first free valley** in the
  deterministic ring order (oases/Natar skipped); its 18 fields come from that valley's
  distribution. After founding, the fields are the village's own stored state — pre-006 villages
  keep theirs (no field backfill; the underlying terrain is simply not surfaced for occupied tiles).
- **Map view.** `map_viewport` assembles a square grid around a center (north up), wrapping at the
  edges, and overlays public markers (`villages_at` — an exact `unnest`-keyed lookup on the
  `(world_id, x, y)` index). Coordinates and ownership are public (GDD §7.3); troops/resources are
  not shown.

## Consequences
- Slice 007 (movement) gets `toroidal_distance` for travel time directly; 012 (oasis occupation)
  layers occupation state over the generate-on-read oases.
- Tuning the world is a balance-data edit (`map.toml` densities/tables); the generator validates
  shape (every distribution sums to 18), not contents.
- Because terrain is a pure function, any tile can be re-derived for audit from just the world seed.

## Links
specs/constitution.md (P1, P2, P4, P6); specs/features/006-world-map/;
specs/balance/map.toml; crates/domain/src/map.rs (WorldMap), crates/domain/src/world.rs
(toroidal_distance); crates/application/src/map.rs (viewport); crates/infrastructure/src/repo.rs
(placement, villages_at), crates/infrastructure/src/world.rs (ensure_world);
migrations/0009_world_seed.sql.
