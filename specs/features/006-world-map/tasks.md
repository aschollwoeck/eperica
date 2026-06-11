# Feature 006 — World map generation — Tasks

**Plan:** ./plan.md · **Spec:** ./spec.md

Ordered for dependency and testability (pure domain first).

## Domain (pure, test-first)

- [x] **T1 — Toroidal distance.** `toroidal_distance(a, b, radius)` + `Coordinate::wrapped(radius)`
  in `world.rs`. Unit tests: symmetry, zero-iff-equal, `(0,0)→(3,4)=5`, near-edge wrap shorter than
  direct (**AC4**).
- [x] **T2 — Map domain.** `map.rs`: `FieldDistribution` (checked, sum 18), `OasisBonus`,
  `TileKind`, `MapRules` (validated, fail-fast), `WorldMap` (`tile_at` / `is_valley` / `distance`),
  integer hash + weighted bucket. Unit tests: determinism, distributions sum to 18, density of
  oases/Natar over a sample, fail-fast on a bad table (**AC1**, **AC2**, **AC3**).

## Balance + persistence

- [ ] **T3 — Map balance data + loader.** `specs/balance/map.toml` (densities, distribution +
  oasis-bonus tables); `balance.rs` loader → `MapRules`, fail-fast. Move the `4-4-4-6` default into
  the table. Tests (**AC2**).
- [ ] **T4 — Seed migration + ensure_world.** `0009_world_seed.sql` (add `seed`, backfill
  deterministic per-world, set NOT NULL); `ensure_world` generates a seed for new worlds and
  returns `{ id, seed }`. DB tests: backfill non-null + a pre-existing village unchanged (**AC6**).
- [ ] **T5 — Valley placement + markers.** `Village::found` takes a `FieldDistribution`;
  `PgAccountRepository` holds a `WorldMap`, places on the first free **valley**, and builds fields
  from the tile; new `villages_in_area` port + query. DB tests: founded village on a valley with
  matching fields; non-valley tiles skipped (**AC5**).

## Application

- [ ] **T6 — Viewport use-case.** `map_viewport(map, center, half, markers) -> Viewport` (wrapping
  cells, overlaying markers). Fake-based tests: grid extent, wrap at edges, marker overlay (**AC7**
  assembly side).

## Web

- [ ] **T7 — Map view.** `.map-grid` added to the style guide first; `GET /map?x=&y=` viewport
  centered on the player's village, tile labels + village markers, recenter links/form; **Map**
  link from `/village`. Integration tests: grid renders, marker + owner, oasis/valley labels,
  recenter, visitor → login (**AC7**).

## Documentation & acceptance

- [ ] **T8 — Technical docs.** rustdoc; `docs/architecture/0008-world-map.md` (generate-on-read
  terrain, toroidal distance); `CLAUDE.md` active slice.
- [ ] **T9 — End-user docs.** `docs/manual/` map guide (reading the map, valleys/oases); link from
  index.
- [ ] **T10 — Review & accept.** Full gates + P11; `eperica-reviewer` on the slice diff; fix until
  **APPROVE**; PR.

## Done when

Per the [Definition of Done](../../implementation-workflow.md#definition-of-done-checklist--applies-to-every-slice):
**AC1–AC8** pass with tests (incl. the seed migration-boundary guard), all gates green, both docs
written, reviewer **APPROVE**, PR merged, `spec.md`/`plan.md` **Verified**, roadmap updated.
