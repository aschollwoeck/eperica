# Feature 078 — the account/meta pages redesign

## Why

The account/meta surfaces — the **worlds lobby**, **settings**, **search**, and **account sitting** — are the
last non-alliance, non-admin pages on plain panels. This slice brings them onto the design system (the `.phead`
header + styled sections, forms, and result lists).

Presentation only — **no domain/routing/auth change** (P3/P4); every join/settings/search/sitting POST + link
+ the owner-scoping is unchanged.

## Acceptance criteria

- **AC1 — Worlds lobby.** `.phead` header + "Your worlds" / "Join a world" section heads over the tables
  (Home/Current badges, the per-world Enter, the join tribe-radio form) — all preserved.
- **AC2 — Settings.** `.phead` + the notification-preference checkboxes in a card (styled checkboxes).
- **AC3 — Search.** `.phead` + the search box + the coordinate/players/alliances results (reusing the 077
  `.conversations` list); the empty/typed states preserved.
- **AC4 — Sitting.** `.phead` + the "currently sitting" banner + section heads over the sitters / accounts-you-
  sit-for / audit tables + the grant/revoke/sit forms — all preserved.
- **AC5 — Behaviour preserved.** Every route/link/POST + the owner-scope/auth of each action is unchanged.

## Constitution

- **P3** — pure presentation; templates + CSS only. **P4** — no auth/scope change. **P11** — no new query.

## Out of scope

- The alliance overview page (`/alliance`) — its own slice (079); admin/moderation — a later slice.
