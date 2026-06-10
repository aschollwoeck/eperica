# Feature 002 — Resource production — Technical Plan

**Status:** Reviewed
**Spec:** ./spec.md

Builds on slice 001's stack and layering (Rust workspace; pure `domain`; `application` ports;
`infrastructure` SQLx adapters; `web` Axum). No new external dependencies expected.

## Constitution check

- **P1 (lazy/event-driven):** production is **never** ticked. `compute_economy` derives current
  amounts from stored state + elapsed time on read. The only writes happen on mutating commands
  (none yet in 002 beyond creating the starting resources).
- **P2 (reproducible):** current values are a pure function of persisted `(amount, updated_at)` + rates
  + the clock; identical before/after restart (AC5).
- **P3 (pure domain):** all economy math (rates from field levels, capacity clamp, population/upkeep,
  net crop, settle) lives in `domain`; balance numbers are injected as data, not hardcoded.
- **P7 (configurable speed):** production rate = base field production × world speed.
- **P11 (performance):** reading the economy is O(fields+buildings) arithmetic + at most one extra
  indexed query for stored amounts; well within the read-path budget. `timestamptz` µs precision.

## Domain model (`domain`, pure)

New `economy` module (**integer units** — `i64` throughout):
- `ResourceAmounts { wood, clay, iron, crop: i64 }`.
- `ProductionRates { wood, clay, iron, crop_net: i64 }` (per hour; `crop_net` already minus upkeep,
  may be negative).
- `Capacities { warehouse, granary: i64 }`.
- `EconomyRules` — the injected balance: `field_production[ResourceKind][level]`,
  `field_population[level]`, `building_population[BuildingKind][level]`, `base_capacity`,
  `starting_amounts`. (Pure data struct; no I/O.)
- Functions:
  - `production_rates(fields, buildings, rules, speed) -> ProductionRates` — sum per-resource field
    production × speed; `crop_net = cropProduction − population`, where population sums field +
    building population from the tables.
  - `capacities(buildings, rules) -> Capacities` — base capacity now (Warehouse/Granary add to it in
    003).
  - `accrue(stored, rate_per_hour, elapsed_secs, capacity) -> i64` =
    `(stored + rate·elapsed_secs/3600).clamp(0, capacity)` — integer arithmetic (rate is `base × speed`
    rounded to `i64`; integer division drops the sub-unit remainder, recomputed from the original
    `(stored, updated_at)` each read so reads lose nothing; a future settle on spend accepts the
    sub-unit drop, Travian-style).
  - `compute_economy(stored: ResourceAmounts, updated_at, fields, buildings, rules, speed, now)
    -> (current: ResourceAmounts, rates, capacities)` — the read-path entry point.
- Unit tests assert AC1 (accrual formula), AC2 (speed scaling), AC3 (clamp/overflow), AC4 (net crop
  incl. negative), AC5 (idempotent/reproducible), AC6 (starting village positive).

## Persistence (`infrastructure` + migration)

- **Migration `0002_village_resources.sql`:**
  `village_resources (village_id uuid PK REFERENCES villages(id) ON DELETE CASCADE,
   wood bigint, clay bigint, iron bigint, crop bigint, updated_at timestamptz NOT NULL)`.
- **Slice-001 `create_account` extended:** within the same transaction, insert a `village_resources`
  row with the balance **starting amounts** and `updated_at = now()`.
- **New read:** `stored_resources(village_id) -> Option<(ResourceAmounts, updated_at)>`.

## Application / services

- Extend the repository port (or a small `EconomyRepository`) with `stored_resources`.
- Use-case `load_economy(repo, rules, speed, now, owner) -> Option<VillageEconomy>`: fetch the owner's
  village (fields/buildings, from 001) + stored resources, call domain `compute_economy`, return a
  `VillageEconomy { amounts, rates, capacities, coordinate }` view.
- `EconomyRules` + `GameSpeed` come from config/balance, threaded through `AppState`.

## Interface (`web`)

- The **`/village`** handler now builds a `VillageEconomy` and renders it.
- `village.html`: per resource show **current amount** (floored), **/h** (net for crop), and
  **capacity** (e.g. `Wood 312 / 800  (+30/h)`), using the resource color tokens + tabular figures.
  Crop's `/h` gets the `--c-warning`/`danger` treatment when `≤ 0`. Conforms to ui-style-guide.
- No new routes. Server stays authoritative (P4): amounts are computed server-side; the client may
  later extrapolate, but that's not required in 002.

## Balance data (`specs/balance/`)

- `economy.toml`: field production per level (per resource), field/building population per level, base
  warehouse/granary capacity, and starting amounts. Loaded by infra into `EconomyRules`
  (serde DTO → domain), like the starting-village loader.

## Test strategy

| AC | Test |
|----|------|
| AC1 | Domain unit: `accrue`/`compute_economy` exact accrual over a fixed elapsed. |
| AC2 | Domain unit: rates scale linearly with `GameSpeed`. |
| AC3 | Domain unit: amount clamps at capacity; excess discarded. |
| AC4 | Domain unit: `crop_net = production − population`, incl. a negative case. |
| AC5 | Domain unit: two reads at the same `now` match; recompute from same inputs is identical. |
| AC6 | Domain unit + infra: starting village has positive wood/clay/iron and positive net crop. |
| AC7 | Web integration: `GET /village` shows amount/cap/`/h` for each resource (extends 001 tests). |

## Notes / follow-ups

- Field upgrades & Warehouse/Granary (capacity growth) and resource **spending** arrive in 003; the
  `accrue`/settle functions are written so spending just settles-then-debits.
