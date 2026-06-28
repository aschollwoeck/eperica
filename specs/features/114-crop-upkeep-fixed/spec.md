# Feature 114 — crop upkeep is fixed across world speed (faithful Travian)

## Why
002 AC4 scaled **both** production and consumption with world speed, so net crop scaled linearly — at
1000× a mild −50/h base deficit became −50,000/h, the opposite of Travian. In Travian, **production scales
with server speed but per-unit/per-population upkeep is fixed**, so fast worlds are crop-**abundant**. This
slice adopts that model (supersedes 002 AC4).

## Acceptance criteria
- **AC1 — crop net.** `crop_net = (crop field output × speed) − population − troopUpkeep`. Field output
  (incl. oasis bonus) scales with world speed; the **village population** and **troop upkeep** are NOT scaled
  (fixed crop/hour, per Travian). Wood/clay/iron are unchanged (production-only, no upkeep). At speed 1× the
  result is identical to before.
- **AC2 — starvation stays consistent (005 AC7).** When stored crop is dry and net crop < 0, the garrison is
  culled until sustainable. The crop budget for troops is the speed-scaled field output minus population
  (= `crop_net + upkeep`), so the cull matches the new net.
- **AC3 — high speed is crop-positive.** A village whose 1× balance is healthy is comfortably positive at high
  speed (scaled production dominates fixed upkeep); a genuinely under-developed village can still go negative.

## Out of scope
- Rebalancing field/upkeep tables; the world `speed` value itself.
