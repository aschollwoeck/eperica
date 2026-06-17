# Feature 062 — notification mutes apply across all the account's worlds

**Status:** Verified
**Amends:** 029 (notification preferences) under the 043–046 multi-world model. Closes the 061 deferral.

**Note:** A notification **mute** is an account-level preference — the settings page (029) is account-level
and `notification_mutes.player_id` references `users(id)` (the account). But the generation-time mute check
compares that account-keyed mute against the notification's `player_id`, which (after the 0045 repoint) is
the **per-world player id**. They coincide only in the home world, so a muted kind was still recorded in
non-home worlds. Make the mute apply in every world. No domain change (P3).

## Problem

`muted_kinds` / `set_mute` read/write `notification_mutes` keyed by the **account** (`AuthUser` = the `users`
id, the account-level settings handler). But the two generation sites that gate on mutes —
`record_battle` (`battle_report`) and the generic `record` — filter
`WHERE NOT EXISTS (… notification_mutes m WHERE m.player_id = u.player_id AND m.kind = …)`, where
`u.player_id` is the notification's **per-world** player id. In a non-home world `u.player_id ≠ account`, so
the mute never matches and the notification is recorded despite being muted. (The **DM** new-message gate
already compares against the recipient's `users` id, so DM mutes were unaffected.)

## Goal

- **AC1 — Mute spans worlds.** A kind the account has muted is suppressed at generation in **every** world
  it plays, not just the home world.
- **AC2 — No over-suppression / regression.** An unmuted kind is still recorded; muting one kind never
  suppresses another; home-world behavior is unchanged (the account id equals the home player id there).

## Design

The mute check resolves the notification's `player_id` (a `players.id`) to its owning `players.user_id` (the
account) before matching the account-keyed mute — `crates/infrastructure/src/repo.rs`, `record_battle` and
`record`:

```sql
WHERE NOT EXISTS (
    SELECT 1 FROM notification_mutes m
     JOIN players pl ON pl.id = u.player_id
     WHERE m.player_id = pl.user_id AND m.kind = …)
```

The mute read/write paths and the account-level settings handler are unchanged.

## Out of scope

- Per-world mute preferences (a mute is intentionally account-wide — one settings page, one row per kind).
