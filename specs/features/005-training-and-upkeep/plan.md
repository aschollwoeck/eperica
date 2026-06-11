# Feature 005 — Training & upkeep — Technical Plan

**Status:** Verified
**Spec:** ./spec.md

Builds on 004: training batches reuse the optimistic-settle + due-event order machinery; the
starvation check is a new, re-validating event shape. No new external dependencies.

## Constitution check

- **P1 (lazy/event-driven):** a batch is **one** due-timestamped row whose `next_complete_at`
  advances as units finish — not an event per unit, and never a tick over villages. Starvation is
  **one pending depletion check per village**, (re)scheduled only at the mutation points where the
  net rate or the store changes; between mutations rates are constant, so the scheduled depletion
  instant is exact. Upkeep itself is pure compute-on-read (extends the 002 rate model).
- **P2 (reproducible):** garrison counts, batch progress (`count_done`), and checks are fully
  persisted; applying completions derives `k` from `started_at`/`per_unit_secs` and commits
  garrison + progress in **one transaction**, so a crash/restart resumes without loss or
  duplication (AC5). The starvation cull is a pure, deterministic function of persisted state.
- **P3 (pure domain):** per-unit time, training gates, upkeep, the cull order, and depletion time
  are pure functions over injected `UnitRules`/`EconomyRules`.
- **P4 (server authority):** the client sends `(unit, count)`; building, research, tribe, cost,
  time, and the one-batch-per-building rule are enforced server-side (partial unique index, as
  003/004). Settles are snapshot-guarded (004's optimistic settle) — concurrent debits can't be
  lost.
- **P7 (speed):** `perUnit = trainTime ÷ (speed × trainingFactor(level))`.
- **P11 (performance):** claims reuse the indexed `FOR UPDATE SKIP LOCKED` pattern; the
  starvation sync is a handful of point lookups bundled into mutations that already load the
  village.

## Domain (`domain`, pure)

- `units.rs` additions:
  - `TrainingRules { building_factor_per_level: Vec<f64> }` (shared by all troop buildings;
    clamped like the MB factor) — lives in `UnitRules.training`.
  - `per_unit_time_secs(train_secs, building_level, &TrainingRules, speed) -> i64` (≥ 1).
  - `TrainDenied { NotResearched, BuildingMissing, BuildingUnavailable, CountOutOfRange }` and
    `can_train(spec, is_researched, count, &buildings) -> Result<(), TrainDenied>` —
    `BuildingUnavailable` for `trained_in` kinds no slice has made trainable (Residence).
  - `batch_cost(spec, count) -> ResourceAmounts` (saturating).
  - `MAX_TRAINING_BATCH: u32 = 9999`.
- `economy.rs`: `production_rates` (and `compute_economy`) gain a `troop_upkeep: i64` argument:
  `crop_base = crop fields − population − troop_upkeep` (call sites updated; 0 for no garrison).
- New garrison/starvation rules (in `units.rs`):
  - `garrison_upkeep(garrison: &[(UnitId, u32)], roster) -> i64`.
  - `starve(garrison, roster, net_without_troops: i64) -> (Vec<(UnitId, u32)>, Vec<(UnitId, u32)>)`
    — repeatedly removes one unit of the garrisoned type with the **highest `cropUpkeep`** (ties:
    roster order) until `net_without_troops − upkeep(remaining) ≥ 0` or empty; returns
    (survivors, casualties). Pure and exactly testable (AC7).
  - `depletion_secs(crop_stored, net_rate) -> Option<i64>` — `None` when net ≥ 0; else
    `ceil(stored × 3600 ÷ deficit)`.

## Persistence (`infrastructure` + migration `0008_training.sql`)

```
village_units(village_id FK CASCADE, unit_id text, count integer CHECK (count > 0),
              PRIMARY KEY (village_id, unit_id))
training_orders(id uuid PK, village_id FK CASCADE, building text, unit_id text,
                count_total integer, count_done integer DEFAULT 0, per_unit_secs bigint,
                started_at timestamptz, next_complete_at timestamptz,
                status text DEFAULT 'active', created_at timestamptz)
CREATE UNIQUE INDEX one_active_training_per_building ON training_orders (village_id, building)
    WHERE status IN ('active', 'processing');
CREATE INDEX training_orders_due ON training_orders (status, next_complete_at, id);
starvation_checks(village_id uuid PK REFERENCES villages ON DELETE CASCADE,
                  due_at timestamptz, status text DEFAULT 'pending')
CREATE INDEX starvation_checks_due ON starvation_checks (status, due_at);
```

- Port `TrainingRepository`:
  - `start_training(village, settled, settled_from, now, NewTrainingOrder)` — optimistic settle
    (004) + insert; unique index ⇒ `Duplicate` for a busy building.
  - `active_training(village) -> Vec<ActiveTraining>`. (`garrison(village)` lives on
    **`AccountRepository`** instead — it is village state on the economy read path, used by
    `load_economy` and every settle; `village_by_id` joins it there for the system processors.)
  - `claim_due_training(now, limit)` (`active → processing`, `next_complete_at ≤ now`).
  - `apply_training(due, now)` — compute `k = min(elapsed ÷ per_unit − done, remaining)` **in the
    use-case**, then one tx: garrison upsert (`+k`), `count_done += k`, advance
    `next_complete_at`, status `active`/`done` (AC5; idempotent because progress and garrison move
    together).
  - `requeue_orphaned_training()`.
- Port `StarvationRepository`:
  - `schedule_starvation_check(village, due_at)` — upsert (`INSERT … ON CONFLICT (village_id) DO
    UPDATE SET due_at = EXCLUDED.due_at, status = 'pending'`); `cancel_starvation_check(village)`.
  - `claim_due_starvation(now, limit)`; `apply_starvation(village, settled, settled_from, now,
    survivors)` — one tx: snapshot-guarded resource settle, replace garrison rows, mark check
    done; `Conflict` ⇒ leave pending (retried next tick).

## Application (use-cases)

- `order_train(accounts, training, units, economy_rules, unit_rules, speed, now, owner, unit,
  count)` — gates (researched incl. tier-1, building present & trainable-here, count range,
  affordability of `count × cost`), settle/debit, insert batch with
  `next_complete_at = now + perUnit`; then **sync the starvation check** (training start changed
  the store). `TrainError` mirrors 004's error enums (incl. `Conflict`).
- `process_due_training(training, …, now, limit)` — claim, apply `k` completions per batch, and
  sync the starvation check for each affected village (upkeep rose).
- `sync_starvation_check(…, village, now)` — load stored + fields/buildings + garrison; net ≥ 0 or
  empty garrison ⇒ cancel; else upsert the check at `now + depletion_secs` (AC7/AC8). Called after
  every successful settle/mutation: order_build / order_research / order_smithy_upgrade /
  order_train / training completions / build completions (only when a garrison exists) / a
  starvation apply that left net < 0 with troops (defensive; the cull makes net ≥ 0 or empties).
- `process_due_starvation(…, now, limit)` — claim due checks; per village: settle to now; if
  `crop == 0 ∧ net < 0 ∧ garrison non-empty` → `starve` (domain) and `apply_starvation`; else if
  net < 0 with troops → reschedule at the fresh depletion time; else mark done (re-validation,
  AC7).
- The infra `Scheduler` ticks both processors and requeues orphaned training at startup.

## Interface (`web`)

- **`/village`** — a **Garrison** panel (unit name + count, total upkeep) and links to built troop
  buildings; the crop rate already shows the (now upkeep-reduced) net (AC6/AC9).
- **`GET /village/troops/{barracks|stable|workshop}`** — one handler parameterized by building:
  researched units trained there, cost, per-unit time at current level, count input; the active
  batch with remaining count + countdown to the next completion.
  **`POST /village/train`** (form: `unit`, `count`) → `order_train` → PRG back to the building
  page. Auth via `AuthUser`; everything re-validated server-side (P4).

## Balance data

- `specs/balance/units.toml` += `[training] building_factor = [11 entries]` (index = building
  level; ≥ 1, higher ⇒ faster).
- `specs/balance/construction.toml` += `stable`, `workshop` (10 levels; prerequisites per AC1).
- `specs/balance/economy.toml` += population tables for `stable`, `workshop`.

## Test strategy

| AC | Test |
|----|------|
| AC1 | infra: loaded prerequisites match spec; (gating reuses 003 AC4 tests). |
| AC2 | app: batch debits `count × cost`, `next_complete_at = now + perUnit`; infra: insert + busy-building `Duplicate`. |
| AC3 | app (fakes): every rejection reason leaves state untouched. |
| AC4 | domain: per-unit time strictly decreases with building level; halves at speed 2. |
| AC5 | infra (DB): apply with elapsed = 2.5 × perUnit adds exactly 2 units; re-claim/apply after "crash" (orphan requeue) completes the batch with no duplication; finished batch claims nothing. |
| AC6 | domain: `production_rates` subtracts troop upkeep; web integration: village shows reduced net. |
| AC7 | domain: `starve` culls highest-upkeep-first (exact counts, tie order); infra/app: due check on a depleted village culls exactly once, survives restart; a check on a recovered village reschedules/no-ops. |
| AC8 | app: empty garrison ⇒ check cancelled, store floors at 0. |
| AC9 | web integration: troop page lists units + form; after training, queue + countdown render; garrison appears on the village page after a completion. |

## Notes

- The depletion instant is exact because every store/rate mutation re-syncs the check; a fired
  check **re-validates from live state** before culling, so stale or early checks are harmless.
- `apply_training` adds units and advances progress in one transaction keyed to the claimed row —
  the claim/requeue cycle can re-run the computation but never double-apply a completion.
- Settlers/administrators (`trained_in = residence`) surface as `BuildingUnavailable` until 013.
