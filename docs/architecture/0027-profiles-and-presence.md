# Player profiles & presence

**Status:** Current
**Date:** 2026-06-14 · **Slice:** 025

## Context
Players want a little identity and a sense of who is around: an editable **bio** on their public profile,
and a lightweight **presence** signal (online / last-seen) surfaced where players already look at each
other — the public stats page, the leaderboards, conversations, and the map. This must stay
**server-authoritative** (P4), **lazy/compute-on-read** (P1), and add **no new activity tick**.

## Design
- **Presence is derived, not stored.** There is no presence table and no heartbeat job. Presence is a pure
  function of the account's existing `last_activity` (introduced by 019 for inactivity/abandonment):
  `presence(last_activity, now, online_secs) -> Online | LastSeen(ts)` in `domain/presence.rs` (P1/P3).
  A player is **Online** iff they acted within the configured window, else **LastSeen** with the timestamp.
  The window `presence_online_secs` lives in `LifecycleRules` / `balance/lifecycle.toml` (P7); reusing the
  lifecycle rules keeps the single "what counts as active" knob in one place.
- **Freshness via a touch middleware.** `last_activity` is refreshed by a `presence_touch` middleware on any
  logged-in request, **excluding** `/static`, `/messages/unread`, and `/messages/stream/*` — the background
  pollers/streams an idle tab fires would otherwise keep a player perpetually "online" and would corrupt the
  019 inactivity signal that reads the same column. The write is **throttled** in the repo (one tiny write
  per window, not per request, P11).
- **Bio.** A plain-text bio (≤ `MAX_BIO` = 500 chars), validated in the pure domain (`valid_bio`), stored on
  `users.bio` (migration 0036, default `''`). Editing is **owner-scoped** (`edit_bio` keys off the session
  player only — P4); the public view is read-only on the stats page. Use-cases: `edit_bio` / `view_profile`
  (`application/profile.rs`, `ProfileView { player, name, bio, last_activity }`).
- **Surfaces (a shared `presence_view` helper).** The web layer maps a `Presence` to an `(online, label)`
  pair once and reuses it everywhere:
  - **Public stats page** (`/stats/player/{id}`): bio + an online/last-seen badge.
  - **Own profile** (`/profile`): the bio edit form (`POST /profile/bio`).
  - **Leaderboards:** `LeaderboardRow` now carries `last_activity` (added to the population / conflict /
    climber board SQL); player rows render an indicator, alliance rows do not.
  - **Conversations:** `ConversationSummary.other_last_activity` (from the `dm_threads` read) drives an
    indicator on each DM row and the DM thread header; channels carry no single presence (`None`).
  - **Map markers:** village labels append the owner's presence (markers already carried
    `owner_last_activity` for the 019 greying).

## Persistence (migration 0036)
- `users.bio text NOT NULL DEFAULT ''` — the only new column. Presence reuses the existing
  `users.last_activity`; boards/threads read it inline as epoch-ms.

## Reuse / decisions
- **No presence store / heartbeat:** presence is a read-time projection of `last_activity`. Zero new write
  path, zero background work, naturally correct across multiple stateless web instances (P5).
- **Reuse the lifecycle activity signal:** one definition of "active" for both presence and inactivity; the
  touch middleware's exclusions protect both.
- **Throttled touch:** keeps an authenticated browse at ≤ one small write per throttle window (P11).

## Consequences
- A read-only presence indicator and an editable bio with no new infrastructure and no new enforcement path.
- "Online" precision is bounded by the touch throttle + the configured window — intentional: presence is a
  hint, not a security boundary; private state (troops, resources) is never exposed (P4/§7.3).
- Richer profiles (avatars, public achievements showcase, status text) and a true real-time presence channel
  remain future work.
