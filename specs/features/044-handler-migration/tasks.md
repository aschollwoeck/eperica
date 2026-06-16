# Feature 044 — Game-handler migration — Tasks

**Plan:** ./plan.md · **Spec:** ./spec.md

Serial; each task a commit; gates (`fmt` / `clippy -D warnings` / `test` + P11) pass. Behaviour-preserving —
the existing suite is the regression oracle. No pure-domain task.

## Migration (by domain area)

- [x] **T1 — Economy & construction.** Migrate `build_submit`, `academy`, `smithy`, `research_submit`,
  `smithy_upgrade_submit` to `GameContext`. Suite green. (AC1/AC2)

- [x] **T2 — Military.** Migrate `troops`, `train_submit`, `rally`, `rally_send`, `rally_return`,
  `oasis_recall`. Suite green. (AC1/AC2)

- [x] **T3 — Trade, map & reports.** Migrate `market`, `market_send`, `map` (header username →
  `ctx.account`), `reports`, `scout_report_detail`, `report_detail`. Suite green. (AC1/AC2)

- [x] **T4 — Wonder action & quests.** Migrate `wonder_build_submit`, `quests_page`. Suite green. (AC1/AC2)

- [x] **T5 — Alliance & forum.** Migrate `alliance` + `alliance_found`/`invite`/`revoke`/`respond`/`leave`/
  `disband`/`expel`/`transfer`/`role`/`diplomacy` + `forum_page`/`forum_new`/`forum_thread_page`/
  `forum_reply`. Suite green. (AC1/AC2)

## Acceptance

- [x] **T6 — Multi-world reach + regression.** Integration: a player joined to a 2nd world with the `world`
  cookie set issues a game action (a build order) → it lands in that world's village (AC3). Full suite
  green (home behaviour preserved); account surfaces unaffected (AC4). Spec/plan/tasks. (AC3/AC4)
