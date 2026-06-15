# Feature 031 — Informative actions (show the effect before you commit)

**Status:** Verified
**Depends on:** 002/003 (economy + construction), 004/005 (units, research, training), 007/008 (movement, trade), 009 (combat/Smithy scaling)
**Roadmap:** app-layer UX — the first **UX information** pass: surface what an action *does*, not just what it costs. Visual theming/imagery is a later pass; this gets the data right first.

## Goal

Across the action pages, show the **effect/outcome** of an action alongside its cost, so a player can decide
without doing the arithmetic in their head. No new game rules — this only **reads + presents** existing
domain computations (server-rendered, or client-side math that exactly mirrors a domain formula).

## Acceptance criteria

- **AC1 — Build / upgrade effect (village).** Each building/field row shows what the **next level** grants:
  resource fields show production current → next (speed-scaled) + population; Warehouse/Granary show storage
  capacity; Outpost shows oasis slots; all show the population change. Blank at max level.

- **AC2 — Academy & Smithy.** The Academy shows each researchable unit's full stats (attack / def-inf /
  def-cav / speed / carry / upkeep) — *already present*. The Smithy shows the **stat gain** an upgrade
  grants (attack & defence at the next level), via the Smithy combat-strength scaling.

- **AC3 — Training (Barracks / Stable / Workshop).** Per-unit stats + per-unit cost/time/upkeep are shown
  (already), plus a **live batch total** (cost · time · crop-upkeep) that updates as the count changes.

- **AC4 — Rally Point.** A **live army preview** as troops are selected: combined attack / defence, total
  carry capacity, the army's slowest speed, and — when a target tile is set — the one-way travel time. The
  client-side distance/time **exactly mirrors** the domain (toroidal distance × world-speed-scaled
  unit speed).

- **AC5 — Marketplace.** A **live shipment preview**: merchants needed for the entered amounts (per-merchant
  capacity shown), a "only N free" hint when it exceeds availability, and the round-trip travel time —
  again mirroring the domain's distance × merchant speed.

- **AC6 — No functional/regression change (P3/P4).** No game rule changes; all effects are derived from
  existing pure domain rules. New domain code is **read-only accessors** only (`EconomyRules` per-level
  getters). Server-authoritative validation on the POST is unchanged.

- **AC7 — Reproducibility (P2/P7).** Displayed effects are deterministic from the balance rules + world
  speed; nothing is hardcoded.

## Notes / design

- New pure `EconomyRules` accessors expose per-level values for display (`field_production_per_hour`,
  `field_population`, `building_population_at`, `warehouse_capacity`, `granary_capacity`).
- The Smithy stat gain uses `CombatRules::smithy_factor` (added `combat_rules` to the web `AppState`).
- The Rally/Marketplace travel previews are **client-side** for responsiveness, but the JS replicates the
  exact domain formulas — toroidal axis width `2·radius+1`, Euclidean axis-gap distance, and
  `round(distance / (speed × world_multiplier) × 3600)` — fed by origin/radius/speed passed from the
  handler. **If a movement formula changes in the domain, update these previews to match.**

## Out of scope

- Visual styling / theming / imagery (a later UX pass).
- Effects for buildings whose rules live outside `EconomyRules` (Wall defence %, Main-Building build
  speed-up, Marketplace merchant count, Town Hall culture, Residence/Palace slots) — a possible follow-up;
  this slice covers the economically central ones (production, storage, population, oasis slots) + the
  combat/training/movement/trade previews.
