# Feature 046 — Multi-world ranking & public pages — Plan

**Spec:** ./spec.md · **Program design:** [ADR 0034](../../../docs/architecture/0034-multi-world-and-administration.md)

## Approach

Rewrite the board/stat/search reads to the uniform `players`-join pattern (world-scope + name-resolve in one
hop), then add the player-less `WorldScope` extractor and route the five public pages through it. Each stage
is behaviour-preserving in the home world (the existing suite is the oracle); a second-world test proves the
new path. No domain change (P3).

## Stages (each a commit; suite green before advancing)

1. **Population + climber boards (`repo.rs`).** Re-base `population_board` and `climber_board` on `players p
   WHERE p.world_id = $world` (join users for the name, group/return `p.id`), quadrant filter on the player
   column. (AC1)
2. **Conflict boards (`repo.rs`).** Re-point `conflict_board` + `alliance_conflict_board` through `players p
   ON p.id = <pid> AND p.world_id = $world JOIN users`, adding the previously-missing world filter; group on
   the player / alliance as before. (AC1)
3. **Alliance population board + stat pages (`repo.rs`).** Re-point `alliance_population_board`,
   `player_stats` (name via `players` + `p.world_id` not-found guard), `alliance_stats` (re-key the
   per-member sub-aggregates onto `am.player_id`, name via `players`). (AC1/AC2)
4. **Search (`repo.rs`).** `search_players` → players in `self.world_id` joined to users. (AC3)
5. **`WorldScope` extractor + public pages (`auth.rs`, `handlers.rs`).** Add the extractor; migrate
   `leaderboard`, `wonder`, `search_page`, `player_stats_page`, `alliance_stats_page`. (AC4)
6. **Acceptance.** Second-world integration: a player joined to a 2nd world selects it → its leaderboard
   shows that world's standings + the player's name; the home leaderboard is unchanged. Full suite green;
   spec/plan/tasks. (AC5)

## Key decisions

- **`p.world_id` is the world filter.** Because a player id is world-specific, the `players` join with
  `world_id = $world` both scopes and resolves — no battle-table `world_id` column (no migration) and no
  per-query village join. The conflict boards gain the world filter they never had.
- **Boards return the player id, not the user id.** Home parity holds (`p.id == u.id`); in a second world
  the leaderboard's `/stats/player/{id}` links carry the world player id, which `player_stats` resolves
  under the same selected-world repo. Internally consistent.
- **`WorldScope` stays public.** The read pages must not start requiring login; the extractor defaults to
  home and never redirects.

## Risk

- Bind-index churn (the conflict boards gain a `world_id` bind) — caught by compile + the full suite. Each
  rewrite is verified home-parity by the existing board tests and a new second-world assertion.
- Per-request cost: the `players` hop is a single PK/`(user_id,world_id)`-indexed join on top-N rows — within
  P11; the boards are not hot-path.
