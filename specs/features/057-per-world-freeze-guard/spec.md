# Feature 057 — per-world freeze guard

**Status:** Verified
**Amends:** 021 (Wonder victory freeze) under the 056 multi-world URL routing.

**Note:** The POST freeze guard (`action_guard`) checks only the **home** world's win/freeze state, so a
mutating action in a *non-home* world is not blocked even after that world has been won and frozen. Under the
056 `/w/{world}/…` routing the request's target world is in the URL, so the guard can — and must — check
**that** world. No domain change (P3).

## Problem

`action_guard` calls `state.accounts.world_ended()`, where `state.accounts` is the home-world repo. After a
non-home world is won (021), its game-action POSTs (`/w/{world}/village/build`, `…/rally/send`, …) are still
accepted — the freeze isn't enforced for it, while the home world (or none) is checked instead. (Flagged
out-of-scope in 056; this slice closes it.)

## Goal

- **AC1 — Freeze the targeted world.** For a POST under `/w/{world}/…`, `action_guard` resolves **that**
  world and rejects with `403` if it is won/frozen (021). A POST to a non-frozen world is unaffected.
- **AC2 — Account POSTs unaffected.** A POST with no world in the path (account routes: `/profile/bio`,
  `/settings/*`, `/sitting/*`, `/report`, `/admin/*`, `/mod/*`, `/worlds/join`, `/messages/send`,
  `/notifications/read`, `/logout`) is not freeze-checked (it is not a world game action). The per-account
  sanction check (022) is unchanged for all POSTs.

## Design

- `crates/web/src/lib.rs` `action_guard`: parse the world UUID from `req.uri().path()` (`/w/{uuid}/…`); if
  present, resolve its repo via `state.world_registry.context_for(world)` and check `repo.world_ended()` —
  `Some(_)` ⇒ `403`. If the path has no world, skip the freeze check (keep the sanction check). A small
  `world_in_path(&str) -> Option<WorldId>` helper does the parse. Reuses the registry's cached
  `context_for` (no new DB shape).

## Out of scope

- Per-world notifications/messages (058/059). The Wonder-build use-case's own `world_ended` guard
  (`wonder.rs`) already keys on its world's repo and is unchanged.
