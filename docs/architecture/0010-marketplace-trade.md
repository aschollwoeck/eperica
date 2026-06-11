# Marketplace & trade — a resource-carry engine on the movement rails

**Status:** Current
**Date:** 2026-06-11 · **Slice:** 008

## Context
Resources are per-village (P-economy) and must be physically moved to be shared (GDD §2.4). Slice 008
adds the **Marketplace** and a **resource-carry engine**: merchants carry a bundle to another village,
credit its stores on arrival (capped to capacity, overflow lost), then return empty and free up. It
reuses the 007 due-event movement pattern over the 002 economy/storage caps and the 006 distance —
the rails trade and aid run on, with the negotiated-offer market deferred (direct transfer only).

## Design
- **Merchants are accounted, not entities.** A village's free merchants =
  `merchants_total(marketplaceLevel) − committed`, where `committed` is the sum of `merchants` over
  the village's in-flight trade legs (`in_transit` **and** `processing`). Only one leg of a trade is
  in flight at a time, so the sum is exact; counting `processing` avoids a free-count dip between a
  deliver's claim and the insertion of its return leg. This is pure compute-on-read (P1) — no merchant
  rows are ticked. Per-tribe `capacity`/`speed` and merchants-per-level are balance data (`trade.toml`).
- **A trade is two due-event legs.** `trade_movements` (migration 0011) holds a `deliver` leg then a
  `return` leg — kept separate from `troop_movements` because the payload is resources and combat
  (009) will extend troop movements. Same engine: claim `FOR UPDATE SKIP LOCKED` by `(arrive_at, id)`,
  apply-in-one-tx, `processing → in_transit` orphan requeue at startup.
- **Travel time is the slowest-pace formula, paced by merchant speed.** Reuses 007's
  `travel_time_secs_floored(distance, merchantSpeed, worldSpeed)` (P7); the return leg is the same
  distance and **departs at the delivery instant** (`leg.arrive_at`), so timing is reproducible
  regardless of when the scheduler runs (P2).
- **Send is an optimistic-settle debit.** `order_trade` validates the Marketplace, the bundle (each
  amount ≤ the sender's *settled* stores), free merchants (`merchants_required(total, capacity)` ≤
  available), and the target (a village on another tile); then `start_trade` debits the sender under
  the `updated_at` snapshot guard (the 002/004 pattern) and inserts the deliver leg — one transaction,
  `Conflict` on a race so nothing is half-sent (AC1/AC2/P4).
- **Delivery is a guarded, capped credit — exactly once.** `process_due_trades` claims due legs; for a
  **deliver** it settles the target's economy to the arrival instant, adds the bundle clamped to the
  Warehouse/Granary capacities (`deposit_capped`; overflow lost), and `deliver_and_schedule_return`
  writes the capped amounts under the target's snapshot guard, marks the deliver `done`, and inserts
  the empty return — all one transaction. The credit is **idempotent** because it writes an absolute
  settled amount under the guard (not a delta); a crash before commit is requeued and retried (a few
  optimistic retries absorb a concurrent settle). A **return** leg just flips to `done`, freeing its
  merchants (AC4/AC5). Credited targets are returned so the scheduler re-syncs their starvation check
  (crop rose).
- **The Marketplace is a normal constructable building** (`BuildingKind::Marketplace`, prereq Main
  Building 1) — the new exhaustive variant drove compile errors at every mapping site (balance, repo,
  web), which is the intended safety net.

## Consequences
- The same engine carries combat loot returns (009) and could carry settlers (013) — a new `kind` and
  a different apply, no new queue.
- Capacity overflow is lost by construction (the credit clamps before writing); nothing stores above
  the cap, consistent with production overflow (002).
- The target village id is frozen into the shipment at send, so a later ownership change of that tile
  does not redirect resources in flight (mirrors 007).
- Merchant availability needs no migration of merchant state across builds/restarts — it is always
  derived from the Marketplace level and the in-flight legs.

## Links
specs/constitution.md (P1, P2, P4, P7); specs/features/008-marketplace-trade/;
specs/balance/trade.toml; crates/domain/src/trade.rs (merchants, capped delivery),
crates/application/src/trade.rs (order_trade / process_due_trades), crates/application/src/ports.rs
(TradeRepository); crates/infrastructure/src/repo.rs (optimistic-debit start, claim/deliver/return,
orphan requeue), crates/infrastructure/src/event_store.rs (scheduler tick);
crates/web/src/handlers.rs (Marketplace page, shipments panel); migrations/0011_trade.sql.
