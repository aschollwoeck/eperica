# Protection & lifecycle — a fair start and a self-renewing world

**Status:** Current
**Date:** 2026-06-14 · **Slice:** 019

## Context
Two mechanics keep the world fair at the start and fresh over time (GDD §12.2–12.3): **beginner's
protection** shields new players from attack for a (speed-scaled) window so they aren't spawn-camped, and
the **inactivity lifecycle** turns long-abandoned accounts into farmable, then map-reclaimable, valleys.

## Design
Both are **server-authoritative** (P4) and **reproducible** from persisted state (P2/P6); the client never
decides who is protected, inactive, or abandoned. Time-based durations **scale by world speed** (P7); the
thresholds and the sweep cadence are config.

- **Pure rules (`domain/lifecycle.rs`, P3).** `LifecycleRules` plus the predicates `is_protected`,
  `protection_expiry` (speed-scaled), `is_inactive` (speed-scaled), `abandon_cutoff` (period-anchored,
  reusing `medals::period_start`), and `protection_ended_by_population`. All unit-tested, no I/O.
- **Beginner's protection.** Registration grants `protected_until = now + scaled(beginner_protection_secs)`
  (`create_account`, AC1). A player is protected while `now < protected_until`. Protection **ends early**
  (`protected_until ← now`, a one-way idempotent write) when either the player **launches an attack**
  (in `order_attack`, AC3 — you can't shelter while on the offensive) or their **population reaches the
  threshold** (`end_protection_if_established`, evaluated lazily on the authenticated view, AC4). The
  attack gate is a single timestamp compare on the hot path: `order_attack` rejects with
  `CombatError::TargetProtected` when the target owner is protected, before any movement is created (AC2).
- **Activity signal.** Each authenticated view refreshes `last_activity` via `touch_activity`, a
  **throttled conditional `UPDATE`** (rewrites only when staler than a fixed freshness window) so it costs
  at most one tiny write per window, not per request (AC5, P11).
- **Inactivity (stage 1) is derived, not stored (P1).** A player is inactive iff
  `now − last_activity > scaled(inactive_after_secs)`. The map computes this on read (`is_inactive`) and
  **greys / flags** inactive villages so active players can find farms (AC6). No stored flag, no tick;
  inactive players are attackable under the normal rules — inactivity makes them *discoverable*.
- **Abandonment (stage 2) is a state-driven recurring sweep (P1), mirroring the 017 settlement.** The
  latest swept period is the watermark `MAX(inactivity_sweeps.period)`; the scheduler tick
  (`process_due_lifecycle`) settles any complete-but-unswept period. Each period's deletion **cutoff is
  anchored to its boundary** (`period_start(P+1) − scaled(abandon_after_secs)`), so the same persisted
  activity always abandons the same set (P2/P6). `LifecycleRepository::sweep_abandoned` runs in **one
  transaction**: record the watermark (`ON CONFLICT DO NOTHING` — the claim guard), then for every live
  account past the cutoff delete its villages and flag it abandoned. Idempotent: a recorded period is a
  no-op, and already-abandoned accounts are excluded regardless (AC7).
- **Abandonment is a soft-delete (AC8).** The villages are **hard-deleted** — freeing the valley tiles for
  resettlement (the map renews) and cascading their village-scoped child rows — but the **`users` row is
  retained** and flagged `abandoned_at`. This is deliberate: many tables reference `users(id)` **without**
  `ON DELETE CASCADE` (`battle_reports`, `scout_reports`, `alliances.founder_id`, …), so hard-deleting the
  account would fail or orphan history; keeping the row preserves referential integrity and auditability
  (P6). An abandoned account **cannot log in** (`LoginError::Abandoned`) and is hidden from rankings.

## Persistence (migration 0029)
- `users` += `protected_until timestamptz NULL`, `last_activity timestamptz NOT NULL DEFAULT now()`,
  `abandoned_at timestamptz NULL`; a partial index on `last_activity` (live accounts) for the sweep scan.
- `inactivity_sweeps (world_id, period, swept_at, abandoned_count)` — PK `(world_id, period)`, the sweep
  watermark.

## Balance (P7)
- `lifecycle.toml` — `beginner_protection_secs`, `population_threshold`, `inactive_after_secs`,
  `abandon_after_secs`, `sweep_interval_secs`. Loaded fail-fast via `lifecycle_rules()`.

## Consequences
- Deleting an abandoned player's villages cascades **village-scoped** reports and any in-flight movements
  to/from them — faithful (the village is gone) and accepted; the long abandon threshold makes in-flight
  collisions rare.
- **Alliance membership of an abandoned account is left attached** in this slice (safe — the user row is
  retained, so no FK breaks; a villageless retired member is cosmetic). Founder-transfer / auto-leave on
  abandonment is deferred.
- The attack hot-path adds one indexed `protection_of` lookup; the sweep is one bounded batch per period;
  inactivity greying is a derived read — all within the P11 budget.
