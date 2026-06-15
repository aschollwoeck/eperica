# Informative actions — show the effect, not just the cost

**Status:** Current
**Date:** 2026-06-15 · **Slice:** 031

## Context
The UI was functionally complete but told players only what an action *cost*, not what it *did* — e.g. a
field upgrade showed its price but not the new production rate. This is the first **UX information** pass
(visual theming comes later): surface the outcome of each action so players can decide informed.

## Design
- **Read-only, no rule changes (P3/P4).** Every effect is derived from existing pure domain rules; the only
  new domain code is **read accessors**. Server-side validation on the POST is untouched.
- **Build/upgrade (village).** New `EconomyRules` accessors — `field_production_per_hour` (speed-scaled,
  matching the displayed rates), `field_population`, `building_population_at`, `warehouse_capacity`,
  `granary_capacity` — let the village handler render each row's next-level effect (production, storage,
  oasis slots, population). Blank at max.
- **Smithy.** The combat-strength scaling (`CombatRules::smithy_factor`) gives the attack/defence at the
  next level; `combat_rules` was added to the web `AppState` for this (combat itself still runs only in the
  scheduler).
- **Training / Rally / Marketplace previews.** These depend on the player's live input (quantities, target
  tile), so they're computed **client-side** for responsiveness — but the JS **replicates the exact domain
  formulas**, fed by data the handler emits:
  - training batch totals: per-unit cost/time/upkeep × count;
  - rally army preview: Σ attack/defence/carry, min speed, and travel time;
  - market shipment: `ceil(total / merchant_capacity)` merchants + round-trip time.
  The movement math mirrors the domain precisely: toroidal axis width `2·radius+1`, Euclidean axis-gap
  distance, and `round(distance / (speed × world_multiplier) × 3600)`.

## Reuse / decisions
- **Domain accessors over recomputation** — the web layer never re-implements production/population; it
  reads the same tables `production_rates`/`population` use.
- **Client-side previews are a deliberate duplication** of the movement formula for instant feedback. The
  risk (divergence if the domain formula changes) is documented in the slice spec and called out in the
  templates; the authoritative travel time is still computed server-side on send.

## Consequences
- Players see production/storage/population deltas on upgrades, unit stat gains on Smithy upgrades, and live
  cost/time/power/ETA previews on training, rally, and trade — before committing.
- **Follow-up (delivered in slice 032):** the remaining building effects — Wall defence %, Cranny hidden
  resources, Marketplace merchants, Town Hall culture/h, Residence/Palace expansion slots, Main-Building
  build-speed, and Barracks/Stable/Workshop training-speed (a new pure `CombatRules::wall_bonus`; the rest
  reuse existing accessors) — plus resource bars (fill + time-to-full / -empty). The visual/theming pass
  remains.
