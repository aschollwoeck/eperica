# Feature 006 — World map generation

**Status:** Reviewed
**Depends on:** 001 (world, coordinates, village founding, the `coordinates_within` placement order)
**Roadmap:** M3 · slice 006 · GDD §7 — the map, seeded reproducibility (P6).

## Goal

Give the world a **map**: every tile has a **deterministic, seeded** kind — a **valley** (with a
fixed field distribution), an **oasis** (with a production bonus), or a reserved **Natar** tile.
Add the **toroidal distance** function the movement engine (007) will ride on, place new villages
on **valleys** with the field layout their tile dictates, and let a player **view the map** around
their village. The map is a pure function of the world's seed (P6) — reproducible and auditable.

## Concepts

- **Tile:** one map coordinate's terrain. **Valleys** are occupiable and carry a **field
  distribution** (how many of the 18 fields are wood/clay/iron/crop — GDD §3.1). **Oases** grant a
  production **bonus** and are not directly occupiable here (occupation is slice 012). **Natar**
  tiles are reserved for the end-game (§11); they exist in the model but have no behavior yet.
- **Seed:** a persisted per-world integer. The whole terrain is `tile(seed, x, y)` — a pure
  function (P6); the map is never stored tile-by-tile, it is **computed on read** (P1 spirit).
  Mutable state (villages now; oasis occupation later) is stored separately and layered on top.
- **Toroidal distance:** the map **wraps** at its edges (GDD §7.2), so distance between two tiles
  is the shortest **wrapped Euclidean** distance on the radius-R torus.
- **Field distribution at founding:** a new village's 18 fields are built from its valley tile's
  distribution. Once founded, the fields are the village's own stored state.

## User stories

- As a **player**, I want my new village to sit on a real valley with a sensible field layout, so
  that the map feels like a place.
- As a **player**, I want to see the map around my village — valleys, oases, my neighbours — so
  that I can get my bearings and plan.

## Acceptance criteria

> The map is server-authoritative (P4) and seeded (P6): tile kinds, distributions, bonuses, and
> placement are computed server-side from the world seed and balance data; the client only reads.

- **AC1 — Seeded, deterministic terrain.** Given a world seed `S` and radius `R`, every in-bounds
  coordinate has a deterministic tile kind. The **same** `(S, x, y)` always yields the **same**
  tile; a **different** seed yields a materially different map. Out-of-bounds coordinates have no
  tile.

- **AC2 — Valley field distributions.** Every valley carries a field distribution of exactly **18
  fields** whose four counts sum to 18, drawn by seeded weight from the balance distribution table;
  the balanced `4·4·4·6` dominates, with rarer croppers (e.g. `3·3·3·9`). The same tile always
  yields the same distribution. Loading **fails fast** if any balance distribution does not sum to
  18.

- **AC3 — Oases & Natar by configured density.** A balance-configured fraction of tiles are
  **oases** (each with a seeded production bonus from the balance bonus table) and a smaller
  fraction **Natar**; the remainder are valleys. The densities and bonus table are balance data.

- **AC4 — Toroidal distance.** Distance between two tiles is the shortest **wrapped Euclidean**
  distance on the radius-`R` torus: it is **symmetric**, **zero iff** the tiles are equal, equals
  the plain Euclidean distance when that is shortest (e.g. `(0,0)→(3,4) = 5`), and uses the wrap
  when the wrapped path is shorter (e.g. two tiles near opposite edges are close).

- **AC5 — Villages are placed on valleys.** A new village is founded on the **nearest free valley**
  tile in the deterministic ring order (oases and Natar are skipped), and its 18 fields are built
  from that valley's distribution. The origin region contains valleys, so the first player is
  always placeable.

- **AC6 — Existing villages and data survive (migration boundary).** Adding the seed does **not**
  move pre-006 villages or change their stored fields. The world `seed` column is backfilled
  **NOT NULL** with a deterministic per-world value for the existing world. Migration-boundary test
  required.

- **AC7 — Map view.** A logged-in player can view a **viewport** of the map centered on a
  coordinate (default: their own village), wrapping seamlessly at the edges. Each cell shows its
  tile kind (valley distribution / oasis bonus / Natar) and a **marker** for any village on it with
  the **owner's name** (coordinates and ownership are public per GDD §7.3). The player can
  **recenter**. Troop counts, resources, and defenses are **not** shown (revealed only by scouting,
  later).

- **AC8 — Seed is server config (P6/P7-style).** The seed is part of the world's persisted
  configuration (operator-set or generated at world creation); no terrain is hardcoded. Densities,
  distributions, and oasis bonuses are balance data.

## Roles & permissions

Per [roles.md](../../roles.md).

| Role | Permitted | Denied (server-enforced) |
|------|-----------|--------------------------|
| **Visitor** | N/A (considered). | View the map (redirected to login). |
| **Player** | View the map; recenter it. Placement of their starting village happens server-side at registration. | Choose their village's tile or field distribution; place onto an occupied or non-valley tile (placement is server-chosen; the client supplies no coordinate). |
| **Moderator** | N/A (considered). | — |
| **Administrator** | Sets the world's seed and radius as configuration (AC8). | — (superset). |
| **System** | *(system-initiated)* Generate the terrain deterministically from the seed (AC1); place the starting village on a valley at founding (AC5). | — |

## Out of scope

- **Troop movement & travel time** (which consume the distance function) → slice 007.
- **Oasis occupation, clearing wild animals, the Outpost** → slice 012; oases here are terrain with
  a bonus value only, not occupiable.
- **Natar behavior, the Wonder of the World, artifacts** → end-game; Natar is a tile kind only.
- **Player-founded additional villages** (settlers/Residence) → slice 013; only the starting
  village is placed here.
- **Non-wrapping (bounded) maps** → a later config option; wrap is the faithful default.
- **A full pannable/zoomable map UI, scouting-revealed details, alliance map overlays** → later
  (the Map UI matures through M3–M4).

## Decisions

- **Generate-on-read, not materialized.** Terrain is the pure function `tile(seed, x, y)`; there is
  **no tiles table**. This fits P6 (seeded), the P1 compute-on-read spirit, and avoids a
  ~160k-row materialization for a normal world; mutable state (villages; later oasis occupation) is
  stored separately and layered on top.
- **Valley distribution applies at founding only.** The tile sets a new village's 18 fields; after
  that the fields are the village's own stored state. Pre-006 villages keep their stored fields —
  no field backfill (their underlying generated terrain is simply not surfaced for occupied tiles).
- **Wrap (toroidal) is the behavior;** a bounded/non-wrapping option is deferred to config.
- **Densities and tables are balance data** (`specs/balance/map.toml`): oasis/Natar permille,
  the weighted field-distribution table, and the weighted oasis-bonus table.
- **The map view is auth-gated (Player) for this slice** even though GDD §7.3 calls the layout
  public; a truly public map can come later. Visitors are redirected to login, consistent with the
  rest of the app.
- **Seeded selection is integer-only** (a deterministic hash of `(seed, x, y)` bucketed by permille
  weights) — no floating-point RNG — so the map is exactly reproducible across platforms (P6).
