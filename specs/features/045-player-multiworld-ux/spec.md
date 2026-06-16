# Feature 045 — Player multi-world UX (lobby + join + switch + read re-pointing)

**Status:** Verified
**Depends on:** 042 (player FKs + `create_player_in_world`), 043 (`GameContext` + `world` cookie +
`select_world`), 044 (game handlers on `GameContext`).
**Roadmap:** M9 — sub-program slice 4 of 4 (042 FK → 043 context → 044 handler migration → **045 lobby**).
See [ADR 0034](../../../docs/architecture/0034-multi-world-and-administration.md).
**Program note:** The capstone that makes second worlds reachable by real players. A post-login **world
lobby** lists the worlds you play and the worlds you can join; **joining** creates your player in that world
(042 primitive) and selects it; a nav **switcher** moves between your joined worlds. Finally the
cross-player **read joins** are re-pointed through `players` so a second-world player's name (and every
roster/report/leaderboard name) resolves correctly — the read half of the 042 FK switch-over.

## Problem

After 044 a logged-in player can *operate* in a selected world, but:
1. There is no UI to **join** a second world or **switch** between worlds — the `world` cookie is set only
   by tests, so non-home worlds are unreachable for real users.
2. The cross-player name reads still `JOIN users u ON u.id = <game player id>`. That works only in the home
   world (where `player.id == user.id`); for a **second-world** player (`player.id != user.id`) the join
   finds no user row, so names go missing on the map, reports, oases, alliance rosters, and leaderboards.
3. The public read pages (`leaderboard`, `wonder`, `search`, player/alliance stats) always read the **home**
   world, ignoring the selected world.

## Goal

- **AC1 — Per-row read re-pointing.** Every **per-row** repo read that resolves a **game player id →
  username** joins through `players` (`JOIN players p ON p.id = <game id> JOIN users u ON u.id =
  p.user_id`) instead of directly on `users`. Home-world behaviour is unchanged (`player.id == user.id`); a
  second-world player's name now resolves. Covers the 13 per-row name reads: map village owners,
  reinforcements (`reinforcements_at`/`_of`), battle-report attacker/defender, oasis occupants, alliance
  members + member villages, alliance invitations, forum thread/post authors, and scout-report
  scouter/target. *(The aggregate **ranking boards** + **search** — which `GROUP BY` the account and are not
  world-filtered — need world-scoping + GROUP-BY rework, a coherent change deferred to **046**.)*
- **AC2 — World lobby.** `GET /worlds` (login required) lists the account's **joined** worlds (with the
  current one marked) and the **joinable** worlds (running, not yet joined), each with speed/size. Linked
  from the nav.
- **AC3 — Join a world.** `POST /worlds/join` with a world + tribe creates the account's player in that
  world via `create_player_in_world` (042; server-authoritative — only a running, not-already-joined world),
  selects it (sets the `world` cookie), and redirects to `/village`. Re-joining is a no-op (idempotent).
- **AC4 — Switch worlds.** From the lobby, selecting a joined world posts to `POST /world/select` (043) and
  switches the active world. The nav shows the current world and links to the lobby.
- **AC5 — Behaviour preserved.** No domain change (P3). The full existing suite passes; home-world play,
  names, and the public pages are unchanged.

## Design

- **Read re-pointing (`repo.rs`).** Mechanically rewrite the affected `JOIN users u ON u.id = <game id>`
  clauses to `JOIN players p ON p.id = <game id> JOIN users u ON u.id = p.user_id`. The NPC player has a
  `players` row (042), so NPC-owned villages/oases still resolve. Account-level joins (sitters, sitter
  actions, `find_user_by_id`, protection) key on **user** ids and are left untouched.
- **Lobby (`/worlds`).** `worlds_of_user(account)` (037) → joined; `list_worlds()` minus joined and minus
  not-running → joinable. A `WorldsTemplate` renders both lists; join posts `{ world, tribe }`.
- **Join (`POST /worlds/join`).** Validate the world is running + not already joined; `create_player_in_world`
  (`RepoError::Duplicate` ⇒ treat as already-joined); add the `world` cookie; redirect `/village`.
- **Nav.** A "Worlds" link + the current world's label in the shared layout.

## Out of scope

- **Multi-world ranking boards, public read pages + search (046).** World-scoping the aggregate boards
  (`conflict_board`, population boards, alliance population aggregate, top-climbers, `player_stats`) and
  `search_players` — they `GROUP BY` the account and join battle tables that carry no `world_id`, so making
  them per-world is a GROUP-BY + village-join rework, not a per-row name swap. The player-less `WorldScope`
  extractor and the public read pages (`leaderboard`, `wonder`, `search_page`, `player_stats_page`,
  `alliance_stats_page`) move with it, since their world-correctness *is* the board world-scoping. Until 046
  the boards/public pages render the home world's data. (`profile_of` — the `/stats/player/{id}` lookup —
  also resolves `PlayerId → users.id` directly and is re-pointed/world-scoped there, not in 045; today it is
  reached only for the home/account player so no link breaks.)
- Per-world unread/notification fan-out changes, per-world chat scoping beyond what exists, and any
  gameplay-rule change. World **creation** stays admin-only (036). Deleting/leaving a world.
- A per-page dropdown switcher in every header beyond the nav link to the lobby (the lobby is the switch
  hub); a richer in-header dropdown can follow if wanted.
