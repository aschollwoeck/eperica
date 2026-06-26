# Feature 080 — the admin & moderation pages redesign

## Why

The admin console (`/admin`), the moderation review queue (`/mod`), and the per-account moderation view
(`/mod/account/{id}`) are the last functional pages on plain panels. This slice brings them onto the design
system, completing the redesign of every player- and staff-facing page.

Presentation only — **no domain/routing/auth change** (P3/P4); every admin/mod POST, the role gates, and the
advisory-only detection signals are unchanged.

## Acceptance criteria

- **AC1 — Admin console.** `.phead` header + section heads over: a stat-card summary (speed/radius/accounts/
  villages/pending) + the server detail table; the worlds table + the create-world form; and the account
  search + role-management table (make/remove mod & admin, self-protection) — all preserved.
- **AC2 — Review queue.** `.phead` + the open-reports table with the per-report resolve form (note + sanction
  select) — preserved.
- **AC3 — Mod account view.** `.phead` (the username) + Status (banned/suspended) + Detection signals (shared
  IP, action rate, with their flags) in cards + the sanction form; the "advisory only" note kept.
- **AC4 — Behaviour preserved.** Every route/POST/hidden field/role gate + the timestamp JS is unchanged.

## Constitution

- **P3** — pure presentation; 3 templates + (existing) CSS. **P4** — admin/mod gates stay server-side; the
  templates only render what they're given. **P11** — no new query.

## Out of scope

- Any change to the detection/sanction logic — display only.
