# Settings & preferences

**Status:** Current
**Date:** 2026-06-15 · **Slice:** 029

## Context
Players want control over their own experience. This slice introduces a **Settings** page and its first,
most concrete preference — **per-kind notification preferences** — completing the deferral noted in 026.
Owner-scoped (P4); persisted (P2/P6).

## Design
- **Preference = a mute, default-on.** For each 026 notification **kind** (incoming attack / battle report /
  new message) a player is either subscribed (default) or has **muted** it. Muting means a notification of
  that kind is **not recorded** for that player — no feed row, no bell, no live nudge. The simplest faithful
  semantics: "don't notify me about X" = don't generate it.
- **Row-means-muted.** `notification_mutes (player_id, kind)` holds a row only for muted kinds; absence =
  enabled. New accounts get everything with no migration backfill, and the common case (all on) stores
  nothing.
- **Enforced at generation, transactionally.** All three 026 generation paths gain the same gate —
  `WHERE NOT EXISTS (SELECT 1 FROM notification_mutes m WHERE m.player_id = <recipient> AND m.kind =
  <kind>)`:
  - `record` (the bulk UNNEST insert — covers incoming-attack, which flows through it),
  - the `apply_battle` battle-report insert (in the report transaction),
  - the `send_dm` new-message insert.
  The live `pg_notify('notifications', …)` is emitted `FROM ins` / `FROM note`, so a muted recipient gets
  **no** nudge either. The DM's own `comms` notify is unaffected (only the notification is gated).
- **Owner-scoped use-cases.** `notification_settings(player)` returns every kind with its enabled state
  (enabled = not in `muted_kinds`); `set_notification_pref(player, kind, enabled)` maps to
  `set_mute(player, kind, !enabled)`. The caller passes the session player — no target id to forge (P4).
- **Web.** `GET /settings` renders a checkbox per kind (checked = enabled). `POST /settings/notifications`
  iterates **every** kind, setting enabled = "checkbox present in the form" (so unchecking is honoured), then
  redirects. A Settings link sits in the nav.

## Persistence (migration 0040)
- `notification_mutes (player_id uuid → users ON DELETE CASCADE, kind text, PRIMARY KEY (player_id, kind))`.
  The gate's `NOT EXISTS` is a primary-key probe (P11).

## Reuse / decisions
- **Mute = don't record** (not record-then-hide) — no wasted storage, and the gate makes a muted kind cost
  nothing downstream.
- **Gate in the existing 026 inserts** — preserves the transactional generation while honouring the
  preference; no second write path.
- **`NotificationKind::ALL`** drives both the settings page and the POST loop, so adding a future kind
  surfaces automatically.

## Consequences
- A Settings page players own, with working per-kind notification control enforced server-side.
- **Out of scope (deferred):** display options (theme/locale/density — need per-render plumbing),
  vacation/away mode (a simulation mechanic), per-channel chat prefs, email/digests, quiet hours, sounds.
