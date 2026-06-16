# Feature 046 — Multi-world ranking & public pages — Tasks

**Plan:** ./plan.md · **Spec:** ./spec.md

Serial; each task a commit; gates (`fmt` / `clippy -D warnings` / `test` + P11) pass. Behaviour-preserving in
the home world — the existing suite is the regression oracle. No pure-domain task.

## Boards

- [ ] **T1 — Population + climber boards.** Re-base `population_board` and `climber_board` on `players p
  WHERE p.world_id = $world` (name via `users`, return/group `p.id`, quadrant on the player column). Home
  parity; suite green. (AC1)

- [ ] **T2 — Conflict boards.** Re-point `conflict_board` + `alliance_conflict_board` through `players p ON
  p.id = <pid> AND p.world_id = $world JOIN users`, adding the missing world filter. (AC1)

- [ ] **T3 — Alliance population board + stat pages.** Re-point `alliance_population_board`; `player_stats`
  (name via `players` + `p.world_id` not-found guard); `alliance_stats` (re-key per-member sub-aggregates
  onto `am.player_id`, name via `players`). (AC1/AC2)

- [ ] **T4 — Search.** `search_players` → players in `self.world_id` joined to `users`. (AC3)

## Public pages

- [ ] **T5 — `WorldScope` extractor + public pages.** Add the player-less/login-less extractor; migrate
  `leaderboard`, `wonder`, `search_page`, `player_stats_page`, `alliance_stats_page` to it. (AC4)

## Acceptance

- [ ] **T6 — Second-world boards + regression.** Integration: a player joined to a 2nd world selects it →
  its leaderboard shows that world's standings + the player's name; the home leaderboard is unchanged. DB
  test for a board's second-world correctness. Full suite green; spec/plan/tasks. (AC5)
