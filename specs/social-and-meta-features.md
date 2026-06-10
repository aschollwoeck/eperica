# Eperica — Social & Meta Features

**Status:** Backlog / outline
**Governed by:** [constitution.md](./constitution.md)
**Relationship to the GDD:** [game-design.md](./game-design.md) defines the *simulation mechanics*
(the rules of the world). This document collects **application-layer features** that surround the
game but are **not simulation rules** — communication, presentation, and account-meta UX. They are
listed here so the data/account models leave room for them (P10); each becomes its own feature spec
under `features/` when built.

Game-rule competition (leaderboards, medals, win condition) lives in the **GDD §11**, not here — that
is a simulation/scoring concern. This document is for the *social and presentation* surface.

## Communication

- **Private messaging** — player-to-player in-game mail (inbox, compose, archive).
- **Alliance forum** — threaded boards scoped to an alliance (and confederations), with per-role
  posting rights (ties to GDD §10.1).
- **In-game chat** — real-time chat channels (alliance, possibly public/region).
- **Reports inbox** — battle/scout/trade reports delivered to the player, with read/unread, grouping,
  sharing to alliance, and filtering. (Report *content* is defined by GDD §9.5; this is the inbox UX.)
- **Notifications / alerts** — incoming-attack warnings, build/training completion, message arrival.

## Presentation & profiles

- **Player profile pages** — public profile, **medals** and **achievement badges** (granted per GDD
  §11.2), description, alliance, villages.
- **Map UI** — interactive map browsing, markers, distance/send shortcuts.
- **Leaderboard/statistics UI** — the presentation of the rankings defined in GDD §11.2.
- **Search / who-is** — find players, alliances, villages, coordinates.

## Account & meta UX

- **Tutorial / quest presentation** — the UX wrapper around the quest rewards defined in GDD §12.1.
- **Settings & preferences** — language, notification prefs, display options.
- **Account sitting & vacation mode** — authorized co-login and away status (deferred mechanics noted
  in GDD §12.4; the UX and access-control belong here).
- **Admin / moderation tools** — the enforcement surface for the fair-play *rules* in GDD §12.5:
  player reporting, review queues, sanctions, and surfacing multi-account/bot detection signals.

## Notes

- None of these change simulation outcomes; the server stays authoritative (P4) and state remains
  reproducible (P2). They read and present game state, or carry social content alongside it.
- Build order: these are sequenced **after** the core game loop unless a feature spec explicitly
  pulls one earlier (e.g. a minimal reports inbox is needed as soon as combat exists).
