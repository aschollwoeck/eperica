# Feature 004 — Tribes & units — Technical Plan

**Status:** Verified
**Spec:** ./spec.md

Builds on 003: research and Smithy upgrades reuse the settle-debit-order pattern and the due-event
processor shape (`claim → apply → done`, `FOR UPDATE SKIP LOCKED`). No new external dependencies.

## Constitution check

- **P1 (lazy/event-driven):** research and upgrades are **due-timestamped order rows** applied only
  when due, exactly like build orders. Researched-set and unit levels are plain persisted state read
  on demand; nothing ticks per unit.
- **P2 (reproducible):** tribe, researched units, unit levels, and pending orders are fully
  persisted; a pending order survives restart and applies once (idempotent apply + status flip).
- **P3 (pure domain):** rosters, research/upgrade gating, costs, and time formulas are pure
  functions over injected `UnitRules`; tribes are a domain enum (already present).
- **P4 (server authority):** the client sends only `(unit_id)` / `(unit_id)`-for-upgrade / tribe
  choice; tribe membership, requirements, costs, levels, and **queue rules are DB-guaranteed under
  races** via partial unique indexes (one research + one upgrade per village; build-queue *lanes*
  for the Roman trait).
- **P7 (speed):** `researchTime = base ÷ speed`, `upgradeTime = base ÷ speed`; no hardcoded
  wall-clock values.
- **P11 (performance):** ordering = settle + debit + insert in one tx (indexed); the processor
  reuses the 003 claim pattern ordered by `(complete_at, id)`.

## Domain (`domain`, pure)

- `Tribe` enum exists (`village.rs`); becomes mandatory on the player: tribe is **account-level**,
  stamped on each village at founding (`Village.tribe: Option<Tribe>` stays for storage symmetry but
  is always `Some` after this slice).
- **`BuildingKind` gains `Barracks`, `Academy`, `Smithy`, `Stable`, `Workshop`** (exhaustive matches
  force every mapper/label to handle them). Only Barracks/Academy/Smithy enter the 004 build catalog
  (construction.toml entries + prerequisites per spec AC5); Stable/Workshop exist as kinds so unit
  research requirements can reference them, and become constructable in 005.
- New module `units.rs`:
  - `UnitId` — stable slug newtype (e.g. `legionnaire`), `UnitRole` enum
    (Infantry/Cavalry/Scout/Siege/Expansion).
  - `UnitSpec { id, name, role, attack, defense_infantry, defense_cavalry, speed, carry_capacity,
    crop_upkeep, cost: ResourceAmounts, train_secs, trained_in: BuildingKind,
    research: Option<ResearchSpec> }` — `research: None` ⇒ **tier-1, implicitly researched** (AC9).
  - `ResearchSpec { cost: ResourceAmounts, time_secs, requirements: Vec<(BuildingKind, u8)> }`.
  - `UnitRules { rosters: HashMap<Tribe, Vec<UnitSpec>>, smithy: SmithyRules }` with
    `roster(tribe)`, `unit(tribe, id)`; **validation** (each tribe exactly 10 units, unique ids,
    exactly one research-free unit per tribe) enforced by the loader (fail fast, AC4).
  - `SmithyRules { cost_permille_per_level: Vec<u32>, time_secs_per_level: Vec<i64> }` (20 entries):
    `upgrade_cost(unit, level) = unit.cost × permille[level−1] / 1000` (per-component, rounded),
    `upgrade_time(level, speed)`. Max level = `len()` = 20; gate `level < smithy_level` too (AC10).
  - Pure gates: `can_research(spec, researched: &set, buildings) -> Result<(), ResearchDenied>`,
    `can_upgrade(spec, researched, current_level, smithy_level, rules)`, plus
    `research_time(spec, speed)`. Reuse `can_afford`/`debit` from construction.
- **Roman lane rule:** `queue_lane(tribe, target) -> Lane` — `Romans` ⇒ `Field`/`Building` by target
  category; others ⇒ `All` (AC13). Pure function, unit-tested.

## Persistence (`infrastructure` + migrations)

- **`0005_tribes.sql`** — `ALTER TABLE users ADD COLUMN tribe text`; backfill `'gauls'`;
  `SET NOT NULL` + CHECK in (`'romans'`,`'teutons'`,`'gauls'`); `UPDATE villages SET tribe='gauls'
  WHERE tribe IS NULL` (AC3 — migration-boundary test).
- **`0006_units.sql`** —
  ```
  village_research(village_id FK CASCADE, unit_id text, researched_at, PK(village_id, unit_id))
  village_unit_levels(village_id FK CASCADE, unit_id text, level smallint, PK(village_id, unit_id))
  unit_orders(id uuid PK, village_id FK CASCADE, kind text 'research'|'smithy', unit_id text,
              target_level smallint NULL, complete_at timestamptz, status text DEFAULT 'pending',
              created_at timestamptz)
  CREATE UNIQUE INDEX one_active_research ON unit_orders(village_id) WHERE status='pending' AND kind='research';
  CREATE UNIQUE INDEX one_active_smithy   ON unit_orders(village_id) WHERE status='pending' AND kind='smithy';
  CREATE INDEX unit_orders_due ON unit_orders(status, complete_at, id);
  ```
  One research **and** one upgrade may run concurrently (different partial indexes); a second of
  either kind races into `RepoError::Duplicate` (P4).
- **`0007_build_lanes.sql`** — `ALTER TABLE build_orders ADD COLUMN lane text NOT NULL DEFAULT
  'all'`; drop `one_active_build`; `CREATE UNIQUE INDEX one_active_build_per_lane ON
  build_orders(village_id, lane) WHERE status='pending'`. Lane values: `'all' | 'field' |
  'building'`, computed **server-side** from tribe + target. Existing pending rows keep `'all'`
  (their villages are backfilled Gauls — consistent).
- Repo additions (extend `AccountRepository` as in 003): user tribe in `create_account` (+ form
  value `parse_tribe`, already present), `researched_units(village_id)`, `unit_levels(village_id)`,
  `start_unit_order(...)` (settle + debit + insert, one tx), `active_unit_orders(village_id)`,
  `claim_due_unit_orders(now, limit)`, `apply_unit_order(due)` (idempotent: research = `INSERT … ON
  CONFLICT DO NOTHING`; smithy = upsert to `target_level`), `requeue_orphaned_unit_orders()`.
- Balance loader (`balance.rs`): embed `specs/balance/units.toml` via `include_str!`, DTO → 
  `UnitRules`, **fail fast** on incomplete rosters (AC4).

## Application (use-cases)

- `register`: `RegisterCommand` gains `tribe: String`; `validate()` requires a known tribe (AC1);
  `create_account` stores it on user + starting village.
- `order_research(repo, unit_rules, economy_rules, speed, now, owner, unit_id)` — load village
  (ownership), economy, building levels, researched set, then domain gates (Academy present,
  requirements, not researched, affordable) → `start_unit_order`. Errors mirror 003's `BuildError`
  shape (`ResearchError`).
- `order_smithy_upgrade(...)` — same shape with level gates (AC10/AC11).
- `process_due_unit_orders(repo, now, limit)` — claim + apply (System actor, AC8/AC12); wired into
  the infra `Scheduler` tick alongside `process_due_builds`, with orphan requeue at startup.
- `order_build` change: derive `lane` from owner tribe + target (AC13); non-Roman behavior
  unchanged (single `'all'` lane).

## Interface (`web`)

Per [ui-style-guide.md](../../ui-style-guide.md); new components (tribe picker card/radio, unit
table rows) are added to the guide first if not covered.

- **`/register`** — required tribe radio group (three options + one-line descriptions, GDD §5
  flavor); server rejects missing/unknown (AC1). 
- **`/village`** — shows the tribe; links to Academy/Smithy sections when those buildings exist.
- **`GET /village/academy`** — the tribe's roster: per unit, attributes, research state
  (researched / available / requirements-unmet incl. which), cost, time; **Research** action when
  available+affordable; live countdown on the active research (same JS deadline pattern as 003).
  **`POST /village/academy/research`** (form: `unit`) → `order_research` → redirect (PRG, as 003).
- **`GET /village/smithy`** + **`POST /village/smithy/upgrade`** — researched units with current →
  next level, cost/time, gate reasons (smithy level cap), countdown on the active upgrade.
- No Academy/Smithy in the village ⇒ pages explain the building is required (no actions). All
  actions re-validated server-side (P4); handlers use the `AuthUser` extractor (Visitor → login).

## Balance data (`specs/balance/units.toml`)

- `[smithy]` — `cost_permille_per_level` (20), `time_secs_per_level` (20).
- `[[romans.units]] / [[teutons.units]] / [[gauls.units]]` — 10 each, faithful Travian-style stats:
  all §6.2 attributes + `role`, `trained_in`, and (absent for tier-1) `[research]` cost/time/
  requirements. Requirements use building levels ≤ 10 (current max) so every in-slice path is
  reachable; cavalry/siege reference Stable/Workshop and unlock in 005.
- `construction.toml` gains `barracks`, `academy`, `smithy` (10 levels: cost/time) + prerequisites
  per spec AC5.

## Test strategy

| AC | Test |
|----|------|
| AC1 | app: register with each tribe persists it; missing/unknown tribe rejected. web integration: form round-trip. |
| AC2 | no mutation path exists; app test asserts no API; (covered by code review + absence). |
| AC3 | infra (DB): pre-migration-shaped rows backfilled to Gauls — migration-boundary test per DoD. |
| AC4 | infra: loader yields 3 rosters × 10 complete units; corrupted/missing-field TOML fails fast (unit test on DTO validation). |
| AC5 | domain: prerequisite gates for Barracks/Academy/Smithy; app: order rejected when unmet. |
| AC6/AC7 | app (in-memory): research success debits + creates order with `complete_at = now + t/speed`; each rejection reason leaves state untouched. |
| AC8 | infra (DB): due research applies once; double-apply is a no-op; pending survives a fresh processor. |
| AC9 | domain: tier-1 implicitly researched (no row); app: tier-1 research order rejected as already-researched. |
| AC10/AC11 | domain + app: level/cap/smithy-level gates; success path debits and orders. |
| AC12 | infra (DB): due upgrade bumps level exactly once; restart-survival. |
| AC13 | domain: lane function; infra (DB): Roman field+building coexist, same-lane duplicate → Duplicate; non-Roman second order → Duplicate. |
| AC14 | domain: research/upgrade times at speed 1 vs 2 halve. |
| AC15 | web integration: register page offers tribes; village shows tribe; academy/smithy pages render states, actions, countdown deadline. |

## Notes

- Research/upgrade orders deliberately mirror `build_orders` (status lifecycle, claim query,
  orphan requeue) rather than generalizing into one table — same proven shape, simpler migrations;
  a unified event table can be a later refactor if a third queue appears.
- Tier-1-as-`research: None` keeps AC9 data-free: no seeded rows, no backfill for existing villages.
- Smithy effect on combat stats is **not** computed anywhere yet (009); only levels are stored.
