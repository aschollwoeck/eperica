# Feature 046 — Multi-world ranking boards & public read pages

**Status:** Draft
**Depends on:** 045 (per-row read re-pointing, lobby/join/switch), 043 (`world` cookie + `context_for`).
**Roadmap:** M9 — follow-up to the player-multi-world sub-program (042–045).
See [ADR 0034](../../../docs/architecture/0034-multi-world-and-administration.md).
**Program note:** 045 deferred the aggregate boards + public read pages here because their correctness is a
single coherent change: world-scope **and** name-resolve the boards, then route the public read pages
through a player-less world seam so they reflect the **selected** world. The unifying insight: a game
**player id is world-specific**, so `JOIN players p ON p.id = <game id> AND p.world_id = $world`
simultaneously **world-scopes** (keeps only this world's players) and **name-resolves** (`p.user_id →
users`). No schema change, no per-query village joins.

## Problem

The aggregate ranking queries (`conflict_board`, `population_board`, the two alliance boards,
`player_stats`, `alliance_stats`, `climber_board`) and `search_players` join `users` directly on a game
player id and (for the conflict boards) carry **no** world filter. So once players join second worlds (045):
1. a second-world player (`player.id != user.id`) is **invisible** on every board (the `users` join finds no
   row), and
2. the conflict boards **merge** every world's battle data into one ranking.

The public read pages (`leaderboard`, `wonder`, `search_page`, `player_stats_page`,
`alliance_stats_page`) always read the **home** `AppState.accounts`, ignoring the selected world.

## Goal

- **AC1 — World-scoped, name-resolved boards.** `population_board`, `conflict_board`,
  `alliance_population_board`, `alliance_conflict_board`, and `climber_board` aggregate **only** the repo's
  world (`self.world_id`) and resolve names through `players → users`. Each board groups by / returns the
  **world player id** (home parity: `player.id == user.id`). NPC + abandoned accounts stay excluded.
- **AC2 — World-scoped stat pages.** `player_stats` resolves the name through `players` and treats a player
  not in `self.world_id` as **not found** (returns `None`); `alliance_stats` resolves member names through
  `players` and keys its per-member sub-aggregates on the member's player id (not `users.id`).
- **AC3 — World-scoped search.** `search_players` returns players **in the repo's world** (joining `players`
  filtered by `world_id`), resolving the username, NPC/abandoned excluded.
- **AC4 — World-aware public pages.** A player-less, login-less `WorldScope` extractor resolves the selected
  world's repo/map/speed/radius from the `world` cookie (default = home; **no redirect**, so the pages stay
  public). `leaderboard`, `wonder`, `search_page`, `player_stats_page`, `alliance_stats_page` use it, so a
  player who selected a second world sees **that** world's boards/stats/search; an anonymous visitor sees
  the home world.
- **AC5 — Behaviour preserved.** No domain change (P3). In the home world every board/stat/search/page is
  byte-for-byte unchanged (the reuse-UUID invariant). The full existing suite passes.

## Design

- **Boards (`repo.rs`).** Replace `JOIN users u ON u.id = <player_col>` with `JOIN players p ON p.id =
  <player_col> AND p.world_id = $world JOIN users u ON u.id = p.user_id`; select/group/order on the player
  column (or `p.id`); keep the existing quadrant filter on the player column; bind `self.world_id`. For the
  conflict boards this **adds** the missing world filter (they had none); for the population/climber/alliance
  boards (already partly world-scoped via village/`cur.world_id`) it adds the name resolution and confirms
  the world scope. `player_stats`' name lookup gains the `players` hop + a `p.world_id` guard;
  `alliance_stats` re-keys its `v.owner_id = u.id` / `attacker_player = u.id` / `player_id = u.id`
  sub-aggregates onto `am.player_id` and resolves the name via `players`.
- **`search_players`.** `SELECT p.id, u.username FROM players p JOIN users u ON u.id = p.user_id WHERE
  p.world_id = $world AND <prefix> AND u.abandoned_at IS NULL AND u.is_npc = false` (returns the world
  player id).
- **`WorldScope` extractor (`auth.rs`).** Like `GameContext` but player-less/login-less: read the `world`
  cookie (default `state.world_id`), `context_for` it (home fallback if not running), yield `{ accounts,
  map, speed, radius, world_id }`. The five public handlers swap `State` reads of `state.accounts`/`map`/
  `world.speed` for the `WorldScope` fields.

## Out of scope

- Per-world chat/notification scoping; gameplay-rule change; world creation (036). The medal settlement
  (017) is already world-keyed and unchanged. No new board/metric.
