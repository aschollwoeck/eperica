# Feature 113 — level-by-level demolition (faithful Travian)

## Why
110 cleared a building in one order. Travian (T4) demolishes **one level at a time** via the Main Building.
This makes demolition level-by-level: each order tears down the building's current **top level**.

## Acceptance criteria
- **AC1** — Ordering a demolish enqueues one order to `level − 1` (not straight to 0), taking **that level's
  build time** (Main-Building-scaled). When it completes the building is one level lower; demolishing the
  **last** level (target 0) frees the slot (the 110 delete path). To fully clear a tall building the player
  demolishes repeatedly (one level per action; each occupies the build lane like a build). Demolition stays
  free (no resource cost), Main-Building-gated, and never applies to the Main Building / Palace (112).
- **AC2** — The Demolish action reads as a per-level action (e.g. "Demolish to level N−1", or "free the slot"
  at level 1).

## Out of scope
- Auto-continue (demolishing straight to 0 without re-clicking) — would need demolish-intent tracking; the
  per-level action matches Travian T4.
