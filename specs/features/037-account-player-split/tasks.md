# Feature 037 — Account ↔ Player split — Tasks

**Plan:** ./plan.md · **Spec:** ./spec.md

Each task a commit; gates (`fmt` / `clippy -D warnings` / `test` + P11) pass before advancing. Additive
only — the existing suite is the regression oracle (must pass unchanged). No pure-domain task.

## Persistence & ports

- [x] **T1 — `players` table + backfill (migration 0043).** Create `players` (one per `(user, world)`,
  `id = user_id` in the single world) + index; backfill from `users × the world` idempotently. (AC1/AC2)
- [x] **T2 — Resolution ports + `PlayerWorld` view.** `AccountRepository::player_in_world` +
  `worlds_of_user` (default empty). `PlayerWorld { player, world, tribe }`. (AC4)

## Infrastructure

- [x] **T3 — Repo: player creation + resolution.** Extend `create_account` to insert the player row in the
  same tx (`id = user_id`, `self.world_id`, tribe). Implement `player_in_world` / `worlds_of_user`. **DB
  tests:** registration creates exactly one player with `id = owner_id`; backfill gives one player per user;
  `player_in_world` resolves it; `worlds_of_user` lists the one world. (AC2/AC3/AC4)

## Acceptance

- [x] **T4 — Regression + invariant.** Full workspace suite passes **unchanged** (behaviour preserved,
  AC5). DB test asserts `players.id = villages.owner_id` for a freshly-registered account (the reuse-UUID
  invariant). Spec/plan/tasks + ADR/roadmap cross-refs.
