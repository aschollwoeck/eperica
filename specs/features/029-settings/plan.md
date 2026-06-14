# Feature 029 — Settings & preferences — Plan

**Spec:** ./spec.md · **Status:** Reviewed

A small account-UX slice: a Settings page + per-kind notification preferences, enforced at notification
generation (a muted kind produces no row). Owner-scoped (P4), default-on, persisted (P2).

## Domain (pure, P3) — `crates/domain/src/notification.rs`

- Add `NotificationKind::ALL` (the full set) so the settings page + tests can iterate every kind without
  hardcoding the list. No new module.

## Persistence (migration `0040`)

- `notification_mutes (player_id uuid → users on delete cascade, kind text, PRIMARY KEY (player_id, kind))`.
  A **row means muted**; absence means enabled (default-on, no backfill).

## Application (ports + use-cases)

- Extend `NotificationRepository` (default no-ops):
  - `muted_kinds(player) -> Vec<NotificationKind>` — the player's muted set.
  - `set_mute(player, kind, muted)` — insert (mute) or delete (un-mute) a row; idempotent.
  - **Generation gate:** `record`, and the inline `apply_battle` / `send_dm` notification inserts, gain a
    `WHERE NOT EXISTS (SELECT 1 FROM notification_mutes m WHERE m.player_id = <recipient> AND m.kind = <kind>)`
    so a muted recipient gets no row — and the live `pg_notify('notifications', …)` only fires for rows
    actually inserted (`SELECT … FROM ins`/`FROM note`).
- `crates/application/src/settings.rs`:
  - `notification_settings(notifs, player) -> Vec<(NotificationKind, bool /*enabled*/)>` — every kind with
    its enabled state (enabled = not in `muted_kinds`).
  - `set_notification_pref(notifs, player, kind, enabled)` — `set_mute(player, kind, !enabled)`.
  - `SettingsError` (Backend).

## Web (`crates/web`)

- `GET /settings` — the player's settings page: a checkbox per kind (checked = enabled).
- `POST /settings/notifications` — owner-scoped; for each kind, enabled = the checkbox is present in the
  form (absent = muted). Persists via `set_notification_pref`, redirects back.
- A **Settings** link in `base.html` (next to Profile).

## Reuse / decisions

- **Mute = don't record** (not "record but hide") — simplest faithful semantics; the gate lives at
  generation, so a muted kind costs nothing downstream (no bell, no feed row, no nudge).
- **Row-means-muted, default-on** — no backfill; the common case (everything on) stores nothing.
- **Gate in the existing insert SQL** — keeps notification creation transactional (026) while honouring the
  preference; the `NOT EXISTS` subquery is keyed on the small `notification_mutes` PK.
- **Owner-scoped by construction** — every use-case takes the session player as the subject.

## Risks / testing

- **Domain tests:** `NotificationKind::ALL` covers every variant (round-trips).
- **DB tests:** `set_mute` insert/delete idempotent; `muted_kinds` reflects it; a muted recipient gets no
  row from `record` (and from the `apply_battle` / `send_dm` paths), while a non-muting player still does.
- **Application tests (fakes):** `notification_settings` reports enabled/disabled; `set_notification_pref`
  maps to `set_mute`.
- **Web tests:** the settings page shows toggles; disabling a kind then triggering it yields no
  notification (and the bell stays 0); re-enabling restores it; a Visitor is redirected; one player's mute
  doesn't affect another.
- **Performance (P11):** the gate is a PK `NOT EXISTS`; settings reads are tiny bounded queries.
