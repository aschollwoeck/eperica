# Resource economy (lazy accrual)

**Status:** Current
**Date:** 2026-06-10 · **Slice:** 002

## Context
Resources must grow in real time while players are offline, at scale, without ticking every village
(P1) and reproducibly (P2).

## Design
- Each village stores per-resource **integer amounts** + an **`updated_at`** timestamp
  (`village_resources`). Production **rate** is derived from field levels × world speed (P7);
  `crop_net = cropProduction − population` (population from field/building levels). Both production and
  upkeep scale with world speed, so net crop scales linearly with speed (P7).
- **Compute on read:** `current = (stored + rate·elapsed/3600).clamp(0, capacity)` — pure integer math
  in `domain::economy`, with balance injected as `EconomyRules`. There is **no background job**.
- The read path (`application::load_economy`) fetches stored state + village structure and calls the
  domain; the web `/village` handler renders amount / capacity / rate.
- **Settle** (writing the accrued amount back) happens only on a mutating command (spending, slice
  003); plain reads never write. Integer division drops the sub-unit remainder, but reads always
  recompute from the original snapshot so nothing is lost between reads; a settle accepts the sub-unit
  drop (Travian-style).

## Consequences
- Scales: only the village being viewed is computed; no per-tick work.
- Reproducible (P2): identical given the same stored state + elapsed time, across restarts.
- Capacity overflow is discarded; negative net crop drains crop toward 0 (troop starvation arrives
  with troops).

## Links
specs/constitution.md (P1, P2, P7); specs/features/002-resource-production/; crates/domain/src/economy.rs;
migrations/0002_village_resources.sql.
