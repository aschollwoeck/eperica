# Feature 006 — World map generation — Technical Plan

**Status:** Verified
**Spec:** ./spec.md

The first M3 slice. No new external dependencies. The terrain is a **pure domain function** of the
world seed; the only persistence change is a `seed` column on `worlds`.

## Constitution check

- **P1 (lazy/compute-on-read):** terrain is never stored or ticked — `tile(seed, x, y)` is computed
  on demand. Villages (mutable state) stay in their table; future oasis occupation layers on top.
- **P2 (reproducible):** the map is fully determined by the persisted seed + balance data; no
  hidden state. A village's fields are persisted at founding and never re-derived.
- **P3 (pure domain):** tile generation, field distributions, oasis bonuses, and toroidal distance
  are pure functions over an injected `MapRules`; no I/O in the domain.
- **P4 (server authority):** placement and all tile data are server-computed; the client supplies
  no coordinate or distribution.
- **P6 (seeded determinism):** the spec's headline. Selection is an **integer hash** of
  `(seed, x, y)` bucketed by permille weights — no float RNG — so the map is bit-reproducible.
- **P11 (performance):** `tile_at` is a handful of integer ops; the map view computes ≤ a few
  hundred cells and overlays villages from one indexed range query.

## Domain (`domain`, pure)

- `world.rs`: `toroidal_distance(a: Coordinate, b: Coordinate, radius: u32) -> f64` — per axis,
  `d = min(|Δ|, W − |Δ|)` with width `W = 2·radius + 1`; result `√(dx² + dy²)`. Plus
  `Coordinate::wrapped(radius)` → the canonical in-bounds coordinate (rem-euclid on `W`), for the
  viewport's seamless edges. Unit tests: symmetry, zero-iff-equal, `(0,0)→(3,4)=5`, a wrap case.
- New `map.rs`:
  - `FieldDistribution { wood: u8, clay: u8, iron: u8, crop: u8 }` with `sum()` and a checked
    constructor (must equal `RESOURCE_FIELD_COUNT` = 18); `fields()` → the 18 `ResourceField`s at
    level 0 in a deterministic order.
  - `OasisBonus { wood: u8, clay: u8, iron: u8, crop: u8 }` — percent bonuses (most zero).
  - `TileKind { Valley(FieldDistribution), Oasis(OasisBonus), Natar }`.
  - `MapRules { oasis_permille: u32, natar_permille: u32, distributions: Vec<Weighted<FieldDistribution>>,
    oasis_bonuses: Vec<Weighted<OasisBonus>> }`, validated on construction (every distribution sums
    to 18; non-empty tables; densities < 1000) → `DomainError::InvalidMapRules`.
  - `WorldMap { seed: u64, radius: u32, rules: MapRules }` with `tile_at(coord) -> Option<TileKind>`
    (None out of bounds), `is_valley(coord) -> bool`, and `distance(a, b) -> f64`.
  - Private `mix(seed, x, y) -> u64` (splitmix64-style integer hash) and `bucket(hash, total) ->`
    weighted pick. Tile kind: oasis if `h % 1000 < oasis_permille`, else Natar if in the next
    band, else valley; a second independent draw from `mix` picks the distribution/bonus by weight.
  - Unit tests: determinism (same/different seed), distributions sum to 18 and match the table,
    rough density of oases/Natar over a sample, fail-fast on a bad table.

## Persistence (`infrastructure` + migration `0009_world_seed.sql`)

```
ALTER TABLE worlds ADD COLUMN seed bigint;
UPDATE worlds SET seed = hashtextextended(id::text, 0) WHERE seed IS NULL;  -- deterministic per world (AC6)
ALTER TABLE worlds ALTER COLUMN seed SET NOT NULL;
```

- `world.rs` (infra): `ensure_world` generates a seed for a **new** world **deterministically from
  its id** (`hashtextextended(id::text, 0)` — the same rule as the 0009 backfill, so no RNG
  dependency and a distinct seed per world; the id is itself random) and returns the world's
  `{ id, seed }`. Existing worlds return their stored seed. A migration-boundary test asserts the
  stored seed equals the backfill formula and that a pre-existing village is unmoved (a genuine
  NULL-seed row can't be reproduced post-`SET NOT NULL`). An explicit operator-chosen seed override
  is a later config option.
- Balance loader: embed `specs/balance/map.toml`, parse → `MapRules` (fail fast, AC2).
- `PgAccountRepository` gains a `WorldMap` (built from seed + radius + `MapRules`):
  - **Placement** (`create_account`): iterate `coordinates_within(radius)`, skip coordinates where
    `!map.is_valley(coord)`, and found on the first **free valley**; build the village's fields from
    `map.tile_at(coord)`'s distribution instead of the template's fields (the template still
    supplies the buildings + level-0 fields). `Village::found` gains a `FieldDistribution` parameter.
  - New port method `villages_in_area(world, x0..=x1, y0..=y1) -> Vec<VillageMarker { coord,
    owner_name }>` for the map view (range scan on the `(world_id, x, y)` unique index, joined to
    users for the name).

## Application (`application`)

- A small read use-case `map_viewport(map, center, half, markers) -> Viewport` that produces the
  grid of `(coord, TileKind, Option<marker>)` cells, wrapping each coordinate into bounds — pure
  over the `WorldMap` + the markers the caller fetched. Keeps the web handler thin and the assembly
  unit-testable. (`AccountRepository` gains `villages_in_area`; the handler fetches markers and
  calls the use-case.)

## Interface (`web`)

- **`GET /map?x=&y=`** — the viewport (default center = the player's village). A `MAP_HALF`-cell
  radius grid (9×9). Each cell: tile glyph + label (`4·4·4·6` valley, `+25% wood` oasis, `Natar`)
  and a village marker (`★ <owner>`, highlighting the viewer's own). **Recenter** via N/S/E/W links
  (shift by the viewport span) and a jump-to-coordinate form. A **Map** link from `/village`.
- New `.map-grid` component added to [ui-style-guide.md](../../ui-style-guide.md) **first**, then
  the CSS. Auth via `AuthUser` (Visitor → login). Everything read-only.

## Balance data (`specs/balance/map.toml`)

`oasis_permille`, `natar_permille`; `[[distributions]]` (wood/clay/iron/crop/weight, each summing to
18; `4-4-4-6` dominant); `[[oasis_bonuses]]` (percent bonuses + weight, faithful Travian variants:
+25% single, +50% single, +25%/+25% double).

## Test strategy

| AC | Test |
|----|------|
| AC1 | domain: same `(seed,x,y)` → same tile; a different seed changes a sample of tiles; out-of-bounds → None. |
| AC2 | domain + loader: every valley distribution sums to 18 and is from the table; bad table fails fast. |
| AC3 | domain: over a sampled region, oasis/Natar fractions track the configured permille (within tolerance); each oasis bonus is from the table. |
| AC4 | domain: symmetry, zero-iff-equal, `(0,0)→(3,4)=5`, a near-edge wrap is shorter than the direct path. |
| AC5 | infra (DB): a founded village sits on a valley (`map.is_valley` true) with fields matching the tile's distribution; oasis/Natar coordinates are skipped. |
| AC6 | infra (DB): migration backfills the seed non-null; a pre-existing village keeps its coordinate and fields. |
| AC7 | web integration: `/map` renders a grid centered on the player's village, shows the village marker with the owner, an oasis/valley label, and recenters via `?x=&y=`; a visitor is redirected to login. |
| AC8 | infra: the seed round-trips through `worlds`; `ensure_world` returns it; balance densities/tables drive generation. |

## Notes

- The hash mixes the **unsigned** reinterpretations of `x`/`y` so negative coordinates are handled
  uniformly; `seed` is stored as `i64` and reinterpreted as `u64` for mixing.
- `Village::found` changes signature (adds the distribution); 001–005 call sites and tests update.
  `starting-village.toml`'s field section is no longer the source of village fields — its counts
  move into `map.toml` as the dominant `4-4-4-6` distribution; the template keeps buildings.
- The viewport wraps coordinates for display, so a center near an edge shows the far edge's tiles —
  exercising the same wrap the distance function uses.
