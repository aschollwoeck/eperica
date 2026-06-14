# Wonder of the World & victory — the round capstone

**Status:** Current
**Date:** 2026-06-14 · **Slice:** 021

## Context
The end-game (GDD §11.3) closes with the **Wonder of the World**. After the artifact phase (020), a second
date releases capturable **Wonder plans** and conquerable **Wonder construction sites**; the first alliance
to raise a Wonder to level 100 **wins the round**, which then **freezes**. This slice delivers that
capstone — the M7 launch milestone. (Auto-archival + spawning a fresh world is out of scope.)

## Design
- **Reuse over new machinery.** The slice adds almost no new runtime path: plan capture reuses the 020
  defeat-and-claim mechanic, site conquest reuses the 014 conquest path, and Wonder construction reuses the
  003 build queue. The genuinely new surface is the **release**, the **construction gate**, the **victory
  record**, and the **freeze**.
- **Pure Wonder model (`domain/wonder.rs`, P3).** `MAX_WONDER_LEVEL = 100`; `wonder_complete(level)`; and
  `wonder_level_spec(rules)` generates a **100-entry `LevelSpec`** (geometric cost/time) so the Wonder is an
  ordinary `BuildingKind::Wonder` that flows through `order_build`/`process_due_builds` unchanged. The cost
  curve is bounded to fit the economy's storage cap (a maxed Warehouse/Granary holds 12 000) so every level
  stays affordable (P7).
- **Sites are conquerable; vaults are not.** A Natar **Wonder site** (`villages.is_wonder_site`) is an
  ordinary Natar village that — unlike an artifact vault — **can be conquered** (014). The guard is a single
  pure predicate `Village::is_conquerable` (`!is_capital && (!is_natar || is_wonder_site)`), used by
  `conquest_outcome`'s `unconquerable` input.
- **Plan capture is defeat-and-claim, Treasury-gated.** A **won** attack from a top (unique-tier) Treasury
  village that does not already hold a plan captures the target's Wonder plan — from a Natar vault or a
  beaten holder. `resolve_attack_one` decides the `PlanCapture`; `apply_battle` performs the guarded
  `UPDATE wonder_plans SET holder_village` **in the battle transaction** (exactly-once, P5). Mirrors 020's
  `ArtifactCapture`.
- **Release is a one-time, state-driven due check (P1).** The world carries `wonder_release_at` (config
  offset, after the artifact date). `process_due_wonder_release` (a scheduler tick) calls the idempotent
  `release_wonder`: at/after the date it ensures the synthetic Natar owner (shared with 020), then places
  `site_count` conquerable sites + `plan_count` plan vaults on **free reserved Natar tiles in seeded ring
  order** (P6), each garrisoned with a developed Main Building. Guarded by existing sites/plans, so it runs
  at most once.
- **Construction is gated (`order_wonder_build`, AC4).** Accepted only when the village is a Wonder site
  the orderer **controls** (`select_village` returns only their own), the orderer's **alliance holds ≥ 1
  plan** (`alliance_holds_plan`), and the Wonder is **below 100**. It then delegates to `order_build`, so
  cost/affordability/queue all reuse 003.
- **Victory + freeze (AC6/AC7).** `process_due_wonder_victory` (a scheduler tick) reads `top_wonders`
  (highest first); when the leader is complete it calls the guarded `record_victory`
  (`UPDATE worlds … WHERE won_at IS NULL`) — so the **first** alliance wins, exactly once, and a later
  completion cannot overwrite it. Once won, a **server-authoritative freeze guard** (web middleware)
  rejects mutating `POST`s (authentication + all reads stay available, P4). A victory banner shows on the
  `/wonder` race page and the village view.

## Persistence (migration 0033)
- `worlds.wonder_release_at` (the schedule), `worlds.won_at` + `worlds.winner_alliance_id` (the result;
  both set ⇒ the world is frozen).
- `villages.is_wonder_site` (a conquerable Natar construction site).
- `wonder_plans (id, world_id, holder_village → villages ON DELETE SET NULL, origin_x/y, released_at)` —
  one row per capturable plan; `holder_village` is the current holder (a Natar vault at release, a player's
  village once captured).

## Balance (P7)
- `wonder.toml` — the construction curve (bounded to the storage cap), plan/site counts, and the site
  garrison; `economy.toml` carries the Wonder's population curve.

## Consequences
- The capstone reuses the attack/garrison/report, conquest, and build-queue machinery — complexity is
  concentrated in release, the construction gate, victory, and the freeze, not new combat or build paths.
- The freeze is enforced at the HTTP chokepoint; reads + login remain so players can see the result.
- The Wonder cost curve is deliberately modest to fit the scaled economy's storage; raising the end-game's
  scale is a future balance/economy change (larger warehouses), not a code change.
