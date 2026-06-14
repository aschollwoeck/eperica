# Feature 025 — Player profiles & presence

**Status:** Reviewed
**Depends on:** 019 (`last_activity` + the throttled `touch_activity` — the activity signal), 016 (the public player stats page presence/bio attach to), 024 (conversations, where presence is most useful), 001 (auth/sessions)
**Roadmap:** app-layer social/meta (`social-and-meta-features.md` §Presentation & profiles) — a richer **public profile** (an editable bio) and an **online / last-seen presence** indicator surfaced across the UI.

## Goal

Make players legible to each other: a public profile they can personalise with a **bio**, and an
**online / last-seen** indicator derived from their activity — shown where you encounter other players
(their profile, the leaderboard, conversations, the map). Server-authoritative (P4); presence is a pure,
reproducible read of the existing `last_activity` signal (P1/P2).

## Concepts

- **Profile bio.** A short free-text description on the account, editable **only by its owner** (P4),
  shown on the player's public profile. Validated (non-empty-after-trim is not required — clearing is
  allowed — but length-capped). No HTML/markup interpretation (rendered as text).

- **Presence.** Derived from `last_activity` (019) vs. now: **Online** if active within a configured
  **online window** (P7), else **Last seen `<when>`**. A pure function (`presence`), reproducible (P2) —
  not a stored flag. It is wall-clock (real-time), independent of game speed.

- **Activity freshness.** `last_activity` already updates (throttled) on the village page + on every
  mutating action; this slice **broadens** the touch to genuine page navigations (a small middleware) so
  presence reflects real use — while **excluding background pollers** (the unread-badge poll, the SSE
  stream) so an idle open tab does not read as perpetually online (and does not defeat the 019
  inactivity/abandonment signal, which shares `last_activity`).

- **Surfaces.** Presence is shown on the **public profile**, next to names on the **leaderboard**, on the
  **conversations** list + DM header (the other party), and on **map** village markers (which already carry
  the owner's `last_activity`). The bio shows on the public profile.

## Acceptance criteria

> Editing is owner-only and server-authoritative (P4). Presence + bio are reproducible from persisted state
> (P1/P2); the online window is config (P7).

- **AC1 — Edit own bio.** A logged-in player can set/clear their profile bio (length-capped, trimmed),
  persisted. Another player cannot edit it (server-enforced); a Visitor cannot.

- **AC2 — Public profile shows bio + presence.** A player's public profile shows their bio (if any) and a
  presence indicator (Online, or "last seen `<when>`"), alongside the existing identity/stats (016/017).

- **AC3 — Presence rule.** Presence is **Online** iff `now − last_activity ≤ online_window` (config), else
  **Last seen** at `last_activity`. Deterministic from persisted state; wall-clock (not speed-scaled).

- **AC4 — Presence surfaced across the UI.** The same presence indicator appears on the **leaderboard**
  rows, the **conversations** list + DM view (the other party), and **map** markers.

- **AC5 — Activity stays fresh on navigation.** Browsing authenticated pages keeps `last_activity` current
  (throttled), so presence is meaningful — **but** background pollers (unread badge, SSE stream) and static
  assets do **not** count as activity (so an idle tab is not perpetually "online", preserving the 019
  inactivity signal).

- **AC6 — Roles.** A public profile is viewable by anyone (incl. a Visitor, as today); **editing** requires
  the logged-in **owner**. No client action edits another player's profile or fakes presence (P4).

- **AC7 — Reproducibility & config.** Bio + `last_activity` are persisted; presence is recomputed on read
  (P1/P2). The online window is config (P7).

## Roles & permissions

Per [roles.md](../../roles.md). Profiles are public reads; editing is owner-only.

| Role | Permitted | Denied (server-enforced) |
|------|-----------|--------------------------|
| **Visitor** | View public profiles + presence. | Edit any profile. |
| **Player** | View profiles + presence; **edit their own** bio. | Edit another player's profile; fake presence. |
| **Moderator/Administrator** | (as Player) + their elevated functions (022). | — |

## Out of scope

- Avatars/images, profile themes, rich text/markup, profile privacy settings — future work.
- Custom status messages, "typing…", and per-conversation presence beyond online/last-seen.
- Notification preferences / settings (a separate account-UX slice).
