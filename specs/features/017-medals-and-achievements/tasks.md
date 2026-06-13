# Feature 017 — Medals & achievements — Tasks

**Plan:** ./plan.md · **Spec:** ./spec.md

Ordered for dependency and testability (**pure domain first**). Each task is a commit; gates
(`fmt` / `clippy -D warnings` / `test` + the P11 budget) pass before advancing. The slice adds the first
**recurring** scheduled due-event, **no combat/economy math changes** (only reads, scheduled grants, and
small rewards via existing credit paths).

## Domain & balance

- [x] **T1 — Pure medal + achievement rules + balance (`domain`, P3/P7).** `domain/medals.rs`
  (`MedalCategory`, `MedalRules`, `period_index`/`boundary` real-time arithmetic, deterministic `rank_top`)
  and `domain/achievements.rs` (`PlayerProgress`, `Reward`, `AchievementKind`/`AchievementDef`, `met`,
  `newly_earned`). `medals.toml` + `achievements.toml` + fail-fast `medal_rules()` /
  `achievement_catalogue()` loaders. **Unit tests:** period/boundary math; `rank_top` ties; `met` /
  `newly_earned` truth tables (AC1, AC4, AC8); catalogue loads (AC7).

## Persistence — snapshots, medals, achievements

- [x] **T2 — Migration + persistence (`infrastructure`).** Migration `0027_medals.sql`:
  `population_snapshots` (PK `(world,player,period)`), `medals` (UNIQUE `(period,category,rank)`),
  `player_achievements` (PK `(player,achievement_id)`). `EventKind::WeeklyMedalSettlement` +
  `kind_str`/`parse_kind`. `MedalRepository` skeleton + the write/read methods: `snapshot_population`,
  `award_medals`, `medals_for`, `held_achievements`, `player_progress`, `grant_achievement` (insert +
  reward in one tx). **DB tests:** snapshot idempotent (PK); medal award idempotent (UNIQUE); a grant is
  once-per-(player,achievement) and applies its reward exactly once (AC2, AC5, AC6, AC9).

## Application — the recurring settlement

- [x] **T3 — Weekly settlement processor (`application` + Scheduler).** `process_due_medal_settlement`:
  `ensure_settlement_scheduled`; claim a due settlement; in one tx snapshot period `P`, award each
  category's top-N (attacker/defender/raider period-windowed; climber = snapshot delta P vs P−1; alliances
  via alliance boards), schedule `P+1`, mark done. Add the conflict board **upper** time bound. Wire it
  into `Scheduler.run()` (with `world_start` + `MedalRules`) and `main.rs`. **DB tests:** a due settlement
  processes P and schedules P+1; re-running P is a no-op (idempotent); first period awards no climber; a
  later period awards by delta (AC1, AC3, AC4, AC6).

## Application — achievements

- [x] **T4 — Achievement evaluation + rewards (`application`).** `evaluate_achievements(player)` — gather
  `PlayerProgress`, grant newly-earned (idempotent) with reward to the capital (capped) / CP. **DB tests:**
  each seed achievement (2nd village, N defensive wins, first oasis, population N, research-all) grants at
  the crossing and not before; reward applied once; re-evaluation grants nothing new (AC8, AC9, AC10).

- [x] **T5 — Hook wiring.** Call `evaluate_achievements` for the affected player(s) after
  `process_due_combat` (defenders + attacker), `process_due_settles` (founder), `process_due_oasis_combat`
  (occupier), and unit-research completion; plus lazily on a player's own stats view. **Tests:** an
  end-to-end resolve/settle/occupy/research triggers the matching grant (AC8/AC10); existing processor
  tests stay green.

## Application + interface — climbers, history, web

- [x] **T6 — Climbers board + population history reads.** `climber_board`/`population_history` repo reads
  and the `climbers_leaderboard` / `population_history` use-cases (latest settled period delta;
  quadrant-filtered, bounded). **DB tests:** climbers rank by latest-period delta; history returns the
  player's snapshots (AC11).

- [x] **T7 — Web (climbers category + medals/achievements/history on stat pages).** Add the **Climbers**
  leaderboard category; render **medals** + **achievements** + **population-over-time** on the player stat
  page and **medals** on the alliance stat page — public, no private state. **Integration tests:** climbers
  board renders for a visitor; a stat page shows a granted medal/achievement; no troops/resources leak
  (AC11, AC12).

## Docs & acceptance

- [x] **T8 — Technical/end-user docs.** rustdoc on new public items; `docs/architecture/0019-medals.md`
  (the recurring settlement pattern, snapshots, idempotent grants); `docs/manual/` medals & achievements
  guide; `CLAUDE.md` active slice → 017.

- [ ] **T9 — Review & accept.** Full gates + P11; `eperica-reviewer` on the slice diff; fix until
  **APPROVE**; PR.

## Done when

Per the [Definition of Done](../../implementation-workflow.md#definition-of-done-checklist--applies-to-every-slice):
**AC1–AC13** pass with tests, all gates green, both docs written, reviewer **APPROVE**, PR merged,
`spec.md` / `plan.md` **Verified**, roadmap updated (017 ✅).
