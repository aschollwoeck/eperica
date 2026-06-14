# Feature 028 — Search / who-is — Tasks

**Plan:** ./plan.md · **Spec:** ./spec.md

Pure-domain first; each task a commit; gates (`fmt` / `clippy -D warnings` / `test` + P11) pass before
advancing. Read-only over public data.

## Domain

- [x] **T1 — Coordinate parser (`domain/search.rs`; P3).** `parse_coordinate` for `x|y` / `(x|y)` / `x,y` /
  `x y`. **Unit tests:** each accepted form, whitespace, negatives; junk rejected (AC3).

## Persistence & ports

- [x] **T2 — Search queries (migration `0039`).** `AccountRepository::search_players` (username prefix,
  excl. abandoned/NPC) + `AllianceRepository::search_alliances` (name/tag prefix); `PlayerHit` /
  `AllianceHit`. Functional index `users(lower(username) text_pattern_ops)`. **DB tests:** prefix match,
  abandoned/NPC excluded, cap respected, alliance name + tag match (AC1, AC2, AC5).

## Use-cases

- [x] **T3 — Search use-case (`application/src/search.rs`).** `search(accounts, alliances, q) ->
  SearchResults { players, alliances, coordinate }`; empty query → empty; coordinate via the domain parser.
  **Tests (fakes):** empty/whitespace → empty; coordinate detected; results assembled + capped (AC3, AC4).

## Web

- [x] **T4 — Search page + nav box.** `GET /search?q=` (public — no auth); a search form in `base.html`.
  Results: players + alliances (links to stat pages) + a coordinate map link; prompt + no-results states.
  **Integration tests:** finds a player + alliance with links; coordinate query offers the map link; empty →
  prompt; no-match → "no results"; reachable without login (AC1–AC6).

## Acceptance

- [x] **T5 — Docs + review.** rustdoc on new public items; `docs/architecture/0030-search.md`;
  `docs/manual/` search note; `CLAUDE.md` active slice → 028. Full gates + P11; `eperica-reviewer` until
  **APPROVE**; PR.

## Done when

Per the [Definition of Done](../../implementation-workflow.md#definition-of-done-checklist--applies-to-every-slice):
**AC1–AC7** pass with tests, all gates green, both docs written, reviewer **APPROVE**, PR merged,
`spec.md` / `plan.md` **Verified**, roadmap note updated.
