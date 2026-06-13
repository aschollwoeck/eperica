# Feature 016 — Ranking, leaderboards & statistics — Tasks

**Plan:** ./plan.md · **Spec:** ./spec.md

Ordered for dependency and testability (**pure domain first**). Each task is a commit; gates
(`fmt` / `clippy -D warnings` / `test` + the P11 latency budget) pass before advancing. The slice is
**read-side ranking** plus a contained **combat-report amendment** (per-defender reports + proportional
defense points). **No combat *math* changes** — only reporting, three persisted scalars, and a
per-defender row; **no ranking tick** (points are written inside the existing 009 battle due-event).

## Domain & balance

- [x] **T1 — Pure ranking rules + quadrant + balance (`domain`, P3/P7).** Add `domain::world::{Quadrant,
  quadrant(Coordinate)}` (sign rule incl. axis/origin ties). New `domain/ranking.rs`: `RankingRules
  { point_value, windows, page_size }`, `battle_value(losses, rules)` (Σ count × point_value),
  `apportion(total, weights) -> Vec<i64>` (deterministic **largest-remainder** split, sum-preserving).
  Add `point_value` to `UnitRule` + the unit DTO + `parse_unit_rules` and `point_value` to every
  `units.toml` entry (default ≈ crop upkeep, P7). New `ranking.toml` (`windows_days`,
  `leaderboard_page_size`) + fail-fast `ranking_rules()`. **Unit tests:** `quadrant` truth table;
  `battle_value`; `apportion` (3:1→75/25, all-zero weights, rounding edges so shares sum to total)
  (**AC4**, **AC7**).

## Persistence — battle facts

- [x] **T2 — Migration + per-defender persistence (`infrastructure`).** Migration `0026_ranking.sql`:
  `battle_reports.attack_points bigint not null default 0` and the new `battle_defenders` table
  (`battle_id`→`battle_reports` cascade, `player_id`, `village_id`, `is_owner`, `forces`, `losses`,
  `defense_value`, `defense_points`, `occurred_at`) with indexes `(player_id, occurred_at desc)` and
  `(battle_id)`. Extend `apply_battle` (and `BattleApply`/`ports`) to set `attack_points` and insert one
  `battle_defenders` row per `DefenderContribution` **in the same exactly-once transaction** (keyed by
  `movement_id`); oasis/PvE battles write none (`attack_points = 0`). **DB tests:** rows persist; a
  crash-resume re-run does **not** duplicate them (**AC3**, **AC11**).

## Application — combat amendment

- [x] **T3 — Per-defender contributions + points in `resolve_one` (`application/combat.rs`).** Build
  `defender_contributions`: the owner (garrison forces/losses, `is_owner=true`) + one per reinforcement
  group (forces from `reinforcements`, losses from the existing `reinforcement_losses`, owner from the
  group's home village, `defense_value` from a per-group `add_defense`). Compute `attack_points =
  battle_value(defender_losses_total)` and split `battle_value(attacker_losses)` via `apportion` across
  the contributions; thread through `BattleApply`. **Battle outcome, casualties, loot, returns, conquest
  are untouched.** **DB tests:** owner + 2 reinforcers → 3 contribution rows with correct forces/losses;
  `attack_points` = valued defender losses; defenders' `defense_points` sum to valued attacker losses;
  existing 007/009/011/014 combat tests stay green (**AC3**, **AC4**).

## Application — leaderboards & stats

- [x] **T4 — Ranking reads: population + attacker/defender/raider boards (`RankingRepository`).** New
  port + `PgRankingRepository`: `population_board`, `attack_board`, `defense_board`, `raider_board`
  (window + quadrant scope, bounded page). Population summed from each player's villages via
  `population()` (quadrant by capital coord); points/loot via indexed `SUM … WHERE occurred_at ≥ $win …
  ORDER BY … LIMIT`. Ranking use-cases validate scope/window/page against `RankingRules` (P4/P7). **DB
  tests:** population order + (pop,id) tie-break + page bound; conquered village counts for new owner;
  attacker/defender/raider rank by summed window facts; zero-activity players omitted; quadrant scoping
  filters correctly; all-time vs windowed (**AC1**, **AC2**, **AC5**, **AC6**, **AC7**).

- [x] **T5 — Alliance boards, stat pages & reinforcer inbox.** `alliance_boards` (aggregate current
  members' population / attack / defense points), `player_stats`, `alliance_stats`, and
  `defender_reports_for(player)` (the reinforcer inbox over `battle_defenders`). **DB tests:** alliance
  aggregates over current membership; memberless player contributes to none; disbanded alliance absent;
  stat pages expose pop/points/loot/history and **never** troop counts or stored resources; a reinforcer
  reads **only their own** contribution for a battle at a village they don't own (**AC8**, **AC9**,
  **AC10**, **AC11**, **AC12**).

## Interface — web

- [x] **T6 — Leaderboard + statistics pages + inbox surfacing (`web`).** Public **leaderboard** page
  (category tabs: Population / Attackers / Defenders / Raiders / Alliances; quadrant + window selectors;
  ranked paged table), public **player/alliance statistics** pages, and the reinforcer's `battle_defenders`
  reports surfaced in the existing reports inbox — all obeying the ui-style-guide; scope/window/page
  parsed + bounded server-side; no private state rendered. **Integration tests:** boards render and rank
  without auth (Visitor); a stats page hides troops/resources; a reinforcer sees their own report; latency
  within budget on a populated fixture (**AC2**, **AC9**, **AC10**, **AC11**, **AC12**).

## Docs & acceptance

- [x] **T7 — Technical/end-user docs.** rustdoc on the new public items; `docs/architecture/0018-ranking.md`
  (the read-side derivation + the points-as-persisted-fact decision + the per-defender report amendment);
  `docs/manual/` ranking & statistics guide; `CLAUDE.md` active slice → 016.

- [x] **T8 — Review & accept.** Full gates + P11; `eperica-reviewer` on the slice diff; fix until
  **APPROVE**; PR.

## Done when

Per the [Definition of Done](../../implementation-workflow.md#definition-of-done-checklist--applies-to-every-slice):
**AC1–AC12** pass with tests, all gates green, both docs written, reviewer **APPROVE**, PR merged,
`spec.md` / `plan.md` **Verified**, roadmap updated (016 ✅).
