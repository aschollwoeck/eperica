# Feature 075 — the battle/scout reports redesign

## Why

The three report pages — the **reports list**, the per-defender **battle report**, and the **scout report** —
are the second core gameplay surface still on plain panels + tables. This slice brings them onto the design
system with a reusable content-page header and styled report layouts.

Presentation only — **no sim/visibility change** (P3/P4): the same reports, the same fog (a detected scout
target still sees only the notification, not the intel), the same per-report routes; only the layout changes.
Introduces a reusable **`.phead`** content-page header (for reports now, and the later leaderboard/stats/
profile/quests redesigns).

## Acceptance criteria

- **AC1 — Page header.** Each page uses the shared `.phead` header (eyebrow + title + sub + back link) on the
  design system, replacing the bare `<h1>` + muted-link.
- **AC2 — Reports list.** The list renders as clickable report **cards** (headline + outcome + relative time +
  link to detail) instead of a `<table>`; the empty state is kept.
- **AC3 — Battle report.** Header (kind + headline + outcome), a summary line (luck / morale / wall), the
  loot / razed / loyalty / **captured** outcomes as callout notes (capture highlighted), and the attacker +
  defender forces as two side-by-side panels with per-unit sent/defending + **lost** rows.
- **AC4 — Scout report.** Header + the revealed intel (resources, or wall + stationed troops) in a card; the
  detected-target view (notification only, no intel) and the "mission lost" state are preserved.
- **AC5 — Behaviour preserved.** Every route, the relative-time JS, the fog-of-war (no new data), and the
  visitor redirect work exactly as before — a reskin, not a rule change.

## Roles (see specs/roles.md)

- **Player** — reads their reports. No authority/visibility change (P4).

## Constitution

- **P3** — pure presentation; templates + CSS only (no handler/struct change). **P4** — fog unchanged.
  **P11** — no new query.

## Out of scope

- The moderation report queue (`/mod`) — a separate admin surface.
