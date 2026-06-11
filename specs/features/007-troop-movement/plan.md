# Feature 007 — Troop movement & travel — Technical Plan

**Status:** Verified
**Spec:** ./spec.md

The movement engine: a new due-event queue (movements) over the 005 garrison and the 006 distance.
No new external dependencies.

## Constitution check

- **P1 (event-driven):** a movement is **one due-timestamped row** applied only at its `arrive_at`;
  nothing polls it. Away troops are persisted state read on demand; no ticking.
- **P2 (reproducible):** garrison, movements, their troop child rows, and stationed reinforcements
  are fully persisted; arrival applies once and survives restart (claim → apply-in-one-tx → done,
  with orphan requeue — the build/training pattern).
- **P3 (pure domain):** `slowest_speed` and `travel_time_secs` are pure over the unit roster +
  injected `GameSpeed`; distance is the pure `toroidal_distance` (006).
- **P4 (server authority):** the client sends only `(target coord, per-unit counts)` / `(host)`;
  ownership, availability, the target, travel time, and arrival are server-computed. The garrison
  debit is an atomic guarded `UPDATE` (count ≥ requested), race-proof.
- **P7 (speed):** `travelTime = distance ÷ (effectiveSpeed × worldSpeed)`.
- **P11 (performance):** sending = a few indexed writes in one tx; arrivals reuse the indexed
  `FOR UPDATE SKIP LOCKED` claim ordered by `(arrive_at, id)`.

## Domain (`domain`, pure)

- New `movement.rs`:
  - `MovementKind { Reinforce, Return }`.
  - `slowest_speed(troops: &[(UnitId, u32)], roster: &[UnitSpec]) -> Option<u32>` — the minimum
    map `speed` of the present unit types (None if empty / all unknown).
  - `travel_time_secs(distance: f64, slowest_speed: u32, speed: GameSpeed) -> i64` =
    `(distance ÷ (max(1, slowest) × worldSpeed) × 3600).round()`, floored at 1 s.
  - Unit tests: time scales with distance (×2 ⇒ ×2), inversely with world speed, a slower unit
    lengthens the mix; `slowest_speed` picks the minimum; 1 s floor.

## Persistence (`infrastructure` + migration `0010_movements.sql`)

```
troop_movements(
  id uuid PK, owner_id uuid FK users, kind text CHECK (kind IN ('reinforce','return')),
  home_village uuid FK villages ON DELETE CASCADE,     -- owner's village; troops belong here
  deliver_village uuid FK villages ON DELETE CASCADE,  -- delivered-to on arrival
  origin_x int, origin_y int, dest_x int, dest_y int,
  depart_at timestamptz, arrive_at timestamptz, status text DEFAULT 'in_transit', created_at timestamptz)
CREATE INDEX troop_movements_due ON troop_movements (status, arrive_at, id);

movement_troops(movement_id uuid FK CASCADE, unit_id text, count int CHECK (count > 0),
                PRIMARY KEY (movement_id, unit_id))

reinforcements(host_village uuid FK villages ON DELETE CASCADE,  -- where stationed
               home_village uuid FK villages ON DELETE CASCADE,  -- owner's village
               unit_id text, count int CHECK (count > 0),
               PRIMARY KEY (host_village, home_village, unit_id))
```

- Port `MovementRepository`:
  - `village_at(coord)` on `AccountRepository` (new) — resolve the target village by tile.
  - `start_reinforcement(home, deliver, owner, origin, dest, now, arrive_at, troops)` — one tx:
    for each `(unit, n)` a guarded `UPDATE village_units SET count = count - n WHERE village_id =
    home AND unit_id = unit AND count >= n` (rows_affected must be 1, else `RepoError::Backend`
    "insufficient" → use-case maps to a reject; the use-case also pre-checks), deleting rows that
    reach 0; then insert the movement + `movement_troops`.
  - `start_return(host, home, owner, origin, dest, now, arrive_at)` — one tx: read+delete the
    `reinforcements` group `(host, home)`, insert the return movement + troops from those counts.
    Returns the troops (so the caller has the composition for the response/log).
  - `active_movements(owner) -> Vec<MovementView>` (home_village = owner's, status in_transit);
    `reinforcements_at(village)` (stationed here, with owner name); `reinforcements_of(owner)`
    (the owner's troops abroad, grouped by host).
  - `claim_due_movements(now, limit) -> Vec<DueMovement>` (`in_transit → processing`, troops
    loaded); `apply_movement(due)` — one tx: reinforce → upsert `reinforcements(+count)`; return →
    upsert `village_units(+count)`; mark the movement `done`. Exactly-once (atomic + requeue).
  - `requeue_orphaned_movements()`.

## Application (`application`)

- `MovementError` (Insufficient / EmptyComposition / NoTargetThere / SameTile / NotFound /
  NothingStationed / Backend) mirroring prior error enums.
- `order_reinforcement(accounts, movements, unit_rules, map, speed, now, owner, target, troops)` —
  load home village + garrison; resolve `village_at(target)` (reject none / same tile); validate
  composition (non-empty, each `1..=` garrison); `slowest_speed` + `map.distance` →
  `travel_time_secs`; `start_reinforcement`; then `sync_starvation_check(home)` (garrison dropped).
- `order_return(accounts, movements, unit_rules, map, speed, now, owner, host)` — validate the
  owner has a stationed group at `host`; recompute travel for the return path; `start_return`.
- `process_due_movements(accounts, movements, starvation, economy_rules, unit_rules, speed, now,
  limit) -> ()` — claim due, apply each; for **return** arrivals re-sync the home village's
  starvation check (garrison grew). The infra `Scheduler` ticks it + requeues orphans at startup.

## Interface (`web`)

- **`GET /village/rally`** (Rally Point) — a send form: target `x`/`y` and a per-garrison-unit count
  input; submitting shows the computed travel time on the village page. Lists the village's own
  movements for context.
- **`POST /village/rally/send`** (form: `x`, `y`, `count_<unit_id>` …) → `order_reinforcement` → PRG.
- **`POST /village/rally/return`** (form: `host`) → `order_return` → PRG.
- **`/village`** gains: **Reinforcements here** (owner names), **Troops abroad** (grouped by host,
  each with a **Send back** button), and **Movements in progress** (direction + composition + live
  countdown to `arrive_at`, the 003 countdown JS). A **Rally Point** link near the troop buildings.
- Auth via `AuthUser` (Visitor → login); everything re-validated server-side (P4).

## Test strategy

| AC | Test |
|----|------|
| AC1 | app: send debits the garrison and creates a movement arriving at `now + travelTime`; infra: garrison decremented, movement+troops rows written. |
| AC2 | app (fakes): each reject reason (over-garrison, empty, no village at target, same tile) leaves the garrison untouched. |
| AC3 | domain: travel time ×2 with distance, ÷2 with 2× speed, lengthens with a slower unit; 1 s floor. |
| AC4 | infra (DB): a due reinforcement stations the troops once; re-claim after a "crash" (orphan requeue) does not double-station; pending survives a fresh processor. |
| AC5 | infra/app (DB): recall removes the stationed group and creates a return; its arrival rejoins the home garrison once; home upkeep/starvation re-synced. |
| AC6 | app/domain: with troops away the home garrison (and thus 005 upkeep) is lower; on return it rises again. |
| AC7 | web integration: rally page sends a reinforcement (PRG); village shows movement + countdown, reinforcements-here with owner, troops-abroad with a send-back; visitor → login. |

## Notes

- `village_at` and the home lookup reuse the 006 `(world_id, x, y)` index and `villages_of`.
- The destination **village id** is fixed into the movement at send, so a later ownership change of
  that tile does not redirect troops already in flight (spec Decision).
- `apply_movement` is exactly-once for the same reason as `apply_build`: the delivery and the
  `status='done'` flip share one transaction, so a committed apply is never re-claimed; a crash
  before commit is recovered by the orphan requeue and re-applied cleanly.
