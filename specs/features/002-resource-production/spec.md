# Feature 002 — Resource production

**Status:** Reviewed
**Depends on:** 001 (foundation, village with resource fields)
**Roadmap:** M1 · slice 002 · GDD §2, §3.1

## Goal

A village's four resources **accrue over real time** from its resource fields, **computed on read**
(never by ticking), bounded by storage capacity, with **crop reduced by upkeep**. This slice makes the
constitution's lazy time model (P1) real and reproducible (P2): at any instant a resource value is a
pure function of stored state + elapsed time + the rules.

## Concepts

- A village holds, per resource, a **stored amount**, a **last-updated timestamp**, and an implied
  **production rate** derived from its field levels × world speed.
- **Compute-on-read:** `current = min(capacity, stored + ratePerHour × hoursElapsed)`. Reading also
  "settles" the stored amount + timestamp so the value is stable and reproducible.
- **Crop is special:** net crop rate = crop-field production − **upkeep** (population from the
  village's fields and buildings). Net crop may be negative.

All numeric values (per-level production, base storage capacity, per-level population/upkeep, starting
amounts) are **balance data** in `specs/balance/`, not hardcoded in logic.

## User stories

- As a **player**, I want my resources to grow while I'm away, so that logging in rewards my economy.
- As a **player**, I want to see each resource's current amount, production rate, and capacity, so I
  can plan what to build and when storage will overflow.

## Acceptance criteria

> "Compute on read" = the value is derived from stored state + elapsed time, with no background job
> mutating it. Times are server-authoritative UTC (P4/P11).

- **AC1 — Lazy accrual.** Given a village resource with stored amount `A`, rate `R`/h, and last-update
  `T0`, when read at `T1` (with `A + R·(T1−T0)/3600 ≤ capacity`), then the value equals
  `A + R·(T1−T0)/3600`. No scheduler/tick mutates it between reads.

- **AC2 — Production from fields × speed (P7).** Each resource's rate is the sum of its fields'
  per-level production (balance data) × the world speed. Doubling speed doubles the rate.

- **AC3 — Capacity cap & overflow.** A resource never exceeds its **capacity**; production beyond
  capacity is discarded (lost), not stored. (Capacity for wood/clay/iron is the warehouse cap; crop
  the granary cap; with no Warehouse/Granary built yet, a **base capacity** from balance applies.)

- **AC4 — Crop upkeep / net production.** The crop rate is `cropFieldProduction − upkeep`, where
  upkeep is the village population (from field + building levels, balance data). The reported crop
  production reflects this net value and may be negative; when negative, stored crop decreases over
  time (never below 0 in this slice — troop starvation arrives with troops). **Both production and
  upkeep scale with world speed**, so net crop scales linearly with speed (P7).

- **AC5 — Settle on read is idempotent & reproducible (P2).** Reading twice in immediate succession
  yields the same value (within rounding); the computed value depends only on persisted state +
  elapsed time, so it is identical before and after an app restart.

- **AC6 — Starting village economy.** A freshly founded village (slice 001 layout: fields at level 0,
  Main Building + Rally Point) has the defined **starting amounts**, positive wood/clay/iron
  production, and **positive net crop** at base balance.

- **AC7 — Village view shows the economy.** The village page shows, per resource, the **current
  amount**, **/hour production** (net for crop), and **capacity**. Numbers use tabular figures
  (ui-style-guide). Crop production is visually flagged when ≤ 0 (warning).

## Roles & permissions

Per [roles.md](../../roles.md).

| Role | Permitted | Denied (server-enforced) |
|------|-----------|--------------------------|
| **Visitor** | N/A (considered) — cannot reach a village. | View any village's resources (redirected to login). |
| **Player** | View **their own** village's resources (AC7); accrual applies to their villages. | View/affect another player's resources. |
| **Moderator** | N/A (considered). | — |
| **Administrator** | World speed (set in 001) scales production (AC2). | — (superset). |
| **System** | *(none)* — there is **no** background job for production; accrual is compute-on-read (P1). | — |

## Out of scope

- **Upgrading fields / construction** (raising production) → slice 003.
- **Warehouse / Granary buildings** (raising capacity) → slice 003; this slice uses base capacity.
- **Spending** resources (build/train costs) → 003+.
- **Troop crop upkeep & starvation** → arrives with troops (M2).
- **Resource boost buildings** (Sawmill, etc.) and **oasis bonuses** → later slices.
- **Marketplace / trading / transfer** → slice 008.

## Decisions

- **Crop upkeep = population-based** (each field/building level adds population; 1 crop/pop/h), with
  population values as balance data. This is the faithful mechanic.
- **Compute transiently on reads; settle (persist) only on mutating commands** (spending arrives in
  003). Plain reads never write — fewer writes, same correctness (P2).
- **Starting amounts & base capacity** are faithful-ish defaults stored as balance data (modest
  starting amounts; a small base warehouse/granary capacity until those buildings exist in 003).
