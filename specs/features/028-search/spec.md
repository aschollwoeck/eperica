# Feature 028 — Search / who-is

**Status:** Reviewed
**Depends on:** 016 (public player + alliance stat pages the results link to), 006 (the map + coordinates), 001 (auth/sessions)
**Roadmap:** app-layer social/meta (`social-and-meta-features.md` §Presentation & profiles → "Search / who-is").

## Goal

Let anyone find their way to a **player**, an **alliance**, or a **map tile** from one search box: type a
name, a tag, or a coordinate and jump to the right public page. Read-only over already-public data (names,
tags, coordinates — never private state, P4).

## Concepts

- **Query.** A short free-text string. It is matched, case-insensitively, against:
  - **Player usernames** (prefix match) → links to the public stat page (`/stats/player/{id}`).
  - **Alliance names and tags** (prefix match) → links to the public stat page (`/stats/alliance/{id}`).
  - A **coordinate**, when the query parses as one (`x|y`, `(x|y)`, `x,y`, or `x y`) → a jump to that map
    tile (`/map?x=&y=`).

- **Public only.** Results expose only public identity: player names, alliance names/tags, and the map
  coordinate. No troops, resources, email, or any private field (P4/§7.3). Abandoned/NPC accounts are
  excluded from player results (consistent with the leaderboard).

- **Bounded.** Each result kind is capped at a small limit (P11); the search is a cheap, index-backed read.

## Acceptance criteria

- **AC1 — Find players.** A query that is a case-insensitive **prefix** of one or more usernames lists
  those players (capped), each linking to their public stat page. Abandoned/NPC accounts are excluded.

- **AC2 — Find alliances.** A query that is a case-insensitive prefix of an alliance **name** or **tag**
  lists those alliances (capped), each linking to their public stat page.

- **AC3 — Coordinate jump.** A query that parses as a coordinate (`x|y`, `(x|y)`, `x,y`, `x y`) offers a
  direct link to that map tile. Parsing is pure + reproducible.

- **AC4 — Empty / no match.** An empty (or whitespace-only) query prompts for input; a non-empty query with
  no matches shows a clear "no results" state. Neither errors.

- **AC5 — Public read.** Search returns only public fields (names, tags, coordinate). It never exposes
  private state, and it is server-authoritative — the result set is computed on the server (P4).

- **AC6 — Roles.** Per [roles.md](../../roles.md): search + the pages it links to are public reads.

| Role | Permitted | Denied (server-enforced) |
|------|-----------|--------------------------|
| **Visitor** | Search; follow links to public stat pages / the map. | See any private state via search. |
| **Player** | (as Visitor). | — |
| **Moderator/Administrator** | (as Visitor) + their elevated tools elsewhere. | — |

- **AC7 — Reproducibility & config.** Matching is deterministic from persisted rows; result caps are
  bounded constants (P11). The coordinate parser is pure (P3).

## Out of scope

- Fuzzy / typo-tolerant / full-text ranking, pagination, and "search as you type" (autocomplete) — a simple
  prefix match + a results page this slice.
- Searching villages by name (villages have no name in this game), messages, forum posts, or reports.
- Filtering by quadrant / online status, and saved searches.
