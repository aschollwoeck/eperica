# Feature 081 — the auth pages (login / register)

## Why

The polished landing page (`index.html`) links to **login** and **register**, which were still plain panels —
a jarring step-down on the first interactive screens. This slice gives them a centered, **branded auth card**
matching the design system, completing the entry experience.

Presentation only — **no auth/routing change** (P3/P4); the same forms, fields, world/tribe inputs, and POSTs.

## Acceptance criteria

- **AC1 — Login.** A centered `.auth` layout with the Eperica brand crest + a `.auth-card` (title, sub, error
  alert, the username/password form with autocomplete, a full-width submit, and the cross-link to register).
- **AC2 — Register.** A wider `.auth-card` with the brand crest, the username/email/password fields, the tribe
  `.choice` (descriptions kept), the hidden `world` field when enlisting into a chosen world, the world-name
  sub, the error alert, a full-width submit, and the cross-link to login.
- **AC3 — Behaviour preserved.** Every field name, the `/login` + `/register` POST actions, the `world`/`tribe`
  inputs, the error/validation paths, and the autocomplete attributes are unchanged.

## Constitution

- **P3** — pure presentation; 2 templates + CSS. **P4** — no auth-logic change. **P11** — no new query.

## Out of scope

- The landing page (already designed) and the legal pages (their own `.legal` styling + real legal text).
