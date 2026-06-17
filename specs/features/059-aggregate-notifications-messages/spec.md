# Feature 059 — notifications aggregated across all the account's worlds

**Status:** Verified
**Amends:** 026 (notifications) under the 043–046 multi-world model + 056 routing.

**Note:** The account-level notifications feed/bell reads only the **home** world's notifications (the
`NotificationRepository` is the home-world repo). A player active in a non-home world never sees that world's
notifications in the bell. Aggregate the bell **across all the account's worlds** — one inbox on every page —
with each notification deep-linking into **its own** world (`/w/{world}/…`). Messages aggregation is a
sibling follow-up (060). No domain change (P3).

## Problem

`notifications_page` / `notifications_unread` / `notifications_read` use `state.accounts` (home repo), whose
`NotificationRepository::{list,unread_count,mark_read}` filter `WHERE world_id = home`. So the bell shows only
home-world notifications, and (056) their deep-links target the home world even when they belong elsewhere.

## Goal

- **AC1 — Aggregated feed.** The notifications page shows the account's notifications from **all** worlds it
  plays, most-recent first, each row carrying **its** world so its deep-link is `/w/{that-world}/…`.
- **AC2 — Aggregated unread badge.** The nav bell count sums unread across all the account's worlds.
- **AC3 — Mark-read across worlds.** Opening the feed marks the account's unread notifications read in **all**
  worlds.
- **AC4 — Server-authoritative (P4).** Aggregation keys on the **account** (a notification belongs to a
  `players` row owned by the account's `user_id`); a player only ever touches their own notifications across
  their own worlds.

## Design

The account's `user_id` equals its home player id (the reuse-UUID invariant), which is the `PlayerId` the
account-level handlers already hold (`AuthUser`). Join `notifications → players ON players.id =
notifications.player_id WHERE players.user_id = {account}` to span the account's worlds.

- `crates/application/src/ports.rs`: `NotificationView` gains `pub world: String` (the row's world UUID, for
  the deep-link). `NotificationRepository` gains `list_for_account` / `unread_count_for_account` /
  `mark_read_for_account` (keyed on the account `PlayerId` = `user_id`).
- `crates/infrastructure/src/repo.rs`: the three account methods (join through `players`, select `world_id`
  per row); `notification_from_row` reads `world_id`; the existing per-world `list` query adds `world_id` so
  it still fills the field (= its own world).
- `crates/application/src/notification.rs`: `list_notifications_for_account` / `notification_unread_for_account`
  / `mark_notifications_read_for_account` wrappers.
- `crates/web/src/handlers.rs`: `notifications_page` / `notifications_unread` / `notifications_read` call the
  account variants; `notification_href(world: &str, …)` builds `/w/{row-world}/…` per notification.

## Out of scope

- **Messages** aggregation (DMs/channels are per-world conversations) — slice 060.
- The **SSE live nudge** (`/notifications/stream`) stays keyed on the home player; non-home-world notifications
  reach the bell via the existing 20-second unread poll (which is now aggregated). Live cross-world nudges are
  a follow-up.
