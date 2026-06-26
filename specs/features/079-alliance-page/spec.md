# Feature 079 — the alliance overview redesign

## Why

The alliance page (`/alliance`) is the largest remaining plain-panel page — both the not-in-alliance state
(found / invitations) and the in-alliance command centre (invite, member roles, diplomacy, shared-defence
incoming, allied villages, leave/disband). This slice brings it onto the design system without restructuring
its many forms.

Presentation only — **no domain/routing/auth change** (P3/P4); every form action, hidden field, role/right
gate (`can_invite`/`can_manage`/`can_expel`/`is_founder`/`can_diplomacy`) and confirm-dialog is preserved.

## Acceptance criteria

- **AC1 — Header.** `.phead` header that adapts: the alliance name + [tag] + role + forum link when in an
  alliance, else "Alliance" with the embassy hint; ← Village return.
- **AC2 — Not-in-alliance.** The embassy notice + the (gated) found form + the invitations table — preserved.
- **AC3 — In-alliance.** Each block (invite, members + role management, diplomacy, incoming shared-defence,
  allied villages, leave/disband) under a `.bld-cols__head` section head, keeping every table, inline form,
  role gate, and the disband/transfer confirm dialogs.
- **AC4 — Behaviour preserved.** Every route/POST/hidden field/gate + the incoming-attack relative-time JS is
  unchanged — a reskin, not a rule change.

## Constitution

- **P3** — pure presentation; one template + (078's already-added) CSS. **P4** — all role/right gates stay
  server-authoritative; the template only renders the gates it was given. **P11** — no new query.

## Out of scope

- The alliance **stats** page (done in 076) and the **forum** (done in 077); admin/moderation — a later slice.
