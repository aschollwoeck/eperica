# Feature 017 — Medals & achievements — Technical Plan

**Status:** Reviewed
**Spec:** ./spec.md

The **prestige layer**: a **weekly medal settlement** (the first recurring scheduled due-event) snapshots
population, awards permanent **medals** per category, and re-schedules itself; **achievements** are
one-time milestone badges granted idempotently from persisted state with optional rewards; and the
016-deferred **top-climbers leaderboard** + **population-over-time** chart land on the new snapshots.
Pure rules (catalogue predicates, climber math, period arithmetic) in the domain; grants are
server-authoritative and reproducible. **No combat/economy math changes** — only reads, scheduled
grants, and small rewards via existing credit paths.

## Constitution check

- **P1 (event-driven / lazy):** the settlement is a **single scheduled due-event per period** that
  re-schedules the next one — no entity ticking. The per-tick cost in the scheduler loop is one cheap
  "is a settlement due?" claim; real work happens only at a boundary. Achievements are evaluated at the
  **natural event hooks** (battle/settle/oasis/research resolution) — no sweep, no polling.
- **P2 / P6 (reproducible):** snapshots + medals are written **once per period** under per-period
  uniqueness; achievement grants are unique per `(player, achievement)`. Everything is computed from
  persisted facts (battle rows, build state, snapshots) with deterministic tie-breaks, so the same history
  yields the same medals/badges. The settlement claim makes a crash-resume re-run a no-op.
- **P3 (pure domain):** `domain/medals.rs` (the `MedalCategory` set, `period_index`/`boundary` arithmetic,
  the deterministic ranker) and `domain/achievements.rs` (`AchievementId`, `AchievementDef` with a
  predicate over a pure `PlayerProgress` value + optional `Reward`, and `evaluate(progress, catalogue)`)
  are pure and unit-tested without I/O. The climber metric is a pure subtraction of two snapshots.
- **P4 (server authority):** medals/achievements/rewards are produced only by the System at settlement /
  at hooks; no client path can self-award. Reward credit reuses the guarded economy paths.
- **P7 (configurable):** `medals.toml` (period seconds, `medals_per_category`, categories, tie-break) and
  `achievements.toml` (the catalogue + rewards) are balance; the period is **real-time** (not
  speed-scaled — the decided exception, documented in the spec).
- **P11 (performance):** the settlement is O(players) once per real week (a bulk snapshot insert + a
  handful of bounded top-N board reads) — amortized negligible. The climbers board and population history
  are bounded, indexed snapshot queries (mirroring 016's boards). Achievement evaluation at a hook is a
  few indexed counts for one player.

## Domain (`domain`, pure)

- `medals.rs` (new):
  - `enum MedalCategory { Attacker, Defender, Raider, Climber, AllianceAttacker, AllianceDefender,
    AlliancePopulation }` (+ `as_str`/parse for persistence).
  - `MedalRules { period_secs: i64, per_category: usize, /* tie-break is value desc, id asc */ }`.
  - `fn period_index(now, world_start, period_secs) -> i64` and `fn boundary(period, world_start,
    period_secs) -> Timestamp` — pure real-time arithmetic (no speed).
  - `fn rank_top(rows: &[(Id, i64)], n) -> Vec<(rank, Id)>` — deterministic top-N (value desc, id asc).
    (Boards already arrive ordered from SQL; this assigns ranks + truncates.)
- `achievements.rs` (new):
  - `struct PlayerProgress { village_count, defensive_wins, oases_held, population, units_researched,
    tribe_unit_count }`.
  - `enum Reward { Resources(ResourceAmounts), Culture(i64), None }`.
  - `struct AchievementDef { id: AchievementId, kind: AchievementKind, threshold: i64, reward: Reward }`
    where `AchievementKind { SecondVillage, DefensiveWins, FirstOasis, Population, ResearchAllUnits }`.
  - `fn met(def, progress) -> bool` (e.g. `village_count >= 2`, `defensive_wins >= threshold`,
    `oases_held >= 1`, `population >= threshold`, `units_researched >= tribe_unit_count`).
  - `fn newly_earned(progress, catalogue, held: &HashSet<AchievementId>) -> Vec<&AchievementDef>`.
  All unit-tested with no I/O.

## Balance (`specs/balance/` + `infrastructure::balance`)

- `medals.toml` — `period_secs = 604800` (7 days), `medals_per_category = 3`, the active category list.
- `achievements.toml` — the seed catalogue: `second_village` (reward: CP), `defensive_wins` (threshold
  100), `first_oasis`, `population` (threshold e.g. 1000, reward: resources), `research_all_units`. Loaded
  fail-fast into `MedalRules` + `Vec<AchievementDef>` by `medal_rules()` / `achievement_catalogue()`.

## Persistence (`infrastructure` + migration `0027_medals.sql`)

- `population_snapshots (world_id, player_id, period bigint, population bigint, taken_at timestamptz,
  PRIMARY KEY (world_id, player_id, period))` — one row per player per settled period; the PK makes the
  snapshot idempotent. Index `(world_id, period)` for the climbers board.
- `medals (id uuid pk, period bigint, category text, rank int, subject_kind text /* player|alliance */,
  subject_id uuid, awarded_at timestamptz default now(), UNIQUE (period, category, rank))` — the
  per-period uniqueness makes the award idempotent. Index `(subject_kind, subject_id)` for a subject's
  medal list.
- `player_achievements (player_id uuid, achievement_id text, granted_at timestamptz default now(),
  PRIMARY KEY (player_id, achievement_id))` — the PK is the exactly-once guard.
- **The settlement is state-driven, not a `scheduled_events` row (refinement).** The generic `process_due`
  claims *all* `scheduled_events` and dispatches by kind with no repo access — but the settlement needs
  repos — so a settlement row there would race the generic claimer. Instead the **latest settled period is
  derived from `MAX(period)` in `population_snapshots`**; the scheduler tick settles any complete,
  unsettled period (its boundary has passed). This is the same observable behavior as the spec's AC1
  (fires at each boundary, one period at a time, self-advancing, idempotent, no entity ticking), with no
  double-claim race and natural crash-catch-up. No `EventKind` variant is added.
- `MedalRepository` (port + `PgAccountRepository` impl): `latest_settled_period() -> Option<i64>`,
  `snapshot_population(period)` (bulk insert via the 016 population SQL, `ON CONFLICT DO NOTHING`),
  `award_medals(period, rows)` (`ON CONFLICT DO NOTHING`), `medals_for(subject_kind, subject_id)`,
  `climber_board(period, prev, scope, limit)` (snapshot delta), `population_history(player)`, and the
  achievement side (T4): `player_progress(player)` (the counts), `held_achievements(player)`,
  `grant_achievement(player, def)` (insert + apply reward in one tx, `ON CONFLICT DO NOTHING`).
- Extend the 016 conflict board with an **upper** time bound so the settlement reads each category over
  `[boundary(P−1), boundary(P))` reproducibly (`conflict_board_window(metric, scope, since, until, n)`;
  the existing `conflict_board` becomes `until = None`).

## Application (`application`)

- `process_due_medal_settlement(accounts, ranking, medals, medal_rules, world_start, now)` — the recurring
  processor the Scheduler calls each tick: derive the latest settled period; while the next period's
  boundary has passed, in one transaction **snapshot** population for period `P` and **award** each
  category's top-N (attacker/defender/raider via the period-windowed board; climber via the snapshot
  delta P vs P−1; alliances via the alliance boards), **schedule** `P+1`, and mark the event done.
  Idempotent via the claim + per-period uniqueness.
- `evaluate_achievements(accounts, medals, catalogue, player)` — gather `PlayerProgress`, diff against
  `held_achievements`, `grant_achievement` (with reward) for each newly-earned. Cheap + idempotent.
  **Hook points:** call it for the affected player(s) after `process_due_combat` (defenders + attacker),
  `process_due_settles` (founder), `process_due_oasis_combat` (occupier), and unit-order completion
  (researcher); plus lazily when a player views their own stats.
- `ranking.rs` — add `climbers_leaderboard(...)` (latest settled period delta) and `population_history(...)`
  use-cases; extend the conflict use-cases to pass an `until` where the settlement needs it.

## Interface (`web`)

- **Leaderboard** — add the **Climbers** category (latest-period population gained), reusing the 016
  page + quadrant filter.
- **Player stats page** — a **Medals** list, an **Achievements** list (earned, with any reward), and a
  **population-over-time** series (sparkline/table from snapshots).
- **Alliance stats page** — the alliance's **medals**.
- All public (Visitor-readable), exposing no private state (P4/§7.3).
- The Scheduler (`event_store.rs`) gains `world_start` + `MedalRules` and calls
  `process_due_medal_settlement` in its tick (next to `process_due_combat`); `main.rs` + `AppState`/tests
  wire `medal_rules` (and the achievement catalogue where hooks live).

## Test strategy

| AC | Test |
|----|------|
| AC1 | domain: `period_index`/`boundary` arithmetic; infra/app: a due settlement processes period P and schedules P+1; only one pending. |
| AC2 | infra (DB): settling P writes one snapshot per player; re-running P inserts none (PK/ON CONFLICT). |
| AC3 | app/infra (DB): top-`per_category` medals per category with rank + tie-break; empty category awards none. |
| AC4 | domain + infra: climber = snapshot delta; first period awards no climber; delta ranking correct. |
| AC5 | infra: a medal persists; a later settlement does not revoke/duplicate it; alliance medal recorded against the alliance. |
| AC6 | infra (DB): processing P twice yields identical medals + snapshots (idempotent). |
| AC7 | balance: `achievements.toml` loads into the catalogue with predicates + rewards. |
| AC8 | domain: `met`/`newly_earned` truth tables; infra (DB): grant is once-per-(player,achievement). |
| AC9 | infra (DB): a reward achievement credits the capital (capped) / adds CP exactly once with the grant. |
| AC10 | infra (DB): 2nd village, N defensive wins, first oasis, population N, research-all each grant at the crossing, not before. |
| AC11 | infra (DB): climbers board ranks by latest-period delta (quadrant-filtered, bounded); population_history returns the player's snapshots. web: climbers category renders. |
| AC12 | web: medals/achievements/climbers/history are visitor-readable and leak no private state. |
| AC13 | infra: determinism (re-run same history → same medals/badges); config drives period/counts/catalogue. |

## Notes / open risks

- **Recurring event is new.** The process-then-reschedule pattern + the startup `ensure_settlement_
  scheduled` self-heal are the template for future periodic work; verify exactly-once under a crash
  between award and `mark_done` (the claim + per-period uniqueness cover it).
- **Settlement snapshots "now", not exactly the boundary.** If the scheduler was down past a boundary, the
  snapshot is taken when it catches up; population is current-value so the lag is bounded and reproducible
  enough. Catch-up settles missed periods oldest-first, one per claim.
- **Achievement hook coverage.** Each milestone must be evaluated at the hook where its progress changes;
  missing a hook delays (not breaks) a grant — the lazy on-view evaluation is the backstop. Keep
  `evaluate_achievements` cheap (indexed counts) since it runs on hot resolution paths.
- **Climber/new-player baseline** and **catalogue-change backfill** per the spec Open questions
  (proposed: growth-from-zero; grant on next action / lazy on view).
- **Phasing (T1–T9):** T1 pure domain (medals + achievements + period math) + balance; T2 migration +
  EventKind + snapshot/medal persistence; T3 the recurring settlement processor (snapshot + medals +
  reschedule, idempotent) wired into the Scheduler; T4 achievements persistence + `evaluate_achievements`
  + reward application; T5 achievement hook wiring across the processors; T6 climbers board +
  population-history reads (+ conflict upper-bound); T7 web (climbers category, medals/achievements/history
  on stat pages); T8 docs; T9 reviewer.
