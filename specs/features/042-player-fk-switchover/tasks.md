# Feature 042 — Player-FK switch-over — Tasks

**Plan:** ./plan.md · **Spec:** ./spec.md

Serial; each task a commit; gates (`fmt` / `clippy -D warnings` / `test` + P11) pass before advancing.
Behaviour-preserving — the existing suite is the regression oracle. No pure-domain task.

## Infrastructure

- [x] **T1 — Placement reuse + `create_player_in_world`.** Extract `place_starting_village(tx, owner,
  tribe, template)` from `create_account` (registration tests stay green); add `create_player_in_world(user,
  tribe, template) -> PlayerId` (fresh player + starting village in this repo's world; `Duplicate` on
  re-join). **DB test:** `account_joins_a_second_world`. (AC3)

- [x] **T2 — NPC player (reuse-UUID).** In the artifact (020) + Wonder (021) release, ensure a `players`
  row for the `Natars` NPC with id = NPC user id, before placing NPC villages; own NPC villages by it. (AC2)

## Persistence

- [x] **T3 — FK re-point migration (0045).** Re-point the ~16 game columns from `users(id)` to
  `players(id)`, preserving `ON DELETE`. Fix any direct-insert test fixtures so the owner has a player
  (via the release / `create_account` / an explicit player row). (AC1)

## Acceptance

- [x] **T4 — Regression + live.** Full workspace suite passes **unchanged** (AC4). Live: restart the dev
  app; existing villages + accounts intact (the re-point holds). Spec/plan/tasks + roadmap/ADR update for
  the sub-program split (042 FK → 043 context → 044 handlers → 045 lobby).
