# Feature 029 — Settings & preferences

**Status:** Reviewed
**Depends on:** 026 (notifications — the kinds these preferences gate), 001 (auth/sessions)
**Roadmap:** app-layer social/meta (`social-and-meta-features.md` §Account & meta UX → "Settings & preferences").

## Goal

Give each player a **Settings** page to control their own preferences. This slice delivers the most
concrete one — **per-kind notification preferences** (mute the kinds you don't want) — completing the
deferral noted in 026. Server-authoritative (P4): a muted kind is never recorded for that player; the page
is the player's own, owner-scoped. Persisted (P2/P6).

## Concepts

- **Notification preference.** For each notification **kind** (incoming attack / battle report / new
  message, 026) a player has an **enabled** toggle, default **on**. Muting a kind means a notification of
  that kind is **not recorded** for that player going forward (so it raises no bell and never appears in the
  feed). Already-recorded notifications are unaffected.

- **Owner-scoped.** A player reads + changes only **their own** settings (keyed by the session player —
  there is no target id to forge, P4).

- **Default on.** The absence of a stored mute means the kind is enabled; muting stores a row, un-muting
  removes it. New accounts get everything (no migration backfill needed).

## Acceptance criteria

> All reads/writes are server-authoritative and owner-scoped (P4). Enforcement is at notification
> **generation** (a muted kind produces no row), reproducible from persisted state (P1/P2).

- **AC1 — View settings.** A logged-in player sees a Settings page listing each notification kind with its
  current enabled/disabled state.

- **AC2 — Change a preference.** A player can enable/disable any kind; the change persists and is reflected
  on reload. Owner-scoped — a player only ever changes their own.

- **AC3 — Mute suppresses generation.** When a kind is disabled for a player, a new event of that kind
  records **no** notification for them (no feed entry, no bell, no live nudge) — verified for all three
  kinds (incoming attack, battle report, new message). Other players (who did not mute) are unaffected.

- **AC4 — Enabled is the default.** With no stored preference a kind is enabled; un-muting restores
  generation. Toggling is idempotent.

- **AC5 — Roles.** Per [roles.md](../../roles.md): settings are private, owner-only.

| Role | Permitted | Denied (server-enforced) |
|------|-----------|--------------------------|
| **Visitor** | — (redirected to login). | View/change any settings. |
| **Player** | View + change **their own** settings. | View or change another player's settings. |
| **Moderator/Administrator** | (as Player) for their own. | — |

- **AC6 — Reproducibility & config.** Preferences are persisted; generation consults them on each event
  (P1/P2). No hardcoded wall-clock behaviour (P7).

## Out of scope

- **Display options** (theme/skin, language/locale, density) — future work (needs per-render plumbing).
- **Vacation / away mode** — a simulation mechanic (attack immunity / away status), deferred per the GDD;
  this slice is preferences only, not sim behaviour.
- Per-channel chat preferences, email/digest delivery, quiet hours, and notification-sound settings.
- Account settings already covered elsewhere (password/email change, tribe) — not part of this slice.
