# Feature 061 — cross-world live notification-bell nudge

**Status:** Verified
**Amends:** 026 (notifications live SSE) under the 043–046 multi-world model. Closes the 059 deferral.

**Note:** 059 aggregated the notification feed/bell **count** across all the account's worlds, but explicitly
left the **live SSE nudge** (`/notifications/stream`) keyed on the home player: a notification raised in a
non-home world reached the bell only via the ~20-second unread poll, not instantly. Make the live nudge span
all the account's worlds too. No domain change (P3).

## Problem

The bell SSE subscribes on the account's private key `notif:<account>` (`notif_key(AuthUser)`, where
`AuthUser` = the account `users` id = the home player id). But the notify side keys each nudge on the
notification's `player_id` — which, after the 0045 repoint, is the **per-world player id**. In the home world
that equals the account id (the reuse-UUID invariant), so the nudge matches; in any other world it does not,
so non-home-world notifications never nudge the bell live (they surface on the next aggregated poll).

Two of the three notify sites are affected — combat **battle reports** (`record_battle`) and the generic
**`record`** path (e.g. incoming-attack alerts) — both emit `notif:<player_id>`. The **DM** new-message notify
already keys on the recipient's `users` id (its `player_id` bind is the recipient's home player), so it is
already account-scoped.

## Goal

- **AC1 — Cross-world live nudge.** A notification raised in **any** world the account plays nudges the
  account's bell stream **live** (no poll wait), exactly as a home-world notification does.
- **AC2 — Still private (P4).** The nudge still routes only to the owning account's private key — a player
  never receives another account's nudge. (The payload remains kind-only; the bell refetches the count.)
- **AC3 — No home-world regression.** Home-world notifications nudge exactly as before (the resolved key is
  identical there, since the home player id equals the account id).

## Design

The notify key is made **account-scoped** by resolving the notification's `player_id` (a `players.id`) to its
owning `players.user_id` (the account) in the same statement. The SSE subscriber is unchanged — it already
keys on `notif_key(account)`.

- `crates/infrastructure/src/repo.rs`: in `record_battle` (the `battle_report` notify) and `record` (the
  generic notify), change the final `pg_notify` to
  `'notif:' || p.user_id::text` via `… FROM ins JOIN players p ON p.id = ins.player_id`. The DM notify is left
  as-is (already account-keyed). No subscriber/handler change.

## Out of scope

- **Notification mutes across worlds** (029): `notification_mutes.player_id` is the account `users` id, so the
  mute check `m.player_id = u.player_id` mismatches a non-home-world player id — a pre-existing cross-world
  gap, tracked separately, not addressed here.
- The **comms/chat** live streams (slice 060 already world-scoped them).
