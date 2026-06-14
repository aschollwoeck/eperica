# Feature 025 — Player profiles & presence — Plan

**Spec:** ./spec.md · **Status:** Verified

A small social slice: an editable bio + a presence indicator derived from the existing `last_activity`
(019). Pure presence rule (P3), owner-only edit (P4), reproducible (P1/P2), online window config (P7).

## Domain (pure, P3) — `crates/domain/src/presence.rs`

- `enum Presence { Online, LastSeen(Timestamp) }` + `fn presence(last_activity, now, online_window_secs) -> Presence`
  (Online iff `now − last_activity ≤ window`, else `LastSeen(last_activity)`). Wall-clock.
- `MAX_BIO` + `fn valid_bio(&str) -> bool` (≤ cap; empty allowed = clearing).
- Online window added to a config rule (reuse `LifecycleRules` or a tiny new field; presence is lifecycle-
  adjacent). Plan: add `presence_online_secs` to `LifecycleRules` (lifecycle.toml) — one knob, P7.

## Persistence (migration `0036`)

- `users` += `bio text NOT NULL DEFAULT ''`.

## Application (ports + use-cases)

- `AccountRepository` (default methods): `set_bio(player, bio)`; `profile_of(player) -> Option<ProfileView>`
  where `ProfileView { name, bio, last_activity: Timestamp }`. `last_activity` also exposed where presence is
  surfaced (see below).
- `crates/application/src/profile.rs`: `edit_bio(accounts, owner, bio)` (validate; owner writes only their
  own — the handler passes the session player, so it's owner-scoped by construction, P4); `view_profile`.
  `ProfileError` (Invalid, NotFound, Backend).
- **Surfacing presence:** add `last_activity_ms` to `LeaderboardRow` (the board queries already join
  `users`) and to the conversations read (`ConversationSummary.other_last_ms` for DMs; channels have none).
  `villages_at` already returns `owner_last_activity` (006/map) — reused as-is.

## Web (`crates/web`)

- **Profile:** the public profile (`/stats/player/{id}`) shows the bio + a presence badge (compute
  `presence` from `last_activity`); on **own** profile, an "Edit bio" form → `POST /profile/bio`.
- **Presence touch middleware:** a layer that, for a logged-in request, calls `touch_activity` (throttled) —
  **excluding** `/static/*`, `/messages/stream/*`, `/messages/unread` (background pollers) so presence
  reflects real navigation and the 019 signal is preserved.
- **Surfaces:** a small presence indicator (a dot + "online"/"last seen …") rendered on the leaderboard
  rows, the conversations list + DM header, and map markers. A shared template helper formats it.
- `AppState` already has everything; the online window comes from `lifecycle_rules` (already in state).

## Reuse / decisions

- **Presence from `last_activity`** — no new signal; the 019 column + throttle are exactly the input.
- **Owner-only edit by construction** — the edit handler uses the authenticated session player as the
  subject, so a player can only ever edit their own bio (no id parameter to forge).
- **Background pollers excluded from touch** — avoids "idle tab = online" and avoids defeating 019's
  inactivity/abandonment (which reads the same `last_activity`).

## Risks / testing

- **Domain tests:** `presence` boundary (online window edge), `valid_bio` bounds.
- **DB tests:** bio set/clear round-trips; `profile_of` returns name+bio+last_activity; presence computed
  from a stale vs fresh `last_activity`.
- **Web tests:** owner can edit their bio + it shows on the profile; another player's `POST /profile/bio`
  only ever affects the actor's own row (no id to target); a profile shows an "online" vs "last seen" badge.
- **Touch/019 interaction:** confirm a normal page navigation touches `last_activity` but the unread poll /
  SSE stream do not (so idle tabs don't stay online and 019 still greys/abandons idle accounts).
