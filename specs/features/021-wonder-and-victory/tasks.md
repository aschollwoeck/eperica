# Feature 021 — Wonder of the World & victory — Tasks

**Plan:** ./plan.md · **Spec:** ./spec.md

Ordered for dependency and testability (**pure domain first**). Each task is a commit; gates
(`fmt` / `clippy -D warnings` / `test` + P11) pass before advancing. Reuses 020 (plan capture), 014
(site conquest), 003 (build queue). New surface: release + plan-gate + victory + freeze.

## Domain & balance

- [ ] **T1 — Wonder rules + curve + `BuildingKind::Wonder` (`domain/wonder.rs`, `building.rs`; P3/P7).**
  `WonderRules` (max 100, geometric cost/time, plan/site counts, garrison), `wonder_level_spec` (100-entry
  `LevelSpec`), `wonder_complete`. `BuildingKind::Wonder` + string/label/slot/population. `wonder.toml` +
  `wonder_rules()`; the construction loader merges the Wonder spec into `BuildRules`. **Unit tests:** curve
  monotonic + 100 entries; `wonder_complete`; spec loads (AC5, AC8).

## World schedule

- [ ] **T2 — Wonder schedule + victory state (`worlds` + `World`).** Migration `0033`: `worlds` +=
  `wonder_release_at`/`won_at`/`winner_alliance_id`; `villages.is_wonder_site`; `wonder_plans` table.
  `World` carries the release date; `ensure_world` sets it (config offset). **DB test:** the release date
  round-trips (AC1, AC8).

## Release

- [ ] **T3 — Wonder release + `WonderRepository` (`infrastructure`).** `release_wonder` (idempotent —
  conquerable `is_wonder_site` Natar villages with garrisons + `plan_count` plans in Natar vaults, seeded
  ring order); `plan_at_village`, `alliance_holds_plan`, `wonder_level`, `top_wonders`, `world_ended`,
  `winner`. `process_due_wonder_release` use-case. **DB tests:** nothing before the date; release
  materializes plans + conquerable sites once; re-run no-op (AC1, AC8).

## Conquest & capture

- [ ] **T4 — Wonder sites are conquerable (`Village.is_wonder_site` + the 014 guard).** Lift the 020
  unconquerable guard for sites (`is_capital || (is_natar && !is_wonder_site)`); `Village` carries
  `is_wonder_site`. **DB test:** a Wonder-site Natar is conquerable; an artifact vault is not (AC3).

- [ ] **T5 — Plan capture in the battle path.** `PlanCapture` on `BattleApply`; `resolve_attack_one`
  captures a target's plan on a won attack from a qualifying Treasury (mirrors 020); `apply_battle`
  transfers it (guarded). **DB test:** a winning Treasury attack captures a plan (AC2).

## Construction & victory

- [ ] **T6 — `order_wonder_build` (gated) + completion via the build queue.** Validate site-control +
  alliance build-right + alliance-holds-plan + level < 100; settle + enqueue a `Building{Wonder}` order;
  `process_due_builds` completes it. **DB tests:** rejected without site/plan/right; accepted with all;
  advances a level; an order at 100 rejected (AC4, AC5).

- [ ] **T7 — Victory + freeze.** `process_due_wonder_victory` (first-to-100 winner + `won_at`, idempotent);
  the `world_ended` freeze guard at the action handlers; scheduler ticks (release + victory). **Tests:** a
  Wonder at 100 records the first alliance once (a later 100 does not overwrite); a mutating action is
  rejected once won (AC6, AC7).

## Interface

- [ ] **T8 — Web: Wonder race page + winner banner.** A `/wonder` page (top alliances by Wonder level; the
  winner banner when won); a victory notice on the village view/index; `AppState`/main wire the rules +
  release date. **Integration tests:** the race page shows progress; the winner banner shows once won
  (AC9).

## Docs & acceptance

- [ ] **T9 — Technical/end-user docs + review.** rustdoc on new public items;
  `docs/architecture/0023-wonder-and-victory.md`; `docs/manual/` Wonder & victory guide; `CLAUDE.md` active
  slice → 021. Full gates + P11; `eperica-reviewer` on the slice diff; fix until **APPROVE**; PR.

## Done when

Per the [Definition of Done](../../implementation-workflow.md#definition-of-done-checklist--applies-to-every-slice):
**AC1–AC9** pass with tests, all gates green, both docs written, reviewer **APPROVE**, PR merged,
`spec.md` / `plan.md` **Verified**, roadmap updated (021 ✅) — **launch-complete**.
