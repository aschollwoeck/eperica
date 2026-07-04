# Resources

> This page explains how your village produces, stores, and spends resources.

## The four resources

Your village works with four resources:

- **Wood**, **Clay**, and **Iron** — used to construct and upgrade almost everything.
- **Crop** — also spent on construction, *and* consumed as upkeep by your population and troops.

## Production over time

Your **resource fields** produce resources every hour, automatically — even while you're logged
out. Upgrading a field raises how much it produces. When you open your village the displayed
amounts are always current: the server calculates how much has been produced since your last
visit.

At level 1 a resource field produces 9 units per hour. At level 10 — the cap for a normal
village — it produces 280 per hour. Your four wood, four clay, four iron, and six crop fields
each run independently.

## Storage

Resources can only be stored up to a **capacity**:

- **Warehouse** — stores wood, clay, and iron. Without any Warehouse built, capacity is **800**
  of each. Once you have Warehouses, total capacity is the **sum** of each Warehouse's level
  value — one level-1 Warehouse gives a total of **1 200**; the 800 base no longer applies.
  Build multiple Warehouses and their values add together.
- **Granary** — stores crop. Same base capacity of **800**, same growth pattern.

Once a store is full, extra production is lost — build and upgrade storage before you overflow.
You may build **multiple** Warehouses or Granaries in your village; their capacities **add
together**, so a village with two level-5 Warehouses has twice the wood/clay/iron capacity of
one with a single level-5.

## Crop, population, and upkeep

Crop is the only resource consumed over time. Two things eat it:

- **Population** — every building and field upgrade increases your village's population by a
  small amount. Each population point eats 1 crop per hour.
- **Troop upkeep** — each unit stationed in or trained at your village consumes crop per hour
  (the amount is listed in the unit's stats).

Your village page shows **net crop per hour**:

```
net crop = (crop field output × world speed) − population − troop upkeep
```

**World speed affects production but not upkeep.** On a fast world your crop fields pour out
resources at the speed multiplier, while population and troop costs stay the same. Fast worlds
are naturally crop-abundant. On a slow world (or if you have many troops and few crop fields)
your net can go negative — watch it carefully, or your garrison will start to starve.

If net crop turns **red**, either upgrade your crop fields, reduce your troop count, or consume
stored crop before the granary empties.

## Field upgrade costs

Upgrading a resource field costs wood, clay, iron, and crop. **The costs are not equal across
resources** — clay is always the most expensive ingredient, which means even upgrading a wood
field costs mainly clay. A level-1 upgrade for a wood, clay, or iron field costs:
**40 wood / 100 clay / 50 iron / 60 crop**.

**Croplands are different.** They have their own cheaper cost table biased toward saving crop —
a level-1 cropland costs **70 wood / 90 clay / 70 iron / 20 crop** (roughly 7 : 9 : 7 : 2).
Because croplands are cheap in crop to upgrade, it is worth prioritising them early.

## Field level caps

- **Normal villages:** fields cap at **level 10**.
- **Your capital:** the field cap rises to **level 20**, and production keeps climbing beyond
  what a normal village can reach. Designate a village as your capital with a **Palace** (see
  [Building & upgrading](buildings.md)).

## See also

- [Building & upgrading](buildings.md) — Warehouse, Granary, and every other building.
- [Training troops & feeding your army](training-and-upkeep.md) — troop upkeep in detail.
- [Getting started](getting-started.md) · [Player Manual index](README.md)
