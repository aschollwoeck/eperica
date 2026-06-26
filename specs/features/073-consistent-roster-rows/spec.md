# Feature 073 — consistent roster rows across research / training / forging

## Why

The three in-village unit rosters — **Academy** (research), **Barracks/Stable/Workshop** (training), and the
**Smithy** (forging) — grew independently (066/067) and lay their information out differently. Most visibly the
**resource cost** sits in a different column on each page: in the action area on the Academy, under the unit's
name on the training pages, and in a middle column on the Smithy. This slice gives all three **one row shape**
so the same information is always in the same place.

Presentation only — **no domain/sim change** (P3), no handler change (the data is unchanged); a CSS + template
refactor of the shared `.unit` roster row.

## Acceptance criteria

- **AC1 — One row shape.** Every roster row (research / training / forging) uses the same four-column grid:
  **portrait · identity+stats · price · action**, with consistent alignment.
- **AC2 — Cost in one place.** The resource cost (and the time) always render in the **price** column, in the
  same position on all three pages — never under the name or in the action area.
- **AC3 — Page-specific action preserved.** The action column keeps each page's control unchanged: Academy →
  Researched badge / Research button / gate; Training → count + Max + Train (+ live batch-total) / gate; Smithy
  → Forge button / "at the anvil" + countdown / gate. The Smithy's forge level + pips + effect stay (in the
  identity column).
- **AC4 — Behaviour preserved.** All forms/POSTs, the train-max + batch-total JS, the forge countdown, the
  gates, the ember "ready"/"forging" highlights, and the mobile reflow keep working — a re-layout, not a
  rule change.

## Roles (see specs/roles.md)

- **Player** — sees the tidier, consistent layout. No authority change.

## Constitution

- **P3** — pure domain untouched; CSS + three templates only. **P4** — gating unchanged (display only).
  **P11** — no new data.

## Out of scope

- Adding combat stats to the Smithy row (SmithyRow carries forge level/effect, not att/def — its identity
  column shows the forge-relevant info instead).
