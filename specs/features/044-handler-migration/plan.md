# Feature 044 — Game-handler migration — Plan

**Spec:** ./spec.md · **Program design:** [ADR 0034](../../../docs/architecture/0034-multi-world-and-administration.md)

## Approach

A uniform, mechanical extractor swap repeated across the in-scope handlers, batched by domain area so the
suite (the regression oracle) stays green at every commit. No domain change (P3); home-world parity is the
invariant. The transformation is identical for every handler (per the spec Design), so the risk is
coverage and the occasional account-level read, not logic.

## Stages (each a commit; suite green before advancing)

1. **Economy & construction.** `build_submit`, `academy`, `smithy`, `research_submit`,
   `smithy_upgrade_submit`. (AC1/AC2)
2. **Military.** `troops`, `train_submit`, `rally`, `rally_send`, `rally_return`, `oasis_recall`. (AC1/AC2)
3. **Trade, map & reports.** `market`, `market_send`, `map` (header username → `ctx.account`), `reports`,
   `scout_report_detail`, `report_detail`. (AC1/AC2)
4. **Wonder action & quests.** `wonder_build_submit`, `quests_page`. (AC1/AC2)
5. **Alliance & forum.** `alliance` + the ten `alliance_*` actions + `forum_page`/`forum_new`/
   `forum_thread_page`/`forum_reply`. (AC1/AC2)
6. **Multi-world reach + regression.** Integration test: a player joined to a 2nd world + the `world`
   cookie set → a build order (or equivalent) lands in that world's village. Full suite green. Spec/plan/
   tasks. (AC3/AC4)

## Key decisions

- **`GameContext` everywhere a handler acts as a player; `AuthUser`/`RealUser` stay on account surfaces.**
  The split is exactly the spec's in-scope vs out-of-scope lists; moderation/admin/sitting/messaging/
  notifications/profile/settings/fair-play are the human and keep their extractors.
- **Cross-player read pages wait for 045.** `leaderboard`/`wonder`-view/`search`/stat pages render other
  players via `owner→users` joins; world-scoping them without the join re-pointing would be half a change.
  They move as one slice with the repo fix.
- **`map`'s username is the only account-level read in scope** — it uses `ctx.account` (the human), while
  the map viewport + the player's own villages key on `ctx.player`/`ctx.map`.

## Risk

- Per-request repo build cost is unchanged from 043 (one `player_in_world` lookup + generate-on-read map)
  — within P11. The migration adds no new query on any hot path; it re-points existing ones to `ctx`.
- Mechanical-edit fallout (a missed `state.*` or an account-vs-player slip) is caught by `clippy -D
  warnings` (type/borrow mismatches: `Arc` vs value repo) and the full suite at each stage.
