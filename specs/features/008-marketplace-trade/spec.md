# Feature 008 — Marketplace & trade

**Status:** Verified
**Depends on:** 002 (resource economy + storage caps), 006 (map, toroidal distance), 007 (movement engine)
**Roadmap:** M3 · slice 008 · GDD §2.4, §4.2 — resource logistics; reuses the movement engine.

## Goal

A player can build a **Marketplace** and **send a shipment of resources** to another village across
the map. The resources **leave the sender immediately**, ride **merchants** over the map (limited in
number and per-tribe carry capacity), arrive after a computed travel time, and are **added to the
target's stores** (overflowing storage is lost). The merchants then travel home **empty** and become
available again. Everything is a **due-event** (P1) on the same engine as troop movement (007).

This is resource logistics: the rails trade and aid run on. No NPC merchant and no instant
conversion (GDD §2.4 baseline exclusions).

## Concepts

- **Marketplace:** a center building (GDD §4.2). It must exist to trade, and its **level** sets how
  many **merchants** the village has (balance data). The Rally Point is for troops; the Marketplace
  is for resources.
- **Merchant:** a carrier with a per-tribe **capacity** (Romans 500 · Teutons 1000 · Gauls 750) and a
  per-tribe map **speed** (balance, fields/hour). A shipment needs
  `ceil(totalResources ÷ capacity)` merchants. They are **committed for the whole round trip** (out
  and back) and free again only when the empty return arrives home.
- **Shipment (trade):** a body of resources `(wood, clay, iron, crop)` leaving the sender's village
  toward a target village. A **discrete due-event** (P1): nothing polls it; on arrival the server
  credits the target authoritatively (P4) and schedules the empty **return**; on the return's
  arrival the merchants are freed.
- **Travel time:** `travelTime = distance ÷ (merchantSpeed × worldSpeed)` — the 006 toroidal
  distance, the tribe's merchant speed, scaled by world speed (P7); never below 1 second. The return
  leg is the same distance.

## User stories

- As a **player**, I want to build a Marketplace and send resources to an ally's village, so I can
  support them or settle a trade we agreed.
- As a **player**, I want to see my merchants — how many are free and what shipments are in transit —
  so I can plan deliveries.
- As a **player**, I want resources sent to me to arrive in my warehouse and granary.

## Acceptance criteria

> All trade is server-authoritative (P4): the Marketplace requirement, the shipment amounts, the
> merchant count, the target, travel time, arrival, and the capped delivery are computed and enforced
> server-side; the client only issues the command.

- **AC1 — Send a shipment.** Given the player's village with a **Marketplace**, stored resources, and
  free merchants, and a target village (another existing village on a different tile), when the player
  sends a resource bundle (each amount `0..=` the stored amount, total `> 0`) that fits in the free
  merchants, then those resources **leave the sender's stores immediately**, the required merchants
  are **committed**, and a shipment is created arriving at `now + travelTime`.

- **AC2 — Send rejected.** Rejected with **nothing debited and no merchant committed** when any of:
  the village has **no Marketplace**; the bundle is empty (all zero); any amount exceeds the stored
  resource; the shipment needs **more merchants than are free**; the target tile holds **no village**;
  the target is the sender's **own** village tile.

- **AC3 — Merchants, capacity & travel time (P7).** A shipment needs `ceil(total ÷ tribeCapacity)`
  merchants (a bigger load or a smaller-capacity tribe needs more); the village's merchant count is
  `marketplaceLevel`-driven (balance); committing a shipment lowers the free count until it returns.
  Travel time scales proportionally with distance, inversely with merchant speed and world speed, and
  is never below 1 second.

- **AC4 — Arrival delivers, exactly once (P1/P2).** When the shipment's due time passes, its
  resources are **added to the target village's stores, clamped to capacity** (wood/clay/iron to the
  Warehouse cap, crop to the Granary cap — any overflow is **lost**), **exactly once**; the credit is
  persisted, survives a restart, and a pending shipment still completes after one. An empty **return**
  is then scheduled back to the sender.

- **AC5 — Round-trip frees the merchants.** When the empty return arrives home, the merchants it
  carried become **available again, exactly once** (P1/P2); a player who has committed all merchants
  can send again only after a return.

- **AC6 — Interface.** The **Marketplace** page shows free/total merchants and the per-tribe capacity,
  and lets the player pick a target coordinate and per-resource amounts (showing the merchants a load
  needs) and send. The **village** page shows **shipments in transit** (direction, contents, and a
  live arrival **countdown**). Unavailable actions are not offered (and are rejected server-side
  regardless). Without a Marketplace the page explains the requirement and offers no action.

## Roles & permissions

Per [roles.md](../../roles.md).

| Role | Permitted | Denied (server-enforced) |
|------|-----------|--------------------------|
| **Visitor** | N/A (considered). | Send/view shipments (redirected to login). |
| **Player** | Send shipments from **their own** village (Marketplace required); see their merchants and shipments; passively receive shipments into their stores. | Send another player's resources; send more than stored or more than free merchants carry; forge the target, amounts, merchant count, or travel time; trade without a Marketplace. |
| **Moderator** | N/A (considered). | — |
| **Administrator** | World speed scales travel time (AC3). | — (superset). |
| **System** | *(system-initiated)* Deliver shipments and free merchants at their due time (AC4/AC5); clamp deliveries to capacity; never mid-flight. | — |

## Out of scope

- **The offer/auction marketplace** (posting buy/sell offers at negotiated ratios and matching them)
  → a later refinement; this slice is the faithful **direct resource transfer** ("send resources"),
  which is what reuses the movement engine and proves resource logistics.
- **Trade Office** (raises merchant carry capacity) → later balance building.
- **Trading between your *own* villages** → needs multi-village (013); a player has one village now,
  so shipment targets are other players' villages.
- **Merchants carrying loot / resources on the return** → returns are always empty here.
- **In-transit recall of a shipment.** Once sent it completes; there is no cancellation after
  departure (consistent with 007 §8.3). Merchants are recovered only by the round trip.
- **Cranny / loot interaction** (009) and **alliance bonuses to trade** (later).

## Decisions

- **Direct transfer, not an offer book.** A shipment is a one-way resource carry to a chosen village;
  the recipient need not agree (you can aid an ally). Negotiated-ratio offers are deferred (Out of
  scope).
- **A trade is two due-event rows on a `trade_movements` table** — a `deliver` leg then a `return`
  leg — kept separate from `troop_movements` (007) because the payload is resources, not troops, and
  combat (009) will extend troop movements. The same engine pattern: claim → apply-in-one-tx → done;
  `FOR UPDATE SKIP LOCKED`; orphan requeue.
- **Merchants are accounted, not stored as rows.** A village's free merchants =
  `merchantsFor(marketplaceLevel) − committed`, where `committed` = the sum of `merchants` over the
  village's **in-flight** trade rows (only one leg of a trade is in flight at a time, so the sum is
  exact). Sending commits `ceil(total ÷ capacity)`; the empty return's completion releases them by
  marking that last row `done`.
- **Capped delivery via the optimistic settle (002/004).** At arrival the target's resources are
  settled to the arrival instant, the shipment is added clamped to the Warehouse/Granary capacity
  (overflow lost), and written back under the `updated_at` snapshot guard (retried on a concurrent
  change) — so a delivery never races a build debit or production into a lost update.
- **Per-tribe merchant `capacity` and `speed`, and merchants-per-Marketplace-level, are balance
  data** (P7) — not fixed in code.
- **Targets are resolved from a coordinate** the player enters; the server requires a village there,
  on another tile. The target village id is fixed into the shipment at send (a later ownership change
  of that tile does not redirect resources already in flight).
