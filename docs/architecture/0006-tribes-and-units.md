# Tribes, unit definitions, and the research/upgrade queues

**Status:** Current
**Date:** 2026-06-11 · **Slice:** 004

## Context
Tribe identity (GDD §5) and the unit attribute model (§6.2) underpin everything military. Slice 004
introduces both without training (005): tribe choice at registration, per-tribe unit rosters as
balance data, Academy research, Smithy upgrades, and the first tribe trait (the Roman parallel
build queue).

## Design
- **Tribe is account-level** (`users.tribe`, chosen once at registration, server-validated) and
  stamped onto each village at founding (`villages.tribe`). Pre-004 rows were backfilled to
  **Gauls** in migration 0005 (no 004-relevant trait ⇒ no retroactive advantage).
- **Unit rosters are data, not code:** `specs/balance/units.toml` defines 3 × 10 units with all
  §6.2 attributes, research costs/times, and building requirements. `UnitRules::new` validates the
  rosters at load (exactly 10 per tribe, unique ids, exactly **one research-free tier-1 unit**) —
  the loader fails fast on incomplete data.
- **Tier-1 is implicit:** a unit with `research: None` is researched-by-default everywhere — no
  seeded rows, no backfill, no special cases in storage.
- **Research and Smithy upgrades reuse the 003 order shape:** `unit_orders` rows (due-timestamped,
  P1) with a **partial unique index per queue kind** (`(village_id, kind) WHERE status='pending'`)
  — one research *and* one upgrade may run concurrently, never two of either, race-proof at the
  storage layer (P4). The scheduler claims due orders (`FOR UPDATE SKIP LOCKED`) and applies them
  idempotently (research = `INSERT … ON CONFLICT DO NOTHING`; upgrade = upsert to the absolute
  target level), with orphan requeue at startup (P2).
- **Roman parallel queue = lanes:** `build_orders.lane` (`all` | `field` | `building`) is computed
  server-side from tribe + target (`domain::queue_lane`); the 003 one-active-order index became
  `(village_id, lane) WHERE status='pending'`. Romans occupy a field and a building lane; everyone
  else occupies the single `all` lane — the same DB-level guarantee as before, generalized.
- **Smithy upgrade math is proportional:** cost = unit training cost × a per-level permille table;
  duration is a global per-level table, both ÷ world speed (P7). A unit's level is capped by the
  Smithy's building level and the table length (20). Combat effects of the level land in 009.

## Consequences
- Slice 005 (training) only needs queues per troop building plus garrison state; the unit model,
  research gating, and balance loading already exist.
- Adding a tribe or unit is a balance-data change; the domain validates shape, not contents.
- The five new `BuildingKind` variants (Barracks/Academy/Smithy now constructable; Stable/Workshop/
  Residence known for requirements, constructable in 005/013) ripple through exhaustive matches by
  design — the compiler finds every mapper.

## Links
specs/constitution.md (P1, P2, P4, P7); specs/features/004-tribes-and-units/;
specs/balance/units.toml; crates/domain/src/units.rs; crates/application/src/units.rs;
crates/infrastructure/src/repo.rs (UnitRepository); migrations/0005–0007.
