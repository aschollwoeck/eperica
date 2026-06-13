# Medals & achievements — the prestige layer

**Status:** Current
**Date:** 2026-06-13 · **Slice:** 017

## Context
The prestige layer (GDD §11.2): a **weekly medal settlement** awards permanent **medals** to the top
performers of each category, and **achievements** are one-time milestone badges (with optional rewards)
granted as players cross thresholds. This slice also **closes the 016 deferral**: the population
**snapshot** it introduces powers the **top-climbers leaderboard** and the **population-over-time** chart.

## Design
- **The settlement is state-driven, not a scheduled-events row.** The generic `process_due` claims *all*
  `scheduled_events` and dispatches by kind with no repo access; the settlement needs repos, so a
  settlement row there would race the generic claimer. Instead the **latest settled period is derived
  from `MAX(population_snapshots.period)`**, and the scheduler tick (`process_due_medal_settlement`)
  settles any complete-but-unsettled period. This is the same observable behavior as a self-rescheduling
  due-event (fires at each boundary, one period at a time, self-advancing, no entity ticking) with no
  double-claim race and natural crash-catch-up. It is the template for future periodic work.
- **Real-time period (the decided faithful exception, P7).** `period_secs` (config, default 7 days) is
  applied in **real time** — `period_index`/`period_start` do not scale by world speed. Faithful to
  Travian's wall-clock weekly medals; world speed still scales everything the medals are *awarded from*.
- **Atomic, idempotent settlement (P1/P2).** Settling period `P` happens in **one transaction**
  (`MedalRepository::settle_period`): write the snapshot (PK `(world, player, period)`), compute the
  **climber** medals from that just-written snapshot, and award all medals (UNIQUE `(period, category,
  rank)`) — commit together. This is essential: the watermark is `MAX(population_snapshots.period)`, so
  the snapshot must not commit without the medals, or a failure between them would advance the watermark
  and lose `P`'s medals forever. All inserts are `ON CONFLICT DO NOTHING`, so a re-settle of `P` is a
  no-op. The non-climber boards (attacker/defender/raider/alliance) are read before the transaction with
  a **period-bounded** window `[period_start(P), period_start(P+1))` (the 016 `conflict_board` gained an
  `until` bound) so awards are reproducible even on late catch-up.
- **Medal categories.** Attacker / defender / raider (valued battle facts over the period), climber
  (snapshot delta `P − (P−1)`; the first period only sets the baseline), and the three alliance
  aggregates. Medals are **permanent** facts (`medals` row, polymorphic player/alliance subject), never
  revoked or recomputed.
- **Pure rules (`domain/medals.rs`, `domain/achievements.rs`, P3).** `MedalCategory`, `period_index`/
  `period_start`, `rank_top`; and the achievement catalogue model — `PlayerProgress`, `AchievementKind`,
  `AchievementDef` (predicate + optional `Reward`), `met`, `newly_earned`. All unit-tested without I/O.
- **Achievements: catalogue + idempotent evaluator + reward.** The catalogue is balance data
  (`achievements.toml`). `evaluate_achievements` gathers a player's `PlayerProgress` (village count,
  defensive wins, oases held, population, units researched; the tribe roster size for "research all"
  comes from `unit_rules`), then grants any newly-earned, not-yet-held entries. `grant_achievement`
  inserts the `player_achievements` row (PK `(player, achievement)` = the exactly-once guard) **and**
  applies the reward in the **same transaction** — culture points added, or resources credited to the
  player's capital capped at its storage (`deposit_capped`).
- **Hook: lazy on the authenticated village view.** Rather than threading the evaluator through every
  resolution path, the village handler evaluates the logged-in player's achievements on load — server
  authoritative, idempotent, and covering every milestone within one village load of crossing it (the
  spec's sanctioned lazy-on-view). The hot combat/settle paths are untouched.
- **016 deferral closed.** `climber_board` (snapshot delta, positive gainers, quadrant-scoped) backs the
  **Top climbers** leaderboard category; `population_history` backs the population-over-time stat-page
  series.

## Persistence (migration 0027)
- `population_snapshots (world, player, period, population, taken_at)` — PK `(world, player, period)`.
- `medals (id, period, category, rank, subject_kind, subject_id, awarded_at)` — UNIQUE `(period,
  category, rank)`.
- `player_achievements (player, achievement_id, granted_at)` — PK `(player, achievement_id)`.
- `World` gained `created_at` (the real-time period anchor; `ensure_world` returns it).

## Balance (P7)
- `medals.toml` — `period_secs`, `medals_per_category`, the active `categories`.
- `achievements.toml` — the catalogue (id, kind, threshold, optional culture/resource reward).

## Consequences
- The settlement runs O(players) once per real week — a bulk snapshot insert plus a handful of bounded
  top-N reads; negligible amortized.
- A player who crosses a milestone while offline is granted it (and its reward) the next time they load
  their village — still server-authoritative and exactly-once, just next-view rather than instant.
- Medals/achievements/climbers/history are **public** and leak no private state (P4/§7.3).
