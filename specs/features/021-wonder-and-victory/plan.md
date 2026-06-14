# Feature 021 — Wonder of the World & victory — Technical Plan

**Status:** Verified
**Spec:** ./spec.md

The launch capstone. Reuses the slice's neighbours heavily: **plans capture via the 020 mechanic**,
**Wonder sites conquer via the 014 path**, and the **Wonder builds via the 003 queue** (a dedicated
`BuildingKind::Wonder` with a config 100-level curve). New surface is small and concentrated: a
**release** (plans + conquerable sites), a **plan-held gate** on Wonder construction, a **victory** check,
and a **world freeze**. Victory/freeze are world state; the round ends with a recorded winner.

## Constitution check

- **P1 (lazy/event-driven):** release + victory are **state-driven due checks** (now ≥ date / first-to-100,
  idempotent), not ticks; Wonder construction is the normal due-build queue.
- **P2/P6 (reproducible):** plan/site placement is seeded (Natar tiles, ring order); victory is the
  first-to-100 by a deterministic tiebreak; capture rides the 020 battle-tx transfer.
- **P3 (pure domain):** `domain/wonder.rs` holds `WonderRules`, the 100-level cost/time, the
  `wonder_max_level`, and the pure victory/gate predicates. `BuildingKind::Wonder` is domain.
- **P4 (server authority):** release, capture, conquest, construction, victory, and the freeze are
  System/server only; construction is gated (site control + plan held + level < 100) server-side.
- **P7 (config):** Wonder-release date, plan/site counts + site garrison, and the cost/time curve are
  balance/config.
- **P11:** the gate adds a couple of indexed reads on the (rare) Wonder-build path; the victory check is one
  bounded scan per tick guarded by `won_at IS NULL`.

## Domain (`domain/wonder.rs` + `building.rs`, pure)

- `struct WonderRules { max_level: u8 (100), base_cost: ResourceAmounts, cost_ratio: f64, base_time_secs:
  i64, time_ratio: f64, plan_count, site_count, garrison spec }`.
- `fn wonder_level_spec(rules) -> LevelSpec` — generate the 100-entry cost/time vectors (geometric) so the
  Wonder reuses `BuildRules`/`order_build`/`build_time_secs` unchanged.
- `fn wonder_complete(level) -> bool` (== max_level); victory tiebreak is `(completed_at, village_id)`.
- `BuildingKind::Wonder` added (string/label/slot/population + the generated construction spec).
- **Unit tests:** the curve is monotonic + 100 entries; `wonder_complete`; spec generation.

## Balance (`specs/balance/wonder.toml` + loaders)

- `wonder.toml` — `max_level`, `base_cost`/`cost_ratio`, `base_time_secs`/`time_ratio`, `plan_count`,
  `site_count`, site `garrison` (unit + base/per-index, reusing the artifact-garrison shape).
- `wonder_rules()` loader; the construction loader merges `wonder_level_spec` into `BuildRules.buildings`.

## Persistence (`infrastructure` + migration `0033`)

- `worlds` += `wonder_release_at timestamptz`, `won_at timestamptz`, `winner_alliance_id uuid REFERENCES
  alliances(id)`. `World` + `ensure_world` carry the release date (config offset, after the artifact date).
- `villages` += `is_wonder_site boolean NOT NULL DEFAULT false`.
- `wonder_plans (id text PK, world_id, holder_village uuid NULL REFERENCES villages ON DELETE SET NULL,
  origin_x, origin_y, released_at)` — capturable like artifacts.
- The Wonder's level lives in `village_buildings` (`building_type = 'wonder'`) at the site — reusing the
  build path + `process_due_builds`.
- `WonderRepository` (port + impl):
  - `release_wonder(release_at, now, rules) -> usize` — idempotent one-shot: place `site_count`
    **conquerable** Natar villages (`is_wonder_site`, with a garrison) + `plan_count` plans in Natar
    **vaults** (extra Natar villages) — on reserved Natar tiles in seeded ring order; guard by existing
    plans/sites.
  - `plan_at_village(village) -> bool`; `alliance_holds_plan(alliance) -> bool` (any member village holds a
    plan); `wonder_level(village)`; `top_wonders(limit)` (alliance + level, for the race view).
  - `declare_victory(now) -> Option<AllianceId>` — if unwon and a site's Wonder ≥ 100, set winner +
    `won_at` (first by `(completed,id)`), once.
  - `world_ended() -> bool` (`won_at IS NOT NULL`); `winner() -> Option<(AllianceId, Timestamp)>`.
- Battle apply gains a **plan transfer** (parallel to the 020 artifact transfer), guarded on the holder.

## Application

- `wonder::process_due_wonder_release(repo, release_at, now, rules)` — mirrors 020 release; idempotent.
- `wonder::order_wonder_build(accounts, builds, wonder_repo, alliance_repo, rules, …)` — validate: the
  village is a Wonder site the orderer controls, the orderer has the alliance **build right**, the
  alliance **holds a plan**, the Wonder is **< 100**; settle + enqueue a `Building{Wonder}` order via the
  existing `BuildRepository` (so `process_due_builds` completes it unchanged). New `WonderError`.
- `wonder::process_due_wonder_victory(repo, now)` — state-driven first-to-100 winner record (idempotent).
- **Conquest guard (014):** a Wonder site is conquerable — lift the 020 `is_natar` guard for
  `is_wonder_site` (`unconquerable = is_capital || (is_natar && !is_wonder_site)`).
- **Plan capture (020):** `resolve_attack_one` also checks the target for a plan; a won attack from a
  qualifying Treasury captures it (`PlanCapture` on `BattleApply`, applied in the battle tx).

## Freeze (round end)

- `world_ended()` gate: the authenticated **action handlers** (POST routes — build/train/attack/research/
  smithy/trade/settle/alliance/wonder) reject with a "the round is over" response once the world is won;
  reads stay open. The scheduler still resolves in-flight events. Centralizes the freeze at the boundary.

## Interface (`web`)

- A **`/wonder`** page: the Wonder race (top alliances by Wonder level) and, when won, the **winner banner**;
  a banner/notice also on the village view + index when the world is won.
- Scheduler: `process_due_wonder_release` + `process_due_wonder_victory` ticks; `AppState`/main wire the
  Wonder rules + release date.

## Test strategy

| AC | Test |
|----|------|
| AC1 | infra: before the date nothing; release materializes plans + conquerable sites once; re-run no-op. |
| AC2 | infra (DB): a winning attack from a Treasury captures a plan (mirrors 020 capture). |
| AC3 | infra (DB): a Wonder-site Natar village is conquerable; an artifact vault is not. |
| AC4 | infra/app (DB): a Wonder order is rejected without site control / held plan / build right; accepted with all. |
| AC5 | domain: curve to 100; infra: build advances a level; an order at 100 is rejected. |
| AC6 | infra (DB): a site Wonder at 100 records the first alliance as winner, once (a later 100 does not overwrite). |
| AC7 | web/app: once won, a mutating action is rejected; reads work. |
| AC8 | domain/infra: seeded placement + deterministic victory; config drives counts/curve. |
| AC9 | web: the Wonder race page + winner banner. |

## Notes / open risks

- **100-level curve via generation, not a 100-line table** — `wonder_level_spec` builds the `LevelSpec`
  from a geometric formula so the Wonder reuses the whole 003 build path.
- **Reuse over new paths** — plans = 020 capture, sites = 014 conquest, build = 003 queue; the genuinely
  new code is release + the plan-held gate + victory + freeze.
- **`Village` gains `is_wonder_site`** (like `is_natar`/`is_capital`); the conquest guard + build gate read
  it. Fixture churn mirrors 020.
- **Phasing (T1–T9):** T1 domain (WonderRules + curve + `BuildingKind::Wonder`) + balance; T2 world
  schedule (release/won/winner columns); T3 release (plans + conquerable sites) + `WonderRepository`; T4
  site conquest guard; T5 plan capture in the battle path; T6 `order_wonder_build` (gated) + completion via
  the build queue; T7 victory + freeze; T8 web (race page + winner banner + wiring); T9 docs + reviewer.
