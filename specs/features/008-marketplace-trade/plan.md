# Feature 008 — Marketplace & trade — Technical Plan

**Status:** Draft
**Spec:** ./spec.md

A resource-carry engine: a new due-event queue (`trade_movements`) over the 002 economy/storage caps,
the 006 distance, and the same claim→apply→done machinery as 007. A new **Marketplace** building and
per-tribe **merchant** balance data. No new external dependencies.

## Constitution check

- **P1 (event-driven):** a shipment is **two due-timestamped rows** (deliver, then return) applied
  only at their `arrive_at`; nothing polls them. Merchant availability is *computed on read* from the
  in-flight rows — no merchant entities are ticked.
- **P2 (reproducible):** shipments, their resource columns, and merchant commitments are fully
  persisted; deliver and return each apply once and survive restart (claim → apply-in-one-tx → done,
  orphan requeue).
- **P3 (pure domain):** `merchants_required`, `MerchantRules` lookups, and travel time are pure over
  balance data + injected `GameSpeed`; distance is the pure 006 `toroidal_distance`.
- **P4 (server authority):** the client sends only `(target coord, per-resource amounts)`; the
  Marketplace requirement, the debit, the merchant count, the target, travel time, the capped
  delivery, and the return are server-computed. The send debit and the delivery credit are atomic and
  optimistically guarded.
- **P7 (speed):** `travelTime = distance ÷ (merchantSpeed × worldSpeed)`; merchant capacity/speed and
  merchants-per-level are balance data.
- **P11 (performance):** sending = an optimistic spend + one insert in a tx; arrivals reuse the
  indexed `FOR UPDATE SKIP LOCKED` claim ordered by `(arrive_at, id)`; the committed-merchant sum is
  one indexed aggregate.

## Domain (`domain`, pure)

- `BuildingKind::Marketplace` (new exhaustive variant — drives compile errors at every mapping site).
- New `trade.rs`:
  - `TradeKind { Deliver, Return }`.
  - `MerchantProfile { capacity: u32, speed: u32 }` and `MerchantRules` holding a per-`Tribe`
    profile and a `per_level: Vec<u32>` (merchant count by Marketplace level), with
    `profile(tribe) -> MerchantProfile`, `merchants_total(level) -> u32`.
  - `merchants_required(total: i64, capacity: u32) -> u32` = `ceil(total ÷ max(1, capacity))`.
  - `ResourceBundle` helper (or reuse `ResourceAmounts`) with `total()` and an all-zero check.
  - Travel time reuses 007's `travel_time_secs_floored(distance, merchantSpeed, speed)`.
  - Tests: `merchants_required` rounds up and scales inversely with capacity; `merchants_total`
    reads the per-level table; an empty bundle has total 0.

## Balance (`specs/balance/` + `infrastructure::balance`)

- `construction.toml` — `[buildings.marketplace]` (prereq `main_building` ≥ 1, like Warehouse) with
  cost/time arrays.
- `economy.toml` — a `marketplace` population row (consistent with other buildings).
- New `trade.toml` — `[merchants] per_level = [...]`; `[tribes.romans|teutons|gauls] capacity, speed`.
  Loaded by a new `merchant_rules() -> MerchantRules` in `balance.rs`; `parse_building` gains a
  `"marketplace"` arm.

## Persistence (`infrastructure` + migration `0011_trade.sql`)

```
trade_movements(
  id uuid PK, owner_id uuid FK users,
  kind text CHECK (kind IN ('deliver','return')),
  home_village   uuid FK villages ON DELETE CASCADE,  -- sender; merchants belong here
  target_village uuid FK villages ON DELETE CASCADE,  -- credited on deliver
  origin_x int, origin_y int, dest_x int, dest_y int,
  wood bigint, clay bigint, iron bigint, crop bigint, -- carried (all 0 on a return)
  merchants int CHECK (merchants > 0),
  depart_at timestamptz, arrive_at timestamptz, status text DEFAULT 'in_transit', created_at timestamptz)
CREATE INDEX trade_movements_due  ON trade_movements (status, arrive_at, id);
CREATE INDEX trade_movements_home ON trade_movements (home_village, status); -- committed sum
```

- Port `TradeRepository`:
  - `committed_merchants(home) -> u32` — `SUM(merchants)` over `home_village = home AND status IN
    ('in_transit','processing')` (counting `processing` avoids a free-count flicker between a
    deliver's claim and the insertion of its return leg).
  - `start_trade(home, target, owner, origin, dest, now, arrive, bundle, merchants, snapshot)` — one
    tx: an **optimistic spend** from the sender (the 002/004 settle-and-debit under the `updated_at`
    snapshot; `Conflict` if raced or insufficient), then insert the `deliver` row. 
  - `claim_due_trades(now, limit) -> Vec<DueTrade>` (`in_transit → processing`, all columns loaded).
  - `deliver_and_schedule_return(due, target_new_amounts, target_snapshot, return_arrive)` — one tx:
    **guarded credit** of the target (write the capped settled amounts WHERE `updated_at = snapshot`,
    else `Conflict` → caller retries), mark the deliver row `done`, and insert the `return` row
    (empty bundle, same `merchants`, swapped origin/dest). Exactly-once (credit + done share the tx).
  - `complete_trade(id)` — mark a `return` row `done` (frees its merchants). Exactly-once via the flip.
  - `active_trades(owner) -> Vec<TradeView>` (home = owner's, in flight) for the village panel.
  - `requeue_orphaned_trades()` — `processing → in_transit` at startup.
- `village_at` / `village_by_id` (007/earlier) resolve the target and its building levels for caps.

## Application (`application`)

- `TradeError` (NoMarketplace / EmptyBundle / Insufficient / NotEnoughMerchants / NoTargetThere /
  SameTile / NotFound / Backend).
- `order_trade(accounts, trades, economy_rules, merchant_rules, map, speed, now, owner, target,
  bundle)` — load the sender's village (require a Marketplace level ≥ 1); settle its economy to `now`
  (reuse `load_economy`); validate the bundle (non-empty, each ≤ settled stored); resolve
  `village_at(target)` (reject none / own tile); `merchants_required(total, capacity)` ≤
  `merchants_total(level) − committed_merchants(home)`; `travel_time_secs_floored(distance, speed)`;
  `start_trade` under the settled snapshot (maps `Conflict` → `Insufficient`).
- `process_due_trades(accounts, trades, economy_rules, map, merchant_rules, speed, now, limit)` —
  claim due; for each **Deliver**: load the target + settle its resources to `arrive`, add the
  shipment clamped to the Warehouse/Granary capacities (overflow lost), and
  `deliver_and_schedule_return` under the target snapshot (retry on `Conflict`); for each **Return**:
  `complete_trade` (frees merchants). The infra `Scheduler` ticks it and requeues orphans at startup.

## Interface (`web`)

- **`GET /village/market`** (Marketplace) — without the building, explain the requirement; otherwise
  show the tribe's merchant **capacity**, **free/total** merchants, and a send form (target `x`/`y` and
  `amount_wood|clay|iron|crop`), noting how many merchants a full load needs. A **Marketplace** link
  near the other buildings on `/village`.
- **`POST /village/market/send`** (form: `x`, `y`, `amount_*`) → `order_trade` → PRG to `/village`.
- **`/village`** gains **Shipments in transit** — each of the player's in-flight trades (deliver:
  "Shipment to (x|y)" with contents; return: "Merchants returning from (x|y)") with a live countdown.
- Auth via `AuthUser` (Visitor → login); everything re-validated server-side (P4).

## Test strategy

| AC | Test |
|----|------|
| AC1 | app (fakes): send debits the sender, commits `ceil(total÷cap)` merchants, creates a deliver arriving at `now + travelTime`; infra (DB): resources decremented, trade row + merchants written. |
| AC2 | app (fakes): each reject (no Marketplace, empty, over-stored, too few merchants, no village, own tile) leaves resources and merchants untouched. |
| AC3 | domain: `merchants_required` rounds up / scales with capacity; `merchants_total` by level; travel time ×2 with distance, ÷2 with 2× speed, 1 s floor. |
| AC4 | infra (DB): a due deliver credits the target clamped to capacity (overflow lost) exactly once; re-claim after a "crash" (orphan requeue) does not double-credit; a return leg is scheduled. |
| AC5 | infra (DB): the return completes and `committed_merchants` drops back; survives a fresh processor. |
| AC6 | web integration: market page sends a shipment (PRG); village shows the shipment + countdown; the target's stores rise; no-Marketplace explains; visitor → login. |

## Notes

- **Exactly-once** holds for the same reason as 007: deliver's credit + `done` flip share one tx (a
  committed credit is never re-claimed; a crash before commit is requeued and retried — the credit is
  idempotent because it writes an absolute settled amount under the snapshot guard, not a delta).
- **Capacity overflow is lost** by construction: the credit clamps `settled + carried` to the cap
  before writing; nothing stores above the cap.
- The target **village id** is fixed into the shipment at send, so a later ownership change of that
  tile does not redirect resources already in flight (spec Decision) — mirrors 007.
- Merchants are **not** entities: availability is a pure function of the Marketplace level and the
  in-flight commitment sum, fitting P1's compute-on-read rule.
